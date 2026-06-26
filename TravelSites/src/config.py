import os
from pathlib import Path
from dotenv import load_dotenv

_ENV_PATH = Path(__file__).resolve().parent.parent / ".env"
load_dotenv(_ENV_PATH)

API_KEY: str = os.getenv("OPENAI_API_KEY", "your-api-key-here")
BASE_URL: str = os.getenv("OPENAI_BASE_URL", "https://api.minimaxi.com/v1")
MODEL_NAME: str = os.getenv("OPENAI_VISION_MODEL_NAME", "MiniMax-M3")

DEFAULT_TARGET_STRUCTURE: dict = {
    "city": "str, 城市名称",
    "province": "str, 所在省份",
    "city_intro": "str, 城市简介，100-200 字，说明历史地位、地理特征、核心特色",
    "city_tags": "list[str], 城市整体标签，3-6 个，例如 ['古迹', '牡丹', '历史', '美食']",
    "best_season": "str, 最佳旅游季节，简洁说明",
    "attractions": "list[dict], 景点列表，每个 dict 包含: name(str), category(str, 类型如古迹/山岳/博物馆/公园/美食街), location(str, 所在区/县，写明'市区'或'XX县/XX区'), distance_from_center_km(float, 距市中心公里数), intro(str, 简介 50-100 字), suggested_hours(float), tags(list[str], 2-4 个), is_nearby(bool, 距市中心 30km 以上标 true)",
    "daily_plan": "list[dict], 每日行程，每个 dict 包含: day(int, 从 1 开始), date(str, YYYY-MM-DD), theme(str, 当日主题), weather_hint(str, 30-50 字, 必须基于真实天气给出活动适配和穿着建议), routes(list[dict], 候选路线，每个 dict 包含: route_id(str, 如 R1), tags(list[str], 2-3 个), activities(list[dict], 含 attraction(str), time_slot(str: 上午/中午/下午/晚上), hours(float), notes(str, 20-50 字)), total_hours(float))",
    "food_recommendations": "list[dict], name(str), type(str), location(str), signature(str), price_range(str)",
    "score": "int, 0-100, 整体推荐度评分，必须基于真实天气、天数匹配度、景点丰富度、交通便利度综合给出",
    "score_breakdown": "dict, days_match(int 0-100, 天数与城市体量匹配度), weather(int 0-100, 天气友好度), attraction_density(int 0-100, 景点丰富度), transport(int 0-100, 交通便利度)",
    "recommendation": "str, 4 选 1: '强烈推荐' (score>=85) / '推荐' (70-84) / '勉强可行' (50-69) / '建议改期' (<50)",
    "weather_strategy": "str, 100-200 字, 雨天/极端天气的活动调整策略与替代方案, 必须针对真实天气给出具体调整",
    "transportation_tips": "str, 50-100 字",
    "accommodation_tips": "str, 50-100 字",
    "general_tips": "list[str], 3-5 条实用贴士, 每条 10-30 字",
}

DEFAULT_BACKGROUND: str = (
    "你是一名资深的本地旅行规划师，对中国各城市的旅游资源了如指掌，"
    "尤其熟悉城市及其周边 150 公里范围内的景点、小众玩法和当季特色。"
    "你擅长基于真实天气预报调整行程：雨天主动切换室内/替代方案，连续极端天气应给出低分和建议改期。"
    "你的任务是基于用户的旅行日期和真实天气数据，为指定城市生成一份真实、可落地、"
    "天气适配的多日旅行方案，并给出诚实的整体推荐度评分（0-100）。"
)

DEFAULT_REQUIREMENTS: list = [
    "景点列表中必须包含至少 3 个位于近郊或周边县城的景点（is_nearby 标为 true），例如洛阳必须包含老君山、白云山、重渡沟等。",
    "每日行程至少提供 2 条主题不同的候选路线（routes），路线之间应有明显的主题或风格差异。",
    "每条路线的活动安排必须符合实际地理距离和交通耗时，单日总时长不超过 14 小时。",
    "所有景点和餐厅必须真实存在，不允许编造。",
    "score 必须综合考虑:天数与城市体量匹配度、天气友好度、景点丰富度、交通便利度。",
    "若行程中包含雷暴/极端天气日，score 应显著降低，recommendation 应为 '建议改期' 或 '勉强可行'。",
    "雨天应主动调整活动为室内/替代方案（如博物馆/美食/文化），并在 weather_hint 和 weather_strategy 中说明。",
    "请确保输出的 JSON 严格符合指定的结构和类型要求，所有 list 字段必须是真正的 list。",
]

LITE_TARGET_STRUCTURE: dict = {
    "city": "str, 城市名称",
    "score": "int, 0-100, 整体推荐度评分",
    "score_breakdown": "dict, days_match(int 0-100), weather(int 0-100), attraction_density(int 0-100), transport(int 0-100)",
    "recommendation": "str, '强烈推荐' (>=85) / '推荐' (70-84) / '勉强可行' (50-69) / '建议改期' (<50)",
    "weather_summary": "str, 50-80 字, 行程期间天气概况及影响",
    "weather_strategy": "str, 80-150 字, 雨天/雷暴天的活动调整策略",
    "top_attractions": "list[str], 3-5 个最值得去的景点 (含近郊/周边县城的景点)",
    "key_highlights": "str, 100-150 字, 行程核心亮点与最适配的玩法",
}

LITE_BACKGROUND: str = (
    "你是一名资深的本地旅行规划师，擅长基于真实天气和旅行日期，为指定城市生成简洁但有判断力的多日旅行评估。"
    "你的输出会被聚合到一个二维矩阵中, 供用户横向比较不同时长和出发日的优劣。"
)

LITE_REQUIREMENTS: list = [
    "score 必须综合考虑:天数与城市体量匹配度、天气友好度、景点丰富度、交通便利度。",
    "若行程中包含雷暴/极端天气日, score 应显著降低 (< 50), recommendation 选 '建议改期'。",
    "若多日降雨, score 应中低 (50-70), recommendation 选 '勉强可行' 或 '推荐'。",
    "top_attractions 必须包含至少 2 个近郊或周边县城的景点。",
    "输出应简洁, 重点在评分和策略, 不需要详细 daily_plan。",
    "所有景点必须真实存在, 不允许编造。",
]
