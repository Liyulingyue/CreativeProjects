"""
一次性脚本：用 LLM 批量生成 city_guides（静态攻略表）。

策略：
  - 每个种子城市 × 1-5 天 × 3 风格 (standard/family/budget)
  - 输入: 城市特征 + 主要景点
  - 输出: 详细行程 (餐/交通/预算/无日期)

用法:
  python src/batch_backfill_city_guides.py [--limit N] [--skip N] [--only-style standard]
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
PROGRESS_PATH = DATA_DIR / "city_guides_progress.json"

MIN_POIS_PER_CITY = 3
DURATIONS = [1, 2, 3, 4, 5]
STYLES = ["standard", "family", "budget"]

SLEEP_BETWEEN_REQUESTS = 2.0
SLEEP_ON_LIMIT_RETRY = 60  # 1 分钟（重试 3 次）


def ensure_table():
    conn = sqlite3.connect(str(DB_PATH))
    conn.execute("""
        CREATE TABLE IF NOT EXISTS city_guides (
            city TEXT NOT NULL,
            duration INTEGER NOT NULL,
            style TEXT NOT NULL,
            guide_json TEXT NOT NULL,
            updated_at TEXT,
            PRIMARY KEY (city, duration, style)
        )
    """)
    conn.execute("CREATE INDEX IF NOT EXISTS idx_guides_city ON city_guides(city)")
    conn.commit()
    conn.close()


def get_seed_cities() -> list[str]:
    """从 seed_config 读取种子城市（标准化为完整地级市名）。"""
    conn = sqlite3.connect(str(DB_PATH))
    rows = conn.execute(
        "SELECT value FROM seed_config WHERE key='cities'"
    ).fetchone()
    conn.close()
    cities = []
    if rows:
        try:
            cities = json.loads(rows[0])
        except Exception:
            cities = []

    # 标准化：seed_config 里可能是 "济南" (无后缀)，
    # 需匹配 geo_cities 里的 "济南市" (有后缀)。
    # 策略：先原名匹配，再尝试加后缀。
    geo_names = {r[0] for r in sqlite3.connect(str(DB_PATH)).execute(
        "SELECT name FROM geo_cities"
    )}
    SUFFIXES = ['市', '自治州', '白族自治州', '地区', '盟']

    normalized = []
    for c2 in cities:
        if c2 in geo_names:
            normalized.append(c2)
            continue
        # 尝试加后缀
        matched = None
        for s in SUFFIXES:
            candidate = c2 + s
            if candidate in geo_names:
                matched = candidate
                break
        if matched:
            normalized.append(matched)
    return normalized


def get_pois(city: str, limit: int = 20) -> list[tuple]:
    conn = sqlite3.connect(str(DB_PATH))
    rows = conn.execute("""
        SELECT name, category FROM attractions
        WHERE city=?
        ORDER BY id LIMIT ?
    """, (city, limit)).fetchall()
    conn.close()
    return rows


def get_features(city: str) -> dict | None:
    conn = sqlite3.connect(str(DB_PATH))
    row = conn.execute("""
        SELECT blurb, best_seasons, suitable_for, tags, trip_capacity
        FROM city_features WHERE city=?
    """, (city,)).fetchone()
    conn.close()
    if not row:
        return None
    return {
        "blurb": row[0],
        "best_seasons": json.loads(row[1]) if row[1] else [],
        "suitable_for": json.loads(row[2]) if row[2] else [],
        "tags": json.loads(row[3]) if row[3] else [],
        "trip_capacity": row[4],
    }


def build_prompt(city: str, duration: int, style: str, features: dict, pois: list) -> str:
    style_desc = {
        "standard": "主流均衡：覆盖主要景点+中等预算+交通便利",
        "family": "家庭友好：节奏慢+公园多+室内备选+童趣餐+推车友好",
        "budget": "穷游：公交+青旅+免费景点+街边小吃+最低花费",
    }[style]

    pois_text = "\n".join(f"- {n} ({c})" for n, c in pois)

    f = features or {}
    blurb = f.get("blurb", "") or ""
    tags = ", ".join(f.get("tags") or [])
    best_seasons = ", ".join(f.get("best_seasons") or [])
    avoid = f.get("avoid", "") or ""

    return f"""你是中文旅游作家，为【{city}】设计 {duration} 天 {style} 风格的详细攻略。

## 城市信息
- 简介: {blurb}
- 标签: {tags}
- 适合季节: {best_seasons}
- 避坑: {avoid}
- 风格定义: {style_desc}

## 主要景点（{len(pois)} 个）
{pois_text}

## 要求

输出严格 JSON：
{{
  "style": "{style}",
  "highlights": ["必去景点1", "必去景点2", ...],
  "daily_plan": [
    {{
      "day": 1,
      "theme": "当日主题",
      "schedule": [
        {{"time": "08:00", "spot": "景点名", "duration_min": 90, "note": "备注"}},
        ...
      ],
      "meals": {{
        "breakfast": {{"place": "...", "price": 30, "specialty": "..."}},
        "lunch": {{"place": "...", "price": 60, "specialty": "..."}},
        "dinner": {{"place": "...", "price": 100, "specialty": "..."}}
      }},
      "transport": "地铁/公交/打车",
      "accommodation": "住宿区域+价位"
    }},
    ...  // {duration} 天
  ],
  "total_budget": {{
    "transport": 200,
    "accommodation": 1200,
    "food": 600,
    "tickets": 300,
    "total": 2300,
    "currency": "CNY"
  }},
  "tips": ["实用建议 1", "实用建议 2", "..."]
}}

## 重要约束

- 严格基于提供的景点，不要编造
- 不含具体日期、天气、时政信息（这是静态攻略）
- daily_plan 数组长度必须等于 {duration}
- schedule 每天 3-5 个时段
- meals 每天必填三餐
- 价格合理：{style} 的预算范围必须合理
- 严格 JSON 输出，**不要**在前后加任何解释
"""


def parse_guide(llm_data: dict) -> dict | None:
    """校验 LLM 返回的 JSON 格式。"""
    if not isinstance(llm_data, dict):
        return None
    if not isinstance(llm_data.get("daily_plan"), list):
        return None
    if not llm_data["daily_plan"]:
        return None
    if not isinstance(llm_data.get("highlights"), list):
        return None
    if not isinstance(llm_data.get("tips"), list):
        return None
    return llm_data


def write_guide(city: str, duration: int, style: str, guide_data: dict):
    conn = sqlite3.connect(str(DB_PATH))
    conn.execute(
        """INSERT OR REPLACE INTO city_guides (city, duration, style, guide_json, updated_at)
           VALUES (?, ?, ?, ?, datetime('now'))""",
        (city, duration, style, json.dumps(guide_data, ensure_ascii=False)),
    )
    conn.commit()
    conn.close()


def load_progress() -> dict:
    if PROGRESS_PATH.exists():
        return json.loads(PROGRESS_PATH.read_text())
    return {"completed": [], "rate_limit_pauses": 0}


def save_progress(p: dict):
    tmp = PROGRESS_PATH.with_suffix(".tmp")
    tmp.write_text(json.dumps(p, ensure_ascii=False, indent=2))
    tmp.replace(PROGRESS_PATH)


def main(limit: int = 0, skip: int = 0, only_style: str = ""):
    ensure_table()
    cities = get_seed_cities()
    if not cities:
        print("❌ 没找到种子城市")
        return

    print(f"种子城市: {len(cities)}")
    print(f"每城 × {len(DURATIONS)} 天 × {len(STYLES)} 风格 = {len(cities)*len(DURATIONS)*len(STYLES)} 条")

    # 构建任务列表
    tasks = []
    for city in cities:
        for d in DURATIONS:
            for s in STYLES:
                if only_style and s != only_style:
                    continue
                tasks.append((city, d, s))

    if skip:
        tasks = tasks[skip:]
    if limit:
        tasks = tasks[:limit]

    print(f"本次跑: {len(tasks)} 条")

    progress = load_progress()
    completed = set(progress.get("completed", []))

    client = OpenAI(api_key=API_KEY, base_url=BASE_URL)
    wrapper = OpenAIJsonWrapper(
        client, model=MODEL_NAME,
        target_structure={
            "style": "standard",
            "highlights": ["A", "B"],
            "daily_plan": [
                {
                    "day": 1,
                    "theme": "...",
                    "schedule": [{"time": "08:00", "spot": "...", "duration_min": 90}],
                    "meals": {"breakfast": {"place": "..."}},
                    "transport": "...",
                    "accommodation": "...",
                }
            ],
            "total_budget": {"total": 1000, "currency": "CNY"},
            "tips": ["..."],
        },
        background="旅游攻略生成助手",
        requirements=["严格 JSON 输出", "无日期/天气"],
    )

    success = 0
    failed = 0
    consecutive_errors = 0
    t0 = time.time()

    for i, (city, duration, style) in enumerate(tasks):
        key = f"{city}|{duration}|{style}"
        if key in completed:
            continue

        print(f"  [{i+1}/{len(tasks)}] {city} {duration}d {style}...", end=" ", flush=True)
        try:
            features = get_features(city)
            pois = get_pois(city, limit=20)
            if len(pois) < MIN_POIS_PER_CITY:
                print("⊘ 跳过 (景点少)")
                completed.add(key)
                progress["completed"] = list(completed)
                save_progress(progress)
                continue

            prompt = build_prompt(city, duration, style, features, pois)
            # 限速重试：最多 3 次，指数退避
            for attempt in range(3):
                resp = wrapper.chat(messages=[{"role": "user", "content": prompt}])
                if not resp.get("error"):
                    break
                msg = resp["error"]
                is_limit = "429" in msg or "rate limit" in msg.lower() or "token" in msg.lower()
                if is_limit and attempt < 2:
                    wait = SLEEP_ON_LIMIT_RETRY * (2 ** attempt)  # 300, 600, 1200
                    print(f"⚠ 限速 (第{attempt+1}次)，等 {wait//60}min")
                    time.sleep(wait)
                    continue
                else:
                    print(f"✗ wrapper: {msg[:50]}")
                    failed += 1
                    time.sleep(SLEEP_BETWEEN_REQUESTS)
                    break
            else:
                print(f"✗ 3 次限速都失败，skip")
                failed += 1
                time.sleep(SLEEP_BETWEEN_REQUESTS)
                continue

            if resp.get("error"):
                continue  # 已经 fail + sleep 过了

            data = parse_guide(resp.get("data") or {})
            if data is None:
                print("✗ parse fail")
                failed += 1
                time.sleep(SLEEP_BETWEEN_REQUESTS)
                continue

            write_guide(city, duration, style, data)
            completed.add(key)
            progress["completed"] = list(completed)
            save_progress(progress)
            print("✓")
            success += 1
            consecutive_errors = 0

        except RateLimitError:
            # 限速 5h 后才重置。重试 3 次后永久跳过
            count = progress.get("rate_limit_pauses", 0) + 1
            progress["rate_limit_pauses"] = count
            progress["last_rate_limit_at"] = time.strftime("%Y-%m-%d %H:%M:%S")
            save_progress(progress)

            # wrapper 内 3 次重试已用完
            # 5h 重置: 每 30 分钟 wakeup 一次，看是否好了
            print(f"⚠ 限速 #{count} (task: {city} {duration}d {style})，5h 重置，跳过此任务")
            print(f"  累计限速 {count} 次；当前 task 标记失败")
            failed += 1
            # 不重试，直接跳过
            time.sleep(SLEEP_BETWEEN_REQUESTS)
            continue
        except Exception as e:
            consecutive_errors += 1
            print(f"✗ ({type(e).__name__})")
            failed += 1
            if consecutive_errors >= 3:
                print(f"  ⚠ 连续错误，sleep 30s")
                time.sleep(30)
                consecutive_errors = 0
        time.sleep(SLEEP_BETWEEN_REQUESTS)

        if (i + 1) % 20 == 0:
            print(f"  --- 休息 15s ---")
            time.sleep(15)

    elapsed = time.time() - t0
    print()
    print(f"=== 完成 ===")
    print(f"  成功: {success}, 失败: {failed}")
    print(f"  用时: {elapsed/60:.1f} 分钟")
    if progress.get("rate_limit_pauses"):
        print(f"  限速暂停: {progress['rate_limit_pauses']} 次")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--limit", type=int, default=0)
    parser.add_argument("--skip", type=int, default=0)
    parser.add_argument("--only-style", type=str, default="")
    args = parser.parse_args()
    try:
        main(limit=args.limit, skip=args.skip, only_style=args.only_style)
    except KeyboardInterrupt:
        print("\n⏸ 中断，进度已保存")
