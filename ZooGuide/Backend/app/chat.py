"""Chat: 多轮对话 + 实体识别 + 主动追问.

Agent能力层次:
  1. Regex快路径: 简单意图(累/晒/休息) → 直接回复 + 自动replan
  2. 实体识别: 用户提到的"大熊猫"/"panda馆" → 匹配到venue_id
  3. LLM路径: 复杂场景，结合历史上下文
  4. 追问: 当用户表达模糊时，Agent主动问
"""

from __future__ import annotations

import json
import re
from datetime import datetime, timedelta
from typing import Optional

from . import config, data_loader, llm_client, planner
from .models import PlanRequest, ReplanRequest, Route


# Regex快路径
SIMPLE_RULES = [
    (r"^(你好|hi|hello|嗨|哈喽)", "greeting", "嗨！我是你的红山导游。今天想怎么逛？"),
    (r"(累了|走不动|好累|脚酸)", "rest_now", "理解，咱找个有座椅的地方歇会儿，要不要我重新规划一下？"),
    (r"(晒|太热|避阴|阴凉|出汗)", "shade_only", "太阳确实晒，我帮你把后续路线都换成有遮阴的馆。"),
    (r"(少走|短点|轻松|慢点)", "shorter_route", "好嘞，那就少逛几个馆，把节奏放慢。"),
    (r"(多看|多逛|多去|加几个)", "longer_route", "没问题，给你多塞几个必看馆。"),
    (r"(上厕所|卫生间|wc|厕所)", "rest_now", "最近的厕所在场馆出口附近，要不要我标记一下？"),
    (r"(饿|吃东西|吃饭|餐厅)", "rest_now", "红山有几家小餐厅（北门/中心广场附近），要不要我调整路线顺路去？"),
    (r"(谢谢|感谢|多谢|thanks)", "thanks", "不客气！玩得开心最重要。"),
]


# 实体识别：场馆名/动物名 → venue_id
ENTITY_MAP = {
    # 场馆名
    "大熊猫馆": "panda", "熊猫馆": "panda", "熊猫": "panda",
    "考拉馆": "koala", "考拉": "koala",
    "大猩猩馆": "gorilla", "大猩猩": "gorilla", "野菜F4": "gorilla", "野菜": "gorilla",
    "虎馆": "tiger", "老虎": "tiger", "东北虎": "tiger", "孟加拉虎": "tiger",
    "中国猫科馆": "china_cat",
    "猫科星球": "cat_planet",
    "长颈鹿馆": "giraffe", "长颈鹿": "giraffe",
    "亚洲象馆": "asian_elephant", "亚洲象": "asian_elephant", "大象": "asian_elephant",
    "猩猩馆": "orangutan", "黑猩猩": "orangutan", "红猩猩": "orangutan", "猩猩": "orangutan",
    "亚洲灵长区": "asian_primates", "长臂猿": "asian_primates", "山魈": "asian_primates",
    "小熊猫馆": "red_panda", "小熊猫": "red_panda",
    "袋鼠角": "kangaroo", "袋鼠": "kangaroo", "澳洲": "kangaroo",
    "狐猴岛": "lemur", "马岛客厅": "lemur", "狐猴": "lemur", "环尾狐猴": "lemur",
    "犀牛领地": "rhino", "犀牛": "rhino",
    "犀鸟馆": "hornbill", "犀鸟": "hornbill",
    "鹤园": "crane", "丹顶鹤": "crane",
    "狼馆": "wolf", "狼": "wolf",
    "熊馆": "bear", "黑熊": "bear", "马来熊": "bear", "熊": "bear",
    "猴山": "monkey_mountain", "猕猴": "monkey_mountain",
    "细尾獴馆": "meerkat", "细尾獴": "meerkat", "獴": "meerkat",
    "唐家河展区": "tangjiahe", "唐家河": "tangjiahe", "川金丝猴": "tangjiahe",
    "冈瓦纳展区": "gonwana", "冈瓦纳": "gonwana",
    "大壮观阁": "dazhuangguange",
}


def extract_entity(text: str) -> Optional[dict]:
    """从用户消息中识别场馆/动物名."""
    text_lower = text
    for name, vid in ENTITY_MAP.items():
        if name in text or name in text_lower:
            v = data_loader.get_venue_dict_by_id(vid)
            if v:
                return {"type": "venue", "venue_id": vid, "venue_name": v["name"]}
    return None


# 多轮对话system prompt
CHAT_SYSTEM = (
    "你是「红山省力Agent」，在南京红山森林动物园工作。"
    "你和游客正在多轮对话，了解他们的偏好后可以重新规划路线。\n"
    "\n"
    "【多轮对话规则】\n"
    "1. **记住上下文**：用户之前说过的偏好（累/晒/某动物）要延续\n"
    "2. **实体识别**：用户提到「熊猫」「考拉」「唐家河」等，自动知道是哪个馆\n"
    "3. **追问策略**：当用户表达模糊时（如「想去XX」「看更多」），可以反问1个具体问题\n"
    "4. **回复简短**：<60字，不要emoji，不要重复自己说过的话\n"
    "\n"
    "【输出 JSON 结构】\n"
    "{\n"
    '  "reply": "简短回复",\n'
    '  "suggested_replan": bool,  // 是否需要重新规划路线\n'
    '  "extracted_constraint": {  // 用户新表达的约束（可空）\n'
    '    "type": "shorter_route | longer_route | skip_venue | add_venue | rest_now | shade_only | specific_animal",\n'
    '    "venue_id": "可选，涉及的场馆ID",\n'
    '    "venue_name": "可选",\n'
    '    "animal": "可选",\n'
    '    "note": "原始用户表达"\n'
    '  },\n'
    '  "questions": ["可选，反问用户的问题"]\n'
    '}\n'
    "\n"
    "【红山梗库】\n"
    "- 大熊猫：国民顶流、国宝\n"
    "- 大猩猩野菜F4：香椿头/马兰头/小蒜头/枸杞头\n"
    "- 细尾獴：网红'站岗'画面\n"
    "- 小熊猫：和大熊猫撞脸不撞DNA（趋同进化）\n"
    "- 唐家河：2025年10月新开的保护区复制馆\n"
    "- 冈瓦纳：生命进化主题\n"
)


CHAT_TARGET = {
    "reply": "str, 简短中文回复（<60字），不要 emoji",
    "suggested_replan": "bool, 是否需要重新规划",
    "extracted_constraint": "dict 或 null, 抽取的约束",
    "questions": "list[str], 反问（0-1个）",
}


CHAT_REQUIREMENTS = [
    "reply 中文 < 60字",
    "extracted_constraint 仅在用户表达明确意图时填",
    "questions 仅当用户意图模糊时反问",
    "考虑多轮上下文，不要重复",
    "suggested_replan 当且仅当 extracted_constraint 触发 replan",
]


def _rule_based_reply(message: str, history: list[dict]) -> Optional[dict]:
    """Regex快路径：常见意图直接回复."""
    text = message.strip()

    # 检测实体
    entity = extract_entity(text)
    if entity and any(w in text for w in ["看", "去", "加", "想"]):
        return {
            "reply": f"好的，把「{entity['venue_name']}」加入路线。",
            "suggested_replan": True,
            "extracted_constraint": {
                "type": "add_venue",
                "venue_id": entity["venue_id"],
                "venue_name": entity["venue_name"],
                "note": text,
            },
            "questions": [],
        }

    if entity and any(w in text for w in ["跳", "不", "去掉", "不要"]):
        return {
            "reply": f"好的，跳过「{entity['venue_name']}」。",
            "suggested_replan": True,
            "extracted_constraint": {
                "type": "skip_venue",
                "venue_id": entity["venue_id"],
                "venue_name": entity["venue_name"],
                "note": text,
            },
            "questions": [],
        }

    for pattern, ctype, template in SIMPLE_RULES:
        if re.search(pattern, text):
            return {
                "reply": template,
                "suggested_replan": ctype not in ("greeting", "thanks"),
                "extracted_constraint": {"type": ctype, "note": text} if ctype not in ("greeting", "thanks") else None,
                "questions": [],
            }

    # 模糊表达：反问
    if any(w in text for w in ["随便", "都行", "你定", "不知道"]):
        return {
            "reply": "那我推荐你先去大熊猫馆（离北门最近），然后细尾獴（网红站岗），你觉得如何？",
            "suggested_replan": False,
            "extracted_constraint": None,
            "questions": ["你比较想看：猫科 / 灵长类 / 网红打卡？"],
        }

    return None


async def chat(req) -> dict:
    """Multi-turn chat with context."""
    history = req.history or []

    # 快路径
    rule_reply = _rule_based_reply(req.message, history)
    if rule_reply:
        # Apply replan if needed
        if req.current_route and rule_reply.get("suggested_replan") and rule_reply.get("extracted_constraint"):
            try:
                new_route = apply_chat_constraint(req.current_route, rule_reply["extracted_constraint"], req.prefs or {})
                if new_route:
                    rule_reply["new_route"] = new_route.model_dump()
            except Exception as e:
                print(f"[warn] rule apply_constraint failed: {e}")
        return rule_reply

    # LLM 路径
    if not llm_client.is_llm_enabled():
        return {
            "reply": "我在听，但LLM没开。你可以说：累/晒/想去XX馆/要多逛",
            "suggested_replan": False,
            "extracted_constraint": None,
            "questions": [],
        }

    # Build context
    ctx_parts = []
    if req.current_route:
        stops = req.current_route.get("stops", [])
        names = [s.get("venue_name", "") for s in stops]
        ctx_parts.append(f"当前路线：{' → '.join(names)}")
    if history:
        ctx_parts.append("对话历史：")
        for h in history[-6:]:
            role = h.get("role", "user")
            content = h.get("content", "")
            ctx_parts.append(f"  {role}: {content}")
    if req.prefs:
        ctx_parts.append(f"用户偏好：{json.dumps(req.prefs, ensure_ascii=False)[:200]}")
    context = "\n".join(ctx_parts) if ctx_parts else ""

    messages = [{"role": "system", "content": CHAT_SYSTEM + "\n\n" + context}]
    for h in history[-6:]:
        messages.append({"role": h.get("role", "user"), "content": h.get("content", "")})
    messages.append({"role": "user", "content": req.message})

    result = llm_client.chat_json(
        messages=messages,
        target_structure=CHAT_TARGET,
        background=CHAT_SYSTEM + "\n\n" + context,
        requirements=CHAT_REQUIREMENTS,
        overall_timeout=60.0,
    )

    if result.get("error") or not result.get("data"):
        return {
            "reply": "我想想…你能再说一遍吗？",
            "suggested_replan": False,
            "extracted_constraint": None,
            "questions": [],
        }
    data = result["data"]
    out = {
        "reply": data.get("reply", "好的"),
        "suggested_replan": bool(data.get("suggested_replan", False)),
        "extracted_constraint": data.get("extracted_constraint"),
        "questions": data.get("questions", []) or [],
    }
    # Apply replan if LLM detected constraint
    if req.current_route and out.get("suggested_replan") and out.get("extracted_constraint"):
        try:
            new_route = apply_chat_constraint(
                req.current_route, out["extracted_constraint"], req.prefs or {}
            )
            if new_route:
                out["new_route"] = new_route.model_dump()
        except Exception as e:
            print(f"[warn] LLM apply_constraint failed: {e}")
    return out


def apply_chat_constraint(current_route: dict, constraint: dict, prefs: dict) -> Optional[Route]:
    """Apply constraint to current route → new route.

    Uses rule-based greedy (fast) - chat should be snappy, not LLM slow.
    For chat-triggered replans, we use a hybrid approach:
      - If add_venue/skip_venue: just modify the stops list directly
      - For other constraints: trigger rule-based replan (force_fast=True)
    """
    ctype = (constraint or {}).get("type")
    vid = (constraint or {}).get("venue_id")

    # Deep copy to avoid mutating caller's dict
    import copy
    current_route = copy.deepcopy(current_route)

    # For add/skip, modify directly without LLM
    if ctype == "add_venue" and vid:
        stops = current_route.get("stops", [])
        existing_ids = {s.get("venue_id") for s in stops}
        if vid not in existing_ids:
            venue = data_loader.get_venue_dict_by_id(vid)
            if venue:
                new_stop = {
                    "venue_id": vid,
                    "venue_name": venue["name"],
                    "arrive_time": "",
                    "leave_time": "",
                    "visit_minutes": venue.get("recommended_visit_minutes", 20),
                    "walk_to_next_minutes": 0,
                    "narration": f"这是你新想加的馆：{venue['name']}。",
                    "tips": [],
                    "rest_here": False,
                }
                # Insert before last stop
                if len(stops) > 0:
                    stops.insert(len(stops) - 1, new_stop)
                else:
                    stops.append(new_stop)
                base = datetime.now().replace(hour=9, minute=0)
                cur_t = base + timedelta(minutes=5)
                for i, s in enumerate(stops):
                    visit = s.get("visit_minutes", 20)
                    s["arrive_time"] = cur_t.strftime("%H:%M")
                    cur_t = cur_t + timedelta(minutes=visit)
                    s["leave_time"] = cur_t.strftime("%H:%M")
                    if i + 1 < len(stops):
                        from .walking import get_inter_venue_minutes
                        walk = get_inter_venue_minutes(s["venue_id"], stops[i + 1]["venue_id"])
                        cur_t = cur_t + timedelta(minutes=walk)
                        stops[i + 1]["walk_to_next_minutes"] = walk
                total = sum(s.get("visit_minutes", 20) for s in stops)
                current_route["stops"] = stops
                current_route["total_minutes"] = total
                from .models import Route
                valid_keys = set(Route.model_fields.keys())
                filtered = {k: v for k, v in current_route.items() if k in valid_keys}
                return Route(**filtered)

    if ctype == "skip_venue" and vid:
        stops = current_route.get("stops", [])
        current_route["stops"] = [s for s in stops if s.get("venue_id") != vid]
        if current_route["stops"]:
            from .models import Route
            return Route(**{k: v for k, v in current_route.items() if k in Route.model_fields})

    # For other constraints, use rule-based replan
    feedback_map = {
        "shorter_route": "我想要更短、更轻松的路线",
        "longer_route": "我想要逛更多馆，看得更细",
        "rest_now": "我现在很累，想坐着歇会儿",
        "shade_only": "太阳太晒了，给我换个有遮阴的路线",
        "specific_animal": f"我特别想看 {constraint.get('animal', '某动物')}",
    }
    feedback = feedback_map.get(ctype, "调整路线")

    replan_req = ReplanRequest(
        original_route=current_route,
        current_venue_id=None,
        elapsed_minutes=0,
        feedback=feedback,
    )
    # Use force_fast=True for chat (don't wait for LLM)
    new_route, _ = planner.replan_route(current_route, replan_req, force_fast=True)
    return new_route