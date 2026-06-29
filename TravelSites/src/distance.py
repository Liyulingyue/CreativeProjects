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


# 常用出发地坐标库（fallback 字典）
ORIGIN_COORDS: dict[str, Tuple[float, float]] = {
    "北京": (39.9042, 116.4074),
    "上海": (31.2304, 121.4737),
    "广州": (23.1291, 113.2644),
    "深圳": (22.5431, 114.0579),
    "杭州": (30.2741, 120.1551),
    "成都": (30.5728, 104.0668),
    "南京": (32.0603, 118.7969),
    "武汉": (30.5928, 114.3055),
    "西安": (34.3416, 108.9398),
    "苏州": (31.2989, 120.5853),
    "青岛": (36.0671, 120.3826),
    "厦门": (24.4798, 118.0894),
    "济南": (36.6512, 117.1201),
    "青岛": (36.0671, 120.3826),
    "重庆": (29.5630, 106.5516),
    "长沙": (28.2282, 112.9388),
    "大连": (38.9140, 121.6147),
    "宁波": (29.8683, 121.5440),
    "福州": (26.0745, 119.2965),
    "合肥": (31.8206, 117.2272),
    "昆明": (25.0389, 102.7183),
    "哈尔滨": (45.8038, 126.5350),
    "沈阳": (41.8057, 123.4315),
    "天津": (39.3434, 117.3616),
    "南昌": (28.6820, 115.8579),
    "南宁": (22.8170, 108.3669),
    "太原": (37.8706, 112.5489),
    "石家庄": (38.0428, 114.5149),
    "贵阳": (26.6470, 106.6302),
    "兰州": (36.0611, 103.8343),
    "银川": (38.4872, 106.2309),
    "西宁": (36.6232, 101.7804),
    "乌鲁木齐": (43.8256, 87.6168),
    "拉萨": (29.6500, 91.1700),
    "呼和浩特": (40.8425, 111.7491),
    "三亚": (18.2528, 109.5119),
    "海口": (20.0444, 110.1992),
    "丽江": (26.8721, 100.2330),
    "大理": (25.6065, 100.2675),
    "桂林": (25.2736, 110.2907),
    "张家界": (29.1170, 110.4791),
    "黄山": (29.7148, 118.3375),
    "敦煌": (40.1421, 94.6612),
    "九寨沟": (33.2604, 104.2368),
    "呼伦贝尔": (49.2120, 119.7570),
    "洛阳": (34.6197, 112.4539),
    "大同": (40.0768, 113.3001),
}


def lookup_origin_from_json(province: str, city: str, county: str) -> Optional[Tuple[float, float]]:
    """
    查找出发地坐标：
    1. 先查 SQLite 数据库（精确到县）
    2. 缺失时 fallback 到内置 ORIGIN_COORDS 字典

    TODO: 接入更精确的县级坐标库（如高德/百度 POI）
    """
    try:
        from .db import lookup_origin_county
        coord = lookup_origin_county(province, city, county)
        if coord:
            return coord
    except Exception:
        pass

    return (ORIGIN_COORDS.get(city) or ORIGIN_COORDS.get(province) or ORIGIN_COORDS.get(county))


def lookup_origin(name: str) -> Optional[Tuple[float, float]]:
    """按名称查找出发地坐标（兼容旧调用）。"""
    return ORIGIN_COORDS.get(name)