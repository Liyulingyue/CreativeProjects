import os
import sys
import json
import argparse
from pathlib import Path

sys.path.insert(0, os.path.dirname(__file__))

from src import PhotoAnalyzer, export_to_json, export_to_csv


def main():
    parser = argparse.ArgumentParser(description="分析单张旅行照片")
    parser.add_argument("image_path", help="图片文件路径")
    parser.add_argument("-o", "--output", help="输出文件路径（不含扩展名）")
    parser.add_argument("--json", action="store_true", help="导出 JSON 格式")
    parser.add_argument("--csv", action="store_true", help="导出 CSV 格式")
    parser.add_argument("--no-export", action="store_true", help="不导出文件，只打印结果")

    args = parser.parse_args()

    image_path = Path(args.image_path)
    if not image_path.exists():
        print(f"错误: 文件不存在 - {image_path}")
        sys.exit(1)

    analyzer = PhotoAnalyzer()
    print(f"正在分析: {image_path.name}")

    result = analyzer.analyze_image(image_path)

    if result.success:
        print(f"\n[成功] {result.file_name}")
        print(json.dumps(result.data, indent=2, ensure_ascii=False))
    else:
        print(f"\n[失败] {result.file_name}: {result.error}")

    if not args.no_export:
        if args.json or (not args.json and not args.csv):
            json_path = args.output or str(image_path.with_suffix(""))
            if not json_path.endswith(".json"):
                json_path += ".json"
            export_to_json([result], json_path)
            print(f"\n已导出 JSON: {json_path}")

        if args.csv:
            csv_path = args.output or str(image_path.with_suffix(""))
            if not csv_path.endswith(".csv"):
                csv_path += ".csv"
            export_to_csv([result], csv_path)
            print(f"已导出 CSV: {csv_path}")


if __name__ == "__main__":
    main()
