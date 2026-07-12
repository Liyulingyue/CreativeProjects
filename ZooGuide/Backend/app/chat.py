"""Chat: Agent + Tool Calling.

架构:
  1. Regex快路径: 简单意图(累/晒/打招呼) → 直接回复 + 自动修改路线
  2. Agent路径: LLM + Tool Calling
     - search_venues: 关键词搜索场馆
     - modify_route: 修改当前路线
  3. Agent可多轮调用工具，直到生成最终回复
"""

from __future__ import annotations

import copy
import json
import re
from pathlib import Path
from typing import Optional

from . import config, data_loader, llm_client, planner
from .models import ReplanRequest, Route


# Regex快路径：常见意图直接回复，省LLM调用
SIMPLE_RULES = [
    (r"^(你好|hi|hello|嗨|哈喽)", None, "嗨！我是你的红山导游。今天想怎么逛？"),
    (r"(累了|走不动|好累|脚酸)", "rest_now", "理解，咱找个有座椅的地方歇会儿。"),
    (r"(晒|太热|避阴|阴凉|出汗)", "shade_only", "太阳确实晒，我帮你把后续路线都换成有遮阴的馆。"),
    (r"(少走|短点|轻松|慢点)", "shorter_route", "好嘞，那就少逛几个馆，把节奏放慢。"),
    (r"(多看|多逛|多去|加几个)", "longer_route", "没问题，给你多塞几个必看馆。"),
    (r"(上厕所|卫生间|wc|厕所)", None, "最近的厕所在场馆出口附近，要不要我标记一下？"),
    (r"(饿|吃东西|吃饭|餐厅)", "rest_now", "红山有几家小餐厅（北门/中心广场附近），要不要我调整路线顺路去？"),
    (r"(谢谢|感谢|多谢|thanks)", None, "不客气！玩得开心最重要。"),
]

# 实体识别：场馆名/动物名 → venue_id
ENTITY_MAP = {
    "大熊猫馆": "panda", "熊猫馆": "panda", "熊猫": "panda",
    "考拉馆": "koala", "考拉": "koala",
    "大猩猩馆": "gorilla", "大猩猩": "gorilla", "野菜F4": "gorilla", "野菜": "gorilla",
    "虎馆": "tiger", "老虎": "tiger", "东北虎": "tiger", "孟加拉虎": "tiger",
    "长颈鹿馆": "giraffe", "长颈鹿": "giraffe",
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
}


TOOLS = [
    {
        "type": "function",
        "function": {
            "name": "search_venues",
            "description": "搜索场馆信息，按名称、动物、区域等关键词查找",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "搜索关键词，如'熊猫'、'猫科'、'大红山'",
                    }
                },
                "required": ["query"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "modify_route",
            "description": "修改当前游览路线。仅在用户明确要求调整路线时才调用。调用前必须先向用户确认修改方案，等用户同意后再调用。如果用户只是在聊天、问问题、闲聊，不要调用此工具。",
            "parameters": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["add_venue", "skip_venue", "shorter", "longer", "rest", "shade"],
                        "description": "add_venue=添加场馆, skip_venue=跳过场馆, shorter=更短轻松, longer=逛更多, rest=需要休息, shade=要遮阴",
                    },
                    "venue_id": {
                        "type": "string",
                        "description": "场馆ID，add_venue/skip_venue时必填",
                    },
                },
                "required": ["action"],
            },
        },
    },
]


AGENT_SYSTEM = ""


def _load_system_prompt() -> str:
    global AGENT_SYSTEM
    try:
        with (Path(__file__).resolve().parent.parent / "data" / "system.json").open(encoding="utf-8") as f:
            cfg = json.load(f)
        parts = [cfg["agent_identity"]]
        if cfg.get("rules"):
            parts.append("\n【规则】\n" + "\n".join(f"{i+1}. {r}" for i, r in enumerate(cfg["rules"])))
        if cfg.get("venue_facts"):
            lines = []
            for vid, v in cfg["venue_facts"].items():
                animals = "、".join(v.get("animals", []))
                tags = "、".join(v.get("tags", []))
                lines.append(f"- {vid}({v['name']}，{v.get('area', '')}，动物：{animals}，标签：{tags})")
            parts.append("\n【场馆速查】\n" + "\n".join(lines))
        if cfg.get("fun_facts"):
            parts.append("\n【红山梗】\n" + "\n".join(f"- {f}" for f in cfg["fun_facts"]))
        if cfg.get("areas"):
            parts.append("\n【区域】\n" + "\n".join(f"- {k}：{v}" for k, v in cfg["areas"].items()))
        if cfg.get("tips"):
            parts.append("\n【应对建议】\n" + "\n".join(f"- {k}：{v}" for k, v in cfg["tips"].items()))
        AGENT_SYSTEM = "\n".join(parts)
    except Exception as e:
        AGENT_SYSTEM = "你是红山省力Agent，帮助游客规划路线。"
    return AGENT_SYSTEM


def _rule_based_reply(message: str) -> Optional[dict]:
    text = message.strip()

    # 实体识别：加/跳场馆
    entity = _extract_entity(text)
    if entity and any(w in text for w in ["看", "去", "加", "想"]):
        return {
            "reply": f"好的，把「{entity['venue_name']}」加入路线。",
            "route_action": "add_venue",
            "venue_id": entity["venue_id"],
        }
    if entity and any(w in text for w in ["跳", "不", "去掉", "不要"]):
        return {
            "reply": f"好的，跳过「{entity['venue_name']}」。",
            "route_action": "skip_venue",
            "venue_id": entity["venue_id"],
        }

    for pattern, action, template in SIMPLE_RULES:
        if re.search(pattern, text):
            return {"reply": template, "route_action": action}
    return None


def _extract_entity(text: str) -> Optional[dict]:
    for name, vid in ENTITY_MAP.items():
        if name in text:
            v = data_loader.get_venue_dict_by_id(vid)
            if v:
                return {"type": "venue", "venue_id": vid, "venue_name": v["name"]}
    return None


def _search_venues(query: str) -> str:
    q = query.lower()
    results = []
    for v in data_loader.get_all_venue_dicts():
        if (
            q in v.get("name", "").lower()
            or q in v.get("area", "").lower()
            or q in v.get("id", "").lower()
            or any(q in a.lower() for a in v.get("animals", []))
        ):
            results.append(v)
    if not results:
        return f"没有找到与「{query}」相关的场馆"
    lines = []
    for v in results[:8]:
        animals = "、".join(v.get("animals", [])[:3])
        lines.append(f"{v['id']}: {v['name']}（{v.get('area', '')}，动物：{animals}）")
    return "\n".join(lines)


def _execute_modify_route(action: str, venue_id: str | None, current_route: dict) -> tuple[str, Optional[Route]]:
    action_map = {
        "add_venue": "add_venue",
        "skip_venue": "skip_venue",
        "shorter": "shorter_route",
        "longer": "longer_route",
        "rest": "rest_now",
        "shade": "shade_only",
    }
    ctype = action_map.get(action, action)
    constraint = {"type": ctype}
    if venue_id:
        constraint["venue_id"] = venue_id
        venue = data_loader.get_venue_dict_by_id(venue_id)
        if venue:
            constraint["venue_name"] = venue["name"]
    try:
        new_route = apply_chat_constraint(current_route, constraint, {})
        if new_route:
            return "路线已修改", new_route
        return "路线修改失败", None
    except Exception as e:
        return f"修改出错：{e}", None


def _execute_tool(name: str, args: dict, current_route: dict | None) -> tuple[str, Optional[Route]]:
    if name == "search_venues":
        return _search_venues(args.get("query", "")), None
    if name == "modify_route":
        if not current_route:
            return "当前没有路线，请先规划路线", None
        return _execute_modify_route(args.get("action", ""), args.get("venue_id"), current_route)
    return f"未知工具: {name}", None


async def chat(req) -> dict:
    # Regex快路径：默认关闭，所有消息走Agent
    # 如需启用，将 USE_REGEX_FAST_PATH 改为 True
    if config.CHAT_REGEX_FAST_PATH:
        rule = _rule_based_reply(req.message)
        if rule:
            new_route = None
            if rule.get("route_action") and req.current_route:
                try:
                    constraint = {"type": rule["route_action"]}
                    if rule.get("venue_id"):
                        constraint["venue_id"] = rule["venue_id"]
                        constraint["venue_name"] = rule.get("venue_name", "")
                    new_route = apply_chat_constraint(
                        req.current_route, constraint, req.prefs or {}
                    )
                except Exception:
                    pass
            return {
                "reply": rule["reply"],
                "new_route": new_route.model_dump() if new_route else None,
            }

    if not llm_client.is_llm_enabled():
        return {
            "reply": "我在听，但LLM没开。你可以说：累/晒/想去XX馆/要多逛",
            "new_route": None,
        }

    messages = [{"role": "system", "content": _load_system_prompt()}]
    if req.current_route:
        stops = req.current_route.get("stops", [])
        names = [s.get("venue_name", "") for s in stops]
        messages[0]["content"] += f"\n\n当前路线：{' → '.join(names)}"
    for h in (req.history or []):
        messages.append({"role": h.get("role", "user"), "content": h.get("content", "")})
    messages.append({"role": "user", "content": req.message})

    client = llm_client.get_client()
    current_route = copy.deepcopy(req.current_route) if req.current_route else None
    final_new_route = None

    for _ in range(5):
        try:
            response = client.chat.completions.create(
                model=config.MODEL_NAME,
                messages=messages,
                tools=TOOLS,
                tool_choice="auto",
            )
        except Exception:
            return {"reply": "想了一下，你能再说一遍吗？", "new_route": None}

        choice = response.choices[0]

        if choice.finish_reason == "tool_calls" and choice.message.tool_calls:
            messages.append(choice.message)
            for tc in choice.message.tool_calls:
                try:
                    args = json.loads(tc.function.arguments)
                except json.JSONDecodeError:
                    args = {}
                result, new_route = _execute_tool(tc.function.name, args, current_route)
                if new_route:
                    current_route = json.loads(new_route.model_dump_json())
                    final_new_route = new_route
                messages.append(
                    {"role": "tool", "tool_call_id": tc.id, "content": result}
                )
        else:
            return {
                "reply": choice.message.content or "好的",
                "new_route": final_new_route.model_dump() if final_new_route else None,
            }

    return {"reply": "我想想，你能再说一遍吗？", "new_route": None}


def apply_chat_constraint(current_route: dict, constraint: dict, prefs: dict) -> Optional[Route]:
    ctype = (constraint or {}).get("type")
    vid = (constraint or {}).get("venue_id")

    current_route = copy.deepcopy(current_route)

    if ctype == "add_venue" and vid:
        stops = current_route.get("stops", [])
        existing_ids = {s.get("venue_id") for s in stops}
        if vid not in existing_ids:
            venue = data_loader.get_venue_dict_by_id(vid)
            if venue:
                from datetime import datetime, timedelta

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
                valid_keys = set(Route.model_fields.keys())
                filtered = {k: v for k, v in current_route.items() if k in valid_keys}
                return Route(**filtered)

    if ctype == "skip_venue" and vid:
        stops = current_route.get("stops", [])
        current_route["stops"] = [s for s in stops if s.get("venue_id") != vid]
        if current_route["stops"]:
            valid_keys = set(Route.model_fields.keys())
            return Route(**{k: v for k, v in current_route.items() if k in valid_keys})

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
    new_route, _ = planner.replan_route(current_route, replan_req, force_fast=True)
    return new_route
