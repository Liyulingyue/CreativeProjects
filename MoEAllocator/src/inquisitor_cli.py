#!/usr/bin/env python3
import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from inquisitor import Inquisitor


def main():
    parser = argparse.ArgumentParser(
        prog="inquisitor",
        description="MoEAllocator Inquisitor: Analyze MoE model weight architecture.",
    )
    parser.add_argument(
        "model_path",
        type=str,
        help="Path to the model directory containing config.json and safetensors index.",
    )
    parser.add_argument(
        "--output", "-o",
        type=str,
        default=None,
        help="Path to save JSON report. If not specified, prints to stdout.",
    )
    parser.add_argument(
        "--json", "-j",
        action="store_true",
        help="Output raw JSON instead of formatted report.",
    )

    args = parser.parse_args()

    model_path = Path(args.model_path)
    if not model_path.exists():
        print(f"Error: Model path does not exist: {model_path}", file=sys.stderr)
        sys.exit(1)

    try:
        inquisitor = Inquisitor(model_path)
        report = inquisitor.scan()

        if args.output:
            inquisitor.save_report(args.output)
            print(f"Report saved to: {args.output}")
        elif args.json:
            import json
            print(json.dumps(report.to_dict(), indent=2, ensure_ascii=False))
        else:
            inquisitor.print_report()

    except FileNotFoundError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)
    except Exception as e:
        print(f"Unexpected error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
