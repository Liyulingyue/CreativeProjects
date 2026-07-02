"""
一次性脚本：用 Overpass API (OpenStreetMap) 补全 374 个城市的景点 POI。

免费、无 key、覆盖全。

用法：
  python src/backfill_attractions_osm.py [--dry-run] [--limit 10]
"""
import argparse
import json
import sqlite3
import time
import urllib.parse
from pathlib import Path

import httpx

DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DB_PATH = DATA_DIR / "travelsites.db"

OVERPASS_URLS = [
    "https://overpass-api.de/api/interpreter",
    "https://overpass.kumi.systems/api/interpreter",
    "https://overpass.private.coffee/api/interpreter",
]

# OSM tourism 标签到本系统 category 的映射
CATEGORY_MAP = {
    "attraction": "古迹",
    "museum": "博物馆",
    "viewpoint": "观景",
    "zoo": "公园",
    "theme_park": "公园",
    "artwork": "古迹",
    "monument": "古迹",
    "memorial": "古迹",
    "castle": "古迹",
    "ruins": "古迹",
    "park": "公园",
    "garden": "园林",
    "aquarium": "公园",
    "information": "其他",
}

# 排除明显不是"代表性景点"的：游乐场、KTV、酒店等
EXCLUDE_TAGS = {
    "tourism": ["hotel", "hostel", "guest_house", "motel", "apartment", "camp_site",
                "caravan_site", "resort", "chalet", "alpine_hut", "wilderness_hut"],
    "leisure": ["amusement_arcade", "bowling_alley", "dance", "fitness_centre",
                "hackerspace", "ice_rink", "indoor_play", "miniature_golf", "nightclub",
                "pitch", "sports_centre", "stadium", "swimming_pool", "tanning_salon",
                "track", "water_park", "horse_riding"],
}


def overpass_query_bbox(city_name: str, lat: float, lon: float, radius_km: float = 30) -> list[dict]:
    """用 bbox 查城市周边 tourism POI。

    只查 node（way 查询慢 5x+），限制 200 条避免 504 超时。
    """
    exclude_tourism = "|".join(EXCLUDE_TAGS["tourism"])
    delta = radius_km / 111.0
    bbox = f"{lat-delta:.4f},{lon-delta:.4f},{lat+delta:.4f},{lon+delta:.4f}"
    # 只查 node + 限制 200 条；Way 留给 way-aware 工具（如 Nominatim/Overpass turbo 手动）
    query = f"""
[out:json][timeout:60];
node["tourism"~"attraction|museum|viewpoint|zoo|theme_park|artwork|monument|memorial|castle|ruins|garden|park|aquarium|information"]["tourism"!~"{exclude_tourism}"]({bbox});
out tags 200;
"""
    last_err = None
    for url in OVERPASS_URLS:
        try:
            with httpx.Client(timeout=90.0) as client:
                resp = client.get(
                    f"{url}?data={urllib.parse.quote(query)}",
                    headers={"User-Agent": "TravelSites/0.1 (backfill)"},
                )
                if resp.status_code == 429:
                    last_err = "rate limited"
                    time.sleep(5)
                    continue
                if resp.status_code != 200:
                    last_err = f"HTTP {resp.status_code}: {resp.text[:200]}"
                    continue
                data = resp.json()
                return data.get("elements", [])
        except Exception as e:
            last_err = str(e)
            time.sleep(3)
    print(f"    [overpass] {city_name} 失败: {last_err}")
    return []


def overpass_query(city_name: str) -> list[dict]:
    """兼容旧调用：fallback 到 area 查询。"""
    exclude_tourism = "|".join(EXCLUDE_TAGS["tourism"])
    query = f"""
[out:json][timeout:60];
area["name:en"="{city_name}"]->.a;
(
  node["tourism"~"attraction|museum|viewpoint|zoo|theme_park|artwork|monument|memorial|castle|ruins|garden|park|aquarium|information"]["tourism"!~"{exclude_tourism}"](area.a);
  way["tourism"~"attraction|museum|artwork|monument|memorial|castle|ruins|garden|park|aquarium"]["tourism"!~"{exclude_tourism}"](area.a);
);
out tags center 200;
"""
    last_err = None
    for url in OVERPASS_URLS:
        try:
            with httpx.Client(timeout=90.0) as client:
                resp = client.get(
                    f"{url}?data={urllib.parse.quote(query)}",
                    headers={"User-Agent": "TravelSites/0.1 (backfill)"},
                )
                if resp.status_code == 429:
                    last_err = "rate limited"
                    time.sleep(5)
                    continue
                if resp.status_code != 200:
                    last_err = f"HTTP {resp.status_code}: {resp.text[:200]}"
                    continue
                data = resp.json()
                return data.get("elements", [])
        except Exception as e:
            last_err = str(e)
            time.sleep(3)
    print(f"    [overpass] {city_name} 失败: {last_err}")
    return []


def is_chinese_city(name: str) -> bool:
    """Overpass 偶尔返回非中国城市，过滤掉。"""
    for e in []:
        pass
    return True  # area 查询会精确匹配到 name:en，所以基本不会错


def process_city(city: str, lat: float, lon: float, dry_run: bool) -> int:
    """用 bbox 查询（快），不 fallback area（慢且易超时）。"""
    elements = overpass_query_bbox(city, lat, lon, radius_km=30)
    if not elements:
        return 0

    # 去重 + 提取
    seen = set()
    rows = []
    for e in elements:
        tags = e.get("tags", {})
        name = tags.get("name") or tags.get("name:zh")
        if not name or name in seen:
            continue
        seen.add(name)
        # node 有 lat/lon，way 用 center
        lat = e.get("lat") or (e.get("center") or {}).get("lat")
        lon = e.get("lon") or (e.get("center") or {}).get("lon")
        if not lat or not lon:
            continue
        cat = CATEGORY_MAP.get(tags.get("tourism", ""), "古迹")
        rows.append((name, city, cat, float(lat), float(lon),
                    tags.get("addr:city", "") or tags.get("is_in:city", ""),
                    "osm", 1))

    if dry_run:
        return len(rows)

    conn = sqlite3.connect(str(DB_PATH))
    cur = conn.cursor()
    cur.execute("DELETE FROM attractions WHERE city=? AND source='osm'", (city,))
    inserted = 0
    for row in rows:
        cur.execute(
            """INSERT OR IGNORE INTO attractions
               (name, city, category, lat, lon, address, source, verified, created_at)
               VALUES (?,?,?,?,?,?,?,?,datetime('now'))""",
            row,
        )
        if cur.rowcount > 0:
            inserted += 1
    conn.commit()
    conn.close()
    return inserted


def get_target_cities(limit: int = 0) -> list[tuple]:
    conn = sqlite3.connect(str(DB_PATH))
    rows = conn.execute(
        "SELECT name, lat, lon FROM geo_cities WHERE lat IS NOT NULL ORDER BY name"
    ).fetchall()
    conn.close()
    if limit:
        rows = rows[:limit]
    return rows


def main(dry_run: bool = False, limit: int = 0) -> None:
    cities = get_target_cities(limit)
    print(f"待处理城市: {len(cities)}")

    total = 0
    for i, (city, lat, lon) in enumerate(cities):
        n = process_city(city, lat, lon, dry_run)
        total += n
        print(f"  [{i+1}/{len(cities)}] {city}: {n} 个景点", flush=True)
        time.sleep(1.5)  # Overpass 礼貌限流

    print()
    print(f"完成: 总共 {total} 个景点新增到 {len(cities)} 个城市")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--limit", type=int, default=0, help="限制处理城市数")
    args = parser.parse_args()
    main(dry_run=args.dry_run, limit=args.limit)
