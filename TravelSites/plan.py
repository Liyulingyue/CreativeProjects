import os
import sys
import argparse
import re
from datetime import datetime
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8")

sys.path.insert(0, os.path.dirname(__file__))

from src import TripPlanner, export_to_json, print_summary


DATE_PATTERN = re.compile(r"^\d{4}-\d{2}-\d{2}$")


def validate_date(date_str: str, label: str) -> str:
    if not DATE_PATTERN.match(date_str):
        print(f"错误: {label} 日期格式应为 YYYY-MM-DD，实际: {date_str}")
        sys.exit(1)
    try:
        datetime.strptime(date_str, "%Y-%m-%d")
    except ValueError as e:
        print(f"错误: {label} 日期无效 - {e}")
        sys.exit(1)
    return date_str


def main():
    parser = argparse.ArgumentParser(
        description="为指定城市生成结构化多日旅行方案",
    )
    parser.add_argument("--city", required=True, help="目的地城市，例如 '洛阳'")
    parser.add_argument("--start", required=True, help="出发日期 YYYY-MM-DD")
    parser.add_argument("--end", required=True, help="返回日期 YYYY-MM-DD")
    parser.add_argument("-o", "--output", help="输出 JSON 路径（不含扩展名），默认 trip_{city}_{start}_{end}")
    parser.add_argument("--no-export", action="store_true", help="不导出文件，只打印结果")

    args = parser.parse_args()

    city = args.city.strip()
    if not city:
        print("错误: 城市名不能为空")
        sys.exit(1)

    start_date = validate_date(args.start, "出发")
    end_date = validate_date(args.end, "返回")

    if end_date < start_date:
        print(f"错误: 返回日期 ({end_date}) 早于出发日期 ({start_date})")
        sys.exit(1)

    duration = (datetime.strptime(end_date, "%Y-%m-%d") - datetime.strptime(start_date, "%Y-%m-%d")).days + 1
    print(f"开始规划: {city}  {start_date} ~ {end_date}  ({duration} 天)")

    planner = TripPlanner()
    result = planner.plan(city, start_date, end_date)

    print_summary(result)

    if result.success and not args.no_export:
        script_dir = Path(__file__).resolve().parent
        output_dir = script_dir / "output"
        output_dir.mkdir(exist_ok=True)
        if args.output:
            json_path = Path(args.output)
            if not json_path.is_absolute():
                json_path = script_dir / json_path
            if not str(json_path).endswith(".json"):
                json_path = json_path.with_suffix(".json")
        else:
            safe_city = re.sub(r"[^\w\u4e00-\u9fff]+", "_", city)
            json_path = output_dir / f"trip_{safe_city}_{start_date}_{end_date}.json"
        export_to_json(result, json_path)
        print(f"\n已导出 JSON: {json_path}")


if __name__ == "__main__":
    main()
