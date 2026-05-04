import json
import tempfile
import unittest
from pathlib import Path

import torch

from src.nexus.master import NexusMaster


class NexusMasterDtypeTest(unittest.TestCase):
    def _write_manifest(self, tmpdir: str, torch_dtype: str = "bfloat16") -> str:
        output_dir = Path(tmpdir) / "split"
        output_dir.mkdir()
        manifest = {
            "architecture": "test",
            "output_dir": str(output_dir),
            "config": {
                "hidden_size": 2560,
                "num_attention_heads": 20,
                "num_key_value_heads": 4,
                "num_layers": 28,
                "moe_k": 6,
                "num_shared_experts": 2,
                "moe_intermediate_size": 1536,
                "torch_dtype": torch_dtype,
            },
        }
        manifest_path = Path(tmpdir) / "manifest.json"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        return str(manifest_path)

    def test_explicit_float32_is_not_overridden_by_manifest_dtype(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            manifest_path = self._write_manifest(tmpdir, torch_dtype="bfloat16")

            master = NexusMaster(manifest_path, dtype=torch.float32)

        self.assertIs(master.dtype, torch.float32)

    def test_explicit_low_precision_dtypes_are_preserved(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            manifest_path = self._write_manifest(tmpdir, torch_dtype="bfloat16")

            fp16_master = NexusMaster(manifest_path, dtype=torch.float16)
            bf16_master = NexusMaster(manifest_path, dtype=torch.bfloat16)

        self.assertIs(fp16_master.dtype, torch.float16)
        self.assertIs(bf16_master.dtype, torch.bfloat16)


if __name__ == "__main__":
    unittest.main()
