"""
批量为 attractions 补充 category / suggested_hours / tags（OpenAIJsonWrapper 批量）。

策略：
  - 每 6 个景点一批
  - 用 input_metadata 类似方式持久化进度
  - token 限速感知（429 自动长 sleep）

usage:
  python src/batch_enrich_attractions.py [--limit N]
"""
import argparse
import json
import sqlite3
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from openai import RateLimitError, APIStatusError

from src.config import API_KEY, BASE_URL, MODEL_NAME
from openai import OpenAI
from openaijsonwrapper import OpenAIJsonWrapper


DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DB_PATH = DATA_DIR / "travelsites.db"
PROGRESS_PATH = DATA_DIR / "attractions_enrich_progress.json"

CATEGORY_OPTIONS = ["古迹", "博物馆", "山岳", "园林", "公园", "美食街", "寺庙", "海滨", "古镇", "现代", "观景", "其他"]

PROMPT = """为以下景点补充游览信息。

景点列表：
{pois_list}

输出严格 JSON：
{{
  "items": [
    {{"name": "故宫", "category": "古迹", "suggested_hours": 3, "tags": ["历史", "皇家建筑"]}},
    ...
  ]
}}

约束：
- category 必须是：{categories}
- suggested_hours 是数字（小时）：小型 1-2h，公园 2-3h，博物馆 2-4h，大型古迹 3-5h，山岳 4-8h
- tags 限 2-3 个，从：自然/人文/历史/宗教/登山/皇家/建筑/古镇/海岛/沙漠/草原/雪景/夜景/网红/亲子/拍照 中选
- 按景点列表顺序输出，name 必须完全一致"""


def get_pending_attractions(limit: int = 0) -> list[dict]:
    """获取还需要补充的景点（category NULL 或 suggested_hours NULL）。"""
    conn = sqlite3.connect(str(DB_PATH))
    rows = conn.execute("""
        SELECT id, name, city FROM attractions
        WHERE source='search_runtime' AND (category IS NULL OR suggested_hours IS NULL)
        ORDER BY id
        LIMIT ?
    """, (limit or 100000,)).fetchall()
    conn.close()
    return [{"id": r[0], "name": r[1], "city": r[2]} for r in rows]


def update_attraction(poi_id: int, category: str, suggested_hours: float, tags: list):
    conn = sqlite3.connect(str(DB_PATH))
    conn.execute(
        """UPDATE attractions SET
            category=?, suggested_hours=?, tags=?
           WHERE id=?""",
        (category, suggested_hours, json.dumps(tags, ensure_ascii=False), poi_id),
    )
    conn.commit()
    conn.close()


def load_progress() -> dict:
    if PROGRESS_PATH.exists():
        return json.loads(PROGRESS_PATH.read_text())
    return {"completed_ids": []}


def save_progress(progress: dict) -> None:
    tmp = PROGRESS_PATH.with_suffix(".tmp")
    tmp.write_text(json.dumps(progress, ensure_ascii=False, indent=2))
    tmp.replace(PROGRESS_PATH)


def process_batch(pois: list[dict], client: OpenAI, wrapper: OpenAIJsonWrapper) -> dict | None:
    """LLM 提炼一批景点。返回 name → 字段 dict，失败 None。"""
    pois_text = "\n".join(f"- {p['name']} ({p['city']})" for p in pois)

    target = {
        "items": [
            {"name": "故宫", "category": "古迹", "suggested_hours": 3, "tags": ["历史", "建筑"]}
        ]
    }
    background = "为景点补充游览参考信息"
    requirements = ["严格 JSON 格式", "按列表顺序输出"]

    try:
        resp = wrapper.chat(messages=[{
            "role": "user",
            "content": PROMPT.format(pois_list=pois_text, categories=CATEGORY_OPTIONS)
        }])
    except RateLimitError:
        raise  # 让外层处理 429
    except APIStatusError as e:
        if e.status_code == 429:
            raise RateLimitError(...) from e
        raise

    if resp.get("error"):
        msg = resp['error']
        # 429 错误通过 error 字段冒出来
        if "429" in msg or "rate limit" in msg.lower() or "quota" in msg.lower():
            raise RateLimitError(message=msg, body=msg, request=None)
        print(f"      wrapper err: {msg[:100]}")
        return None

    data = resp.get("data") or {}
    items = data.get("items", [])
    result = {}
    for item in items:
        if not isinstance(item, dict):
            continue
        name = str(item.get("name", "")).strip()
        if not name:
            continue
        try:
            hours = float(item.get("suggested_hours") or 2)
        except (TypeError, ValueError):
            hours = 2
        result[name] = {
            "category": str(item.get("category") or "其他").strip(),
            "suggested_hours": max(0.5, min(12, hours)),
            "tags": [str(t) for t in (item.get("tags") or [])][:3],
        }
    return result


def is_rate_limit_error(e: Exception) -> bool:
    if isinstance(e, RateLimitError):
        return True
    msg = str(e)
    return any(s in msg for s in ["429", "rate limit", "quota", "token", "budget"])


BATCH_SIZE = 6


def main(limit: int = 0) -> None:
    pending = get_pending_attractions(limit)
    print(f"待处理景点: {len(pending)}")

    progress = load_progress()
    done_ids = set(progress.get("completed_ids", []))
    pending = [p for p in pending if p["id"] not in done_ids]

    if not pending:
        print("🎉 全部完成")
        return

    print(f"  实际要跑: {len(pending)}")
    client = OpenAI(api_key=API_KEY, base_url=BASE_URL)
    wrapper = OpenAIJsonWrapper(
        client, model=MODEL_NAME,
        target_structure={
            "items": [{"name": "故宫", "category": "古迹", "suggested_hours": 3, "tags": [""]}]
        },
        background="为景点批量补全字段",
        requirements=["严格 JSON"],
    )

    success = 0
    failed = 0
    t0 = time.time()
    consecutive_errors = 0

    for i in range(0, len(pending), BATCH_SIZE):
        batch = pending[i:i+BATCH_SIZE]
        names = ", ".join(f"{p['name']}({p['city']})" for p in batch)
        print(f"  [{i//BATCH_SIZE+1}/{(len(pending)+BATCH_SIZE-1)//BATCH_SIZE}] 批 {len(batch)}: {names[:80]}", end=" ... ", flush=True)

        try:
            results = process_batch(batch, client, wrapper)
        except Exception as e:
            if is_rate_limit_error(e):
                sleep_secs = 600  # 10 分钟
                count = progress.get("rate_limit_pauses", 0) + 1
                progress["rate_limit_pauses"] = count
                progress["last_rate_limit_at"] = time.strftime("%Y-%m-%d %H:%M:%S")
                save_progress(progress)
                print(f"⚠ 限速 #{count}: {type(e).__name__}: {str(e)[:80]}")
                print(f"  💤 sleep {sleep_secs//60} 分钟（token 5h 重置）...")
                time.sleep(sleep_secs)
                consecutive_errors = 0
                continue
            consecutive_errors += 1
            if consecutive_errors >= 3:
                print(f"⚠ 连续错误，长 sleep 60s")
                time.sleep(60)
                consecutive_errors = 0
            failed += len(batch)
            continue

        if results is None:
            failed += len(batch)
            print("✗ LLM 失败")
            time.sleep(2)
            continue

        # 写库
        batch_done = 0
        for p in batch:
            r = results.get(p["name"])
            if r:
                update_attraction(p["id"], r["category"], r["suggested_hours"], r["tags"])
                done_ids.add(p["id"])
                batch_done += 1
        success += batch_done
        failed += len(batch) - batch_done

        progress["completed_ids"] = list(done_ids)
        save_progress(progress)

        print(f"✓ {batch_done}/{len(batch)}")
        time.sleep(2)  # 控制 QPS

        # 每 20 批休息
        if (i // BATCH_SIZE + 1) % 20 == 0:
            print(f"  --- 已 {i//BATCH_SIZE+1} 批，休息 15s ---")
            time.sleep(15)

    elapsed = time.time() - t0
    print()
    print(f"=== 完成 ===")
    print(f"  成功: {success}, 失败: {failed}")
    print(f"  用时: {elapsed/60:.1f} 分钟")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--limit", type=int, default=0)
    args = parser.parse_args()
    try:
        main(limit=args.limit)
    except KeyboardInterrupt:
        print("\n⏸  中断，进度已保存")
