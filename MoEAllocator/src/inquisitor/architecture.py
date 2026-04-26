import re
from dataclasses import dataclass, field
from enum import Enum
from typing import Pattern


class WeightCategory(Enum):
    BACKBONE = "backbone"
    GATE = "gate"
    EXPERT = "expert"
    SHARED_EXPERT = "shared_expert"


@dataclass
class WeightEntry:
    name: str
    category: WeightCategory
    layer_idx: int | None
    shard_file: str
    dtype: str = "unknown"
    shape: tuple[int, ...] = ()
    size_bytes: int = 0

    @property
    def is_expert(self) -> bool:
        return self.category == WeightCategory.EXPERT

    @property
    def is_shared_expert(self) -> bool:
        return self.category == WeightCategory.SHARED_EXPERT

    @property
    def is_gate(self) -> bool:
        return self.category == WeightCategory.GATE


@dataclass
class ArchitectureSpec:
    name: str
    model_type: str
    weight_name_patterns: dict[WeightCategory, list[str]]
    expert_name_patterns: dict[WeightCategory, list[Pattern]]
    gate_patterns: list[Pattern]
    shared_expert_patterns: list[Pattern]
    backbone_patterns: list[Pattern]


_ERNIE45_PATTERNS = {
    WeightCategory.GATE: [
        r"\.mlp\.gate\.",
        r"\.mlp\.router\.",
        r"moe_statics",
    ],
    WeightCategory.EXPERT: [
        r"\.mlp\.experts\.(\d+)\.",
    ],
    WeightCategory.SHARED_EXPERT: [
        r"\.mlp\.shared_experts\.",
    ],
    WeightCategory.BACKBONE: [
        r"model\.layers\.\d+\.self_attn\.",
        r"model\.layers\.\d+\.(input_layernorm|post_attention_layernorm)\.",
        r"model\.layers\.\d+\.mlp\.(down_proj|up_proj|gate_proj)\.(?!experts|shared_experts|gate|router|moe_statics)",
        r"model\.embed_tokens\.",
        r"model\.norm\.",
        r"model\.mtp_",
    ],
}


_ARCHITECTURES: dict[str, ArchitectureSpec] = {}


def _build_spec(spec_name: str, model_type: str) -> ArchitectureSpec:
    patterns = _ERNIE45_PATTERNS
    gate_p = [p for p in patterns[WeightCategory.GATE]]
    expert_p = [p.replace("(\\d+)", "(\\d+)") for p in patterns[WeightCategory.EXPERT]]
    shared_p = [p for p in patterns[WeightCategory.SHARED_EXPERT]]
    backbone_p = [p for p in patterns[WeightCategory.BACKBONE]]

    return ArchitectureSpec(
        name=spec_name,
        model_type=model_type,
        weight_name_patterns=patterns,
        gate_patterns=[re.compile(p) for p in gate_p],
        expert_name_patterns={WeightCategory.EXPERT: [re.compile(p) for p in expert_p]},
        shared_expert_patterns=[re.compile(p) for p in shared_p],
        backbone_patterns=[re.compile(p) for p in backbone_p],
    )


_ARCHITECTURES["ernie4_5_moe"] = _build_spec("ERNIE-4.5 MoE", "ernie4_5_moe")
_ARCHITECTURES["mixtral"] = _build_spec("Mixtral", "mixtral")
_ARCHITECTURES["deepseek_v3"] = _build_spec("DeepSeek-V3", "deepseek_v3")
_ARCHITECTURES["minicpm_moe"] = _build_spec("MiniCPM-MoE", "minicpm_moe")


def get_architecture_spec(model_type: str) -> ArchitectureSpec:
    spec = _ARCHITECTURES.get(model_type)
    if spec is None:
        spec = _ARCHITECTURES["ernie4_5_moe"]
    return spec
