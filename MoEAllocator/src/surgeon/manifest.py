from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class ExpertFile:
    expert_id: int
    layer_id: int
    file_path: str
    size_bytes: int
    shard_sources: list[str] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "expert_id": self.expert_id,
            "layer_id": self.layer_id,
            "file_path": self.file_path,
            "size_bytes": self.size_bytes,
            "shard_sources": self.shard_sources,
        }


@dataclass
class BackboneFile:
    category: str
    file_path: str
    size_bytes: int
    weight_names: list[str] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "category": self.category,
            "file_path": self.file_path,
            "size_bytes": self.size_bytes,
            "weight_count": len(self.weight_names),
        }


@dataclass
class ModelManifest:
    model_name: str
    model_path: str
    architecture: str
    output_dir: Path
    num_experts: int
    num_layers: int
    moe_k: int
    num_shared_experts: int
    hidden_size: int
    moe_intermediate_size: int
    expert_files: list[ExpertFile] = field(default_factory=list)
    backbone_files: list[BackboneFile] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        def fmt_size(b: int) -> str:
            if b >= 1 << 30:
                return f"{b / (1 << 30):.2f} GB"
            elif b >= 1 << 20:
                return f"{b / (1 << 20):.2f} MB"
            return f"{b} B"

        total_expert_size = sum(e.size_bytes for e in self.expert_files)
        total_backbone_size = sum(b.size_bytes for b in self.backbone_files)

        return {
            "model_name": self.model_name,
            "model_path": self.model_path,
            "architecture": self.architecture,
            "output_dir": str(self.output_dir),
            "config": {
                "num_experts": self.num_experts,
                "num_layers": self.num_layers,
                "moe_k": self.moe_k,
                "num_shared_experts": self.num_shared_experts,
                "hidden_size": self.hidden_size,
                "moe_intermediate_size": self.moe_intermediate_size,
            },
            "statistics": {
                "total_expert_files": len(self.expert_files),
                "total_expert_size": fmt_size(total_expert_size),
                "total_expert_size_bytes": total_expert_size,
                "total_backbone_files": len(self.backbone_files),
                "total_backbone_size": fmt_size(total_backbone_size),
                "total_backbone_size_bytes": total_backbone_size,
            },
            "expert_files": [e.to_dict() for e in self.expert_files],
            "backbone_files": [b.to_dict() for b in self.backbone_files],
        }

    def get_expert_file(self, expert_id: int, layer_id: int) -> ExpertFile | None:
        for ef in self.expert_files:
            if ef.expert_id == expert_id and ef.layer_id == layer_id:
                return ef
        return None

    def get_experts_for_layer(self, layer_id: int) -> list[ExpertFile]:
        return [ef for ef in self.expert_files if ef.layer_id == layer_id]

    def get_fixed_experts(self, k: int = 6) -> list[ExpertFile]:
        selected: list[ExpertFile] = []
        for layer_id in range(1, self.num_layers + 1):
            layer_experts = sorted(
                [ef for ef in self.expert_files if ef.layer_id == layer_id],
                key=lambda x: x.expert_id
            )
            selected.extend(layer_experts[:k])
        return selected
