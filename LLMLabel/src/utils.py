import json
import random
from pathlib import Path
from typing import Iterator


def read_jsonl(path: str) -> Iterator[dict]:
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                yield json.loads(line)


def write_jsonl(path: str, records: list[dict]):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        for rec in records:
            f.write(json.dumps(rec, ensure_ascii=False) + "\n")


def sample_texts(path: str, n: int, seed: int = 42) -> list[str]:
    random.seed(seed)
    texts = []
    for obj in read_jsonl(path):
        text = obj.get("text") or obj.get("content", "")
        if text:
            texts.append(text)
    return random.sample(texts, min(n, len(texts)))


def load_all_texts(path: str) -> list[str]:
    texts = []
    for obj in read_jsonl(path):
        text = obj.get("text") or obj.get("content", "")
        if text:
            texts.append(text)
    return texts


def merge_labeled_files(input_paths: list[str], output_path: str):
    merged = []
    for p in input_paths:
        for obj in read_jsonl(p):
            merged.append(obj)
    write_jsonl(output_path, merged)
    print(f"[Utils] 合并完成：{len(merged)} 条，保存至 {output_path}")


def load_image_paths(path: str, limit: int | None = None) -> list[str]:
    p = Path(path)
    if p.is_dir():
        exts = {".jpg", ".jpeg", ".png", ".bmp", ".webp"}
        paths = [str(f) for f in p.rglob("*") if f.suffix.lower() in exts]
    elif p.suffix == ".jsonl":
        paths = [obj["image_path"] for obj in read_jsonl(path) if obj.get("image_path")]
    else:
        raise ValueError(f"Unsupported image path: {path}")
    if limit:
        paths = paths[:limit]
    return paths
