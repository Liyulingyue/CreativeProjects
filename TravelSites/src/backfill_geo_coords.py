"""
一次性脚本：用 Open-Meteo Geocoding API 补全 geo_cities 缺失的坐标。

免费、无 key、速度快。Nominatim 经常超时，Open-Meteo 是最佳替代。

用法：
  python src/backfill_geo_coords.py [--dry-run] [--limit N] [--all]
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


def geocode(name: str, province: str, province_coord: tuple[float, float] | None = None) -> tuple[float, float] | None:
    """Open-Meteo 地理编码。

    注意：
    - 名字带"市/自治州/盟"后缀往往查不到，要去掉
    - 多个同名城市需要按 province 过滤
    - 长尾城市（民族州）查不到时 fallback 到省中心
    """
    # 候选查询名
    names_to_try = []
    for variant in [name, name.rstrip("市"), name.replace("市", "")]:
        if variant and variant not in names_to_try:
            names_to_try.append(variant)
    # 替换自治州等
    special = {
        "白族自治州": "", "自治州": "", "地区": "", "盟": "",
    }
    for old, new in special.items():
        if old in name:
            stripped = name.replace(old, new)
            if stripped not in names_to_try:
                names_to_try.append(stripped)

    for q in names_to_try:
        try:
            with httpx.Client(timeout=10.0) as client:
                resp = client.get(
                    GEOCODING_URL,
                    params={"name": q, "count": 5, "language": "zh", "countryCode": "CN"},
                )
                resp.raise_for_status()
                data = resp.json()
                results = data.get("results", [])
                # 优先匹配 province
                for r in results:
                    admin1 = r.get("admin1", "")
                    if province and (province in admin1 or admin1 in province):
                        return float(r["latitude"]), float(r["longitude"])
                    if province and province[:2] in admin1:
                        return float(r["latitude"]), float(r["longitude"])
                if results:
                    return float(results[0]["latitude"]), float(results[0]["longitude"])
        except Exception:
            pass

    # Fallback: 用省中心点
    if province_coord:
        return province_coord
    return None


def get_province_coords() -> dict[str, tuple[float, float]]:
    conn = sqlite3.connect(str(DB_PATH))
    rows = conn.execute("SELECT name, lat, lon FROM geo_provinces").fetchall()
    conn.close()
    return {r[0]: (r[1], r[2]) for r in rows if r[1] and r[2]}


def backfill(dry_run: bool = False, limit: int = 0, all_cities: bool = False) -> dict:
    conn = sqlite3.connect(str(DB_PATH))
    conn.row_factory = sqlite3.Row
    cur = conn.cursor()

    if all_cities:
        # 重写所有坐标（用于修复脏数据）
        rows = cur.execute(
            "SELECT code, province_name, name FROM geo_cities ORDER BY province_name, name"
        ).fetchall()
    else:
        rows = cur.execute(
            """SELECT code, province_name, name FROM geo_cities
               WHERE lat IS NULL OR lon IS NULL
                  OR lat < 18 OR lat > 54 OR lon < 73 OR lon > 135
               ORDER BY province_name, name"""
        ).fetchall()
    if limit:
        rows = rows[:limit]

    total = len(rows)
    success = 0
    failed = []
    province_coords = get_province_coords()
    print(f"待处理: {total} 个城市")

    for i, row in enumerate(rows):
        prov_coord = province_coords.get(row["province_name"])
        coords = geocode(row["name"], row["province_name"], prov_coord)
        if coords:
            if not dry_run:
                cur.execute(
                    "UPDATE geo_cities SET lat=?, lon=? WHERE code=?",
                    (coords[0], coords[1], row["code"]),
                )
            success += 1
            mark = "✓"
        else:
            failed.append({"city": row["name"], "province": row["province_name"]})
            mark = "✗"

        print(f"  [{i+1}/{total}] {mark} {row['province_name']} {row['name']}", flush=True)
        time.sleep(0.2)  # Open-Meteo 礼貌限流

    if not dry_run:
        conn.commit()
    conn.close()

    print()
    print(f"完成: 成功 {success}/{total}, 失败 {len(failed)}")
    if failed:
        out = DATA_DIR / "geo_backfill_failed.json"
        with open(out, "w", encoding="utf-8") as f:
            json.dump(failed, f, ensure_ascii=False, indent=2)
        print(f"失败列表: {out}")
    return {"success": success, "failed": len(failed), "total": total}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--limit", type=int, default=0)
    parser.add_argument("--all", action="store_true", help="重写所有城市坐标（修复脏数据）")
    args = parser.parse_args()
    backfill(dry_run=args.dry_run, limit=args.limit, all_cities=args.all)
