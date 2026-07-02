"""
补全 attractions 坐标：source='search_runtime' 且无坐标的 → 用 city 中心点。

策略：
  1. 查 attractions 里 lat IS NULL 的
  2. 查 city 在 geo_cities 的坐标
  3. UPDATE attractions SET lat, lon = city_lat, city_lon

跑：python src/backfill_poi_coords.py
"""
import sqlite3
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

DB_PATH = Path(__file__).resolve().parent.parent / "data" / "travelsites.db"


def main():
    conn = sqlite3.connect(str(DB_PATH))
    cur = conn.cursor()

    # 找出所有 search_runtime 且无坐标的
    cur.execute("""
        SELECT a.id, a.name, a.city
        FROM attractions a
        WHERE a.lat IS NULL AND a.source = 'search_runtime'
    """)
    rows = cur.fetchall()
    print(f"无坐标景点: {len(rows)} 条")

    # 预加载所有 city 坐标
    cur.execute("SELECT name, lat, lon FROM geo_cities WHERE lat IS NOT NULL")
    city_coords = {r[0]: (r[1], r[2]) for r in cur.fetchall()}
    print(f"geo_cities 有坐标: {len(city_coords)} 个")

    updated = 0
    no_city = 0
    no_city_coord = 0

    for pid, name, city in rows:
        # lookup_city 逻辑：原名匹配 → 加后缀匹配
        if city in city_coords:
            lat, lon = city_coords[city]
        else:
            no_city += 1
            # 跳过（city 不在 geo_cities）
            continue

        cur.execute(
            "UPDATE attractions SET lat=?, lon=? WHERE id=?",
            (lat, lon, pid),
        )
        updated += 1

    conn.commit()
    print(f"\n更新: {updated} 条")
    print(f"city 不在 geo_cities: {no_city} 条")

    # 验证
    n_no_xy = cur.execute(
        "SELECT COUNT(*) FROM attractions WHERE lat IS NULL"
    ).fetchone()[0]
    n_total = cur.execute("SELECT COUNT(*) FROM attractions").fetchone()[0]
    print(f"\n剩余无坐标: {n_no_xy} / {n_total}")


if __name__ == "__main__":
    main()
