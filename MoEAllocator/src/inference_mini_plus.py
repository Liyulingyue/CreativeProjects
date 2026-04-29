#!/usr/bin/env python3
import json
import sys
from pathlib import Path
from typing import Dict

import safetensors.torch as storch
import torch
import torch.nn.functional as F
from transformers import AutoTokenizer


class MiniMoE:
    def __init__(self, manifest_path: str | Path, expert_ids: list[int] | None = None):
        with open(manifest_path) as f:
            data = json.load(f)

        self.cfg = data["config"]
        self.output_dir = Path(data["output_dir"])

        self.hidden_size = self.cfg["hidden_size"]
        self.num_attention_heads = self.cfg.get("num_attention_heads", 20)
        self.num_kv_heads = self.cfg.get("num_key_value_heads", 4)
        self.head_dim = self.hidden_size // self.num_attention_heads
        self.intermediate_size = self.cfg.get("intermediate_size", 12288)
        self.moe_intermediate = self.cfg["moe_intermediate_size"]
        self.num_hidden_layers = self.cfg["num_layers"]
        self.moe_k = self.cfg["moe_k"]
        self.num_shared_experts = self.cfg["num_shared_experts"]
        self.num_experts = self.cfg["num_experts"]
        self.rope_theta = self.cfg.get("rope_theta", 500000.0)
        self.rms_norm_eps = self.cfg.get("rms_norm_eps", 1e-5)
        self.vocab_size = self.cfg.get("vocab_size", None)

        if expert_ids is None:
            expert_ids = list(range(6))
        self.expert_ids = expert_ids
        self.num_experts = len(self.expert_ids)

        self.dtype = torch.float32
        self.device = torch.device("cpu")

        self.weights: dict[str, torch.Tensor] = {}
        self.expert_weights: dict[tuple[int, int], dict[str, torch.Tensor]] = {}

    def _to_tensor(self, tensor: torch.Tensor) -> torch.Tensor:
        return tensor.to(self.dtype).to(self.device)

    def _load_weights_file(self, file_path: Path, selector: str | None = None) -> None:
        if not file_path.exists():
            return
        loaded = storch.load_file(str(file_path))
        for k, v in loaded.items():
            if selector is None or selector in k:
                self.weights[k] = self._to_tensor(v)

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
                self.expert_weights[(layer_id, eid)] = {k: self._to_tensor(v) for k, v in weights.items()}
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
        x32 = x.to(torch.float32)
        variance = x32.pow(2).mean(-1, keepdim=True)
        x_norm = x32 * torch.rsqrt(variance + self.rms_norm_eps)
        return (x_norm * weight.to(torch.float32)).to(x.dtype)

    def _rope_compute(self, seq_len: int, device: torch.device) -> torch.Tensor:
        inv_freq = 1.0 / (self.rope_theta ** (torch.arange(0, self.head_dim, 2, dtype=torch.float32, device=device) / self.head_dim))
        t = torch.arange(seq_len, dtype=torch.float32, device=device)
        freqs = torch.outer(t, inv_freq)
        return freqs

    def _apply_rope(self, q: torch.Tensor, k: torch.Tensor, cos: torch.Tensor, sin: torch.Tensor):
        q1, q2 = q[..., : q.shape[-1] // 2], q[..., q.shape[-1] // 2 :]
        k1, k2 = k[..., : k.shape[-1] // 2], k[..., k.shape[-1] // 2 :]
        q_rot = torch.cat([q1 * cos - q2 * sin, q1 * sin + q2 * cos], dim=-1)
        k_rot = torch.cat([k1 * cos - k2 * sin, k1 * sin + k2 * cos], dim=-1)
        return q_rot, k_rot

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

        freqs = self._rope_compute(seq_len, x.device)
        cos = freqs.cos().view(1, seq_len, 1, self.head_dim // 2)
        sin = freqs.sin().view(1, seq_len, 1, self.head_dim // 2)

        q, k = self._apply_rope(q, k, cos, sin)

        if self.num_attention_heads != self.num_kv_heads:
            n_rep = self.num_attention_heads // self.num_kv_heads
            k = k.transpose(1, 2).repeat_interleave(n_rep, dim=1)
            v = v.transpose(1, 2).repeat_interleave(n_rep, dim=1)
        else:
            k = k.transpose(1, 2)
            v = v.transpose(1, 2)

        q = q.transpose(1, 2)
        scaling = self.head_dim ** -0.5
        attn_scores = torch.matmul(q, k.transpose(-2, -1)) * scaling

        if seq_len > 1:
            causal_mask = torch.triu(torch.ones(seq_len, seq_len, device=x.device, dtype=torch.bool), diagonal=1)
            attn_scores = attn_scores.masked_fill(causal_mask, float("-inf"))

        attn_probs = F.softmax(attn_scores, dim=-1)
        attn_output = torch.matmul(attn_probs, v)
        attn_output = attn_output.transpose(1, 2).contiguous()
        return F.linear(attn_output.view(bsz, seq_len, self.hidden_size), self.weights[f"{prefix}.o_proj.weight"])

    def _ffn(self, x: torch.Tensor, down: torch.Tensor, up: torch.Tensor, gate: torch.Tensor) -> torch.Tensor:
        return F.linear(F.silu(F.linear(x, gate)) * F.linear(x, up), down)

    def _gate_forward(self, x: torch.Tensor, layer_id: int) -> torch.Tensor:
        gate_weight = self.weights[f"model.layers.{layer_id}.mlp.gate.weight"]
        gate_weight = gate_weight[: self.num_experts]
        return F.linear(x.float(), gate_weight.float())

    def _moe_expert_local(self, x: torch.Tensor, layer_id: int, expert_id: int) -> torch.Tensor | None:
        key = (layer_id, expert_id)
        if key not in self.expert_weights:
            return None
        weights = self.expert_weights[key]
        down = self._get_weight(weights, "down_proj.weight")
        up = self._get_weight(weights, "up_proj.weight")
        gate = self._get_weight(weights, "gate_proj.weight")
        return self._ffn(x, down, up, gate)

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
        moe_out = torch.zeros_like(x_norm)
        for _ in range(self.num_shared_experts):
            moe_out = moe_out + self._shared_expert(x_norm, layer_id)

        gate_logits = self._gate_forward(x_norm, layer_id)
        bsz, seq_len, num_experts = gate_logits.shape

        gate_flat = gate_logits.view(-1, num_experts)
        routing_probs = F.softmax(gate_flat, dim=-1)
        topk_probs, topk_indices = torch.topk(routing_probs, k=min(self.moe_k, num_experts), dim=-1)
        norm = topk_probs.sum(dim=-1, keepdim=True).clamp(min=1e-9)
        weights = topk_probs / norm

        for tok_idx in range(bsz * seq_len):
            token_weights = weights[tok_idx]
            token_indices = topk_indices[tok_idx]
            batch_idx = tok_idx // seq_len
            seq_idx = tok_idx % seq_len
            x_tok = x_norm[batch_idx : batch_idx + 1, seq_idx : seq_idx + 1, :]
            for expert_rank, eid in enumerate(token_indices.tolist()):
                e_out = self._moe_expert_local(x_tok, layer_id, eid)
                if e_out is None:
                    continue
                moe_out[batch_idx : batch_idx + 1, seq_idx : seq_idx + 1, :] += e_out * token_weights[expert_rank]

        return moe_out

    def _transformer_layer(self, x: torch.Tensor, layer_id: int) -> torch.Tensor:
        x_norm = self._rms_norm(x, self.weights[f"model.layers.{layer_id}.input_layernorm.weight"])
        x = x + self._attention(x_norm, layer_id)
        x_norm = self._rms_norm(x, self.weights[f"model.layers.{layer_id}.post_attention_layernorm.weight"])
        if layer_id == 0:
            x = x + self._standard_mlp(x_norm, layer_id)
        else:
            x = x + self._moe_layer(x_norm, layer_id)
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
    parser.add_argument("--prompt", "-p", type=str, default="今天天气真好，")
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
