import os
import sys
import argparse
import asyncio
import time
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8")

sys.path.insert(0, os.path.dirname(__file__))

from src.matrix import plan_matrix
from src.exporter import print_matrix, export_matrix_to_csv, export_matrix_to_json
from src.cities import save_learned_to_json, learned_count


def main():
    parser = argparse.ArgumentParser(
        description="生成 (出发日偏移 × 游玩天数) 的多维矩阵旅游方案",
    )
    parser.add_argument("--city", required=True, help="目的地城市")
    parser.add_argument("--max-offset", type=int, default=7, help="最大出发日偏移（天），默认 7")
    parser.add_argument("--max-duration", type=int, default=5, help="最大游玩天数，默认 5")
    parser.add_argument("--concurrency", type=int, default=1, help="并发 LLM 调用数，默认 1（顺序）")
    parser.add_argument("--lite", action="store_true", default=True, help="使用精简结构（仅 score + 策略），默认开启")
    parser.add_argument("--no-lite", action="store_true", help="使用完整结构（含 daily_plan），速度慢 2-3 倍")
    parser.add_argument("-o", "--output", help="输出文件名前缀，默认 matrix_{city}")
    parser.add_argument("--no-export", action="store_true", help="不导出文件")
    parser.add_argument("--csv-only", action="store_true", help="只导出 CSV")
    parser.add_argument("--json-only", action="store_true", help="只导出 JSON")
    parser.add_argument("--no-checkpoint", action="store_true", help="禁用断点续跑")

    args = parser.parse_args()
    city = args.city.strip()
    if not city:
        print("错误: 城市名不能为空")
        sys.exit(1)

    total = args.max_offset * args.max_duration
    print(f"城市: {city}")
    print(f"矩阵: {args.max_offset} 出发日 × {args.max_duration} 天数 = {total} 格")
    print(f"并发: {args.concurrency}")
    print()

    t0 = time.time()

    def on_progress(done: int, total: int, cell) -> None:
        status = "OK " if cell.success else "ERR"
        score = f"score={cell.score}" if cell.score is not None else ""
        rec = f"rec={cell.recommendation}" if cell.recommendation else ""
        print(f"  [{done:>2}/{total}] {status} +{cell.start_offset}d {cell.duration}d  {cell.start_date}  {score} {rec}", flush=True)

    use_lite = not args.no_lite
    script_dir = Path(__file__).resolve().parent
    output_dir = script_dir / "output"
    output_dir.mkdir(exist_ok=True)
    checkpoint = None
    if not args.no_checkpoint:
        safe_city = "".join(c for c in city if c.isalnum() or '\u4e00' <= c <= '\u9fff')
        checkpoint = output_dir / f".matrix_checkpoint_{safe_city}_{args.max_offset}x{args.max_duration}.json"

    print(f"模式: {'lite' if use_lite else 'full'}")
    print(f"断点: {checkpoint or '禁用'}")
    print()

    results = asyncio.run(plan_matrix(
        city=city,
        max_start_offset=args.max_offset,
        max_duration=args.max_duration,
        concurrency=args.concurrency,
        lite=use_lite,
        checkpoint_path=checkpoint,
        on_progress=on_progress,
    ))

    elapsed = time.time() - t0
    print(f"\n耗时: {elapsed:.1f}s  ({elapsed/total:.1f}s/格)")

    print_matrix(results)

    if not args.no_export:
        if args.output:
            prefix = Path(args.output)
            if not prefix.is_absolute():
                prefix = script_dir / prefix
        else:
            prefix = output_dir / f"matrix_{city}"
        prefix.parent.mkdir(parents=True, exist_ok=True)
        if not args.json_only:
            csv_path = prefix.with_suffix(".csv")
            export_matrix_to_csv(results, csv_path)
            print(f"\n已导出 CSV: {csv_path}")
        if not args.csv_only:
            json_path = prefix.with_suffix(".json")
            export_matrix_to_json(results, json_path)
            print(f"已导出 JSON: {json_path}")

    saved = save_learned_to_json()
    if saved:
        print(f"[learned] 已保存 {saved} 个城市坐标到 data/learned_cities.json (累计 {learned_count()})")


if __name__ == "__main__":
    main()
