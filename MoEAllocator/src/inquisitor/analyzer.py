import json
import re
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from safetensors import safe_open

from .architecture import (
    ArchitectureSpec,
    WeightCategory,
    WeightEntry,
    get_architecture_spec,
)


@dataclass
class AnalysisReport:
    model_path: Path
    model_type: str
    architecture_spec: ArchitectureSpec
    total_size_bytes: int
    weights: list[WeightEntry] = field(default_factory=list)
    expert_weights: list[WeightEntry] = field(default_factory=list)
    shared_expert_weights: list[WeightEntry] = field(default_factory=list)
    gate_weights: list[WeightEntry] = field(default_factory=list)
    backbone_weights: list[WeightEntry] = field(default_factory=list)
    total_experts: int = 0
    expert_size_bytes: int = 0
    shared_expert_size_bytes: int = 0
    gate_size_bytes: int = 0
    backbone_size_bytes: int = 0
    num_experts: int = 0
    moe_k: int = 0
    num_shared_experts: int = 0
    moe_intermediate_size: int = 0
    hidden_size: int = 0
    moe_layer_indices: list[int] = field(default_factory=list)
    per_layer_expert_size: dict[int, int] = field(default_factory=dict)
    per_expert_size: int = 0

    def to_dict(self) -> dict[str, Any]:
        def fmt_size(b: int) -> str:
            if b >= 1 << 30:
                return f"{b / (1 << 30):.2f} GB"
            elif b >= 1 << 20:
                return f"{b / (1 << 20):.2f} MB"
            elif b >= 1 << 10:
                return f"{b / (1 << 10):.2f} KB"
            return f"{b} B"

        return {
            "model_path": str(self.model_path),
            "model_type": self.model_type,
            "architecture": self.architecture_spec.name,
            "total_size": fmt_size(self.total_size_bytes),
            "total_size_bytes": self.total_size_bytes,
            "total_experts": self.total_experts,
            "expert_layers": len(set(w.layer_idx for w in self.expert_weights if w.layer_idx is not None)),
            "num_layers": len(set(w.layer_idx for w in self.weights if w.layer_idx is not None)),
            "moe": {
                "num_experts": self.num_experts,
                "moe_k": self.moe_k,
                "num_shared_experts": self.num_shared_experts,
                "moe_intermediate_size": self.moe_intermediate_size,
                "hidden_size": self.hidden_size,
                "moe_layer_indices": self.moe_layer_indices,
                "per_expert_size": fmt_size(self.per_expert_size),
                "per_expert_size_bytes": self.per_expert_size,
                "per_layer_expert_size": {
                    f"layer_{k}": fmt_size(v) for k, v in sorted(self.per_layer_expert_size.items())
                },
            },
            "categories": {
                "backbone": {
                    "size": fmt_size(self.backbone_size_bytes),
                    "size_bytes": self.backbone_size_bytes,
                    "count": len(self.backbone_weights),
                    "ratio": f"{self.backbone_size_bytes / self.total_size_bytes * 100:.1f}%",
                },
                "gate": {
                    "size": fmt_size(self.gate_size_bytes),
                    "size_bytes": self.gate_size_bytes,
                    "count": len(self.gate_weights),
                    "ratio": f"{self.gate_size_bytes / self.total_size_bytes * 100:.1f}%",
                },
                "expert": {
                    "size": fmt_size(self.expert_size_bytes),
                    "size_bytes": self.expert_size_bytes,
                    "count": len(self.expert_weights),
                    "expert_count": self.total_experts,
                    "ratio": f"{self.expert_size_bytes / self.total_size_bytes * 100:.1f}%",
                },
                "shared_expert": {
                    "size": fmt_size(self.shared_expert_size_bytes),
                    "size_bytes": self.shared_expert_size_bytes,
                    "count": len(self.shared_expert_weights),
                    "ratio": f"{self.shared_expert_size_bytes / self.total_size_bytes * 100:.1f}%",
                },
            },
        }


class WeightAnalyzer:
    _DTYPE_SIZES = {
        "F32": 4,
        "F16": 2,
        "BF16": 2,
        "I8": 1,
        "I32": 4,
        "I64": 8,
        "U8": 1,
        "BOOL": 1,
    }

    def __init__(self, model_path: str | Path):
        self.model_path = Path(model_path)
        self._config: dict[str, Any] = {}
        self._spec: ArchitectureSpec | None = None

    def load_config(self) -> dict[str, Any]:
        config_path = self.model_path / "config.json"
        if not config_path.exists():
            raise FileNotFoundError(f"config.json not found at {config_path}")
        with open(config_path) as f:
            self._config = json.load(f)
        self._spec = get_architecture_spec(self._config.get("model_type", ""))
        return self._config

    def parse_weight_index(self) -> dict[str, str]:
        index_path = self.model_path / "model.safetensors.index.json"
        if not index_path.exists():
            index_path = self.model_path / "model.safetensors.index.json"
        with open(index_path) as f:
            data = json.load(f)
        return data.get("weight_map", {})

    def _classify_weight(self, weight_name: str) -> WeightCategory:
        spec = self._spec
        if spec is None:
            raise RuntimeError("Architecture spec not loaded. Call load_config() first.")

        for pattern in spec.gate_patterns:
            if pattern.search(weight_name):
                return WeightCategory.GATE
        for pattern in spec.shared_expert_patterns:
            if pattern.search(weight_name):
                return WeightCategory.SHARED_EXPERT
        for pattern in spec.expert_name_patterns.get(WeightCategory.EXPERT, []):
            if pattern.search(weight_name):
                return WeightCategory.EXPERT
        for pattern in spec.backbone_patterns:
            if pattern.search(weight_name):
                return WeightCategory.BACKBONE
        return WeightCategory.BACKBONE

    def _extract_layer_idx(self, weight_name: str) -> int | None:
        match = re.search(r"model\.layers\.(\d+)\.", weight_name)
        if match:
            return int(match.group(1))
        return None

    def _estimate_size(self, weight_name: str, shard_file: str, config: dict[str, Any]) -> int:
        shape = self._infer_shape(weight_name, config)
        dtype = "BF16"
        size_per_elem = self._DTYPE_SIZES.get(dtype, 2)
        return int(size_per_elem * (shape[0] * shape[1] if len(shape) == 2 else shape[0]))

    def _infer_shape(self, weight_name: str, config: dict[str, Any]) -> tuple[int, ...]:
        hidden_size = config.get("hidden_size", 2560)
        vocab_size = config.get("vocab_size", 103424)
        intermediate_size = config.get("intermediate_size", 12288)
        moe_intermediate = config.get("moe_intermediate_size", 1536)

        if "embed_tokens" in weight_name:
            return (vocab_size, hidden_size)
        q_heads = config.get("num_attention_heads", 20)
        kv_heads = config.get("num_key_value_heads", 4)
        if ".q_proj." in weight_name:
            return (hidden_size, hidden_size)
        if ".k_proj." in weight_name:
            return (hidden_size, hidden_size // q_heads * kv_heads)
        if ".v_proj." in weight_name:
            return (hidden_size, hidden_size // q_heads * kv_heads)
        if ".o_proj." in weight_name:
            return (hidden_size, hidden_size)
        if "gate_proj" in weight_name or "up_proj" in weight_name:
            if "experts" in weight_name or "shared_experts" in weight_name:
                return (moe_intermediate, hidden_size)
            return (intermediate_size, hidden_size)
        if "down_proj" in weight_name:
            if "experts" in weight_name or "shared_experts" in weight_name:
                return (hidden_size, moe_intermediate)
            return (hidden_size, intermediate_size)
        if "gate.weight" in weight_name:
            num_experts = config.get("moe_num_experts", 64)
            return (num_experts, hidden_size)
        if ".norm." in weight_name or "layernorm" in weight_name:
            return (hidden_size,)
        return (hidden_size, hidden_size)

    def analyze(self) -> AnalysisReport:
        config = self.load_config()
        weight_map = self.parse_weight_index()

        spec = self._spec
        if spec is None:
            raise RuntimeError("Architecture spec not loaded.")

        weights: list[WeightEntry] = []
        expert_nums: set[int] = set()

        for weight_name, shard_file in weight_map.items():
            category = self._classify_weight(weight_name)
            layer_idx = self._extract_layer_idx(weight_name)
            size = self._estimate_size(weight_name, shard_file, config)

            entry = WeightEntry(
                name=weight_name,
                category=category,
                layer_idx=layer_idx,
                shard_file=shard_file,
                shape=self._infer_shape(weight_name, config),
                size_bytes=size,
            )
            weights.append(entry)

            if category == WeightCategory.EXPERT:
                match = re.search(r"experts\.(\d+)\.", weight_name)
                if match:
                    expert_nums.add(int(match.group(1)))

        expert_weights = [w for w in weights if w.category == WeightCategory.EXPERT]
        shared_expert_weights = [w for w in weights if w.category == WeightCategory.SHARED_EXPERT]
        gate_weights = [w for w in weights if w.category == WeightCategory.GATE]
        backbone_weights = [w for w in weights if w.category == WeightCategory.BACKBONE]

        moe_layer_indices = sorted(set(w.layer_idx for w in expert_weights if w.layer_idx is not None))
        per_layer_expert_size: dict[int, int] = {}
        for w in expert_weights:
            if w.layer_idx is not None:
                per_layer_expert_size[w.layer_idx] = per_layer_expert_size.get(w.layer_idx, 0) + w.size_bytes

        num_experts = config.get("moe_num_experts", 64)
        num_shared_experts = config.get("moe_num_shared_experts", 0)
        moe_k = config.get("moe_k", 0)
        moe_intermediate = config.get("moe_intermediate_size", 0)
        hidden_size = config.get("hidden_size", 0)

        single_expert_weights = [w for w in expert_weights if re.search(r"experts\.(\d+)\.", w.name) and re.search(r"experts\.0\.", w.name)]
        per_expert_size = sum(w.size_bytes for w in single_expert_weights) if single_expert_weights else 0

        return AnalysisReport(
            model_path=self.model_path,
            model_type=config.get("model_type", "unknown"),
            architecture_spec=spec,
            total_size_bytes=sum(w.size_bytes for w in weights),
            weights=weights,
            expert_weights=expert_weights,
            shared_expert_weights=shared_expert_weights,
            gate_weights=gate_weights,
            backbone_weights=backbone_weights,
            total_experts=len(expert_nums),
            expert_size_bytes=sum(w.size_bytes for w in expert_weights),
            shared_expert_size_bytes=sum(w.size_bytes for w in shared_expert_weights),
            gate_size_bytes=sum(w.size_bytes for w in gate_weights),
            backbone_size_bytes=sum(w.size_bytes for w in backbone_weights),
            num_experts=num_experts,
            moe_k=moe_k,
            num_shared_experts=num_shared_experts,
            moe_intermediate_size=moe_intermediate,
            hidden_size=hidden_size,
            moe_layer_indices=moe_layer_indices,
            per_layer_expert_size=per_layer_expert_size,
            per_expert_size=per_expert_size,
        )
