"""
一次性脚本：用 LLM（OpenAIJsonWrapper）批量生成 city_features。

策略：
  - 按城市一个一个跑
  - 输入：城市名 + 省份 + 经纬度 + 景点列表（前 10）
  - 输出：blurb / best_seasons / suitable_for / tags / avoid / trip_capacity
  - 跳过 < 3 景点的城市
  - 断点续跑
  - 限速感知

用法：
  python src/batch_backfill_city_features.py [--limit N] [--skip N]
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
PROGRESS_PATH = DATA_DIR / "city_features_progress.json"

MIN_POIS_PER_CITY = 3
SLEEP_BETWEEN_REQUESTS = 2.0
SLEEP_ON_RATE_LIMIT = 600  # 10 分钟


def ensure_table():
    conn = sqlite3.connect(str(DB_PATH))
    conn.execute("""
        CREATE TABLE IF NOT EXISTS city_features (
            city TEXT PRIMARY KEY,
            blurb TEXT,
            best_seasons TEXT,
            suitable_for TEXT,
            tags TEXT,
            avoid TEXT,
            trip_capacity TEXT,
            generated_at TEXT
        )
    """)
    conn.commit()
    conn.close()


def get_target_cities(limit: int = 0, skip: int = 0) -> list[dict]:
    """获取要处理的城市列表（缺 city_features 的）。"""
    ensure_table()
    conn = sqlite3.connect(str(DB_PATH))
    rows = conn.execute("""
        SELECT g.name, g.province_name, g.lat, g.lon,
               (SELECT COUNT(*) FROM attractions a WHERE a.city=g.name) as n_pois
        FROM geo_cities g
        WHERE g.lat IS NOT NULL
          AND NOT EXISTS (SELECT 1 FROM city_features c WHERE c.city=g.name)
        ORDER BY n_pois DESC, g.name
    """).fetchall()
    conn.close()

    cities = [dict(name=r[0], province=r[1], lat=r[2], lon=r[3], n_pois=r[4])
             for r in rows if r[4] >= MIN_POIS_PER_CITY]

    if skip:
        cities = cities[skip:]
    if limit:
        cities = cities[:limit]
    return cities


def get_poi_sample(city: str, limit: int = 10) -> list[tuple]:
    conn = sqlite3.connect(str(DB_PATH))
    rows = conn.execute("""
        SELECT name, category FROM attractions
        WHERE city=? AND category IS NOT NULL
        ORDER BY id LIMIT ?
    """, (city, limit)).fetchall()
    conn.close()
    return rows


def generate_for_city(client: OpenAI, wrapper: OpenAIJsonWrapper, city_info: dict) -> dict | None:
    """用 LLM 生成单城市的 features。"""
    pois = get_poi_sample(city_info["name"], limit=15)
    pois_text = "\n".join(f"- {name} ({cat})" for name, cat in pois)

    target_structure = {
        "blurb": "一句话描述 30-50 字，要有画面感不堆砌",
        "best_seasons": ["春季", "秋季"],
        "suitable_for": ["亲子", "美食", "文化"],
        "tags": ["海滨", "古都", "山水"],
        "avoid": "避开 X 月（气候原因），或空字符串",
        "trip_capacity": "S",
    }
    background = (
        f"你是中文旅游作家。{city_info['name']}（{city_info['province']}）"
        f"位于东经 {city_info['lon']:.2f}, 北纬 {city_info['lat']:.2f}。"
        f"该城市有 {city_info['n_pois']} 个景点（最少 1 个，最多 5-7 天能玩完算 L）。"
    )
    requirements = [
        "blurb 30-50 字、有画面感、不堆砌形容词",
        "best_seasons 必须是 ['春季','夏季','秋季','冬季'] 的子集",
        "suitable_for 从 [亲子,情侣,朋友,家庭,户外,文化,美食,摄影,独自] 选 2-4 个",
        "tags 从 [海滨,古都,边塞,山水,古镇,少数民族,温泉,边境,沙漠,草原,雪景,夜景,网红,宗教,自然,人文,都市,美食] 选 2-4 个",
        "trip_capacity: 根据景点数判断：< 8 景点=S(1-2天)，8-19=M(3-4天)，>= 20=L(5天+)；直辖市/特别行政区/省会城市至少 M",
        "避免 trip_capacity: 一定不能错（香港、澳门、北京必须 L 或 M）",
        "avoid 是气候/季节相关警告，没有就空字符串",
        "严格 JSON 输出",
    ]

    prompt = f"""为【{city_info['name']}】生成旅游特征。

主要景点：
{pois_text}

输出严格 JSON。"""

    try:
        resp = wrapper.chat(messages=[{"role": "user", "content": prompt}])
    except RateLimitError:
        raise
    except APIStatusError as e:
        if e.status_code == 429:
            raise RateLimitError(...)
        return None
    except Exception as e:
        print(f"      LLM 错误: {type(e).__name__}: {str(e)[:80]}")
        return None

    if resp.get("error"):
        msg = resp["error"]
        if "429" in msg or "rate limit" in msg.lower():
            raise RateLimitError(message=msg, body=msg, request=None)
        print(f"      wrapper 错误: {msg[:100]}")
        return None

    data = resp.get("data") or {}

    # 验证 + 清洗
    if not isinstance(data.get("blurb"), str) or len(data["blurb"]) < 5:
        return None

    def clean_list(v, allowed):
        if not isinstance(v, list):
            return []
        return [str(x) for x in v if x in allowed][:5]

    def ensure_seasons(v):
        allowed = ["春季", "夏季", "秋季", "冬季"]
        if not isinstance(v, list):
            return ["春季", "秋季"]
        return [x for x in v if x in allowed][:3] or ["春季", "秋季"]

    def ensure_capacity(v):
        return v if v in ("S", "M", "L") else "M"

    # 启发式 fallback：保证 tags 不空 + capacity 合理
    n_pois = city_info["n_pois"]
    if n_pois >= 20:
        fallback_capacity = "L"
    elif n_pois >= 8:
        fallback_capacity = "M"
    else:
        fallback_capacity = "S"

    # 直辖市/省会/特别行政区 fallback 至少 M
    big_cities = {"北京", "上海", "天津", "重庆", "香港特别行政区", "澳门特别行政区"}
    if city_info["name"] in big_cities and fallback_capacity == "S":
        fallback_capacity = "M"

    cap = ensure_capacity(data.get("trip_capacity"))
    # 强制升级
    if cap == "S" and fallback_capacity in ("M", "L"):
        cap = fallback_capacity

    # tags 兜底：从景点 category 聚合
    cats = [cat for _, cat in pois]
    cat_to_tag = {
        "古迹": "人文", "博物馆": "人文", "山岳": "山水", "园林": "人文",
        "公园": "自然", "观景": "自然", "寺庙": "宗教", "海滨": "海滨",
        "古镇": "古镇", "现代": "都市", "美食街": "美食",
    }
    fallback_tags = list(set(cat_to_tag.get(c, "") for c in cats if c in cat_to_tag))
    fallback_tags = [t for t in fallback_tags if t][:3]

    final_tags = clean_list(data.get("tags"),
        {"海滨", "古都", "边塞", "山水", "古镇", "少数民族", "温泉", "边境",
         "沙漠", "草原", "雪景", "夜景", "网红", "宗教", "自然", "人文", "都市", "美食"})
    if not final_tags:
        final_tags = fallback_tags

    return {
        "blurb": data["blurb"][:100],
        "best_seasons": json.dumps(ensure_seasons(data.get("best_seasons")), ensure_ascii=False),
        "suitable_for": json.dumps(clean_list(data.get("suitable_for"),
            {"亲子", "情侣", "朋友", "家庭", "户外", "文化", "美食", "摄影", "独自"}), ensure_ascii=False),
        "tags": json.dumps(final_tags, ensure_ascii=False),
        "avoid": (data.get("avoid") or "")[:50] if isinstance(data.get("avoid"), str) else "",
        "trip_capacity": cap,
    }


def write(city: str, feat: dict):
    conn = sqlite3.connect(str(DB_PATH))
    conn.execute(
        """INSERT OR REPLACE INTO city_features
           (city, blurb, best_seasons, suitable_for, tags, avoid, trip_capacity, generated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))""",
        (city, feat["blurb"], feat["best_seasons"], feat["suitable_for"],
         feat["tags"], feat["avoid"], feat["trip_capacity"]),
    )
    conn.commit()
    conn.close()


def load_progress() -> dict:
    if PROGRESS_PATH.exists():
        return json.loads(PROGRESS_PATH.read_text())
    return {"completed": [], "rate_limit_pauses": 0}


def save_progress(progress: dict):
    tmp = PROGRESS_PATH.with_suffix(".tmp")
    tmp.write_text(json.dumps(progress, ensure_ascii=False, indent=2))
    tmp.replace(PROGRESS_PATH)


def main(limit: int = 0, skip: int = 0):
    ensure_table()
    todo = get_target_cities(limit=limit, skip=skip)
    if not todo:
        print("🎉 全部完成！")
        return

    progress = load_progress()
    completed = set(progress.get("completed", []))

    print(f"待处理城市: {len(todo)}")
    if completed:
        print(f"  之前已完成: {len(completed)}")

    client = OpenAI(api_key=API_KEY, base_url=BASE_URL)
    wrapper = OpenAIJsonWrapper(
        client, model=MODEL_NAME,
        target_structure={
            "blurb": "string",
            "best_seasons": ["春季"],
            "suitable_for": ["亲子"],
            "tags": ["海滨"],
            "avoid": "string",
            "trip_capacity": "S",
        },
        background="生成城市旅游特征",
        requirements=["严格 JSON"],
    )

    success = 0
    failed = 0
    t0 = time.time()
    consecutive_errors = 0

    for i, city_info in enumerate(todo):
        name = city_info["name"]
        if name in completed:
            continue

        print(f"  [{i+1}/{len(todo)}] {name}...", end=" ", flush=True)
        try:
            feat = generate_for_city(client, wrapper, city_info)
        except RateLimitError as e:
            count = progress.get("rate_limit_pauses", 0) + 1
            progress["rate_limit_pauses"] = count
            progress["last_rate_limit_at"] = time.strftime("%Y-%m-%d %H:%M:%S")
            save_progress(progress)
            print(f"⚠ 限速 #{count}")
            print(f"  💤 sleep {SLEEP_ON_RATE_LIMIT//60}min")
            time.sleep(SLEEP_ON_RATE_LIMIT)
            consecutive_errors = 0
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
            continue

        if feat is None:
            print("✗ (LLM 失败)")
            failed += 1
            time.sleep(SLEEP_BETWEEN_REQUESTS)
            continue

        try:
            write(name, feat)
            completed.add(name)
            progress["completed"] = list(completed)
            save_progress(progress)
            blurb_short = feat["blurb"][:25]
            print(f"✓ {blurb_short}...")
            success += 1
            consecutive_errors = 0
        except Exception as e:
            print(f"✗ (DB: {e})")
            failed += 1

        time.sleep(SLEEP_BETWEEN_REQUESTS)

        # 每 30 个休息 15s
        if (i + 1) % 30 == 0:
            print(f"  --- 休息 15s ---")
            time.sleep(15)

    elapsed = time.time() - t0
    print()
    print(f"=== 完成 ===")
    print(f"  本轮成功: {success}, 失败: {failed}")
    print(f"  用时: {elapsed/60:.1f} 分钟")
    if progress.get("rate_limit_pauses"):
        print(f"  限速暂停: {progress['rate_limit_pauses']} 次")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--limit", type=int, default=0)
    parser.add_argument("--skip", type=int, default=0)
    args = parser.parse_args()
    try:
        main(limit=args.limit, skip=args.skip)
    except KeyboardInterrupt:
        print("\n⏸ 中断，进度已保存")
