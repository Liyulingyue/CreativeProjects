"""LLM prompt templates for route planning and replanning."""

from __future__ import annotations

from .data_loader import get_meta


def _build_system_background() -> str:
    meta = get_meta()
    name = meta.get("name", "动物园")
    short = meta.get("short_name", name[:2])
    extras = meta.get("prompt_extras", {})
    template = extras.get("planner_background", "")

    if not template:
        return f"你是「{short}省力Agent」，帮助游客规划路线。"

    highlights = meta.get("highlights", [])
    highlights_block = "\n".join(f"- {h}" for h in highlights) if highlights else ""

    areas = meta.get("areas", {})
    if areas:
        highlights_block += "\n"
        for k, v in areas.items():
            highlights_block += f"\n- {k}：{v}"

    return template.format(name=name, short_name=short, highlights_block=highlights_block)


SYSTEM_BACKGROUND: str = _build_system_background()


PLAN_REQUIREMENTS: list[str] = [
    "输出必须是严格的 JSON，严格符合 target_structure 定义，不要任何额外文字或 markdown 包裹",
    "每条 stop 必须是候选场馆 ID 中真实存在的 ID",
    "总时长（含步行）不得超过 available_hours × 60 分钟",
    "相邻 stop 的 walk_to_next_minutes 必须使用 walking_matrix 中的真实值，不要凭空编造",
    "narration 必须针对该游客画像（同场馆不同游客应有不同讲解风格）",
    "stops 数量 3-8 个最合适；少于 2 个或超过 8 个都不合理",
    "如果 available_hours < 1.5，至少保留 1 个 must_see=true 的场馆",
    "如果 with_kids=true，narration 要有童趣",
    "如果 sun_tolerance <=2，优先选择 shaded=true 的场馆",
    "如果 willing_to_hike=false，避免坡度大的场馆，路线减少高差大的片区",
    "warnings 复用通用警告 + 针对该用户的额外提示",
    "summary 用一段自然语言总结这条路线的精髓，60-100 字",
    "tips 给 2-3 条针对该用户的具体建议（如『带娃节奏建议每1.5小时休息一次』）",
    "动物 active 程度说明可以提，但不要说绝对时间（如『一般上午活跃』即可）",
]


REPLAN_REQUIREMENTS: list[str] = [
    "输出必须是严格的 JSON，严格符合 target_structure 定义",
    "stops 必须从 current_venue_id 之后的下一个场馆开始（不包括已经走过的）",
    "总剩余时长（含步行）不得超过 (available_hours - elapsed_minutes) × 60 分钟",
    "根据 feedback 调整风格：\n"
    "  ·『累了/晒了/走不动』→ 减少 stops，增加 rest_here=true 的场馆\n"
    "  ·『想看更多』→ 多塞 1-2 个 must_see=false 的深度场馆\n"
    "  ·『娃饿了/要上厕所』→ 提示就近的休息点，narration 加安抚语句\n"
    "narration 要呼应用户的反馈，让用户感觉『这个 Agent 听懂了』",
]


# JSON schema the model must produce for /plan
PLAN_TARGET_STRUCTURE: dict = {
    "id": "str, 路线 ID，如 r_xxx",
    "summary": "str, 60-100 字路线整体概述，叙事化",
    "total_minutes": "int, 包含参观+步行的总时长",
    "total_walk_minutes": "int, 纯步行时长（分钟）",
    "stops": [
        {
            "venue_id": "str, 必须匹配候选场馆 ID",
            "venue_name": "str, 场馆中文名",
            "arrive_time": "str, HH:MM 格式",
            "leave_time": "str, HH:MM 格式",
            "visit_minutes": "int, 实际参观时长（10-45 分钟）",
            "walk_to_next_minutes": "int, 到下一场馆的步行分钟数",
            "narration": "str, 50-100 字个性化讲解词",
            "tips": ["str, 该场馆的温馨提示，每条 10-25 字"],
            "rest_here": "bool, 是否建议在此坐下来歇脚",
        }
    ],
    "warnings": ["str, 通用或针对该用户的警告"],
    "tips": ["str, 针对该用户画像的 2-3 条建议"],
}


# Same shape for replan
REPLAN_TARGET_STRUCTURE: dict = PLAN_TARGET_STRUCTURE
