from typing import Optional, Callable
import httpx


CITY_COORDS: dict[str, tuple[float, float]] = {
    "北京": (39.9042, 116.4074),
    "上海": (31.2304, 121.4737),
    "广州": (23.1291, 113.2644),
    "深圳": (22.5431, 114.0579),
    "杭州": (30.2741, 120.1551),
    "成都": (30.5728, 104.0668),
    "西安": (34.3416, 108.9398),
    "南京": (32.0603, 118.7969),
    "洛阳": (34.6197, 112.4539),
    "苏州": (31.2989, 120.5853),
    "青岛": (36.0671, 120.3826),
    "厦门": (24.4798, 118.0894),
    "大理": (25.6065, 100.2675),
    "丽江": (26.8721, 100.2330),
    "黄山": (29.7148, 118.3375),
    "桂林": (25.2736, 110.2907),
    "长沙": (28.2282, 112.9388),
    "重庆": (29.5630, 106.5516),
    "武汉": (30.5928, 114.3055),
    "天津": (39.3434, 117.3616),
    "哈尔滨": (45.8038, 126.5350),
    "三亚": (18.2528, 109.5119),
    "拉萨": (29.6520, 91.1721),
    "敦煌": (40.1421, 94.6612),
}


def _lookup_local(city: str) -> Optional[tuple[float, float]]:
    if city in CITY_COORDS:
        return CITY_COORDS[city]
    return None


def _lookup_nominatim(city: str) -> Optional[tuple[float, float]]:
    try:
        with httpx.Client(timeout=10.0) as client:
            resp = client.get(
                "https://nominatim.openstreetmap.org/search",
                params={"q": city, "format": "json", "limit": 1},
                headers={"User-Agent": "TravelSites/0.1 (https://github.com/anomalyco/opencode)"},
            )
            resp.raise_for_status()
            results = resp.json()
            if results:
                return float(results[0]["lat"]), float(results[0]["lon"])
    except Exception:
        return None
    return None


def _lookup_openmeteo(city: str) -> Optional[tuple[float, float]]:
    try:
        with httpx.Client(timeout=10.0) as client:
            resp = client.get(
                "https://geocoding-api.open-meteo.com/v1/search",
                params={"name": city, "count": 1, "language": "zh"},
            )
            resp.raise_for_status()
            data = resp.json()
            results = data.get("results", [])
            if results:
                hit = results[0]
                return float(hit["latitude"]), float(hit["longitude"])
    except Exception:
        return None
    return None


STRATEGY_CHAIN: list[tuple[str, Callable[[str], Optional[tuple[float, float]]]]] = [
    ("local", _lookup_local),
    ("openmeteo", _lookup_openmeteo),
    ("nominatim", _lookup_nominatim),
]


def lookup_city(city: str, verbose: bool = False) -> Optional[tuple[float, float]]:
    for name, strategy in STRATEGY_CHAIN:
        result = strategy(city)
        if result is not None:
            if verbose:
                print(f"  [geocode] {city} -> {result} via {name}")
            return result
    if verbose:
        print(f"  [geocode] {city} -> NOT FOUND in any strategy")
    return None


def get_strategies() -> list[str]:
    return [name for name, _ in STRATEGY_CHAIN]
