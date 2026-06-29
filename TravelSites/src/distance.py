"""
距离与车程计算工具。

策略：
  - 距离计算统一基于 city 中心点（geo_cities 表）
  - 不再用 county / 硬编码 fallback
  - 单源原则：DB 是唯一真相
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


def _strip_city_suffix(name: str) -> str:
    """去常见后缀，方便"上海" / "上海市" / "上海黄浦区" 都命中。"""
    for suffix in ("市", "自治州", "白族自治州", "地区", "盟"):
        if name.endswith(suffix):
            return name[: -len(suffix)]
    return name


def lookup_city_coord(city: str) -> Optional[Tuple[float, float]]:
    """
    统一从 DB 查城市坐标。

    支持 "上海" / "上海市" / "西藏自治区" 等写法（自动去后缀）。
    返回 None 表示城市不存在或无坐标。
    距离计算唯一入口。
    """
    try:
        from .db import lookup_city
        # 先试原名
        coord = lookup_city(city)
        if coord:
            return coord
        # 再试去后缀版本
        stripped = _strip_city_suffix(city)
        if stripped != city:
            return lookup_city(stripped)
        return None
    except Exception:
        return None


def calc_distance(origin_city: str, dest_city: str) -> Tuple[float, Optional[dict]]:
    """
    基于 city 中心点计算两地距离。

    Returns:
        (km, transit_info) — km=0 表示无法计算
    """
    o = lookup_city_coord(origin_city)
    d = lookup_city_coord(dest_city)
    if not o or not d:
        return 0, None
    km = haversine_km(o, d)
    return km, estimate_travel_time(km)


# ===== 旧 API（保留兼容，标记 deprecated）=====

ORIGIN_COORDS: dict[str, Tuple[float, float]] = {
    # 已废弃：不再用作距离 fallback 字典
    # 保留只是为兼容旧代码，新代码请用 lookup_city_coord / calc_distance
}


def lookup_origin_from_json(province: str, city: str, county: str) -> Optional[Tuple[float, float]]:
    """
    已废弃：保留仅为兼容。

    距离计算现在统一用 city 中心点。county 维度仅用于 UI 三级联动展示。
    """
    return lookup_city_coord(city)


def lookup_origin(name: str) -> Optional[Tuple[float, float]]:
    """已废弃：用 lookup_city_coord 替代。"""
    return lookup_city_coord(name)
