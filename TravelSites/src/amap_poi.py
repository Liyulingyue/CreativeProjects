"""
高德地图 POI 接入（可选模块）。

启用方法：
1. 在 .env 配置 AMAP_API_KEY
2. 设置 POI_SOURCE_ENABLED=true

主要功能：
- 按城市名查景点列表（POI 文本搜索）
- 按坐标查周边景点（POI 周边搜索）
- 反向地理编码（坐标 → 城市名）
- 距离计算

API 文档：https://lbs.amap.com/api/webservice/guide/api/search
"""
import os
import httpx
from typing import Optional


AMAP_API_KEY = os.getenv("AMAP_API_KEY", "")
POI_SOURCE_ENABLED = os.getenv("POI_SOURCE_ENABLED", "false").lower() in ("true", "1", "yes")

BASE_URL = "https://restapi.amap.com/v3"


def is_enabled() -> bool:
    """POI 源是否启用（需要 API key）。"""
    return POI_SOURCE_ENABLED and bool(AMAP_API_KEY)


async def search_poi_text(
    keywords: str,
    city: str,
    poi_type: str = "观光",
    limit: int = 10,
) -> list[dict]:
    """
    按城市名 + 关键词搜索 POI。
    例如：search_poi_text("西湖", "杭州", "风景名胜")
    """
    if not is_enabled():
        return []

    url = f"{BASE_URL}/place/text"
    params = {
        "key": AMAP_API_KEY,
        "keywords": keywords,
        "city": city,
        "types": poi_type,
        "extensions": "base",
        "offset": min(limit, 25),
        "page": 1,
    }

    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            resp = await client.get(url, params=params)
            data = resp.json()

        if data.get("status") != "1":
            return []

        return [
            {
                "name": p["name"],
                "address": p.get("address", ""),
                "location": p.get("location", ""),  # "lng,lat"
                "type": p.get("type", ""),
                "city": city,
                "source": "amap",
            }
            for p in data.get("pois", [])
        ]
    except Exception:
        return []


async def search_poi_around(
    lng: float,
    lat: float,
    radius: int = 3000,
    poi_type: str = "风景名胜|公园广场",
    limit: int = 10,
) -> list[dict]:
    """
    按坐标查周边 POI。
    """
    if not is_enabled():
        return []

    url = f"{BASE_URL}/place/around"
    params = {
        "key": AMAP_API_KEY,
        "location": f"{lng},{lat}",
        "radius": radius,
        "types": poi_type,
        "extensions": "base",
        "offset": min(limit, 25),
        "page": 1,
    }

    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            resp = await client.get(url, params=params)
            data = resp.json()

        if data.get("status") != "1":
            return []

        return [
            {
                "name": p["name"],
                "address": p.get("address", ""),
                "location": p.get("location", ""),
                "type": p.get("type", ""),
                "source": "amap",
            }
            for p in data.get("pois", [])
        ]
    except Exception:
        return []


async def geocode(city: str) -> Optional[tuple[float, float]]:
    """地理编码：城市名 → (lng, lat)。"""
    if not is_enabled():
        return None

    url = f"{BASE_URL}/geocode/geo"
    params = {
        "key": AMAP_API_KEY,
        "address": city,
    }
    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            resp = await client.get(url, params=params)
            data = resp.json()
        if data.get("status") != "1" or not data.get("geocodes"):
            return None
        loc = data["geocodes"][0].get("location", "")
        lng, lat = loc.split(",")
        return (float(lng), float(lat))
    except Exception:
        return None


async def import_poi_to_db(city: str, poi_type: str = "风景名胜") -> int:
    """
    批量从高德拉景点写入本地 DB。
    返回新增数量。
    """
    from datetime import datetime
    import json
    import sqlite3
    from pathlib import Path

    DB_PATH = Path(__file__).resolve().parent.parent / "data" / "travelsites.db"

    if not is_enabled():
        return 0

    keywords_by_type = {
        "风景名胜": "景区",
        "美食": "美食",
        "酒店": "酒店",
        "购物": "购物",
    }
    keyword = keywords_by_type.get(poi_type, "景区")

    pois = await search_poi_text(keyword, city, poi_type, limit=20)
    if not pois:
        return 0

    conn = sqlite3.connect(str(DB_PATH))
    cur = conn.cursor()
    inserted = 0
    now = datetime.now().isoformat()
    for p in pois:
        # 解析坐标
        try:
            lng, lat = p["location"].split(",")
            lng, lat = float(lng), float(lat)
        except Exception:
            continue
        # 分类简化
        category = p.get("type", "").split(";")[0] if p.get("type") else poi_type
        cur.execute(
            """INSERT OR IGNORE INTO attractions
               (name, city, category, lat, lon, address, source, verified, created_at)
               VALUES (?,?,?,?,?,?,?,1,?)""",
            (p["name"], city, category, lat, lng, p.get("address", ""), "amap", now),
        )
        if cur.rowcount > 0:
            inserted += 1
    conn.commit()
    conn.close()
    return inserted