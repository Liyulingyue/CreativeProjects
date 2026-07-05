"""
一次性脚本：用 LLM 批量生成 city_guides（静态攻略表）。

自适应并发策略：
  - 起步并发 1，最大 5
  - 连续成功 5 次 → 并发 +1
  - 连续失败 2 次 → 并发 -1
  - 调用失败 → 指数退避重试（2s → 4s → 8s → 16s → 64s 封顶）
  - 最大重试 5 次

用法:
  python src/batch_backfill_city_guides.py [--limit N] [--skip N] [--only-style standard]
"""
import argparse
import asyncio
import json
import sqlite3
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from openaijsonwrapper import OpenAIJsonWrapper

DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DB_PATH = DATA_DIR / "travelsites.db"
PROGRESS_PATH = DATA_DIR / "city_guides_progress.json"

MIN_POIS_PER_CITY = 3
DURATIONS = [1, 2, 3, 4, 5]
STYLES = ["standard", "family", "budget"]

INITIAL_CONCURRENCY = 1
MAX_CONCURRENCY = 5
MIN_CONCURRENCY = 1

SUCCESS_STREAK_UP = 5
FAIL_STREAK_DOWN = 2

INITIAL_BACKOFF = 2.0
MAX_BACKOFF = 64.0
MAX_RETRIES = 5


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


def get_all_cities_with_pois(min_pois: int = 3) -> list[str]:
    conn = sqlite3.connect(str(DB_PATH))
    rows = conn.execute("""
        SELECT g.name, COUNT(a.id) as n
        FROM geo_cities g
        JOIN attractions a ON a.city = g.name
        WHERE g.lat IS NOT NULL
        GROUP BY g.name
        HAVING n >= ?
        ORDER BY n DESC, g.name
    """, (min_pois,)).fetchall()
    conn.close()
    return [r[0] for r in rows]


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


MAX_CITY_FAILURES = 3


def load_progress() -> dict:
    if PROGRESS_PATH.exists():
        p = json.loads(PROGRESS_PATH.read_text())
        if "city_failures" not in p:
            p["city_failures"] = {}
        return p
    return {"completed": [], "consecutive_ok": 0, "consecutive_fail": 0, "city_failures": {}}


def save_progress(p: dict):
    tmp = PROGRESS_PATH.with_suffix(".tmp")
    tmp.write_text(json.dumps(p, ensure_ascii=False, indent=2))
    tmp.replace(PROGRESS_PATH)


async def _call_llm(wrapper, prompt: str) -> dict:
    """带指数退避重试的 LLM 调用。"""
    for attempt in range(MAX_RETRIES):
        try:
            loop = asyncio.get_event_loop()
            resp = await loop.run_in_executor(None, wrapper.chat, [{"role": "user", "content": prompt}])
            if not resp.get("error"):
                return resp
            msg = str(resp["error"])
            is_limit = any(k in msg.lower() for k in ["429", "rate limit", "rate_limit", "token", "quota"])
            if is_limit and attempt < MAX_RETRIES - 1:
                backoff = min(INITIAL_BACKOFF * (2 ** attempt), MAX_BACKOFF)
                print(f"  ⚠ rate limit 等 {backoff:.0f}s (attempt {attempt+1}/{MAX_RETRIES})", flush=True)
                await asyncio.sleep(backoff)
                continue
            return resp
        except Exception as e:
            if attempt < MAX_RETRIES - 1:
                backoff = min(INITIAL_BACKOFF * (2 ** attempt), MAX_BACKOFF)
                print(f"  ⚠ {type(e).__name__} 等 {backoff:.0f}s (attempt {attempt+1}/{MAX_RETRIES})", flush=True)
                await asyncio.sleep(backoff)
                continue
            return {"error": str(e)}
    return {"error": f"max retries ({MAX_RETRIES}) exceeded"}


async def process_task(wrapper, city: str, duration: int, style: str, progress: dict) -> bool:
    """处理单个任务，返回是否成功。"""
    features = get_features(city)
    pois = get_pois(city, limit=20)
    if len(pois) < MIN_POIS_PER_CITY:
        return True

    prompt = build_prompt(city, duration, style, features, pois)
    resp = await _call_llm(wrapper, prompt)

    if resp.get("error"):
        print(f"✗ {resp['error'][:60]}", flush=True)
        return False

    data = parse_guide(resp.get("data") or {})
    if data is None:
        print(f"✗ parse fail", flush=True)
        return False

    write_guide(city, duration, style, data)
    return True


class AdaptiveConcurrency:
    def __init__(self, initial: int = INITIAL_CONCURRENCY, max_c: int = MAX_CONCURRENCY):
        self.current = initial
        self.max_c = max_c
        self.success_streak = 0
        self.fail_streak = 0
        self._lock = asyncio.Lock()
        self._cond = asyncio.Condition(self._lock)
        self._running = 0

    def record(self, ok: bool):
        if ok:
            self.success_streak += 1
            self.fail_streak = 0
            if self.success_streak >= SUCCESS_STREAK_UP and self.current < self.max_c:
                self.current = min(self.current + 1, self.max_c)
                self.success_streak = 0
                print(f"\n🚀 并发升到 {self.current}（连续成功 {SUCCESS_STREAK_UP} 次）", flush=True)
        else:
            self.fail_streak += 1
            self.success_streak = 0
            if self.fail_streak >= FAIL_STREAK_DOWN and self.current > MIN_CONCURRENCY:
                self.current = max(self.current - 1, MIN_CONCURRENCY)
                self.fail_streak = 0
                print(f"\n📉 并发降到 {self.current}（连续失败 {FAIL_STREAK_DOWN} 次）", flush=True)

    async def acquire(self):
        async with self._cond:
            while self._running >= self.current:
                await self._cond.wait()
            self._running += 1

    async def release(self):
        async with self._cond:
            self._running -= 1
            self._cond.notify_all()


async def run_tasks(tasks: list, wrapper, concurrency: AdaptiveConcurrency, progress: dict):
    completed = set(progress.get("completed", []))
    total = len(tasks)
    done = 0
    success = 0
    failed = 0
    t0 = time.time()

    queue = asyncio.Queue()
    city_failures = progress.get("city_failures", {})
    for task in tasks:
        if city_failures.get(task[0], 0) >= MAX_CITY_FAILURES:
            continue
        await queue.put(task)

    num_workers = concurrency.max_c

    async def worker(wid: int):
        nonlocal done, success, failed
        while True:
            await concurrency.acquire()
            task = None
            try:
                try:
                    task = queue.get_nowait()
                except asyncio.QueueEmpty:
                    await concurrency.release()
                    break
            except Exception as e:
                await concurrency.release()
                raise

            if task is None:
                await concurrency.release()
                continue

            city, duration, style = task
            key = f"{city}|{duration}|{style}"

            if key in completed:
                await concurrency.release()
                continue

            city_failures = progress.get("city_failures", {})
            if city_failures.get(city, 0) >= MAX_CITY_FAILURES:
                await concurrency.release()
                print(f"🚫 {city} 失败{MAX_CITY_FAILURES}次，跳过", flush=True)
                continue

            print(f"[W{wid}] {city} {duration}d {style}...", end=" ", flush=True)
            ok = await process_task(wrapper, city, duration, style, progress)

            if ok:
                concurrency.record(True)
                completed.add(key)
                progress["completed"] = list(completed)
                if city in progress.get("city_failures", {}):
                    progress["city_failures"][city] = 0
                save_progress(progress)
                print("✓", flush=True)
                success += 1
            else:
                concurrency.record(False)
                city_failures = progress.get("city_failures", {})
                city_failures[city] = city_failures.get(city, 0) + 1
                progress["city_failures"] = city_failures
                if city_failures[city] >= MAX_CITY_FAILURES:
                    print(f"🚫 {city} 失败{city_failures[city]}次，跳过", flush=True)
                save_progress(progress)
                print("✗", flush=True)
                failed += 1

            await concurrency.release()
            done += 1
            if done % 20 == 0:
                elapsed = time.time() - t0
                rate = done / elapsed * 3600
                print(f"\n📊 {done}/{total} ({done*100/total:.0f}%) | 成:{success} 败:{failed} | {rate:.0f}/h | 并发:{concurrency.current}", flush=True)

    workers = [asyncio.create_task(worker(i)) for i in range(num_workers)]
    await asyncio.gather(*workers)

    elapsed = time.time() - t0
    print(f"\n=== 完成 ===")
    print(f"  成功: {success}, 失败: {failed}")
    print(f"  用时: {elapsed/60:.1f} 分钟")
    print(f"  速度: {success / elapsed * 3600:.0f} 条/小时")
    return progress


def main(limit: int = 0, skip: int = 0, only_style: str = ""):
    from src.config import API_KEY, BASE_URL, MODEL_NAME
    from openai import OpenAI

    ensure_table()
    cities = get_all_cities_with_pois(min_pois=3)
    if not cities:
        print("❌ 没找到有 POI 的城市")
        return

    print(f"城市: {len(cities)}")
    print(f"每城 × {len(DURATIONS)} 天 × {len(STYLES)} 风格 = {len(cities)*len(DURATIONS)*len(STYLES)} 条")

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
    print(f"已跳过 {len(progress.get('completed', []))} 条历史完成任务")

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

    conc = AdaptiveConcurrency(initial=INITIAL_CONCURRENCY, max_c=MAX_CONCURRENCY)
    asyncio.run(run_tasks(tasks, wrapper, conc, progress))


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
