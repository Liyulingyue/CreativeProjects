"""
一次性脚本：补全 2021+ 新设的区县到 geo_counties 表。

策略：
  1. 先查 DB 中该省/市是否已存在同名 county
  2. 不存在则用 Nominatim 查坐标后插入
  3. 已存在则跳过

用法：
  python src/backfill_new_counties.py [--dry-run]
"""
import argparse
import sqlite3
import time
from pathlib import Path

import httpx

DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DB_PATH = DATA_DIR / "travelsites.db"

# 2021+ 新设区县清单
NEW_COUNTIES = [
    # 浙江
    ("浙江省", "杭州市", "钱塘区"),
    ("浙江省", "杭州市", "临平区"),
    # 江苏
    ("江苏省", "南通市", "海门区"),
    # 山东
    ("山东省", "济南市", "莱芜区"),
    ("山东省", "济南市", "钢城区"),
    # 河北
    ("河北省", "邢台市", "襄都区"),
    ("河北省", "邢台市", "信都区"),
    # 安徽
    ("安徽省", "芜湖市", "湾沚区"),
    ("安徽省", "芜湖市", "繁昌区"),
    # 吉林
    ("吉林省", "长春市", "九台区"),
    # 海南
    ("海南省", "三沙市", "西沙区"),
    ("海南省", "三沙市", "南沙区"),
    # 云南
    ("云南省", "曲靖市", "沾益区"),
    # 新疆
    ("新疆维吾尔自治区", "吐鲁番市", "高昌区"),
    # 黑龙江
    ("黑龙江省", "绥化市", "北林区"),
    # 西藏
    ("西藏自治区", "拉萨市", "堆龙德庆区"),
]


def fetch_coords(name: str, province: str, city: str) -> tuple[float, float] | None:
    """从 Nominatim 查区县坐标。"""
    queries = [
        f"{name}, {city}, {province}, 中国",
        f"{name}, {province}, 中国",
    ]
    for q in queries:
        try:
            with httpx.Client(timeout=10.0) as client:
                resp = client.get(
                    "https://nominatim.openstreetmap.org/search",
                    params={"q": q, "format": "json", "limit": 1, "countrycodes": "cn"},
                    headers={"User-Agent": "TravelSites/0.1 (backfill)"},
                )
                resp.raise_for_status()
                results = resp.json()
                if results:
                    return float(results[0]["lat"]), float(results[0]["lon"])
        except Exception:
            pass
        time.sleep(1.1)
    return None


def main(dry_run: bool = False) -> None:
    conn = sqlite3.connect(str(DB_PATH))
    conn.row_factory = sqlite3.Row
    cur = conn.cursor()

    # 获取 city_code 映射
    city_map = {
        (r["province_name"], r["name"]): r["code"]
        for r in cur.execute("SELECT code, province_name, name FROM geo_cities").fetchall()
    }

    inserted = 0
    skipped = 0
    failed = []

    for province, city_name, county_name in NEW_COUNTIES:
        # 检查是否已存在
        exists = cur.execute(
            """SELECT 1 FROM geo_counties
               WHERE name=? AND city_name=? AND province_name=?""",
            (county_name, city_name, province),
        ).fetchone()
        if exists:
            skipped += 1
            continue

        city_code = city_map.get((province, city_name))
        if not city_code:
            failed.append((province, city_name, county_name, "city not found"))
            continue

        print(f"  [{province}/{city_name}/{county_name}]", end=" ", flush=True)
        coords = fetch_coords(county_name, province, city_name)
        if coords is None:
            print("失败")
            failed.append((province, city_name, county_name, "no coord"))
            continue

        if not dry_run:
            cur.execute(
                """INSERT INTO geo_counties
                   (name, city_code, city_name, province_name, lat, lon)
                   VALUES (?,?,?,?,?,?)""",
                (county_name, city_code, city_name, province, coords[0], coords[1]),
            )
        print(f"OK ({coords[0]:.4f}, {coords[1]:.4f})")
        inserted += 1

    if not dry_run:
        conn.commit()
    conn.close()

    print()
    print(f"完成: 插入 {inserted}, 跳过 {skipped}, 失败 {len(failed)}")
    for f in failed:
        print(f"  ✗ {f}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    main(dry_run=args.dry_run)
