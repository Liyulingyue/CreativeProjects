"""
全量生成 trip_matrix_cache：35 种子城市 × 5 天行程 = 1225 cells。

策略：
  - 按 city 串行（每个城市内部 plan_matrix 已并发）
  - 跳过 DB 中已有有效 cache 的 cell（input_metadata 匹配）
  - 进度持久化
  - 限速感知（指数退避 + 永久跳过）

跑：python src/batch_generate_matrix.py
"""
import argparse
import json
import sqlite3
import sys
import time
from datetime import date
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from openai import RateLimitError, APIStatusError

from src.config import API_KEY, BASE_URL, MODEL_NAME
from openai import OpenAI
from openaijsonwrapper import OpenAIJsonWrapper
from src.db import get_conn
from app.refresh import save_matrix_to_cache

DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DB_PATH = DATA_DIR / "travelsites.db"
PROGRESS_PATH = DATA_DIR / "matrix_progress.json"

SLEEP_BETWEEN_CITIES = 5
SLEEP_ON_LIMIT_RETRY = 60  # 1 分钟


def get_seed_cities() -> list[str]:
    conn = sqlite3.connect(str(DB_PATH))
    row = conn.execute("SELECT value FROM seed_config WHERE key='cities'").fetchone()
    conn.close()
    if row:
        try:
            return json.loads(row[0])
        except Exception:
            pass
    # fallback
    return []


def load_progress() -> dict:
    if PROGRESS_PATH.exists():
        return json.loads(PROGRESS_PATH.read_text())
    return {"completed_cities": [], "rate_limit_pauses": 0}


def save_progress(p: dict):
    tmp = PROGRESS_PATH.with_suffix(".tmp")
    tmp.write_text(json.dumps(p, ensure_ascii=False, indent=2))
    tmp.replace(PROGRESS_PATH)


def has_valid_cache(city: str, today_str: str, duration: int) -> bool:
    """检查 DB 中是否已有有效 cache（input_metadata 匹配）。"""
    conn = get_conn()
    try:
        row = conn.execute("""
            SELECT input_metadata FROM trip_matrix_cache
            WHERE city=? AND start_date=? AND duration=?
        """, (city, today_str, duration)).fetchone()
    finally:
        conn.close()
    if row and row[0]:
        try:
            meta = json.loads(row[0])
            return "weather_hash" in meta
        except Exception:
            return False
    return False


def generate_city(city: str, max_offset: int = 7, max_duration: int = 5) -> int:
    """生成一个城市的所有 cells。返回新增数。"""
    today_str = date.today().isoformat()

    # 检查可跳过的
    skip = 0
    for d in range(1, max_duration + 1):
        for offset in range(1, max_offset + 1):
            target_date = date.fromisoformat(today_str)
            from datetime import timedelta
            td = (target_date + timedelta(days=offset)).isoformat()
            if has_valid_cache(city, td, d):
                skip += 1

    total_possible = max_duration * max_offset
    if skip >= total_possible:
        print(f"  ↪ {city} 全部 {total_possible} 个 cell 已有 cache")
        return 0

    print(f"  → {city} (剩 {total_possible - skip} 待生成)")
    # 注意：plan_matrix 内部用 LLM + input_metadata 缓存
    cells = plan_matrix(city, max_start_offset=max_offset, max_duration=max_duration, lite=True)
    if cells:
        save_matrix_to_cache(city, cells)
        return len(cells)
    return 0


def main():
    ensure_cache_columns()

    cities = get_seed_cities()
    if not cities:
        print("❌ 没找到种子城市")
        return

    print(f"种子城市: {len(cities)}")
    print(f"每城: 7 起点 × 5 天 = 35 cells")
    print(f"总: {len(cities) * 35} cells")

    progress = load_progress()
    completed = set(progress.get("completed_cities", []))

    success = 0
    failed = 0
    t0 = time.time()

    for i, city in enumerate(cities):
        if city in completed:
            continue
        try:
            n = generate_city(city)
            completed.add(city)
            progress["completed_cities"] = list(completed)
            save_progress(progress)
            success += 1
        except RateLimitError:
            count = progress.get("rate_limit_pauses", 0) + 1
            progress["rate_limit_pauses"] = count
            save_progress(progress)
            print(f"⚠ 限速 #{count}，跳过 {city}，下次跑")
            failed += 1
            time.sleep(SLEEP_ON_LIMIT_RETRY)
            continue
        except Exception as e:
            print(f"  ✗ {e}")
            failed += 1
        time.sleep(SLEEP_BETWEEN_CITIES)

    elapsed = time.time() - t0
    print()
    print(f"=== 完成 ===")
    print(f"  成功: {success}, 失败: {failed}")
    print(f"  用时: {elapsed/60:.1f} 分钟")


def ensure_cache_columns():
    """确保 trip_matrix_cache 有所需的 input_metadata 列（之前已加过）。"""
    conn = get_conn()
    try:
        conn.execute("SELECT input_metadata FROM trip_matrix_cache LIMIT 1")
    except Exception:
        conn.execute("ALTER TABLE trip_matrix_cache ADD COLUMN input_metadata TEXT")
    conn.commit()
    conn.close()


if __name__ == "__main__":
    main()
