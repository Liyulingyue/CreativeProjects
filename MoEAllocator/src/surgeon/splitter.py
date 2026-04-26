import json
import re
import sys
from collections import defaultdict
from pathlib import Path

import safetensors.torch as storch
import torch
from tqdm import tqdm

sys.path.insert(0, str(Path(__file__).parent.parent))
from inquisitor.architecture import WeightCategory, get_architecture_spec
from .manifest import BackboneFile, ExpertFile, ModelManifest


class ModelSplitter:
    def __init__(self, model_path: str | Path, output_dir: str | Path):
        self.model_path = Path(model_path)
        self.output_dir = Path(output_dir)
        self._config: dict = {}
        self._spec = None
        self._weight_map: dict[str, str] = {}

    def load_config(self) -> dict:
        config_path = self.model_path / "config.json"
        with open(config_path) as f:
            self._config = json.load(f)
        self._spec = get_architecture_spec(self._config.get("model_type", ""))
        return self._config

    def load_weight_index(self) -> dict[str, str]:
        index_path = self.model_path / "model.safetensors.index.json"
        with open(index_path) as f:
            data = json.load(f)
        self._weight_map = data.get("weight_map", {})
        return self._weight_map

    def _classify(self, name: str) -> WeightCategory:
        spec = self._spec
        if spec is None:
            raise RuntimeError("Call load_config() first.")

        for pattern in spec.gate_patterns:
            if pattern.search(name):
                return WeightCategory.GATE
        for pattern in spec.shared_expert_patterns:
            if pattern.search(name):
                return WeightCategory.SHARED_EXPERT
        for pattern in spec.expert_name_patterns.get(WeightCategory.EXPERT, []):
            if pattern.search(name):
                return WeightCategory.EXPERT
        for pattern in spec.backbone_patterns:
            if pattern.search(name):
                return WeightCategory.BACKBONE
        return WeightCategory.BACKBONE

    def _parse_expert_id(self, name: str) -> tuple[int, int] | None:
        match = re.search(r"model\.layers\.(\d+)\.mlp\.experts\.(\d+)\.", name)
        if match:
            return int(match.group(1)), int(match.group(2))
        return None

    def _collect_shard_files(self) -> list[Path]:
        shard_files = sorted(self.model_path.glob("model-*.safetensors"))
        if not shard_files:
            raise FileNotFoundError(f"No shard files found in {self.model_path}")
        return shard_files

    def split(self, fixed_k: int | None = None) -> ModelManifest:
        config = self.load_config()
        self.load_weight_index()

        experts_dir = self.output_dir / "experts"
        backbone_dir = self.output_dir / "backbone"
        experts_dir.mkdir(parents=True, exist_ok=True)
        backbone_dir.mkdir(parents=True, exist_ok=True)

        expert_weights: dict[tuple[int, int], dict[str, tuple[str, torch.Tensor]]] = defaultdict(dict)
        gate_weights: dict[str, tuple[str, torch.Tensor]] = {}
        shared_expert_weights: dict[str, tuple[str, torch.Tensor]] = {}
        backbone_weights: dict[str, tuple[str, torch.Tensor]] = {}

        shard_files = self._collect_shard_files()
        for shard_file in tqdm(shard_files, desc="Reading shards"):
            tensors = storch.load_file(str(shard_file))
            for name, tensor in tensors.items():
                category = self._classify(name)
                key = (shard_file.name, name)

                if category == WeightCategory.EXPERT:
                    parsed = self._parse_expert_id(name)
                    if parsed:
                        layer_id, expert_id = parsed
                        if fixed_k is not None and expert_id >= fixed_k:
                            continue
                        expert_weights[(layer_id, expert_id)][name] = (shard_file.name, tensor)
                elif category == WeightCategory.GATE:
                    gate_weights[name] = (shard_file.name, tensor)
                elif category == WeightCategory.SHARED_EXPERT:
                    shared_expert_weights[name] = (shard_file.name, tensor)
                else:
                    backbone_weights[name] = (shard_file.name, tensor)

        expert_files: list[ExpertFile] = []
        for (layer_id, expert_id), weights in tqdm(expert_weights.items(), desc="Writing experts"):
            safe_dict = {name: tensor for name, (shard, tensor) in weights.items()}
            file_name = f"expert_{expert_id:03d}_layer_{layer_id:03d}.safetensors"
            file_path = experts_dir / file_name
            storch.save_file(safe_dict, str(file_path))

            shard_sources = list(set(shard for (shard, _) in weights.values()))
            expert_files.append(ExpertFile(
                expert_id=expert_id,
                layer_id=layer_id,
                file_path=str(file_path),
                size_bytes=file_path.stat().st_size,
                shard_sources=shard_sources,
            ))

        backbone_files: list[BackboneFile] = []
        for cat_name, cat_weights, out_name in [
            ("gate", gate_weights, "gate.safetensors"),
            ("shared_expert", shared_expert_weights, "shared_expert.safetensors"),
            ("backbone", backbone_weights, "backbone.safetensors"),
        ]:
            if not cat_weights:
                continue
            safe_dict = {name: tensor for name, (_, tensor) in cat_weights.items()}
            file_path = backbone_dir / out_name
            storch.save_file(safe_dict, str(file_path))
            backbone_files.append(BackboneFile(
                category=cat_name,
                file_path=str(file_path),
                size_bytes=file_path.stat().st_size,
                weight_names=list(cat_weights.keys()),
            ))

        model_name = self.model_path.name
        manifest = ModelManifest(
            model_name=model_name,
            model_path=str(self.model_path),
            architecture=self._spec.name if self._spec else "unknown",
            output_dir=self.output_dir,
            num_experts=config.get("moe_num_experts", 64),
            num_layers=config.get("num_hidden_layers", 28),
            moe_k=config.get("moe_k", 0),
            num_shared_experts=config.get("moe_num_shared_experts", 0),
            hidden_size=config.get("hidden_size", 0),
            moe_intermediate_size=config.get("moe_intermediate_size", 0),
            expert_files=expert_files,
            backbone_files=backbone_files,
        )

        manifest_path = self.output_dir / "manifest.json"
        with open(manifest_path, "w") as f:
            json.dump(manifest.to_dict(), f, indent=2, ensure_ascii=False)

        return manifest
