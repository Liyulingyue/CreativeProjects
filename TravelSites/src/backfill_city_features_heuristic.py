"""
一次性脚本：纯启发式生成 city_features（无 LLM）。

- trip_capacity: 从景点数推断（S/M/L）
- tags: 从景点 category 聚合
- best_seasons: 从 Open-Meteo 历史天气均值推断
- blurb: 用模板拼接

用法：
  python src/backfill_city_features_heuristic.py [--dry-run] [--limit 10]
"""
import argparse
import json
import sqlite3
import time
from collections import Counter
from pathlib import Path

import httpx

DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DB_PATH = DATA_DIR / "travelsites.db"

OPEN_METEO_HISTORY = "https://archive-api.open-meteo.com/v1/archive"

TAG_KEYWORDS = {
    "海滨": ["岛", "海", "湾", "港", "滨", "州"],
    "古都": ["京", "西安", "洛阳", "开封", "南京", "北京"],
    "边塞": ["嘉峪关", "敦煌", "塞", "北", "疆"],
    "山水": ["山", "水", "峡", "峰", "溪", "林", "谷"],
    "古镇": ["镇", "古城", "村", "寨"],
    "少数民族": ["蒙古", "藏", "维吾尔", "苗", "彝", "傣", "侗"],
    "温泉": ["温泉"],
    "边境": ["满洲里", "二连", "丹东", "瑞丽", "河口"],
    "沙漠": ["沙", "库布齐", "塔克拉玛干"],
    "草原": ["草原", "锡林郭勒", "呼伦贝尔"],
}


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


def get_city_stats() -> dict[str, dict]:
    conn = sqlite3.connect(str(DB_PATH))
    cur = conn.cursor()
    stats = {}
    for row in cur.execute(
        "SELECT city, category FROM attractions WHERE source='osm' OR source='seed' OR source='amap'"
    ).fetchall():
        city, cat = row[0], row[1]
        if city not in stats:
            stats[city] = {"count": 0, "cats": Counter()}
        stats[city]["count"] += 1
        stats[city]["cats"][cat] += 1
    conn.close()
    return stats


def infer_capacity(n_attractions: int) -> str:
    if n_attractions >= 20:
        return "L"
    if n_attractions >= 8:
        return "M"
    return "S"


def infer_tags(city: str, cats: Counter) -> list[str]:
    """从城市名关键词 + 景点 category 推断标签。"""
    tags = []
    for tag, kws in TAG_KEYWORDS.items():
        if any(kw in city for kw in kws):
            tags.append(tag)
    # category → tag
    cat_tag_map = {
        "博物馆": "文化",
        "古迹": "人文",
        "公园": "自然",
        "园林": "山水",
        "观景": "自然",
    }
    for cat, n in cats.most_common(3):
        if cat in cat_tag_map and cat_tag_map[cat] not in tags:
            tags.append(cat_tag_map[cat])
    if not tags:
        tags.append("人文")
    return tags[:4]


def infer_suitable_for(tags: list[str]) -> list[str]:
    """根据 tags 推适合人群。"""
    out = []
    if "古镇" in tags or "少数民族" in tags:
        out.append("摄影")
    if "山水" in tags or "海滨" in tags:
        out.append("家庭")
    if "古都" in tags or "文化" in tags:
        out.append("文化")
    if "草原" in tags or "沙漠" in tags:
        out.append("户外")
    if not out:
        out.append("朋友")
    return out[:3]


def fetch_climate(lat: float, lon: float) -> dict | None:
    """Open-Meteo 历史天气：近 5 年月度均值。"""
    try:
        end_year = 2025
        start_year = 2021
        with httpx.Client(timeout=30.0) as client:
            resp = client.get(
                OPEN_METEO_HISTORY,
                params={
                    "latitude": lat,
                    "longitude": lon,
                    "start_date": f"{start_year}-01-01",
                    "end_date": f"{end_year}-12-31",
                    "daily": "temperature_2m_max,temperature_2m_min,precipitation_sum",
                    "timezone": "auto",
                },
            )
            resp.raise_for_status()
            return resp.json()
    except Exception:
        return None


def infer_seasons(lat: float, lon: float) -> tuple[list[str], str]:
    """返回 (适合季节, 避开提示)。"""
    data = fetch_climate(lat, lon)
    if not data:
        return ["春季", "秋季"], ""
    daily = data.get("daily", {})
    dates = daily.get("time", [])
    tmax = daily.get("temperature_2m_max", [])
    tmin = daily.get("temperature_2m_min", [])
    precip = daily.get("precipitation_sum", [])

    # 按月聚合
    monthly_t = {m: [] for m in range(1, 13)}
    monthly_p = {m: [] for m in range(1, 13)}
    for i, d in enumerate(dates):
        try:
            m = int(d.split("-")[1])
        except (ValueError, IndexError):
            continue
        if i < len(tmax) and tmax[i] is not None:
            monthly_t[m].append(tmax[i])
        if i < len(precip) and precip[i] is not None:
            monthly_p[m].append(precip[i])

    # 评分：温度 18-26 度 + 降水少 = 高分
    scores = {}
    for m, ts in monthly_t.items():
        if not ts:
            scores[m] = 0
            continue
        avg_t = sum(ts) / len(ts)
        avg_p = sum(monthly_p.get(m, [0])) / max(1, len(monthly_p.get(m, [1])))
        # 温度舒适度（18-26 满分）
        if 18 <= avg_t <= 26:
            t_score = 100
        elif 10 <= avg_t <= 30:
            t_score = 70
        else:
            t_score = 30
        # 降水越少越好
        p_score = max(0, 100 - avg_p * 4)
        scores[m] = int(t_score * 0.6 + p_score * 0.4)

    # 选 top 3 月份
    sorted_m = sorted(scores.items(), key=lambda x: -x[1])
    best_m = [m for m, _ in sorted_m[:5] if scores[m] >= 60]
    if not best_m:
        best_m = [m for m, _ in sorted_m[:3]]

    season_map = {3: "春季", 4: "春季", 5: "春季", 6: "夏季", 7: "夏季", 8: "夏季",
                  9: "秋季", 10: "秋季", 11: "秋季", 12: "冬季", 1: "冬季", 2: "冬季"}
    best_seasons = sorted(set(season_map[m] for m in best_m))

    # 避开：月评分 < 40 的月份
    avoid = ""
    worst_m = [m for m, s in scores.items() if s < 40]
    if worst_m:
        worst_seasons = sorted(set(season_map[m] for m in worst_m))
        avoid = f"避开 {','.join(worst_seasons)}（气候不适宜）"

    return best_seasons, avoid


def generate_blurb(city: str, tags: list[str], capacity: str) -> str:
    """模板化 blurb，无 LLM。"""
    tag_desc = {
        "海滨": "海岸风光旖旎",
        "古都": "历史底蕴深厚",
        "边塞": "塞外风光壮阔",
        "山水": "山明水秀",
        "古镇": "古韵悠长",
        "少数民族": "民族风情浓郁",
        "温泉": "温泉资源丰富",
        "边境": "边境风情独特",
        "沙漠": "大漠孤烟",
        "草原": "草原辽阔",
        "文化": "文化气息浓郁",
        "人文": "人文景观荟萃",
        "自然": "自然风光秀丽",
    }
    parts = [city, "，"]
    seen = set()
    for t in tags:
        if t in tag_desc and tag_desc[t] not in seen:
            seen.add(tag_desc[t])
            parts.append(tag_desc[t])
            parts.append("、")
    if parts[-1] == "、":
        parts.pop()
    cap_desc = {"L": "值得深度游", "M": "适合小住几日", "S": "周末即够"}[capacity]
    parts.append(f"，{cap_desc}。")
    return "".join(parts)


def process_city(city: str, lat: float, lon: float, stats: dict, dry_run: bool) -> bool:
    city_stats = stats.get(city, {"count": 0, "cats": Counter()})
    capacity = infer_capacity(city_stats["count"])
    tags = infer_tags(city, city_stats["cats"])
    suitable = infer_suitable_for(tags)
    seasons, avoid = infer_seasons(lat, lon)
    blurb = generate_blurb(city, tags, capacity)

    if dry_run:
        print(f"  {city} [{capacity}] {tags} {seasons} | {blurb}")
        return True

    conn = sqlite3.connect(str(DB_PATH))
    conn.execute(
        """INSERT OR REPLACE INTO city_features
           (city, blurb, best_seasons, suitable_for, tags, avoid, trip_capacity, generated_at)
           VALUES (?,?,?,?,?,?,?,datetime('now'))""",
        (
            city,
            blurb,
            json.dumps(seasons, ensure_ascii=False),
            json.dumps(suitable, ensure_ascii=False),
            json.dumps(tags, ensure_ascii=False),
            avoid,
            capacity,
        ),
    )
    conn.commit()
    conn.close()
    return True


def main(dry_run: bool = False, limit: int = 0) -> None:
    ensure_table()
    stats = get_city_stats()
    conn = sqlite3.connect(str(DB_PATH))
    cities = conn.execute(
        "SELECT name, lat, lon FROM geo_cities WHERE lat IS NOT NULL ORDER BY name"
    ).fetchall()
    conn.close()
    if limit:
        cities = cities[:limit]
    print(f"待处理: {len(cities)} 个城市")

    success = 0
    for i, (city, lat, lon) in enumerate(cities):
        ok = process_city(city, lat, lon, stats, dry_run)
        if ok:
            success += 1
        if (i + 1) % 20 == 0:
            print(f"  [{i+1}/{len(cities)}] 成功 {success}")
        time.sleep(0.5)  # Open-Meteo 礼貌限流

    print()
    print(f"完成: 成功 {success}/{len(cities)}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--limit", type=int, default=0)
    args = parser.parse_args()
    main(dry_run=args.dry_run, limit=args.limit)
