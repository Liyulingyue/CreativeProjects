"""Chat endpoint: natural-language replan with simple intent extraction."""

from __future__ import annotations

import json
import re
from typing import Optional

from . import config, data_loader, llm_client, planner
from .models import PlanRequest, ReplanRequest, Route


CHAT_SYSTEM = (
    "你是「红山省力Agent」，一个在南京红山森林动物园工作的私人导游。"
    "你正在和游客对话，他们已经有一条规划好的路线（或刚刚开始）。"
    "\n你的任务：\n"
    "1. 简短自然地回应（<60字）\n"
    "2. 如果游客表达了明确约束（累了、想看更多、想换场馆、要厕所、避雨、怕晒），"
    "就在 extracted_constraint 里返回 JSON（见下）\n"
    "3. 如果游客说要重新规划，把 suggested_replan 设为 true\n"
    "\n红山梗：大猩猩野菜F4 / 大熊猫 / 细尾獴站岗 / 小熊猫撞脸不撞DNA / 唐家河2025新馆\n"
    "语气：亲切、不端着、自带梗。"
)


CONSTRAINT_SCHEMA = {
    "type": "str, one of: shorter_route | longer_route | skip_venue | add_venue | rest_now | shade_only | specific_animal",
    "venue_id": "str (optional), 涉及到的场馆 ID",
    "animal": "str (optional), 涉及的动物",
    "note": "str (optional), 补充说明",
}


CHAT_TARGET = {
    "reply": "str, 简短中文回复（<60字），不要 emoji",
    "suggested_replan": "bool, 是否建议重新规划",
    "extracted_constraint": "dict, 抽取的约束（如果有），按 CONSTRAINT_SCHEMA；无则 null",
}


CHAT_REQUIREMENTS = [
    "reply 用中文，必须 < 60 字",
    "如果用户表达明确意图，extracted_constraint 必须填",
    "如果游客只是聊天/问问题，extracted_constraint 可以为 null",
    "suggested_replan 当用户希望改变路线时设为 true",
]


SIMPLE_RULES = [
    # (pattern, constraint_type, response_template)
    (r"累了|走不动|休息", "rest_now", "理解，咱找个有座椅的地方歇会儿，要不要我重新规划一下？"),
    (r"晒|太热|避阴|阴凉", "shade_only", "太阳确实晒，我帮你把后续路线都换成有遮阴的馆。"),
    (r"少走|短点|轻松", "shorter_route", "好嘞，那就少逛几个馆，把节奏放慢。"),
    (r"多看|多逛|多去几个", "longer_route", "没问题，给你多塞几个必看馆。"),
    (r"上厕所|卫生间|wc", "rest_now", "最近的厕所在场馆出口附近，要不要我标记一下？"),
]


def _rule_based_reply(message: str) -> Optional[dict]:
    """Fast path: regex-based intent extraction."""
    text = message.strip()
    for pattern, ctype, template in SIMPLE_RULES:
        if re.search(pattern, text):
            return {
                "reply": template,
                "suggested_replan": True,
                "extracted_constraint": {"type": ctype, "note": text},
            }
    return None


async def chat(req) -> dict:
    """Process chat message, return reply + extracted constraint."""
    # Fast path
    rule_reply = _rule_based_reply(req.message)
    if rule_reply:
        return rule_reply

    # Slow path: LLM
    if not llm_client.is_llm_enabled():
        return {
            "reply": "收到。LLM未启用，我用规则引擎理解：你想说什么？",
            "suggested_replan": False,
            "extracted_constraint": None,
        }

    context = ""
    if req.current_route:
        stops = req.current_route.get("stops", [])
        names = [s.get("venue_name", "") for s in stops]
        context += f"\n当前路线：{' → '.join(names)}\n"

    messages = [
        {"role": "system", "content": CHAT_SYSTEM + context},
    ]
    for h in req.history[-6:]:
        messages.append(h)
    messages.append({"role": "user", "content": req.message})

    result = llm_client.chat_json(
        messages=messages,
        target_structure=CHAT_TARGET,
        background=CHAT_SYSTEM + context,
        requirements=CHAT_REQUIREMENTS,
        overall_timeout=60.0,
    )
    if result.get("error") or not result.get("data"):
        return {
            "reply": "我收到了，让我再想想…",
            "suggested_replan": False,
            "extracted_constraint": None,
        }
    data = result["data"]
    return {
        "reply": data.get("reply", "好的"),
        "suggested_replan": bool(data.get("suggested_replan", False)),
        "extracted_constraint": data.get("extracted_constraint"),
    }


def apply_chat_constraint(current_route: dict, constraint: dict, prefs: dict) -> Optional[Route]:
    """Given current route + a constraint, return a new adjusted Route.

    Returns None if constraint can't be applied.
    """
    ctype = (constraint or {}).get("type")
    feedback_map = {
        "shorter_route": "我想要更短、更轻松的路线",
        "longer_route": "我想要逛更多馆，看得更细",
        "rest_now": "我现在很累，想坐着歇会儿",
        "shade_only": "太阳太晒了，给我换个有遮阴的路线",
        "skip_venue": f"跳过 {constraint.get('venue_id', '某个馆')}",
        "add_venue": f"加上 {constraint.get('animal', '更多动物')}",
    }
    if ctype not in feedback_map:
        return None

    # Build a fake ReplanRequest and use existing replan logic
    replan_req = ReplanRequest(
        original_route=current_route,
        current_venue_id=None,
        elapsed_minutes=0,
        feedback=feedback_map[ctype],
    )
    new_route, _ = planner.replan_route(current_route, replan_req)
    return new_route