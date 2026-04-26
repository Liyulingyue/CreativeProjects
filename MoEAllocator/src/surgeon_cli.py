#!/usr/bin/env python3
import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from surgeon import Surgeon


def main():
    parser = argparse.ArgumentParser(
        prog="surgeon",
        description="MoEAllocator Surgeon: Split MoE model weights into expert/backbone files.",
    )
    parser.add_argument(
        "model_path",
        type=str,
        help="Path to the model directory.",
    )
    parser.add_argument(
        "--output", "-o",
        type=str,
        default=None,
        help="Output directory for split files. Defaults to output/splits/<model_name>/",
    )
    parser.add_argument(
        "--fixed-k",
        type=int,
        default=None,
        metavar="K",
        help="Mini-mode: only extract first K experts per layer (for validation). "
             "e.g. --fixed-k 6 extracts only the first 6 experts per layer.",
    )
    parser.add_argument(
        "--load-only",
        action="store_true",
        help="Only load an existing manifest, don't split.",
    )
    parser.add_argument(
        "--manifest", "-m",
        type=str,
        default=None,
        help="Path to manifest.json (for --load-only).",
    )

    args = parser.parse_args()

    model_path = Path(args.model_path)
    if not model_path.exists():
        print(f"Error: Model path does not exist: {model_path}", file=sys.stderr)
        sys.exit(1)

    if args.output:
        output_dir = Path(args.output)
    else:
        output_dir = Path("output/splits") / model_path.name

    surgeon = Surgeon(model_path, output_dir)

    try:
        if args.load_only:
            manifest = surgeon.load_manifest(args.manifest)
        else:
            if args.fixed_k:
                print(f"[Mini-mode] Extracting first {args.fixed_k} experts per layer...")
            manifest = surgeon.split(fixed_k=args.fixed_k)

        surgeon.print_summary()

        print(f"Manifest saved to: {output_dir / 'manifest.json'}")
        print(f"Output directory : {output_dir}")

        if args.fixed_k:
            print(f"\n[Mini-mode] Total expert files: {len(manifest.expert_files)}")
            print(f"  Expected (6 experts × 27 layers): {6 * 27} = 162")
            total_size = sum(ef.size_bytes for ef in manifest.expert_files)
            print(f"  Expert size: {total_size / (1 << 30):.2f} GB")

    except FileNotFoundError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)
    except Exception as e:
        import traceback
        print(f"Unexpected error: {e}", file=sys.stderr)
        traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    main()
