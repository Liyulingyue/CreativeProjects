"""
批量跑全 374 城：从 Bing 搜索 + LLM 提取景点，写入 DB。

支持：
  - 断点续跑（DB 已有 ≥5 个景点的城市跳过）
  - token 限额感知：遇到 429/限额错误自动 sleep 5 分钟
  - 普通错误重试 1 次

用法：
  python src/batch_backfill_pois.py [--limit N] [--skip N]

参数：
  --limit N   本轮最多跑 N 个城市
  --skip N    跳过前 N 个城市
"""
import argparse
import json
import sqlite3
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from openai import APIStatusError, APIConnectionError, RateLimitError

from src.pois_via_search import (
    search_bing, extract_pois, insert_poi,
)


DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DB_PATH = DATA_DIR / "travelsites.db"
PROGRESS_PATH = DATA_DIR / "poi_backfill_progress.json"

MIN_POIS_PER_CITY = 5
SLEEP_BETWEEN_REQUESTS = 3.0       # 正常间隔
SLEEP_ON_RATE_LIMIT = 300          # 5 分钟（如果 token 限额）
SLEEP_ON_AUTH_ERROR = 600          # 10 分钟（如果 API key 失效，等刷新）
MAX_RETRIES_PER_CITY = 2           # 每个城市最多重试 2 次


def get_cities_to_process(limit: int = 0, skip: int = 0) -> list[str]:
    """获取需要跑的城市（已有 ≥5 跳过）。"""
    conn = sqlite3.connect(str(DB_PATH))
    cities = [r[0] for r in conn.execute(
        "SELECT name FROM geo_cities WHERE lat IS NOT NULL ORDER BY name"
    ).fetchall()]
    conn.close()

    conn = sqlite3.connect(str(DB_PATH))
    counts = dict(conn.execute(
        "SELECT city, COUNT(*) FROM attractions GROUP BY city"
    ).fetchall())
    conn.close()

    todo = [c for c in cities if counts.get(c, 0) < MIN_POIS_PER_CITY]

    if skip:
        todo = todo[skip:]
    if limit:
        todo = todo[:limit]
    return todo


def load_progress() -> dict:
    if PROGRESS_PATH.exists():
        try:
            return json.loads(PROGRESS_PATH.read_text())
        except Exception:
            pass
    return {"completed": [], "failed": [], "rate_limit_pauses": 0}


def save_progress(progress: dict) -> None:
    # 原子写：先写临时文件，再 rename
    tmp = PROGRESS_PATH.with_suffix(".tmp")
    tmp.write_text(json.dumps(progress, ensure_ascii=False, indent=2))
    tmp.replace(PROGRESS_PATH)


def process_one(city: str) -> tuple[int, str]:
    """处理单个城市。返回 (新增景点数, 错误信息)。"""
    results = search_bing(f"{city} 著名景点", limit=5)
    if not results:
        return 0, "no_search_results"

    names = extract_pois(city, results)
    if not names:
        return 0, "extract_empty"

    inserted = 0
    for name in names:
        if insert_poi(city, name):
            inserted += 1
    return inserted, None


def is_rate_limit_error(e: Exception) -> bool:
    """检查是否是速率/限额错误。"""
    if isinstance(e, (RateLimitError, APIStatusError)):
        return True
    msg = str(e).lower()
    return any(s in msg for s in [
        "rate limit", "too many requests", "429",
        "unauthorized", "invalid api key", "401",
        "sensitive", "422",  # 敏感词过滤
        "budget", "quota",
    ])


def handle_rate_limit(e: Exception, progress: dict, sleep_secs: int) -> None:
    """遇到限速：长 sleep，记录次数。"""
    progress["rate_limit_pauses"] = progress.get("rate_limit_pauses", 0) + 1
    progress["last_rate_limit_at"] = time.strftime("%Y-%m-%d %H:%M:%S")
    save_progress(progress)
    print(f"\n  ⚠ 限速错误: {type(e).__name__}: {str(e)[:100]}")
    print(f"  💤 暂停 {sleep_secs//60} 分钟...")
    save_progress(progress)
    time.sleep(sleep_secs)
    print(f"  ▶ 恢复运行")


def main(limit: int = 0, skip: int = 0) -> None:
    todo = get_cities_to_process(limit=limit, skip=skip)
    if not todo:
        print("🎉 全部完成！没有需要补全的城市")
        return

    progress = load_progress()
    completed = set(progress.get("completed", []))

    print(f"待处理城市: {len(todo)}")
    if completed:
        print(f"  之前已完成: {len(completed)}")
    if progress.get("rate_limit_pauses"):
        print(f"  历史限速暂停: {progress['rate_limit_pauses']} 次")
    print()

    success = 0
    failed = 0
    total_inserted = 0
    t0 = time.time()
    consecutive_errors = 0

    for i, city in enumerate(todo):
        if city in completed:
            print(f"  [{i+1}/{len(todo)}] ⏭  {city}: 已跳过（之前完成）", flush=True)
            continue

        print(f"  [{i+1}/{len(todo)}] {city}...", end=" ", flush=True)
        ok = False
        for retry in range(MAX_RETRIES_PER_CITY):
            try:
                inserted, err = process_one(city)
                if err:
                    print(f"✗ ({err})")
                    failed += 1
                    progress.setdefault("failed", []).append({"city": city, "reason": err})
                else:
                    print(f"✓ +{inserted}")
                    success += 1
                    total_inserted += inserted
                    completed.add(city)
                    progress["completed"] = list(completed)
                    consecutive_errors = 0
                save_progress(progress)
                ok = True
                break
            except Exception as e:
                consecutive_errors += 1
                if is_rate_limit_error(e):
                    # 限速错误：长 sleep，重试
                    sleep_secs = SLEEP_ON_AUTH_ERROR if "401" in str(e) or "unauthorized" in str(e).lower() else SLEEP_ON_RATE_LIMIT
                    handle_rate_limit(e, progress, sleep_secs)
                    if retry < MAX_RETRIES_PER_CITY - 1:
                        continue  # 重试
                else:
                    print(f"✗ ({type(e).__name__}: {str(e)[:100]})")
                failed += 1
                progress.setdefault("failed", []).append({
                    "city": city,
                    "reason": f"{type(e).__name__}: {str(e)[:100]}",
                })
                save_progress(progress)
                break

        if not ok and consecutive_errors >= 5:
            # 连续 5 个错误，主动长 sleep 后继续
            print(f"\n  ⚠ 连续 {consecutive_errors} 个错误，主动暂停 10 分钟")
            time.sleep(600)
            consecutive_errors = 0

        time.sleep(SLEEP_BETWEEN_REQUESTS)

        # 每 30 个休息
        if (i + 1) % 30 == 0:
            print(f"  --- 已 {i+1}/{len(todo)}，休息 20 秒 ---")
            time.sleep(20)

    elapsed = time.time() - t0
    print()
    print(f"=== 完成 ===")
    print(f"  本轮成功: {success}, 失败: {failed}")
    print(f"  本轮新增: {total_inserted} 个景点")
    print(f"  累计完成: {len(completed)} 城")
    print(f"  用时: {elapsed/60:.1f} 分钟")
    if progress.get("rate_limit_pauses"):
        print(f"  限速暂停: {progress['rate_limit_pauses']} 次")
    if progress.get("failed"):
        print(f"  失败列表: {len(progress['failed'])} 城")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--limit", type=int, default=0)
    parser.add_argument("--skip", type=int, default=0)
    args = parser.parse_args()
    try:
        main(limit=args.limit, skip=args.skip)
    except KeyboardInterrupt:
        print("\n\n⏸  被中断，进度已保存")
        print(f"  重跑: python src/batch_backfill_pois.py")
