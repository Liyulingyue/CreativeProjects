import httpx
from datetime import date as _date, datetime
from dataclasses import dataclass
from typing import Optional


WMO_CODE_DESC: dict[int, str] = {
    0: "晴",
    1: "少云", 2: "多云", 3: "阴",
    45: "雾", 48: "雾凇",
    51: "小毛毛雨", 53: "毛毛雨", 55: "大毛毛雨",
    56: "冻毛毛雨", 57: "大冻毛毛雨",
    61: "小雨", 63: "中雨", 65: "大雨",
    66: "冻雨", 67: "大冻雨",
    71: "小雪", 73: "中雪", 75: "大雪",
    77: "雪粒",
    80: "小阵雨", 81: "中阵雨", 82: "大阵雨",
    85: "小阵雪", 86: "大阵雪",
    95: "雷暴",
    96: "雷暴伴小冰雹", 99: "雷暴伴大冰雹",
}

FORECAST_HORIZON_DAYS = 16


@dataclass
class DailyWeather:
    date: str
    weather_code: int
    weather_desc: str
    temp_max: float
    temp_min: float
    precipitation_mm: float
    precipitation_probability: Optional[int]


def _wmo_to_desc(code: int) -> str:
    return WMO_CODE_DESC.get(code, f"未知({code})")


def is_extreme_weather(code: int) -> bool:
    return code in (95, 96, 99)


def is_rainy(code: int) -> bool:
    return 51 <= code <= 67 or 80 <= code <= 82


def _parse_date(s: str) -> _date:
    return datetime.strptime(s, "%Y-%m-%d").date()


def fetch_weather(
    latitude: float,
    longitude: float,
    start_date: str,
    end_date: str,
    timeout: float = 30.0,
) -> list[DailyWeather]:
    start = _parse_date(start_date)
    end = _parse_date(end_date)
    today = _date.today()

    if end < today:
        raise ValueError(f"end_date {end_date} 已过, 只能查今天及之后")
    if start < today:
        start = today

    forecast_days = (end - today).days + 1
    if forecast_days > FORECAST_HORIZON_DAYS:
        forecast_days = FORECAST_HORIZON_DAYS

    url = "https://api.open-meteo.com/v1/forecast"
    params = {
        "latitude": latitude,
        "longitude": longitude,
        "daily": "weather_code,temperature_2m_max,temperature_2m_min,precipitation_sum,precipitation_probability_max",
        "timezone": "auto",
        "forecast_days": forecast_days,
    }
    with httpx.Client(timeout=timeout) as client:
        resp = client.get(url, params=params)
        resp.raise_for_status()
        data = resp.json()

    daily = data.get("daily", {})
    dates = daily.get("time", [])
    codes = daily.get("weather_code", [])
    tmax = daily.get("temperature_2m_max", [])
    tmin = daily.get("temperature_2m_min", [])
    precip = daily.get("precipitation_sum", [])
    prob = daily.get("precipitation_probability_max", [])

    result = []
    for i, date_str in enumerate(dates):
        d = _parse_date(date_str)
        if d < start or d > end:
            continue
        code = codes[i] if i < len(codes) else 0
        result.append(DailyWeather(
            date=date_str,
            weather_code=code,
            weather_desc=_wmo_to_desc(code),
            temp_max=float(tmax[i]) if i < len(tmax) and tmax[i] is not None else 0.0,
            temp_min=float(tmin[i]) if i < len(tmin) and tmin[i] is not None else 0.0,
            precipitation_mm=float(precip[i]) if i < len(precip) and precip[i] is not None else 0.0,
            precipitation_probability=int(prob[i]) if i < len(prob) and prob[i] is not None else None,
        ))
    return result
