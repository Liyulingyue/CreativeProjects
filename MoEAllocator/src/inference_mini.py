#!/usr/bin/env python3
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent.parent))

import torch
import safetensors.torch as storch
from transformers import AutoTokenizer


class MiniMoE:
    def __init__(self, manifest_path: str | Path):
        import json
        with open(manifest_path) as f:
            data = json.load(f)

        self.hidden_size = data["config"]["hidden_size"]
        self.moe_intermediate = data["config"]["moe_intermediate_size"]
        self.output_dir = Path(data["output_dir"])

        self.embed_tokens = None
        self.norm = None
        self.gate = None
        self.shared_expert = None
        self.experts: dict[int, dict] = {}

    def load_backbone(self):
        backbone_path = self.output_dir / "backbone" / "backbone.safetensors"
        weights = storch.load_file(str(backbone_path))

        self.embed_tokens = weights.get("model.embed_tokens.weight")
        self.norm = weights.get("model.norm.weight")
        print(f"  backbone loaded: embed={self.embed_tokens.shape}, norm={self.norm.shape}")

    def load_gate(self):
        gate_path = self.output_dir / "backbone" / "gate.safetensors"
        weights = storch.load_file(str(gate_path))
        gate_weights = {k: v for k, v in weights.items() if ".mlp.gate." in k}
        self.gate = gate_weights
        print(f"  gate loaded: {len(gate_weights)} tensors")

    def load_shared_expert(self):
        shared_path = self.output_dir / "backbone" / "shared_expert.safetensors"
        weights = storch.load_file(str(shared_path))
        shared = {k: v for k, v in weights.items() if "shared_experts" in k}
        self.shared_expert = shared
        print(f"  shared_expert loaded: {len(shared)} tensors")

    def load_experts(self, expert_ids: list[int]):
        for eid in expert_ids:
            for layer_id in range(1, 28):
                file_name = f"expert_{eid:03d}_layer_{layer_id:03d}.safetensors"
                file_path = self.output_dir / "experts" / file_name
                if file_path.exists():
                    weights = storch.load_file(str(file_path))
                    self.experts[(layer_id, eid)] = weights

        print(f"  experts loaded: {len(self.experts)} files "
              f"({len(expert_ids)} experts × {len(self.experts) // max(len(expert_ids), 1)} layers)")

    def silu(self, x):
        return x * torch.sigmoid(x)

    def _get_weight(self, weights: dict, suffix: str) -> torch.Tensor:
        for k, v in weights.items():
            if k.endswith(suffix):
                return v.to(torch.float32)
        raise KeyError(f"No weight ending with {suffix}")

    def expert_forward(self, hidden, expert_weights):
        up = torch.nn.functional.linear(hidden, self._get_weight(expert_weights, "up_proj.weight"))
        gate = torch.nn.functional.linear(hidden, self._get_weight(expert_weights, "gate_proj.weight"))
        gate = self.silu(gate)
        down = torch.nn.functional.linear(up * gate, self._get_weight(expert_weights, "down_proj.weight"))
        return down

    def moe_layer_forward(self, hidden_states, layer_id, expert_ids: list[int]):
        routed = {}
        for eid in expert_ids:
            key = (layer_id, eid)
            if key in self.experts:
                routed[eid] = self.expert_forward(hidden_states, self.experts[key])

        return routed

    def load_all(self, expert_ids: list[int] | None = None):
        if expert_ids is None:
            expert_ids = list(range(6))

        print("Loading mini model from split files...")
        self.load_backbone()
        self.load_gate()
        self.load_shared_expert()
        self.load_experts(expert_ids)
        print("Done.\n")


def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", "-m", type=str,
        default="output/splits/ERNIE-4.5-21B-A3B-PT-k6/manifest.json")
    parser.add_argument("--experts", "-e", type=str, default="0,1,2,3,4,5",
        help="Comma-separated expert IDs to load (default: 0-5)")
    parser.add_argument("--prompt", "-p", type=str,
        default="今天天气真好，")
    args = parser.parse_args()

    expert_ids = [int(x) for x in args.experts.split(",")]
    m = MiniMoE(args.manifest)
    m.load_all(expert_ids)

    tokenizer = AutoTokenizer.from_pretrained(
        "models/PaddlePaddle/ERNIE-4.5-21B-A3B-PT",
        trust_remote_code=True,
    )

    print(f"Prompt: {args.prompt}")
    tokens = tokenizer(args.prompt, return_tensors="pt")
    input_ids = tokens["input_ids"]
    print(f"Input tokens: {input_ids.tolist()}")
    print(f"Token count: {input_ids.shape[1]}")

    embed = m.embed_tokens
    hidden = embed[input_ids].to(torch.float32)
    print(f"Embedding shape: {hidden.shape}, dtype: {hidden.dtype}")

    layer_id = 1
    routed = m.moe_layer_forward(hidden, layer_id, expert_ids)
    print(f"\nMoE Layer {layer_id} results:")
    for eid, out in sorted(routed.items()):
        out_f = out.float()
        print(f"  Expert {eid}: shape={out.shape}, mean={out_f.mean().item():.6f}, "
              f"std={out_f.std().item():.6f}, min={out_f.min().item():.4f}, max={out_f.max().item():.4f}")

    print(f"\nMini inference complete!")
    print(f"Loaded experts: {expert_ids}")
    print(f"Note: This is a proof-of-concept. Full inference requires")
    print(f"      all 64 experts per layer + all 27 MoE layers.")


if __name__ == "__main__":
    main()
