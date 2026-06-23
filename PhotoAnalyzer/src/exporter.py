import csv
import json
from pathlib import Path
from typing import Optional, Union
from .analyzer import AnalysisResult


def export_to_json(
    results: list[AnalysisResult],
    output_path: str | Path,
    include_failed: bool = True,
) -> None:
    output_path = Path(output_path)
    data = [r.to_dict() for r in results if include_failed or r.success]
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)


def export_to_csv(
    results: list[AnalysisResult],
    output_path: str | Path,
    include_failed: bool = True,
) -> None:
    output_path = Path(output_path)

    rows = []
    for r in results:
        if not include_failed and not r.success:
            continue

        row = {
            "file_path": r.file_path,
            "file_name": r.file_name,
            "success": r.success,
            "error": r.error or "",
        }

        if r.data:
            for key, value in r.data.items():
                if isinstance(value, list):
                    row[key] = "; ".join(str(v) for v in value)
                else:
                    row[key] = value
        else:
            for key in ["score", "style", "caption", "main_objects", "blurry", "comments", "recommendations"]:
                row[key] = ""

        rows.append(row)

    if not rows:
        return

    fieldnames = ["file_path", "file_name", "success", "error",
                  "score", "style", "caption", "main_objects", "blurry", "comments", "recommendations"]

    with open(output_path, "w", newline="", encoding="utf-8-sig") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


def print_summary(results: list[AnalysisResult]) -> None:
    total = len(results)
    success = sum(1 for r in results if r.success)
    failed = total - success

    print(f"\n{'='*50}")
    print(f"分析完成！总计: {total} | 成功: {success} | 失败: {failed}")
    print(f"{'='*50}")

    if failed > 0:
        print("\n失败的文件:")
        for r in results:
            if not r.success:
                print(f"  - {r.file_name}: {r.error}")


def load_results_from_jsonl(jsonl_path: str | Path) -> list[dict]:
    jsonl_path = Path(jsonl_path)
    results = []
    if not jsonl_path.exists():
        return results
    with open(jsonl_path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                results.append(json.loads(line))
    return results


def convert_jsonl_to_json(jsonl_path: str | Path, output_path: Optional[str | Path] = None) -> str:
    jsonl_path = Path(jsonl_path)
    if output_path is None:
        output_path = jsonl_path.with_suffix(".json")
    else:
        output_path = Path(output_path)

    results = load_results_from_jsonl(jsonl_path)
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(results, f, indent=2, ensure_ascii=False)
    return str(output_path)


def convert_jsonl_to_csv(jsonl_path: str | Path, output_path: Optional[str | Path] = None) -> str:
    jsonl_path = Path(jsonl_path)
    if output_path is None:
        output_path = jsonl_path.with_suffix(".csv")
    else:
        output_path = Path(output_path)

    results = load_results_from_jsonl(jsonl_path)
    if not results:
        return str(output_path)

    fieldnames = ["file_path", "file_name", "success", "error",
                  "score", "style", "caption", "main_objects", "blurry", "comments", "recommendations"]

    with open(output_path, "w", newline="", encoding="utf-8-sig") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        for r in results:
            row = {k: r.get(k, "") for k in fieldnames}
            if r.get("data"):
                for k in ["score", "style", "caption", "main_objects", "blurry", "comments", "recommendations"]:
                    if k in r["data"]:
                        row[k] = r["data"][k]
            if isinstance(row.get("main_objects"), list):
                row["main_objects"] = "; ".join(str(v) for v in row["main_objects"])
            writer.writerow(row)

    return str(output_path)
