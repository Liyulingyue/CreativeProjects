import json
from pathlib import Path

from .manifest import ExpertFile, ModelManifest
from .splitter import ModelSplitter


class Surgeon:
    def __init__(self, model_path: str | Path, output_dir: str | Path):
        self.model_path = Path(model_path)
        self.output_dir = Path(output_dir)
        self._splitter = ModelSplitter(self.model_path, self.output_dir)
        self._manifest: ModelManifest | None = None

    def split(self, fixed_k: int | None = None) -> ModelManifest:
        self._manifest = self._splitter.split(fixed_k=fixed_k)
        return self._manifest

    def load_manifest(self, manifest_path: str | Path | None = None) -> ModelManifest:
        if manifest_path is None:
            manifest_path = self.output_dir / "manifest.json"
        with open(manifest_path) as f:
            data = json.load(f)

        expert_files = [ExpertFile(**e) for e in data["expert_files"]]
        from .manifest import BackboneFile
        backbone_files = []
        for b in data["backbone_files"]:
            bf = BackboneFile(
                category=b["category"],
                file_path=b["file_path"],
                size_bytes=b["size_bytes"],
            )
            backbone_files.append(bf)

        standard_config_keys = {
            "num_experts", "num_layers", "moe_k", "num_shared_experts",
            "hidden_size", "moe_intermediate_size",
        }

        self._manifest = ModelManifest(
            model_name=data["model_name"],
            model_path=data["model_path"],
            architecture=data["architecture"],
            output_dir=Path(data["output_dir"]),
            num_experts=data["config"]["num_experts"],
            num_layers=data["config"]["num_layers"],
            moe_k=data["config"]["moe_k"],
            num_shared_experts=data["config"]["num_shared_experts"],
            hidden_size=data["config"]["hidden_size"],
            moe_intermediate_size=data["config"]["moe_intermediate_size"],
            extra_config={k: v for k, v in data["config"].items() if k not in standard_config_keys},
            expert_files=expert_files,
            backbone_files=backbone_files,
        )
        return self._manifest

    def print_summary(self) -> None:
        if self._manifest is None:
            raise RuntimeError("No manifest. Call split() or load_manifest() first.")
        m = self._manifest

        print(f"\n{'='*60}")
        print(f"  Surgeon Split Summary")
        print(f"{'='*60}")
        print(f"  Model        : {m.model_name}")
        print(f"  Architecture : {m.architecture}")
        print(f"  Output Dir   : {m.output_dir}")
        print(f"{'-'*60}")
        print(f"  Experts     : {len(m.expert_files)} files")
        print(f"  Backbone    : {len(m.backbone_files)} files")
        print(f"  Config      : {m.num_experts} experts/layer, K={m.moe_k}, {m.num_shared_experts} shared")
        print(f"{'='*60}\n")

        print("  Backbone Files:")
        for bf in m.backbone_files:
            size_gb = bf.size_bytes / (1 << 30)
            print(f"    {bf.category:<20}  {size_gb:.3f} GB  ({len(bf.weight_names)} weights)")

        print(f"\n  Expert Files (first 10):")
        for ef in sorted(m.expert_files, key=lambda x: (x.layer_id, x.expert_id))[:10]:
            size_mb = ef.size_bytes / (1 << 20)
            print(f"    layer_{ef.layer_id:03d} / expert_{ef.expert_id:03d}  {size_mb:.1f} MB")
        remaining = len(m.expert_files) - 10
        if remaining > 0:
            print(f"    ... and {remaining} more expert files")

        print()
