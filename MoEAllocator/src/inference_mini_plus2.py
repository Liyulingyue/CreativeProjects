#!/usr/bin/env python3
import json
import sys
from pathlib import Path
from typing import Dict

import safetensors.torch as storch
import torch
import torch.nn.functional as F
from transformers import AutoTokenizer

try:
    from src.moe_grouped import grouped_moe_forward
except ModuleNotFoundError:
    from moe_grouped import grouped_moe_forward


class MiniMoE:
    def __init__(self, manifest_path: str | Path, expert_ids: list[int] | None = None, dtype: torch.dtype | None = None):
        with open(manifest_path) as f:
            data = json.load(f)

        self.cfg = data["config"]
        self._merge_original_config(data.get("model_path"))
        self.output_dir = Path(data["output_dir"])

        self.hidden_size = self.cfg["hidden_size"]
        self.num_attention_heads = self.cfg.get("num_attention_heads", 20)
        self.num_kv_heads = self.cfg.get("num_key_value_heads", 4)
        self.head_dim = self.hidden_size // self.num_attention_heads
        self.intermediate_size = self.cfg.get("intermediate_size", 12288)
        self.moe_intermediate = self.cfg["moe_intermediate_size"]
        self.num_hidden_layers = self.cfg["num_layers"]
        self.moe_k = self.cfg["moe_k"]
        self.moe_norm_min = self.cfg.get("moe_norm_min", 1e-12)
        self.num_shared_experts = self.cfg["num_shared_experts"]
        self.num_experts = self.cfg["num_experts"]
        self.rope_theta = self.cfg.get("rope_theta", 500000.0)
        self.rms_norm_eps = self.cfg.get("rms_norm_eps", 1e-5)
        self.vocab_size = self.cfg.get("vocab_size", None)

        if expert_ids is None:
            expert_ids = list(range(self.num_experts))
        self.expert_ids = expert_ids

        self.dtype = dtype or self._dtype_from_config()
        self.device = torch.device("cpu")

        self.weights: dict[str, torch.Tensor] = {}
        self.expert_weights: dict[tuple[int, int], dict[str, torch.Tensor]] = {}
        self.debug = False

    def _merge_original_config(self, model_path: str | None) -> None:
        if not model_path:
            return
        config_path = Path(model_path) / "config.json"
        if not config_path.exists():
            return
        with open(config_path) as f:
            original_config = json.load(f)

        mapped_keys = {
            "moe_num_experts": "num_experts",
            "num_hidden_layers": "num_layers",
            "moe_num_shared_experts": "num_shared_experts",
        }
        for source_key, target_key in mapped_keys.items():
            if target_key not in self.cfg and source_key in original_config:
                self.cfg[target_key] = original_config[source_key]

        for key in (
            "torch_dtype",
            "num_attention_heads",
            "num_key_value_heads",
            "intermediate_size",
            "rope_theta",
            "rms_norm_eps",
            "vocab_size",
            "moe_norm_min",
            "moe_layer_start_index",
            "moe_layer_end_index",
            "moe_layer_interval",
        ):
            if key not in self.cfg and key in original_config:
                self.cfg[key] = original_config[key]

    def _dtype_from_config(self) -> torch.dtype:
        if self.cfg.get("torch_dtype") in ("bfloat16", "bf16"):
            return torch.bfloat16
        return torch.float32

    def _should_keep_fp32(self, name: str) -> bool:
        keep = self.cfg.get("keep_in_fp32", ["gate.weight", "moe_statics"])
        return any(pattern in name for pattern in keep)

    def _to_tensor(self, name: str, tensor: torch.Tensor) -> torch.Tensor:
        target_dtype = torch.float32 if self._should_keep_fp32(name) else self.dtype
        return tensor.to(target_dtype).to(self.device)

    def _load_weights_file(self, file_path: Path, selector: str | None = None) -> None:
        if not file_path.exists():
            return
        loaded = storch.load_file(str(file_path))
        for k, v in loaded.items():
            if selector is None or selector in k:
                self.weights[k] = self._to_tensor(k, v)

    def load_backbone(self):
        backbone_path = self.output_dir / "backbone" / "backbone.safetensors"
        self._load_weights_file(backbone_path)
        print(f"  backbone loaded: {len([k for k in self.weights if k.startswith('model.')])} tensors")

    def load_gate(self):
        gate_path = self.output_dir / "backbone" / "gate.safetensors"
        self._load_weights_file(gate_path)
        print(f"  gate loaded: {len([k for k in self.weights if '.mlp.gate.' in k])} tensors")

    def load_shared_expert(self):
        shared_path = self.output_dir / "backbone" / "shared_expert.safetensors"
        self._load_weights_file(shared_path)
        print(f"  shared_expert loaded: {len([k for k in self.weights if 'shared_experts' in k])} tensors")

    def load_experts(self) -> None:
        loaded = 0
        for eid in self.expert_ids:
            for layer_id in range(1, self.num_hidden_layers + 1):
                file_name = f"expert_{eid:03d}_layer_{layer_id:03d}.safetensors"
                file_path = self.output_dir / "experts" / file_name
                if not file_path.exists():
                    continue
                weights = storch.load_file(str(file_path))
                self.expert_weights[(layer_id, eid)] = {k: self._to_tensor(k, v) for k, v in weights.items()}
                loaded += 1
        print(f"  experts loaded: {loaded} files ({len(self.expert_ids)} experts × {self.num_hidden_layers} layers)")

    def load_all(self) -> None:
        print("Loading mini model from split files...")
        self.load_backbone()
        self.load_gate()
        self.load_shared_expert()
        self.load_experts()
        print("Done.\n")

    def _get_weight(self, weights: Dict[str, torch.Tensor], suffix: str) -> torch.Tensor:
        for k, v in weights.items():
            if k.endswith(suffix):
                return v
        raise KeyError(f"No weight ending with {suffix}")

    def _rms_norm(self, x: torch.Tensor, weight: torch.Tensor) -> torch.Tensor:
        input_dtype = x.dtype
        x = x.to(torch.float32)
        variance = x.pow(2).mean(-1, keepdim=True)
        x = x * torch.rsqrt(variance + self.rms_norm_eps)
        return weight * x.to(input_dtype)

    def _rotate_half(self, x: torch.Tensor) -> torch.Tensor:
        """Rotates half the hidden dims of the input."""
        x1 = x[..., 0::2]
        x2 = x[..., 1::2]
        return torch.stack((-x2, x1), dim=-1).flatten(-2)

    def _rope_compute(self, seq_len: int, device: torch.device) -> tuple[torch.Tensor, torch.Tensor]:
        inv_freq = 1.0 / (self.rope_theta ** (torch.arange(0, self.head_dim, 2, dtype=torch.float32, device=device) / self.head_dim))
        t = torch.arange(seq_len, dtype=torch.float32, device=device)
        freqs = torch.outer(t, inv_freq)
        emb = torch.cat((freqs, freqs), dim=-1)
        cos = emb.cos()
        sin = emb.sin()
        return cos, sin

    def _apply_rotary_pos_emb(self, q: torch.Tensor, k: torch.Tensor, cos: torch.Tensor, sin: torch.Tensor, unsqueeze_dim: int = 1) -> tuple[torch.Tensor, torch.Tensor]:
        """Applies Rotary Position Embedding to the query and key tensors."""
        cos = cos.unsqueeze(unsqueeze_dim)
        sin = sin.unsqueeze(unsqueeze_dim)
        cos = cos[..., : cos.shape[-1] // 2].repeat_interleave(2, dim=-1)
        sin = sin[..., : sin.shape[-1] // 2].repeat_interleave(2, dim=-1)
        q_embed = (q.float() * cos) + (self._rotate_half(q).float() * sin)
        k_embed = (k.float() * cos) + (self._rotate_half(k).float() * sin)
        return q_embed.to(q.dtype), k_embed.to(k.dtype)

    def _attention(self, x: torch.Tensor, layer_id: int) -> torch.Tensor:
        prefix = f"model.layers.{layer_id}.self_attn"
        q_w = self.weights[f"{prefix}.q_proj.weight"]
        k_w = self.weights[f"{prefix}.k_proj.weight"]
        v_w = self.weights[f"{prefix}.v_proj.weight"]

        q = F.linear(x, q_w)
        k = F.linear(x, k_w)
        v = F.linear(x, v_w)

        bsz, seq_len, _ = x.shape
        q = q.view(bsz, seq_len, self.num_attention_heads, self.head_dim)
        k = k.view(bsz, seq_len, self.num_kv_heads, self.head_dim)
        v = v.view(bsz, seq_len, self.num_kv_heads, self.head_dim)

        cos, sin = self._rope_compute(seq_len, x.device)
        q, k = self._apply_rotary_pos_emb(q, k, cos, sin)
        q = q.transpose(1, 2)
        k = k.transpose(1, 2)
        v = v.transpose(1, 2)

        try:
            attn_output = F.scaled_dot_product_attention(
                q,
                k,
                v,
                scale=self.head_dim ** -0.5,
                is_causal=seq_len > 1,
                enable_gqa=self.num_attention_heads != self.num_kv_heads,
            )
            attn_output = attn_output.transpose(1, 2).contiguous()
            return F.linear(attn_output.view(bsz, seq_len, self.hidden_size), self.weights[f"{prefix}.o_proj.weight"])
        except TypeError:
            pass

        if self.num_attention_heads != self.num_kv_heads:
            n_rep = self.num_attention_heads // self.num_kv_heads
            k = k.repeat_interleave(n_rep, dim=1)
            v = v.repeat_interleave(n_rep, dim=1)

        scaling = self.head_dim ** -0.5
        attn_scores = torch.matmul(q, k.transpose(-2, -1)) * scaling

        if seq_len > 1:
            causal_mask = torch.triu(torch.ones(seq_len, seq_len, device=x.device, dtype=torch.bool), diagonal=1)
            attn_scores = attn_scores.masked_fill(causal_mask, float("-inf"))

        attn_probs = F.softmax(attn_scores, dim=-1, dtype=torch.float32).to(q.dtype)
        attn_output = torch.matmul(attn_probs, v)
        attn_output = attn_output.transpose(1, 2).contiguous()
        return F.linear(attn_output.view(bsz, seq_len, self.hidden_size), self.weights[f"{prefix}.o_proj.weight"])

    def _ffn(self, x: torch.Tensor, down: torch.Tensor, up: torch.Tensor, gate: torch.Tensor) -> torch.Tensor:
        return F.linear(F.silu(F.linear(x, gate)) * F.linear(x, up), down)

    def _gate_forward(self, x: torch.Tensor, layer_id: int) -> torch.Tensor:
        gate_weight = self.weights[f"model.layers.{layer_id}.mlp.gate.weight"]
        return F.linear(x.float(), gate_weight.float())

    def _gate_with_bias(self, x: torch.Tensor, layer_id: int) -> tuple[torch.Tensor, torch.Tensor]:
        gate_weight = self.weights[f"model.layers.{layer_id}.mlp.gate.weight"]
        router_logits = F.linear(x.float(), gate_weight.float())
        routing_weights = F.softmax(router_logits, dim=-1, dtype=torch.float)
        
        # Add e_score_correction_bias
        bias_key = f"model.layers.{layer_id}.mlp.moe_statics.e_score_correction_bias"
        if bias_key in self.weights:
            routing_weights = routing_weights + self.weights[bias_key]
        
        return router_logits, routing_weights

    def _moe_expert_local(self, x: torch.Tensor, layer_id: int, expert_id: int) -> torch.Tensor | None:
        key = (layer_id, expert_id)
        if key not in self.expert_weights:
            return None
        weights = self.expert_weights[key]
        down = self._get_weight(weights, "down_proj.weight")
        up = self._get_weight(weights, "up_proj.weight")
        gate = self._get_weight(weights, "gate_proj.weight")
        return self._ffn(x, down, up, gate)

    def _moe_expert_packed(self, x: torch.Tensor, layer_id: int, expert_id: int) -> torch.Tensor | None:
        key = (layer_id, expert_id)
        if key not in self.expert_weights:
            return None
        weights = self.expert_weights[key]
        down = self._get_weight(weights, "down_proj.weight")
        up = self._get_weight(weights, "up_proj.weight")
        gate = self._get_weight(weights, "gate_proj.weight")
        gate_up = torch.cat((gate, up), dim=0)
        gate_out, up_out = F.linear(x, gate_up).chunk(2, dim=-1)
        return F.linear(F.silu(gate_out) * up_out, down)

    def _pack_layer_experts(self, layer_id: int) -> tuple[torch.Tensor, torch.Tensor] | None:
        # 只有某一层的全部 routed experts 都加载齐全时，才能复刻
        # Transformers 的 grouped-mm 专家路径；partial expert 调试模式继续走下面的旧循环。
        gate_up_proj = None
        down_proj = None
        for expert_id in range(self.num_experts):
            weights = self.expert_weights.get((layer_id, expert_id))
            if weights is None:
                return None
            gate = self._get_weight(weights, "gate_proj.weight")
            up = self._get_weight(weights, "up_proj.weight")
            down = self._get_weight(weights, "down_proj.weight")
            if gate_up_proj is None:
                # Transformers 中 gate_up_proj 的顺序是 gate 在前、up 在后。
                gate_up_proj = torch.empty(
                    self.num_experts,
                    gate.shape[0] + up.shape[0],
                    gate.shape[1],
                    dtype=gate.dtype,
                    device=gate.device,
                )
                down_proj = torch.empty(
                    self.num_experts,
                    down.shape[0],
                    down.shape[1],
                    dtype=down.dtype,
                    device=down.device,
                )
            gate_up_proj[expert_id, : gate.shape[0]].copy_(gate)
            gate_up_proj[expert_id, gate.shape[0] :].copy_(up)
            down_proj[expert_id].copy_(down)
        if gate_up_proj is None or down_proj is None:
            return None
        return gate_up_proj, down_proj

    def _moe_experts_grouped(
        self,
        x_flat: torch.Tensor,
        layer_id: int,
        top_k_index: torch.Tensor,
        top_k_weights: torch.Tensor,
    ) -> torch.Tensor | None:
        packed = self._pack_layer_experts(layer_id)
        if packed is None:
            return None
        gate_up_proj, down_proj = packed
        return grouped_moe_forward(x_flat, top_k_index, top_k_weights, gate_up_proj, down_proj, self.num_experts)

    def _shared_expert(self, x: torch.Tensor, layer_id: int) -> torch.Tensor:
        prefix = f"model.layers.{layer_id}.mlp.shared_experts"
        down = self.weights.get(f"{prefix}.down_proj.weight")
        up = self.weights.get(f"{prefix}.up_proj.weight")
        gate = self.weights.get(f"{prefix}.gate_proj.weight")
        if down is None or up is None or gate is None:
            return torch.zeros_like(x)
        return self._ffn(x, down, up, gate)

    def _standard_mlp(self, x: torch.Tensor, layer_id: int) -> torch.Tensor:
        prefix = f"model.layers.{layer_id}.mlp"
        down = self.weights[f"{prefix}.down_proj.weight"]
        up = self.weights[f"{prefix}.up_proj.weight"]
        gate = self.weights[f"{prefix}.gate_proj.weight"]
        return self._ffn(x, down, up, gate)

    def _moe_layer(self, x_norm: torch.Tensor, layer_id: int) -> torch.Tensor:
        bsz, seq_len, _ = x_norm.shape
        x_flat = x_norm.view(-1, x_norm.shape[-1])

        # Shared experts are one wider MLP of size moe_intermediate_size * num_shared_experts.
        # Transformers computes routed experts first, then adds this shared output.
        shared_output = self._shared_expert(x_flat, layer_id)
        if self.debug and layer_id == 1:
            se_out = shared_output.view(bsz, seq_len, -1)
            print(f"[DBG] L{layer_id} shared_expert: sum={se_out.sum().item():.6f}, first5={se_out[0, 0, :5].tolist()}")

        # Routing
        gate_weight = self.weights[f"model.layers.{layer_id}.mlp.gate.weight"]
        router_logits = F.linear(x_flat.float(), gate_weight.float())
        routing_weights = F.softmax(router_logits, dim=-1, dtype=torch.float)
        
        # Add bias for selection only
        bias_key = f"model.layers.{layer_id}.mlp.moe_statics.e_score_correction_bias"
        routing_for_selection = routing_weights
        if bias_key in self.weights:
            routing_for_selection = routing_weights + self.weights[bias_key]

        K = min(self.moe_k, routing_weights.shape[-1])
        _, top_k_index = torch.topk(routing_for_selection, K, dim=-1)
        top_k_weights = torch.gather(routing_weights, dim=-1, index=top_k_index)
        top_k_weights = top_k_weights / torch.clamp(top_k_weights.sum(dim=-1, keepdim=True), min=self.moe_norm_min)
        top_k_weights = top_k_weights.to(x_flat.dtype)

        # 全专家可用时走 grouped-mm，对齐 Transformers 默认 experts backend；
        # 专家不全时返回 None，保留后面的 per-expert fallback 方便局部诊断。
        routed_output = self._moe_experts_grouped(x_flat, layer_id, top_k_index, top_k_weights)
        if routed_output is not None:
            if self.debug and layer_id == 1:
                print(f"[DBG] L{layer_id} tok 0: selected={top_k_index[0].tolist()}, weights={[round(w.item(), 4) for w in top_k_weights[0]]}")
            return (routed_output + shared_output).view(bsz, seq_len, self.hidden_size)

        routed_output = torch.zeros_like(x_flat)
        with torch.no_grad():
            expert_mask = F.one_hot(top_k_index, num_classes=routing_weights.shape[-1])
            expert_mask = expert_mask.permute(2, 1, 0)
            expert_hit = torch.greater(expert_mask.sum(dim=(-1, -2)), 0).nonzero()

        for expert_idx in expert_hit:
            expert_id = int(expert_idx[0])
            if (layer_id, expert_id) not in self.expert_weights:
                continue
            top_k_pos, token_idx = torch.where(expert_mask[expert_id])
            current_state = x_flat[token_idx]
            current_hidden_states = self._moe_expert_packed(current_state, layer_id, expert_id)
            if current_hidden_states is None:
                continue
            current_hidden_states = current_hidden_states * top_k_weights[token_idx, top_k_pos, None]
            routed_output.index_add_(0, token_idx, current_hidden_states.to(routed_output.dtype))

            if self.debug and layer_id == 1 and 0 in token_idx.tolist():
                first_pos = (token_idx == 0).nonzero()[0, 0]
                print(f"[DBG]   expert {expert_id}: sum={current_hidden_states.sum().item():.6f}, first5={current_hidden_states[first_pos, :5].tolist()}")

        if self.debug and layer_id == 1:
            print(f"[DBG] L{layer_id} tok 0: selected={top_k_index[0].tolist()}, weights={[round(w.item(), 4) for w in top_k_weights[0]]}")

        return (routed_output + shared_output).view(bsz, seq_len, self.hidden_size)

    def _transformer_layer(self, x: torch.Tensor, layer_id: int) -> torch.Tensor:
        x_norm = self._rms_norm(x, self.weights[f"model.layers.{layer_id}.input_layernorm.weight"])
        attn_out = self._attention(x_norm, layer_id)
        x = x + attn_out
        x_norm = self._rms_norm(x, self.weights[f"model.layers.{layer_id}.post_attention_layernorm.weight"])
        if layer_id == 0:
            x = x + self._standard_mlp(x_norm, layer_id)
        else:
            moe_out = self._moe_layer(x_norm, layer_id)
            x = x + moe_out
        if self.debug and layer_id < 3:
            print(f"[DBG] layer {layer_id} out: sum={x.sum().item():.6f}, first5={x[0, 0, :5].tolist()}")
        return x

    def forward(self, input_ids: torch.Tensor) -> torch.Tensor:
        x = F.embedding(input_ids, self.weights["model.embed_tokens.weight"])
        for layer_id in range(self.num_hidden_layers):
            x = self._transformer_layer(x, layer_id)
        x = self._rms_norm(x, self.weights["model.norm.weight"])
        return F.linear(x, self.weights["model.embed_tokens.weight"])

    @torch.no_grad()
    def generate(self, input_ids: torch.Tensor, max_new_tokens: int = 50, eos_token_id: int = 2) -> list[int]:
        generated = input_ids[0].tolist()
        for _ in range(max_new_tokens):
            logits = self.forward(input_ids)
            next_token = logits[0, -1].argmax().item()
            generated.append(next_token)
            if next_token == eos_token_id:
                break
            input_ids = torch.cat([input_ids, torch.tensor([[next_token]], dtype=input_ids.dtype)], dim=1)
        return generated

    def chat(self, prompt: str, tokenizer, max_new_tokens: int = 50) -> str:
        tokens = tokenizer(prompt, return_tensors="pt")
        input_ids = tokens["input_ids"]
        generated_ids = self.generate(input_ids, max_new_tokens=max_new_tokens, eos_token_id=tokenizer.eos_token_id)
        return tokenizer.decode(generated_ids, skip_special_tokens=True)


def main():
    import argparse

    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", "-m", type=str,
                        default="output/splits/ERNIE-4.5-21B-A3B-PT-k6/manifest.json")
    parser.add_argument("--prompt", "-p", type=str, default="What a nice day!")
    parser.add_argument("--max-tokens", "-t", type=int, default=20)
    args = parser.parse_args()

    model = MiniMoE(args.manifest, expert_ids=list(range(6)))
    model.load_all()

    tokenizer = AutoTokenizer.from_pretrained(
        "models/PaddlePaddle/ERNIE-4.5-21B-A3B-PT",
        trust_remote_code=True,
    )

    print(f"Prompt: {args.prompt}")
    result = model.chat(args.prompt, tokenizer, max_new_tokens=args.max_tokens)
    print(f"Result: {result}")


if __name__ == "__main__":
    main()
