"""Planner: orchestrates rule engine + LLM with graceful fallback."""

from __future__ import annotations

import json
import random
import uuid
from datetime import datetime, timedelta
from typing import Optional

from . import config, data_loader, llm_client, prompts
from .models import PlanRequest, ReplanRequest, Route, RouteStop
from .rule_engine import filter_and_rank, max_stops_by_time
from .walking import (
    build_walking_matrix,
    get_entry_venue_minutes,
    get_inter_venue_minutes,
)


def _hhmm(t: datetime) -> str:
    return t.strftime("%H:%M")


def _party_style_guide(party_type: str, with_kids: bool, kids_age: Optional[int]) -> str:
    quiz = data_loader.get_quiz_config()
    guides = quiz.get("party_style_guides", {})
    if with_kids:
        if kids_age is not None and kids_age <= 6:
            return guides.get("family_young", guides.get("default", "一般游客：平衡科普与趣味"))
        return guides.get("family_teen", guides.get("default", "一般游客：平衡科普与趣味"))
    return guides.get(party_type, guides.get("default", "一般游客：平衡科普与趣味"))


def _build_user_prompt_plan(prefs: PlanRequest, candidates: list[dict], walking_matrix: dict) -> str:
    style = _party_style_guide(prefs.party_type, prefs.with_kids, prefs.kids_age)
    # Only send top N candidates to LLM to keep prompt small
    top_n = min(6, len(candidates))
    top_candidates = candidates[:top_n]
    cand_brief = [
        {
            "id": c["id"],
            "name": c["name"],
            "animals": c.get("animals", [])[:3],
            "tags": [t for t in c.get("tags", []) if t in ("明星动物", "亲子", "网红", "2025新馆", "恒温", "有遮阴")],
            "themes": c.get("themes", [])[:2],
            "minutes": c.get("recommended_visit_minutes", 20),
            "shaded": c.get("shaded", False),
            "rest": c.get("rest_spots", False),
            "must": c.get("must_see", False),
        }
        for c in top_candidates
    ]

    # Truncate walking matrix to only top candidates
    top_ids = {c["id"] for c in top_candidates}
    matrix_trimmed = {
        a: {b: v for b, v in row.items() if b in top_ids}
        for a, row in walking_matrix.items()
        if a in top_ids
    }

    quiz = data_loader.get_quiz_config()
    tr = quiz.get("planner_translations", {})
    party_desc = tr.get("party_types", {}).get(prefs.party_type, prefs.party_type)
    stamina_map = quiz.get("stamina_descriptions", {})
    stamina_desc = stamina_map.get(str(prefs.stamina), f"体力{prefs.stamina}/5")
    sun_map = quiz.get("sun_descriptions", {})
    sun_desc = sun_map.get(str(prefs.sun_tolerance), f"防晒{prefs.sun_tolerance}/5")
    hike_map = quiz.get("hike_options", {})
    hike_desc = hike_map.get(str(prefs.willing_to_hike).lower(), "不想爬山" if not prefs.willing_to_hike else "愿意爬山")
    interest_tr = tr.get("interests", {})
    interests = "、".join([interest_tr.get(i, i) for i in prefs.animal_interests]) if prefs.animal_interests else tr.get("no_interest", "没有特别偏好，看什么都行")
    gate_tr = tr.get("gates", {})
    gate_desc = gate_tr.get(prefs.entry_gate, prefs.entry_gate)

    return f"""游客画像：{party_desc}{'，孩子'+str(prefs.kids_age)+'岁' if prefs.with_kids and prefs.kids_age else '，带娃' if prefs.with_kids else ''}，{stamina_desc}，{sun_desc}，{hike_desc}。感兴趣的动物：{interests}。从{gate_desc}入园，{prefs.start_time}开始，总共能玩{prefs.available_hours}小时。

导游风格：{style}

候选场馆({len(top_candidates)})：
{json.dumps(cand_brief, ensure_ascii=False)}

步行矩阵(分钟)：
{json.dumps(matrix_trimmed, ensure_ascii=False)}

要求：
- 总时长（含步行）≤ {int(prefs.available_hours * 60)} 分钟
- stops 3-{min(8, top_n)} 个，按 id 选
- 必看场馆优先
- narration 50-100 字
- 输出符合 target_structure 的 JSON"""


def _build_user_prompt_replan(
    original: dict,
    prefs: ReplanRequest,
    candidates: list[dict],
    walking_matrix: dict,
    remaining_candidates: list[dict],
) -> str:
    style = _party_style_guide(
        original.get("_party_type", "solo"),
        original.get("_with_kids", False),
        original.get("_kids_age"),
    )
    cand_brief = [
        {
            "id": c["id"],
            "name": c["name"],
            "area": c["area"],
            "animals": c.get("animals", []),
            "tags": c.get("tags", []),
            "must_see": c.get("must_see", False),
            "shaded": c.get("shaded", False),
            "rest_spots": c.get("rest_spots", False),
        }
        for c in remaining_candidates
    ]
    remaining_minutes = max(0, int(original.get("_available_hours", 3) * 60) - prefs.elapsed_minutes)

    return f"""【原始路线】
{json.dumps(original.get('stops', []), ensure_ascii=False)}

【当前状态】
- 已走到：{prefs.current_venue_id or '还未开始'}
- 已用时：{prefs.elapsed_minutes} 分钟
- 用户反馈：『{prefs.feedback}』
- 剩余时间：约 {remaining_minutes} 分钟

【讲解风格】{style}

【未参观候选】
{json.dumps(cand_brief, ensure_ascii=False)}

【步行矩阵(分钟)】
{json.dumps(walking_matrix, ensure_ascii=False)}

请重新规划从 {prefs.current_venue_id or '起点'} 开始的后半段路线，stops 必须从下一个未参观场馆开始。
- 剩余总时长（含步行）不超过 {remaining_minutes} 分钟
- stops 数量 2-5 个
- narration 要回应用户反馈（『{prefs.feedback}』），让游客感觉导游听懂了
- 输出严格符合 target_structure 的 JSON"""


def _parse_hhmm(s: str) -> Optional[datetime]:
    """Parse HH:MM to today's datetime. Returns None on failure."""
    try:
        now = datetime.now()
        h, m = s.split(":")
        return now.replace(hour=int(h), minute=int(m), second=0, microsecond=0)
    except Exception:
        return None


def _best_exit_gate(last_venue_id: str, entry_gate: str, exit_gate_pref: Optional[str] = None) -> tuple[str, int]:
    """Find the nearest exit gate from the last venue.
    Returns (gate_name, walk_minutes).
    If exit_gate_pref is a specific gate, use it.
    If 'auto' or None, pick nearest (preferring entry gate if within +5min).
    """
    if exit_gate_pref and exit_gate_pref != "auto" and exit_gate_pref in ("north", "south", "east"):
        return exit_gate_pref, get_entry_venue_minutes(exit_gate_pref, last_venue_id)
    gates = ["north", "south", "east"]
    dists = {g: get_entry_venue_minutes(g, last_venue_id) for g in gates}
    nearest = min(dists, key=dists.get)
    nearest_min = dists[nearest]
    if nearest != entry_gate and dists.get(entry_gate, 999) <= nearest_min + 5:
        return entry_gate, dists[entry_gate]
    return nearest, nearest_min


def _route_from_llm_data(
    data: dict,
    candidates: list[dict],
    entry_gate: str,
    start_time: str,
    elapsed_minutes: int = 0,
    exit_gate_pref: Optional[str] = None,
) -> Route:
    """Validate + transform LLM JSON to a Route object."""
    cand_map = {c["id"]: c for c in candidates}
    stops: list[RouteStop] = []
    base = _parse_hhmm(start_time)
    if base is None:
        base = datetime.now().replace(hour=9, minute=0, second=0, microsecond=0)
    base = base + timedelta(minutes=elapsed_minutes)

    cur = base
    for s in data.get("stops", []):
        vid = s.get("venue_id")
        venue = cand_map.get(vid, {})
        visit_minutes = int(s.get("visit_minutes", venue.get("recommended_visit_minutes", 20)))
        walk_to_next = int(s.get("walk_to_next_minutes", 0))
        arrive = cur
        leave = arrive + timedelta(minutes=visit_minutes)
        stops.append(
            RouteStop(
                venue_id=vid or "",
                venue_name=s.get("venue_name") or venue.get("name", ""),
                arrive_time=_hhmm(arrive),
                leave_time=_hhmm(leave),
                visit_minutes=visit_minutes,
                walk_to_next_minutes=walk_to_next,
                narration=s.get("narration", ""),
                tips=s.get("tips", []) or [],
                rest_here=bool(s.get("rest_here", False)),
            )
        )
        cur = leave + timedelta(minutes=walk_to_next)

    last_vid = stops[-1].venue_id if stops else ""
    exit_gate, walk_to_exit = _best_exit_gate(last_vid, entry_gate, exit_gate_pref) if last_vid else ("", 0)
    entry_walk = get_entry_venue_minutes(entry_gate, stops[0].venue_id) if stops else 0
    total_walk = entry_walk + sum(s.walk_to_next_minutes for s in stops) + walk_to_exit
    total = entry_walk + sum(s.visit_minutes + s.walk_to_next_minutes for s in stops) + walk_to_exit

    return Route(
        id=data.get("id") or f"r_{uuid.uuid4().hex[:8]}",
        summary=data.get("summary", ""),
        total_minutes=int(data.get("total_minutes", total)),
        total_walk_minutes=int(data.get("total_walk_minutes", total_walk)),
        stops=stops,
        exit_gate=exit_gate,
        walk_to_exit_minutes=walk_to_exit,
        warnings=data.get("warnings", []) or config.UNIVERSAL_WARNINGS,
        tips=data.get("tips", []) or [],
        fallback=False,
    )


def _greedy_fallback_route(
    prefs: PlanRequest,
    candidates: list[dict],
    remaining_minutes: Optional[int] = None,
    current_venue_id: Optional[str] = None,
    style: Optional[str] = None,
) -> Route:
    """Pure rule-based route. Always works even without LLM."""
    budget = remaining_minutes if remaining_minutes is not None else int(prefs.available_hours * 60)
    style = style or prefs.style or "balanced"

    # Apply style to candidates (re-rank)
    candidates = _apply_style(candidates, style)

    max_stops = max_stops_by_time(budget / 60.0)
    picked: list[dict] = []
    used_ids: set[str] = set()
    if current_venue_id:
        used_ids.add(current_venue_id)

    # Entry first: depends on style
    entry_minutes_first = 0
    if current_venue_id is None:
        gate = prefs.entry_gate
        if style == "must_see":
            # Top-ranked must_see venue (regardless of distance)
            first = next((c for c in candidates if c["id"] not in used_ids), None)
        elif style == "hidden_gem":
            # Pick a non-must_see venue closer to the user's area
            sorted_by_entry = sorted(
                [c for c in candidates if c["id"] not in used_ids and not c.get("must_see")],
                key=lambda c: get_entry_venue_minutes(gate, c["id"]),
            )
            first = sorted_by_entry[0] if sorted_by_entry else candidates[0]
        else:
            # Balanced: nearest must-see to gate
            sorted_by_entry = sorted(
                [c for c in candidates if c["id"] not in used_ids],
                key=lambda c: get_entry_venue_minutes(gate, c["id"]),
            )
            first = sorted_by_entry[0] if sorted_by_entry else None
        if first:
            picked.append(first)
            used_ids.add(first["id"])
            entry_minutes_first = get_entry_venue_minutes(gate, first["id"])

    cur_id = picked[-1]["id"] if picked else current_venue_id
    cur_time_used = entry_minutes_first + (picked[0]["recommended_visit_minutes"] if picked else 0)

    # For hidden_gem, prefer moving to less-visited areas
    visited_areas = {picked[0]["area"]} if picked else set()

    for _ in range(max_stops - len(picked)):
        candidates_left = [c for c in candidates if c["id"] not in used_ids]
        if not candidates_left:
            break
        if style == "hidden_gem":
            # Prefer different area (variety)
            candidates_left.sort(
                key=lambda c: (
                    0 if c["area"] not in visited_areas else 5,
                    get_inter_venue_minutes(cur_id, c["id"]),
                )
            )
        else:
            candidates_left.sort(key=lambda c: get_inter_venue_minutes(cur_id, c["id"]))
        nxt = candidates_left[0]
        walk = get_inter_venue_minutes(cur_id, nxt["id"])
        visit = nxt.get("recommended_visit_minutes", 20)
        if cur_time_used + walk + visit > budget:
            break
        picked.append(nxt)
        used_ids.add(nxt["id"])
        cur_id = nxt["id"]
        cur_time_used += walk + visit
        visited_areas.add(nxt["area"])

    stops: list[RouteStop] = []
    base = _parse_hhmm(prefs.start_time) or datetime.now().replace(hour=9, minute=0, second=0, microsecond=0)
    cur = base
    if picked and current_venue_id is None:
        first_walk = get_entry_venue_minutes(prefs.entry_gate, picked[0]["id"])
        cur = cur + timedelta(minutes=first_walk)
    for i, v in enumerate(picked):
        visit = v.get("recommended_visit_minutes", 20)
        arrive = cur
        leave = arrive + timedelta(minutes=visit)
        walk_to_next = (
            get_inter_venue_minutes(v["id"], picked[i + 1]["id"]) if i + 1 < len(picked) else 0
        )
        narration = _fallback_narration(v, prefs)
        tips = _fallback_tips(v, prefs)
        stops.append(
            RouteStop(
                venue_id=v["id"],
                venue_name=v["name"],
                arrive_time=_hhmm(arrive),
                leave_time=_hhmm(leave),
                visit_minutes=visit,
                walk_to_next_minutes=walk_to_next,
                narration=narration,
                tips=tips,
                rest_here=bool(v.get("rest_spots", False)) and (i == len(picked) // 2),
            )
        )
        cur = leave + timedelta(minutes=walk_to_next)

    total_visit = sum(s.visit_minutes for s in stops)
    total_inter_walk = sum(s.walk_to_next_minutes for s in stops)
    entry_walk = get_entry_venue_minutes(prefs.entry_gate, picked[0]["id"]) if picked else 0
    last_vid = picked[-1]["id"] if picked else ""
    exit_gate, walk_to_exit = _best_exit_gate(last_vid, prefs.entry_gate, prefs.exit_gate) if last_vid else ("", 0)
    total_walk = entry_walk + total_inter_walk + walk_to_exit
    total = entry_walk + total_visit + total_inter_walk + walk_to_exit

    summary = _fallback_summary(picked, prefs, style)

    return Route(
        id=f"r_{uuid.uuid4().hex[:8]}",
        summary=summary,
        total_minutes=total,
        total_walk_minutes=total_walk,
        stops=stops,
        exit_gate=exit_gate,
        walk_to_exit_minutes=walk_to_exit,
        warnings=config.UNIVERSAL_WARNINGS,
        tips=_fallback_general_tips(prefs),
        fallback=True,
    )


def _apply_style(candidates: list[dict], style: str) -> list[dict]:
    """Re-rank candidates by style. Must_see / hidden_gem / balanced.

    Each style tags picks differently so greedy diverges:
      - must_see: massive bonus to must_see venues → greedy picks those first
      - hidden_gem: penalty to must_see + bonus to kid_friendly + animal variety
      - balanced: original scores
    """
    if style == "must_see":
        return sorted(
            candidates,
            key=lambda c: (
                100 if c.get("must_see") else 0,  # huge boost for must-see
                c.get("photo_op", 0) * 3,
                c.get("_score", 0),
            ),
            reverse=True,
        )
    if style == "hidden_gem":
        return sorted(
            candidates,
            key=lambda c: (
                0 if c.get("must_see") else 10,  # bonus to non-must-see
                c.get("kid_friendly", 0) * 2,
                len(c.get("animals", [])) * 3,  # more diverse animals = bonus
                c.get("_score", 0),
            ),
            reverse=True,
        )
    return candidates


def _fallback_summary(venues: list[dict], prefs: PlanRequest, style: str = "balanced") -> str:
    if not venues:
        return "时间太紧张啦，建议把可用时间调到 1.5 小时以上，再来一次。"
    names = " → ".join(v["name"] for v in venues)
    prefix = {"must_see": "【必看精选】", "hidden_gem": "【小众探索】", "balanced": ""}.get(style, "")
    meta = data_loader.get_meta()
    short = meta.get("short_name", meta.get("name", "动物园")[:2])
    defaults = meta.get("planner_defaults", {})
    template = defaults.get("summary_template", "{prefix}为你选了 {count} 个场馆：{names}。{short_name}的故事，由这些场馆串起来。")
    return template.format(prefix=prefix, count=len(venues), names=names, short_name=short)


def _fallback_narration(v: dict, prefs: PlanRequest) -> str:
    name = v["name"]
    animals = v.get("animals", [])
    animal_str = "、".join(animals[:2]) if animals else "这里的动物"
    quiz = data_loader.get_quiz_config()
    templates = quiz.get("narration_templates", {})
    meta = data_loader.get_meta()
    defaults = meta.get("planner_defaults", {})
    fallback = defaults.get("narration_fallback", "{name}是不可错过的场馆之一，{animals}在这里生活得很自在。")
    template = templates.get(prefs.party_type)
    if template:
        return template.format(name=name, animals=animal_str)
    return fallback.format(name=name, animals=animal_str)


def _fallback_tips(v: dict, prefs: PlanRequest) -> list[str]:
    tips: list[str] = []
    if v.get("shaded"):
        tips.append("场馆有遮阴，不怕晒")
    if v.get("rest_spots"):
        tips.append("附近有座椅，可以歇脚")
    if "周一二闭馆" in v.get("tags", []):
        tips.append("注意：周一、周二闭馆")
    if prefs.with_kids and v.get("kid_friendly", 0) >= 4:
        tips.append("很适合小朋友近距离观察")
    if not tips:
        tips.append("放慢脚步，多看看动物的自然行为")
    return tips[:3]


def _fallback_general_tips(prefs: PlanRequest) -> list[str]:
    tips: list[str] = []
    if prefs.with_kids:
        tips.append("带娃节奏建议每 1.5 小时休息一次")
    if prefs.sun_tolerance <= 2:
        tips.append("防晒优先：尽量选有遮阴的场馆停留")
    if not prefs.willing_to_hike:
        meta = data_loader.get_meta()
        short = meta.get("short_name", meta.get("name", "动物园")[:2])
        defaults = meta.get("planner_defaults", {})
        steep = defaults.get("steep_tip", "少爬坡：部分片区地形起伏大，建议平地为主")
        tips.append(steep)
    if prefs.stamina <= 2:
        tips.append("体力保留：每两个场馆间坐下来休息一下")
    if not tips:
        meta = data_loader.get_meta()
        short = meta.get("short_name", meta.get("name", "动物园")[:2])
        defaults = meta.get("planner_defaults", {})
        terrain = defaults.get("terrain_tip", "慢慢逛最有味道")
        tips.append(terrain)
    return tips[:3]


async def plan_route(prefs: PlanRequest, force_fast: bool = False) -> tuple[Route, bool]:
    """Returns (route, used_llm)."""
    candidates = filter_and_rank(prefs)
    walking_matrix = build_walking_matrix([c["id"] for c in candidates])

    # Try LLM (unless fast mode forced)
    if llm_client.is_llm_enabled() and not force_fast:
        print("[plan] 🧠 LLM enabled, calling async_chat_json...")
        user_prompt = _build_user_prompt_plan(prefs, candidates, walking_matrix)
        print(f"[plan] 📝 User prompt: {len(user_prompt)} chars, candidates: {len(candidates)}")
        messages = [
            {"role": "system", "content": "直接输出路线JSON，不要写推理过程。在```json```代码块中输出，不要写其他内容。"},
            {"role": "user", "content": [{"type": "text", "text": user_prompt}]},
        ]
        result = await llm_client.async_chat_json(
            messages=messages,
            target_structure=prompts.PLAN_TARGET_STRUCTURE,
            background=prompts.SYSTEM_BACKGROUND,
            requirements=prompts.PLAN_REQUIREMENTS,
            overall_timeout=300.0,
        )
        if not result.get("error") and result.get("data"):
            try:
                route = _route_from_llm_data(result["data"], candidates, prefs.entry_gate, prefs.start_time, exit_gate_pref=prefs.exit_gate)
                print("[plan] ✅ LLM route generated successfully")
                return route, True
            except Exception as e:
                print(f"[plan] ⚠️ LLM output parse failed: {e}, falling back to rule engine")
        else:
            err = result.get("error", "unknown")
            raw_len = len(result.get("raw_content") or "")
            data_present = result.get("data") is not None
            print(f"[plan] ⚠️ LLM call failed: {err} | data={data_present} | raw_len={raw_len}, falling back to rule engine")
    else:
        reason = "fast mode" if force_fast else "LLM not enabled"
        print(f"[plan] ⏭️ Skipping LLM ({reason}), using rule engine")

    # Fallback (also used if LLM disabled or failed)
    route = _greedy_fallback_route(prefs, candidates, style=prefs.style)
    print("[plan] 📐 Rule engine fallback route generated")
    return route, False


async def plan_route_variants(prefs: PlanRequest) -> list[dict]:
    curated = data_loader.get_curated_routes()
    if curated:
        return _curated_to_variants(curated)
    styles = ["must_see", "hidden_gem", "balanced"]
    labels = {"balanced": "⚖️ 平衡推荐", "must_see": "⭐ 必看精选", "hidden_gem": "💎 小众探索"}
    variants = []
    for style in styles:
        p = prefs.model_copy()
        p.style = style
        route, _ = await plan_route(p, force_fast=True)
        d = route.model_dump()
        d["variant_label"] = labels[style]
        variants.append(d)
    return variants


def _curated_to_variants(curated: list[dict]) -> list[dict]:
    all_venues = {v["id"]: v for v in data_loader.get_all_venue_dicts()}
    sample = random.sample(curated, min(3, len(curated)))
    variants = []
    for cr in sample:
        stop_venues = [all_venues[sid] for sid in cr["stops"] if sid in all_venues]
        if not stop_venues:
            continue
        stops = []
        t = datetime.strptime("09:00", "%H:%M")
        for i, v in enumerate(stop_venues):
            visit = 25
            arrive = t.strftime("%H:%M")
            t += timedelta(minutes=visit)
            leave = t.strftime("%H:%M")
            t += timedelta(minutes=5)
            stops.append({
                "venue_id": v["id"],
                "venue_name": v["name"],
                "area": v.get("area", ""),
                "arrive_time": arrive,
                "leave_time": leave,
                "visit_minutes": visit,
                "narration": "",
                "tips": [],
                "rest_here": False,
                "walk_to_next_minutes": 5,
            })
        last_vid = stop_venues[-1]["id"] if stop_venues else ""
        entry_gate = cr.get("start_gate", "north")
        exit_gate, walk_to_exit = _best_exit_gate(last_vid, entry_gate, None) if last_vid else ("", 0)
        entry_walk = get_entry_venue_minutes(entry_gate, stop_venues[0]["id"]) if stop_venues else 0
        route_id = str(uuid.uuid4())[:8]
        variants.append({
            "id": route_id,
            "summary": cr["summary"],
            "variant_label": cr["label"],
            "stops": stops,
            "total_minutes": cr.get("total_minutes", 180),
            "total_walk_minutes": (len(stop_venues) - 1) * 5,
            "start_gate": entry_gate,
            "exit_gate": exit_gate,
            "walk_to_exit_minutes": walk_to_exit,
            "fallback": False,
        })
    return variants


async def replan_route(
    original_route: dict,
    prefs: ReplanRequest,
    force_fast: bool = False,
) -> tuple[Route, bool]:
    """Re-plan the second half of a route based on user feedback."""
    # Use the original prefs to recompute candidates
    party_type = original_route.get("_party_type", "solo")
    plan_prefs = PlanRequest(
        available_hours=original_route.get("_available_hours", 3),
        party_type=party_type,
        with_kids=original_route.get("_with_kids", False),
        kids_age=original_route.get("_kids_age"),
        stamina=original_route.get("_stamina", 3),
        sun_tolerance=original_route.get("_sun_tolerance", 3),
        willing_to_hike=original_route.get("_willing_to_hike", True),
        animal_interests=original_route.get("_animal_interests", []),
        entry_gate=original_route.get("_entry_gate", "north"),
        exit_gate=original_route.get("_exit_gate", "auto"),
        start_time=original_route.get("_start_time", "09:00"),
    )
    candidates = filter_and_rank(plan_prefs)
    walking_matrix = build_walking_matrix([c["id"] for c in candidates])

    visited_ids = {s.get("venue_id") for s in original_route.get("stops", []) if s.get("venue_id")}
    remaining_candidates = [c for c in candidates if c["id"] not in visited_ids]

    # Adjust remaining time budget
    remaining_minutes = max(0, int(plan_prefs.available_hours * 60) - prefs.elapsed_minutes)

    if llm_client.is_llm_enabled() and not force_fast:
        print("[replan] 🧠 LLM enabled, calling async_chat_json...")
        user_prompt = _build_user_prompt_replan(
            original_route, prefs, candidates, walking_matrix, remaining_candidates
        )
        messages = [
            {"role": "system", "content": "直接输出路线JSON，不要写推理过程。在```json```代码块中输出，不要写其他内容。"},
            {"role": "user", "content": [{"type": "text", "text": user_prompt}]},
        ]
        result = await llm_client.async_chat_json(
            messages=messages,
            target_structure=prompts.REPLAN_TARGET_STRUCTURE,
            background=prompts.SYSTEM_BACKGROUND,
            requirements=prompts.REPLAN_REQUIREMENTS,
            overall_timeout=300.0,
        )
        if not result.get("error") and result.get("data"):
            try:
                anchor = None
                if prefs.current_venue_id:
                    for s in original_route.get("stops", []):
                        if s.get("venue_id") == prefs.current_venue_id:
                            anchor = s.get("leave_time")
                            break
                anchor_time = anchor or plan_prefs.start_time
                route = _route_from_llm_data(
                    result["data"], candidates, plan_prefs.entry_gate, anchor_time,
                    exit_gate_pref=plan_prefs.exit_gate,
                )
                print("[replan] ✅ LLM route generated successfully")
                return route, True
            except Exception as e:
                print(f"[replan] ⚠️ LLM output parse failed: {e}, falling back to rule engine")
        else:
            err = result.get("error", "unknown")
            raw_len = len(result.get("raw_content") or "")
            data_present = result.get("data") is not None
            print(f"[replan] ⚠️ LLM call failed: {err} | data={data_present} | raw_len={raw_len}, falling back to rule engine")
    else:
        reason = "fast mode" if force_fast else "LLM not enabled"
        print(f"[replan] ⏭️ Skipping LLM ({reason}), using rule engine")

    # Fallback: greedy on remaining candidates
    fallback = _greedy_fallback_route(
        plan_prefs,
        remaining_candidates,
        remaining_minutes=remaining_minutes,
        current_venue_id=prefs.current_venue_id,
    )
    print("[replan] 📐 Rule engine fallback route generated")
    return fallback, False