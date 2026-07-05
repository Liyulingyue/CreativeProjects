import time
import json
from pathlib import Path
from typing import Optional
from dataclasses import dataclass, asdict, field

from openaijsonwrapper import OpenAIJsonWrapper
from openai import OpenAI

from .config import (
    API_KEY, BASE_URL, MODEL_NAME,
    DEFAULT_TARGET_STRUCTURE, DEFAULT_BACKGROUND, DEFAULT_REQUIREMENTS,
    LITE_TARGET_STRUCTURE, LITE_BACKGROUND, LITE_REQUIREMENTS,
)
from .weather import fetch_weather, is_extreme_weather, is_rainy, DailyWeather
from .cities import lookup_city


@dataclass
class TripPlanResult:
    city: str
    start_date: str
    end_date: str
    duration_days: int
    error: Optional[str] = None
    data: Optional[dict] = None
    reasoning: Optional[str] = None
    raw_content: Optional[str] = None
    success: bool = False
    weather_forecast: list = field(default_factory=list)

    def to_dict(self) -> dict:
        return asdict(self)


def _format_weather_for_prompt(weather: list[DailyWeather]) -> str:
    lines = ["【真实天气预报（来自 Open-Meteo 公开 API）】"]
    for w in weather:
        flags = []
        if is_extreme_weather(w.weather_code):
            flags.append("[雷暴极端天气]")
        elif is_rainy(w.weather_code):
            flags.append("[雨天]")
        flag_str = " | ".join(flags) if flags else "天气良好"
        prob = f"{w.precipitation_probability}%" if w.precipitation_probability is not None else "N/A"
        lines.append(
            f"  {w.date}: {w.weather_desc}  气温 {w.temp_min:.0f}°C ~ {w.temp_max:.0f}°C  "
            f"降水 {w.precipitation_mm:.1f}mm  降水概率 {prob}  {flag_str}"
        )
    rainy_days = sum(1 for w in weather if is_rainy(w.weather_code))
    storm_days = sum(1 for w in weather if is_extreme_weather(w.weather_code))
    if storm_days >= len(weather):
        lines.append(f"[警告] 行程内 {len(weather)} 天均为雷暴，请慎重考虑是否出行。")
    elif storm_days > 0:
        lines.append(f"[注意] 行程内有 {storm_days} 天雷暴天气。")
    if rainy_days > 0:
        lines.append(f"[提示] 行程内有 {rainy_days} 天降雨，请规划室内/替代活动。")
    return "\n".join(lines)


class TripPlanner:
    def __init__(
        self,
        api_key: Optional[str] = None,
        base_url: Optional[str] = None,
        model: Optional[str] = None,
        target_structure: Optional[dict] = None,
        background: Optional[str] = None,
        requirements: Optional[list] = None,
        lite: bool = False,
        max_retries: int = 3,
        retry_delay: float = 5.0,
    ):
        self.client = OpenAI(
            api_key=api_key or API_KEY,
            base_url=base_url or BASE_URL,
        )
        self.model = model or MODEL_NAME
        if lite:
            self.target_structure = LITE_TARGET_STRUCTURE
            self.background = LITE_BACKGROUND
            self.requirements = LITE_REQUIREMENTS
        else:
            self.target_structure = target_structure or DEFAULT_TARGET_STRUCTURE
            self.background = background or DEFAULT_BACKGROUND
            self.requirements = requirements or DEFAULT_REQUIREMENTS
        self.lite = lite
        self.max_retries = max_retries
        self.retry_delay = retry_delay

        self.wrapper = OpenAIJsonWrapper(
            self.client,
            model=self.model,
            target_structure=self.target_structure,
            background=self.background,
            requirements=self.requirements,
        )

    def plan(
        self,
        city: str,
        start_date: str,
        end_date: str,
        preset_weather: Optional[list] = None,
        preference_tags: Optional[list[str]] = None,
        preference_text: Optional[str] = None,
    ) -> TripPlanResult:
        from datetime import datetime
        start = datetime.strptime(start_date, "%Y-%m-%d")
        end = datetime.strptime(end_date, "%Y-%m-%d")
        duration_days = (end - start).days + 1

        result = TripPlanResult(
            city=city,
            start_date=start_date,
            end_date=end_date,
            duration_days=duration_days,
        )

        coords = lookup_city(city)
        if coords is None:
            result.error = f"未找到城市坐标: {city}（请确认城市名正确，或加入 src/cities.py）"
            return result
        lat, lon = coords

        if preset_weather is not None:
            weather = preset_weather
        else:
            try:
                weather = fetch_weather(lat, lon, start_date, end_date)
            except Exception as e:
                result.error = f"天气查询失败: {e}"
                return result

        result.weather_forecast = [
            {
                "date": w.date,
                "weather_code": w.weather_code,
                "weather_desc": w.weather_desc,
                "temp_max": w.temp_max,
                "temp_min": w.temp_min,
                "precipitation_mm": w.precipitation_mm,
                "precipitation_probability": w.precipitation_probability,
            }
            for w in weather
        ]

        weather_section = _format_weather_for_prompt(weather)

        preference_section = ""
        if preference_tags or preference_text:
            parts = []
            if preference_tags:
                parts.append(f"偏好主题: {', '.join(preference_tags)}")
            if preference_text:
                parts.append(f"用户需求: {preference_text}")
            preference_section = "【用户偏好】\n" + "\n".join(parts) + "\n\n"

        if self.lite:
            user_prompt = (
                f"请为【{start_date}】到【{end_date}】（共 {duration_days} 天）的"
                f"【{city}】之行做一个简洁的旅行评估。\n\n"
                f"{weather_section}\n\n"
                f"{preference_section}"
                f"【要求】\n"
                f"1. 给出整体推荐度评分 score (0-100) 和 recommendation\n"
                f"2. 多日雷暴 → score<50, '建议改期'\n"
                f"3. 雨天 → score 50-70, '勉强可行' 或 '推荐'\n"
                f"4. 天气良好 → score 85+, '强烈推荐'\n"
                f"5. top_attractions 必须包含至少 2 个近郊/周边县城景点，且优先体现用户偏好主题\n"
                f"6. 输出严格符合指定的 JSON 结构"
            )
        else:
            user_prompt = (
                f"请为我在【{start_date}】到【{end_date}】（共 {duration_days} 天）的"
                f"【{city}】之行规划一份详尽的旅行方案。\n\n"
                f"{weather_section}\n\n"
                f"{preference_section}"
                f"【规划要求】\n"
                f"1. 景点必须包含 {city} 市中心经典景点与至少 3 个近郊/周边县城的景点，且优先体现用户偏好主题\n"
                f"2. 每天至少给出 2 条主题不同的候选路线\n"
                f"3. **必须严格根据上面真实天气调整行程**：\n"
                f"   - 雨天优先安排室内/文化/美食类活动\n"
                f"   - 雷暴天应避开户外高山/峡谷类高风险景区\n"
                f"   - 在 weather_hint 中给出当天的穿着与活动建议\n"
                f"4. 给出一个诚实的整体推荐度评分 score (0-100)：\n"
                f"   - 综合考虑:天数匹配度、天气友好度、景点丰富度、交通便利度\n"
                f"   - 若行程多日雷暴/极端天气，score 应低于 50，recommendation 选 '建议改期'\n"
                f"   - 若有雨但可调整活动，score 50-70，recommendation '勉强可行' 或 '推荐'\n"
                f"   - 天气良好且行程充实，score 85+ '强烈推荐'\n"
                f"5. 输出严格符合指定的 JSON 结构"
            )

        messages = [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": user_prompt},
                ],
            }
        ]

        for attempt in range(self.max_retries):
            try:
                response = self.wrapper.chat(messages=messages)

                if not response["error"]:
                    result.data = response["data"]
                    result.reasoning = response.get("reasoning")
                    result.success = True
                    return result
                else:
                    result.raw_content = response.get("raw_content")
                    if attempt < self.max_retries - 1:
                        time.sleep(self.retry_delay)
                        continue
                    result.error = response["error"]
                    return result

            except Exception as e:
                if attempt < self.max_retries - 1:
                    time.sleep(self.retry_delay)
                    continue
                result.error = str(e)
                return result

        return result
