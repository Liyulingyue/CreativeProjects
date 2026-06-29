"""
一次性脚本：用 Open-Meteo Geocoding 补全 geo_counties 缺失的坐标。

免费、无 key。

策略：
  1. 先尝试精确查询 "县区名 + 城市名"
  2. 再试只查 "县区名"（按 admin2 过滤）
  3. 最后 fallback 到城市中心点

用法：
  python src/backfill_county_coords.py [--dry-run] [--limit 10]
"""
import argparse
import json
import sqlite3
import time
from pathlib import Path

import httpx

DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DB_PATH = DATA_DIR / "travelsites.db"

GEOCODING_URL = "https://geocoding-api.open-meteo.com/v1/search"


def geocode_county(county: str, city: str) -> tuple[float, float] | None:
    """用县区名 + 城市名查坐标。"""
    queries = [
        f"{county}",
        f"{city}{county}",
    ]
    for q in queries:
        try:
            with httpx.Client(timeout=10.0) as client:
                resp = client.get(
                    GEOCODING_URL,
                    params={"name": q, "count": 5, "language": "zh", "countryCode": "CN"},
                )
                resp.raise_for_status()
                data = resp.json()
                results = data.get("results", [])
                # 优先 admin2 包含 county 名 + admin1 包含 city 名
                county_short = county.rstrip("区").rstrip("县").rstrip("市")
                for r in results:
                    admin1 = r.get("admin1", "")
                    admin2 = r.get("admin2", "")
                    if city[:2] in admin1 and county_short in admin2:
                        return float(r["latitude"]), float(r["longitude"])
                # 退化：admin2 包含 county
                for r in results:
                    admin2 = r.get("admin2", "")
                    if county_short in admin2:
                        return float(r["latitude"]), float(r["longitude"])
                if results:
                    return float(results[0]["latitude"]), float(results[0]["longitude"])
        except Exception:
            pass
    return None


def get_city_coords() -> dict[str, tuple[float, float]]:
    conn = sqlite3.connect(str(DB_PATH))
    rows = conn.execute("SELECT name, lat, lon FROM geo_cities").fetchall()
    conn.close()
    return {r[0]: (r[1], r[2]) for r in rows if r[1] and r[2]}


def get_target_counties(limit: int = 0) -> list[tuple]:
    conn = sqlite3.connect(str(DB_PATH))
    conn.row_factory = sqlite3.Row
    rows = conn.execute("""
        SELECT id, name, city_name, province_name
        FROM geo_counties
        WHERE lat IS NULL OR lon IS NULL
           OR lat < 18 OR lat > 54 OR lon < 73 OR lon > 135
        ORDER BY province_name, city_name, name
    """).fetchall()
    conn.close()
    if limit:
        rows = rows[:limit]
    return rows


def main(dry_run: bool = False, limit: int = 0) -> None:
    city_coords = get_city_coords()
    targets = get_target_counties(limit)
    print(f"待处理县区: {len(targets)}")

    conn = sqlite3.connect(str(DB_PATH))
    cur = conn.cursor()
    success = 0
    failed = []
    fallback = 0

    for i, row in enumerate(targets):
        city = row["city_name"]
        county = row["name"]

        coords = geocode_county(county, city)
        mark = "✓"

        if not coords:
            # fallback 到城市中心
            city_c = city_coords.get(city)
            if city_c:
                coords = city_c
                mark = "≋"  # 城市中心 fallback
                fallback += 1
            else:
                mark = "✗"
                failed.append({"county": county, "city": city})

        if coords and not dry_run:
            cur.execute(
                "UPDATE geo_counties SET lat=?, lon=? WHERE id=?",
                (coords[0], coords[1], row["id"]),
            )

        if mark == "✓":
            success += 1
        print(f"  [{i+1}/{len(targets)}] {mark} {row['province_name']} {city} {county} → {coords}", flush=True)
        time.sleep(0.2)

    if not dry_run:
        conn.commit()
    conn.close()

    print()
    print(f"完成: 精确命中 {success - fallback}, 城市 fallback {fallback}, 失败 {len(failed)}")
    if failed:
        out = DATA_DIR / "county_backfill_failed.json"
        with open(out, "w", encoding="utf-8") as f:
            json.dump(failed, f, ensure_ascii=False, indent=2)
        print(f"失败列表: {out}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--limit", type=int, default=0)
    args = parser.parse_args()
    main(dry_run=args.dry_run, limit=args.limit)
