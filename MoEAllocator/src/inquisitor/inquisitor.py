import json
from pathlib import Path

from .analyzer import AnalysisReport, WeightAnalyzer


class Inquisitor:
    def __init__(self, model_path: str | Path):
        self.model_path = Path(model_path)
        self.analyzer = WeightAnalyzer(self.model_path)
        self._report: AnalysisReport | None = None

    def scan(self) -> AnalysisReport:
        self._report = self.analyzer.analyze()
        return self._report

    def save_report(self, output_path: str | Path) -> None:
        if self._report is None:
            raise RuntimeError("No report available. Call scan() first.")
        path = Path(output_path)
        path.parent.mkdir(parents=True, exist_ok=True)
        with open(path, "w") as f:
            json.dump(self._report.to_dict(), f, indent=2, ensure_ascii=False)

    def print_report(self) -> None:
        if self._report is None:
            raise RuntimeError("No report available. Call scan() first.")
        r = self._report.to_dict()

        print(f"\n{'='*60}")
        print(f"  Inquisitor Report: {r['model_path']}")
        print(f"{'='*60}")
        print(f"  Architecture : {r['architecture']}")
        print(f"  Model Type   : {r['model_type']}")
        print(f"  Total Size   : {r['total_size']}")
        print(f"  Layers       : {r['num_layers']}")
        print(f"  MoE Expert Layers : {r['expert_layers']}")
        print(f"  Total Experts    : {r['total_experts']}")
        print(f"{'-'*60}")

        moe = r.get("moe", {})
        print(f"  MoE Configuration:")
        print(f"    Experts per layer     : {moe.get('num_experts', 'N/A')}")
        print(f"    Activated Experts (K) : {moe.get('moe_k', 'N/A')}")
        print(f"    Shared Experts        : {moe.get('num_shared_experts', 'N/A')}")
        print(f"    MoE Intermediate Size : {moe.get('moe_intermediate_size', 'N/A')}")
        print(f"    Hidden Size           : {moe.get('hidden_size', 'N/A')}")
        print(f"    MoE Layer Indices     : {moe.get('moe_layer_indices', [])[0] if moe.get('moe_layer_indices') else 'N/A'} - {moe.get('moe_layer_indices', [])[-1] if moe.get('moe_layer_indices') else 'N/A'}")
        print(f"    Per-Expert Size       : {moe.get('per_expert_size', 'N/A')}")
        print(f"{'-'*60}")

        cats = r["categories"]
        for name, cat in [("Backbone", "backbone"), ("Gate/Router", "gate"),
                           ("Expert", "expert"), ("Shared Expert", "shared_expert")]:
            c = cats[cat]
            print(f"  {name:<14}  count={c['count']:<5}  size={c['size']:<12}  ({c['ratio']})")

        print(f"{'='*60}\n")

        if moe.get("per_layer_expert_size"):
            print("  Expert Size per Layer:")
            for layer, size in sorted(moe["per_layer_expert_size"].items()):
                print(f"    {layer:<10}  {size}")

        print(f"\n  Expert Weight Distribution (per shard file):")
        expert_names = sorted(set(
            w.name.split(".experts.")[1].split(".")[0]
            for w in self._report.expert_weights
        ))
        print(f"    Unique expert IDs: {expert_names[:10]}{'...' if len(expert_names) > 10 else ''} ({len(expert_names)} total)")

        shard_expert_sizes: dict[str, int] = {}
        for w in self._report.expert_weights:
            shard_expert_sizes[w.shard_file] = shard_expert_sizes.get(w.shard_file, 0) + w.size_bytes
        for shard, size in sorted(shard_expert_sizes.items()):
            size_gb = size / (1 << 30)
            print(f"    {shard:<28}  expert size: {size_gb:.2f} GB")

        print()
