import os
import sys
import json
from pathlib import Path
from typing import Optional, Callable

sys.path.insert(0, os.path.dirname(__file__))

from src import PhotoAnalyzer, AnalysisResult, get_image_files, is_image_file


def load_checkpoint(checkpoint_path: Path) -> set[str]:
    if checkpoint_path.exists():
        with open(checkpoint_path, "r", encoding="utf-8") as f:
            return set(json.load(f))
    return set()


def save_checkpoint(checkpoint_path: Path, completed: set[str]):
    with open(checkpoint_path, "w", encoding="utf-8") as f:
        json.dump(list(completed), f, ensure_ascii=False)


def append_result_jsonl(result: AnalysisResult, jsonl_path: Path):
    with open(jsonl_path, "a", encoding="utf-8") as f:
        f.write(json.dumps(result.to_dict(), ensure_ascii=False) + "\n")


def print_progress(current: int, total: int, filename: str, success: bool, error: Optional[str] = None):
    percent = current / total * 100 if total > 0 else 0
    bar_len = 30
    filled = int(bar_len * current / total) if total > 0 else 0
    bar = "█" * filled + "░" * (bar_len - filled)
    status = "[OK]" if success else "[FAIL]"
    error_msg = f" - {error}" if error else ""
    print(f"\r[{bar}] {percent:5.1f}% ({current}/{total}) {status} {filename}{error_msg}", end="", flush=True)


def interactive_analyze():
    print("=" * 50)
    print("       PhotoAnalyzer - 旅行照片分析工具")
    print("=" * 50)
    print()

    folder_path = input("📁 输入文件夹路径: ").strip()
    while not folder_path or not Path(folder_path).is_dir():
        if folder_path:
            print("路径不存在，请重新输入")
        folder_path = input("📁 输入文件夹路径: ").strip()

    output_name = input("📝 输入输出文件名（不含扩展名，默认 analysis）: ").strip() or "analysis"
    recursive_input = input("🔄 是否递归子文件夹? (Y/n): ").strip().lower()
    recursive = recursive_input != "n"

    resume_input = input("⏩ 是否启用断点续传? (y/N): ").strip().lower()
    enable_resume = resume_input == "y"

    delay_input = input("⏱️  请求间隔秒数（直接回车默认1.0）: ").strip()
    delay = float(delay_input) if delay_input else 1.0

    print()
    print("-" * 50)
    print(f"文件夹: {folder_path}")
    print(f"输出: {output_name}.jsonl (+ .checkpoint.json)")
    print(f"递归: {'是' if recursive else '否'}")
    print(f"断点续传: {'启用' if enable_resume else '禁用'}")
    print(f"请求间隔: {delay}s")
    print("-" * 50)

    confirm = input("\n确认开始分析? (Y/n): ").strip().lower()
    if confirm == "n":
        print("已取消")
        return

    image_files = get_image_files(Path(folder_path))
    if not image_files:
        print("未找到支持的图片文件")
        return

    total = len(image_files)
    print(f"\n找到 {total} 张图片，开始分析...\n")

    jsonl_path = Path(output_name + ".jsonl")
    checkpoint_path = Path(output_name + ".checkpoint.json")
    results = []
    completed_paths = load_checkpoint(checkpoint_path) if enable_resume else set()

    if enable_resume and completed_paths:
        print(f"已加载 {len(completed_paths)} 个断点，继续分析...\n")

    analyzer = PhotoAnalyzer(delay_between_requests=delay)

    for i, img_path in enumerate(image_files, 1):
        img_path_str = str(img_path.absolute())

        if enable_resume and img_path_str in completed_paths:
            print_progress(i, total, img_path.name, True)
            continue

        result = analyzer.analyze_image(img_path)
        results.append(result)

        append_result_jsonl(result, jsonl_path)

        if enable_resume:
            completed_paths.add(img_path_str)
            save_checkpoint(checkpoint_path, completed_paths)

        print_progress(i, total, img_path.name, result.success, result.error)

        if analyzer.delay > 0:
            time.sleep(analyzer.delay)

    print(f"\n\n✅ 分析完成！结果已保存至: {jsonl_path}")

    success_count = sum(1 for r in results if r.success)
    print(f"   成功: {success_count}/{len(results)}")

    if enable_resume:
        print(f"   断点已保存: {checkpoint_path}")


if __name__ == "__main__":
    import time
    interactive_analyze()
