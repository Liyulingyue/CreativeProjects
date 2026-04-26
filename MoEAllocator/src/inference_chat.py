#!/usr/bin/env python3
import json
import math
import sys
from pathlib import Path
from typing import Optional

import safetensors.torch as storch
import torch
import torch.nn.functional as F
from tqdm import tqdm
from transformers import AutoTokenizer


def rotate_half(x):
    x1, x2 = x[..., : x.shape[-1] // 2], x[..., x.shape[-1] // 2 :]
    return torch.cat([-x2, x1], dim=-1)


class MiniErnieModel:
    def __init__(self, manifest_path: str | Path, fixed_experts: list[int] | None = None):
        self.device = torch.device("cpu")
        self.dtype = torch.float32

        with open(manifest_path) as f:
            data = json.load(f)
        self.cfg = data["config"]
        self.output_dir = Path(data["output_dir"])

        self.vocab_size = self.cfg.get("vocab_size", 103424)
        self.hidden_size = self.cfg["hidden_size"]
        self.num_attention_heads = self.cfg.get("num_attention_heads", 20)
        self.num_kv_heads = self.cfg.get("num_key_value_heads", 4)
        self.head_dim = self.hidden_size // self.num_attention_heads
        self.intermediate_size = self.cfg.get("intermediate_size", 12288)
        self.moe_intermediate = self.cfg["moe_intermediate_size"]
        self.num_hidden_layers = self.cfg["num_layers"]
        self.moe_k = self.cfg["moe_k"]
        self.num_shared_experts = self.cfg["num_shared_experts"]
        self.rope_theta = self.cfg.get("rope_theta", 500000.0)
        self.rms_norm_eps = self.cfg.get("rms_norm_eps", 1e-5)

        if fixed_experts is None:
            fixed_experts = list(range(self.moe_k))
        self.fixed_experts = fixed_experts

        self.weights: dict[str, torch.Tensor] = {}
        self.expert_weights: dict[tuple[int, int], dict[str, torch.Tensor]] = {}

    def _load_backbone(self):
        print("Loading backbone weights...")
        backbone_path = self.output_dir / "backbone" / "backbone.safetensors"
        weights = storch.load_file(str(backbone_path))
        for k, v in weights.items():
            self.weights[k] = v.to(self.dtype)
        print(f"  Loaded {len(weights)} backbone tensors")

        shared_path = self.output_dir / "backbone" / "shared_expert.safetensors"
        if shared_path.exists():
            shared = storch.load_file(str(shared_path))
            for k, v in shared.items():
                self.weights[k] = v.to(self.dtype)
            print(f"  Loaded {len(shared)} shared expert tensors")

    def _load_experts(self, expert_ids: list[int]):
        print(f"Loading experts {expert_ids} for all {self.num_hidden_layers} layers...")
        loaded = 0
        for eid in tqdm(expert_ids, desc="Loading experts"):
            for layer_id in range(1, self.num_hidden_layers + 1):
                file_name = f"expert_{eid:03d}_layer_{layer_id:03d}.safetensors"
                file_path = self.output_dir / "experts" / file_name
                if file_path.exists():
                    w = storch.load_file(str(file_path))
                    self.expert_weights[(layer_id, eid)] = {k: v.to(self.dtype) for k, v in w.items()}
                    loaded += 1
        print(f"  Loaded {loaded} expert files")

    def load(self, expert_ids: list[int] | None = None):
        if expert_ids is None:
            expert_ids = self.fixed_experts
        self._load_backbone()
        self._load_experts(expert_ids)

    def _rms_norm(self, x, weight):
        variance = x.pow(2).mean(-1, keepdim=True)
        x = x * torch.rsqrt(variance + self.rms_norm_eps)
        return x * weight

    def _attention(self, x, layer_id: int):
        layer_prefix = f"model.layers.{layer_id}.self_attn"

        q = F.linear(x, self.weights[f"{layer_prefix}.q_proj.weight"])
        k = F.linear(x, self.weights[f"{layer_prefix}.k_proj.weight"])
        v = F.linear(x, self.weights[f"{layer_prefix}.v_proj.weight"])

        bsz, seq_len, _ = x.shape
        q = q.view(bsz, seq_len, self.num_attention_heads, self.head_dim)
        k = k.view(bsz, seq_len, self.num_kv_heads, self.head_dim)
        v = v.view(bsz, seq_len, self.num_kv_heads, self.head_dim)

        freqs = self._rope_compute(seq_len, x.device)
        cos = freqs.cos().view(seq_len, 1, self.head_dim)
        sin = freqs.sin().view(seq_len, 1, self.head_dim)
        cos_half = cos[..., :self.head_dim // 2]
        sin_half = sin[..., :self.head_dim // 2]

        q0, q1 = q[..., :self.head_dim // 2], q[..., self.head_dim // 2:]
        k0, k1 = k[..., :self.head_dim // 2], k[..., self.head_dim // 2:]
        q_rope = torch.cat([q0 * cos_half - q1 * sin_half, q0 * sin_half + q1 * cos_half], dim=-1)
        k_rope = torch.cat([k0 * cos_half - k1 * sin_half, k0 * sin_half + k1 * cos_half], dim=-1)

        if self.num_attention_heads != self.num_kv_heads:
            n_rep = self.num_attention_heads // self.num_kv_heads
            k_rope = k_rope.transpose(1, 2).repeat_interleave(n_rep, dim=1)
            v = v.transpose(1, 2).repeat_interleave(n_rep, dim=1)
        else:
            k_rope = k_rope.transpose(1, 2)
            v = v.transpose(1, 2)
        q_rope = q_rope.transpose(1, 2)

        attn_weights = torch.matmul(q_rope, k_rope.transpose(-2, -1)) / math.sqrt(self.head_dim)
        attn_weights = F.softmax(attn_weights, dim=-1)
        attn_output = torch.matmul(attn_weights, v)
        attn_output = attn_output.transpose(1, 2).contiguous().view(bsz, seq_len, self.hidden_size)
        output = F.linear(attn_output, self.weights[f"{layer_prefix}.o_proj.weight"])
        return output

    def _rope_compute(self, seq_len, device):
        dim = self.head_dim
        theta = self.rope_theta
        freqs = 1.0 / (theta ** (torch.arange(0, dim, 2, dtype=torch.float32, device=device) / dim))
        t = torch.arange(seq_len, dtype=torch.float32, device=device)
        freqs = torch.outer(t, freqs)
        freqs = torch.cat([freqs, freqs], dim=-1)
        return freqs

    def _ffn(self, x, down_weight, up_weight, gate_weight):
        down = F.linear(
            torch.nn.functional.silu(F.linear(x, gate_weight)) * F.linear(x, up_weight),
            down_weight
        )
        return down

    def _moe_expert(self, x, layer_id: int, expert_id: int):
        key = (layer_id, expert_id)
        if key not in self.expert_weights:
            return None
        w = self.expert_weights[key]
        down = None
        up = None
        gate = None
        for k, v in w.items():
            if k.endswith("down_proj.weight"):
                down = v
            elif k.endswith("up_proj.weight"):
                up = v
            elif k.endswith("gate_proj.weight"):
                gate = v
        if down is None or up is None or gate is None:
            return None
        return self._ffn(x, down, up, gate)

    def _shared_expert(self, x, layer_id: int):
        prefix = f"model.layers.{layer_id}.mlp.shared_experts"
        down = self.weights.get(f"{prefix}.down_proj.weight")
        up = self.weights.get(f"{prefix}.up_proj.weight")
        gate = self.weights.get(f"{prefix}.gate_proj.weight")
        if down is None or up is None or gate is None:
            return None
        return self._ffn(x, down, up, gate)

    def _moe_layer(self, x, layer_id: int):
        moe_out = torch.zeros_like(x)
        for se_id in range(self.num_shared_experts):
            se_out = self._shared_expert(x, layer_id)
            if se_out is not None:
                moe_out = moe_out + se_out

        for eid in self.fixed_experts:
            e_out = self._moe_expert(x, layer_id, eid)
            if e_out is not None:
                moe_out = moe_out + e_out

        return moe_out

    def _standard_mlp(self, x, layer_id: int):
        prefix = f"model.layers.{layer_id}.mlp"
        down = self.weights[f"{prefix}.down_proj.weight"]
        up = self.weights[f"{prefix}.up_proj.weight"]
        gate = self.weights[f"{prefix}.gate_proj.weight"]
        return self._ffn(x, down, up, gate)

    def _transformer_layer(self, x, layer_id: int):
        x_norm = self._rms_norm(x, self.weights[f"model.layers.{layer_id}.input_layernorm.weight"])
        attn_out = self._attention(x_norm, layer_id)
        x = x + attn_out

        x_norm = self._rms_norm(x, self.weights[f"model.layers.{layer_id}.post_attention_layernorm.weight"])
        if layer_id == 0:
            mlp_out = self._standard_mlp(x_norm, layer_id)
        else:
            mlp_out = self._moe_layer(x_norm, layer_id)
        x = x + mlp_out
        return x

    def forward(self, input_ids: torch.Tensor) -> torch.Tensor:
        bsz, seq_len = input_ids.shape
        x = F.embedding(input_ids, self.weights["model.embed_tokens.weight"])

        for layer_id in range(self.num_hidden_layers):
            x = self._transformer_layer(x, layer_id)

        x = self._rms_norm(x, self.weights["model.norm.weight"])
        logits = F.linear(x, self.weights["model.embed_tokens.weight"])
        return logits

    @torch.no_grad()
    def generate(self, input_ids: torch.Tensor, max_new_tokens: int = 100, eos_token_id: int = 2) -> list[int]:
        print(f"\nGenerating (max_new_tokens={max_new_tokens})...")
        generated = input_ids[0].tolist()

        for step in tqdm(range(max_new_tokens), desc="Generating"):
            logits = self.forward(input_ids)
            next_token_logits = logits[0, -1, :]
            next_token = next_token_logits.argmax(dim=-1).item()
            generated.append(next_token)

            if next_token == eos_token_id:
                break

            new_ids = torch.tensor([[next_token]], dtype=input_ids.dtype)
            input_ids = torch.cat([input_ids, new_ids], dim=1)

        return generated

    def chat(self, prompt: str, tokenizer, max_new_tokens: int = 100):
        print(f"\n{'='*60}")
        print(f"User: {prompt}")
        tokens = tokenizer(prompt, return_tensors="pt")
        input_ids = tokens["input_ids"]

        print(f"Input tokens: {input_ids.shape[1]}")
        generated = self.generate(input_ids, max_new_tokens=max_new_tokens, eos_token_id=tokenizer.eos_token_id)
        response = tokenizer.decode(generated, skip_special_tokens=True)
        print(f"Model: {response}")
        print(f"{'='*60}")
        return response


def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", "-m", type=str,
        default="output/splits/ERNIE-4.5-21B-A3B-PT-k6/manifest.json")
    parser.add_argument("--experts", "-e", type=str, default="0,1,2,3,4,5",
        help="Comma-separated expert IDs to load (default: 0-5)")
    parser.add_argument("--prompt", "-p", type=str, default="今天天气真好，")
    parser.add_argument("--max-tokens", "-t", type=int, default=50)
    args = parser.parse_args()

    expert_ids = [int(x) for x in args.experts.split(",")]
    print(f"Expert IDs: {expert_ids}")

    print("Initializing model...")
    model = MiniErnieModel(args.manifest, fixed_experts=expert_ids)
    print(f"  Config: hidden={model.hidden_size}, layers={model.num_hidden_layers}, "
          f"experts={model.cfg['num_experts']}, moe_k={model.moe_k}")

    print("Loading weights (this will take a while)...")
    model.load(expert_ids=expert_ids)
    print("All weights loaded!")

    print(f"\nMemory usage estimate:")
    embed_size = model.weights["model.embed_tokens.weight"].numel() * 4 / (1<<30)
    norm_size = sum(v.numel() * 4 for k, v in model.weights.items() if "norm" in k) / (1<<30)
    attn_size = sum(v.numel() * 4 for k, v in model.weights.items() if "self_attn" in k) / (1<<30)
    ffn_size = sum(v.numel() * 4 for k, v in model.weights.items() if "mlp" in k and "experts" not in k) / (1<<30)
    expert_loaded_size = sum(t.numel() * 4 for layer_dict in model.expert_weights.values() for t in layer_dict.values()) / (1<<30)
    print(f"  embed={embed_size:.2f}GB, norm={norm_size:.2f}GB, attn={attn_size:.2f}GB")
    print(f"  ffn/shared={ffn_size:.2f}GB, experts_loaded={expert_loaded_size:.2f}GB")
    print(f"  Total ~{embed_size+norm_size+attn_size+ffn_size+expert_loaded_size:.2f}GB")

    tokenizer = AutoTokenizer.from_pretrained(
        "models/PaddlePaddle/ERNIE-4.5-21B-A3B-PT",
        trust_remote_code=True,
    )

    model.chat(args.prompt, tokenizer, max_new_tokens=args.max_tokens)


if __name__ == "__main__":
    main()
