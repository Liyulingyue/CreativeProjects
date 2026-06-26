"""
城市查找 + 运行时学习。

策略链: learned -> local -> openmeteo -> nominatim
学习机制: openmeteo 返回 country_code=CN 时, 自动写入 LEARNED_COORDS
持久化: 调用 save_learned_to_json() 写到 data/learned_cities.json
启动加载: import 时自动从 JSON 加载
"""
import json
from pathlib import Path
from typing import Optional, Callable
import httpx


ROOT = Path(__file__).resolve().parent.parent
DATA_DIR = ROOT / "data"
LEARNED_FILE = DATA_DIR / "learned_cities.json"


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


LEARNED_COORDS: dict[str, tuple[float, float]] = {}


def _load_learned_from_disk() -> None:
    if not LEARNED_FILE.exists():
        return
    try:
        data = json.loads(LEARNED_FILE.read_text(encoding="utf-8"))
        for city, coords in data.items():
            if isinstance(coords, list) and len(coords) == 2:
                LEARNED_COORDS[city] = (float(coords[0]), float(coords[1]))
    except Exception:
        pass


_load_learned_from_disk()


def _lookup_learned(city: str) -> Optional[tuple[float, float, str]]:
    if city in LEARNED_COORDS:
        lat, lon = LEARNED_COORDS[city]
        return (lat, lon, "CN")
    return None


def _lookup_local(city: str) -> Optional[tuple[float, float, str]]:
    if city in CITY_COORDS:
        lat, lon = CITY_COORDS[city]
        return (lat, lon, "CN")
    return None


def _lookup_openmeteo(city: str) -> Optional[tuple[float, float, str]]:
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
                lat = float(hit["latitude"])
                lon = float(hit["longitude"])
                country = hit.get("country_code", "")
                if country == "CN":
                    LEARNED_COORDS[city] = (lat, lon)
                return (lat, lon, country)
    except Exception:
        return None
    return None


def _lookup_nominatim(city: str) -> Optional[tuple[float, float, str]]:
    try:
        with httpx.Client(timeout=10.0) as client:
            resp = client.get(
                "https://nominatim.openstreetmap.org/search",
                params={"q": city, "format": "json", "limit": 1, "countrycodes": "cn"},
                headers={"User-Agent": "TravelSites/0.1 (https://github.com/anomalyco/opencode)"},
            )
            resp.raise_for_status()
            results = resp.json()
            if results:
                hit = results[0]
                lat = float(hit["lat"])
                lon = float(hit["lon"])
                return (lat, lon, "CN")
    except Exception:
        return None
    return None


STRATEGY_CHAIN: list[tuple[str, Callable[[str], Optional[tuple[float, float, str]]]]] = [
    ("learned", _lookup_learned),
    ("local", _lookup_local),
    ("openmeteo", _lookup_openmeteo),
    ("nominatim", _lookup_nominatim),
]


def lookup_city(city: str, learn: bool = True, verbose: bool = False) -> Optional[tuple[float, float]]:
    for name, strategy in STRATEGY_CHAIN:
        result = strategy(city)
        if result is not None:
            lat, lon, country = result
            if verbose:
                print(f"  [geocode] {city} -> ({lat:.4f}, {lon:.4f}) via {name} [{country}]")
            return (lat, lon)
    if verbose:
        print(f"  [geocode] {city} -> NOT FOUND in any strategy")
    return None


def get_strategies() -> list[str]:
    return [name for name, _ in STRATEGY_CHAIN]


def save_learned_to_json(path: Optional[Path] = None) -> int:
    target = path or LEARNED_FILE
    if not LEARNED_COORDS:
        return 0
    payload = {city: [lat, lon] for city, (lat, lon) in LEARNED_COORDS.items()}
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )
    return len(payload)


def learned_count() -> int:
    return len(LEARNED_COORDS)
