import os
import sys
import argparse
from pathlib import Path

sys.path.insert(0, os.path.dirname(__file__))

from src import PhotoAnalyzer, export_to_json, export_to_csv, print_summary, get_image_files


def main():
    parser = argparse.ArgumentParser(description="批量分析文件夹中的旅行照片")
    parser.add_argument("folder_path", help="文件夹路径")
    parser.add_argument("-o", "--output", default="analysis_result", help="输出文件路径（不含扩展名），默认: analysis_result")
    parser.add_argument("-r", "--recursive", action="store_true", default=True, help="递归遍历子文件夹（默认开启）")
    parser.add_argument("--no-recursive", action="store_true", help="不递归遍历子文件夹")
    parser.add_argument("--json", action="store_true", help="导出 JSON 格式")
    parser.add_argument("--csv", action="store_true", help="导出 CSV 格式")
    parser.add_argument("--no-export", action="store_true", help="不导出文件，只打印摘要")
    parser.add_argument("--delay", type=float, default=1.0, help="每次请求间隔（秒），默认: 1.0")
    parser.add_argument("--dry-run", action="store_true", help="仅列出将要分析的图片，不执行分析")

    args = parser.parse_args()

    folder_path = Path(args.folder_path)
    if not folder_path.exists() or not folder_path.is_dir():
        print(f"错误: 文件夹不存在或不是有效目录 - {folder_path}")
        sys.exit(1)

    recursive = not args.no_recursive
    image_files = get_image_files(folder_path)

    if not image_files:
        print(f"在 {folder_path} 中未找到支持的图片文件")
        sys.exit(0)

    print(f"找到 {len(image_files)} 张图片")
    print(f"模式: {'递归' if recursive else '非递归'}遍历\n")

    if args.dry_run:
        print("以下文件将被分析:")
        for f in image_files:
            print(f"  - {f}")
        sys.exit(0)

    analyzer = PhotoAnalyzer(delay_between_requests=args.delay)
    print("开始分析...\n")

    results = analyzer.analyze_folder(folder_path, recursive=recursive)

    print_summary(results)

    if not args.no_export:
        if args.json or (not args.json and not args.csv):
            json_path = args.output + ".json"
            export_to_json(results, json_path)
            print(f"\n已导出 JSON: {json_path}")

        if args.csv or (not args.json and not args.csv):
            csv_path = args.output + ".csv"
            export_to_csv(results, csv_path)
            print(f"已导出 CSV: {csv_path}")


if __name__ == "__main__":
    main()
