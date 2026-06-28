"""
距离与车程计算工具。

当前使用 Haversine 公式粗算直线距离，
再根据交通方式估算耗时。

TODO: 后续接入高德/百度地图 API 获取真实道路里程和耗时
"""
from math import asin, cos, radians, sin, sqrt
from typing import Optional, Tuple


EARTH_RADIUS_KM = 6371


def haversine_km(coord1: Tuple[float, float], coord2: Tuple[float, float]) -> float:
    """
    Haversine 公式：地球两点直线距离（km）
    coord: (lat, lon)
    """
    lat1, lon1 = radians(coord1[0]), radians(coord1[1])
    lat2, lon2 = radians(coord2[0]), radians(coord2[1])

    dlat = lat2 - lat1
    dlon = lon2 - lon1

    a = sin(dlat / 2) ** 2 + cos(lat1) * cos(lat2) * sin(dlon / 2) ** 2
    c = 2 * asin(sqrt(a))

    return EARTH_RADIUS_KM * c


def estimate_travel_time(km: float) -> dict:
    """
    根据直线距离估算交通耗时和方式推荐。
    粗略规则，后续应接入真实地图 API。

    Returns:
        {
            "distance_km": float,
            "recommended_mode": str,  # "高铁" | "飞机" | "自驾" | "周边"
            "transit_hours": float,   # 单程耗时
            "round_trip_hours": float,
        }
    """
    if km < 300:
        mode = "高铁/自驾"
        speed_kmh = 200
    elif km < 800:
        mode = "高铁"
        speed_kmh = 250
    elif km < 1500:
        mode = "高铁/飞机"
        speed_kmh = 280
    else:
        mode = "飞机"
        speed_kmh = 700

    # 加 1.5h 中转/取票时间
    transit_hours = round(km / speed_kmh + 1.5, 1)
    round_trip_hours = round(transit_hours * 2, 1)

    return {
        "distance_km": round(km, 1),
        "recommended_mode": mode,
        "transit_hours": transit_hours,
        "round_trip_hours": round_trip_hours,
    }


def transport_score(distance_km: float, duration_days: int) -> int:
    """
    车程占比得分 (0-100)。
    逻辑: 往返车程占整段行程时间比例越低越好。

    duration_days: 行程天数（>=1）
    """
    info = estimate_travel_time(distance_km)
    total_hours = duration_days * 24
    ratio = info["round_trip_hours"] / total_hours

    # 比例 0% → 100分, 比例 50% → 50分, 比例 > 80% → 0分
    score = max(0, min(100, int(100 * (1 - ratio * 1.25))))

    # 800km 内高铁可达的基础分
    if distance_km > 1500 and duration_days <= 3:
        score = min(score, 60)

    return score


# 常用出发地坐标库
ORIGIN_COORDS: dict[str, Tuple[float, float]] = {
    "北京": (39.9042, 116.4074),
    "上海": (31.2304, 121.4737),
    "广州": (23.1291, 113.2644),
    "深圳": (22.5431, 114.0579),
    "杭州": (30.2741, 120.1551),
    "成都": (30.5728, 104.0668),
}


def lookup_origin_from_json(province: str, city: str, county: str) -> Optional[Tuple[float, float]]:
    """
    从 china_regions_enriched.json 查找出发地坐标（精确到县/区）。
    县无独立坐标时 fallback 到城市中心。
    TODO: 接入更精确的县级坐标库（如高德/百度行政区划 API）
    """
    import json
    from pathlib import Path

    json_path = Path(__file__).resolve().parent.parent / "data" / "china_regions_enriched.json"
    try:
        with open(json_path, "r", encoding="utf-8") as f:
            data = json.load(f)
    except Exception:
        return None

    prov = data.get(province)
    if not prov:
        return lookup_origin(province) or lookup_origin(city) or lookup_origin(county)

    cities = prov.get("cities", {})
    city_data = cities.get(city)
    if not city_data:
        return lookup_origin(city) or lookup_origin(province)

    return (city_data.get("latitude", 0), city_data.get("longitude", 0))


def lookup_origin(name: str) -> Optional[Tuple[float, float]]:
    """按名称查找出发地坐标（兼容旧调用）。"""
    return ORIGIN_COORDS.get(name)