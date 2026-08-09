"""Photo evaluation: multi-modal LLM analyzes animal photos taken at the zoo.

Two modes:
  1) photo-checkin: verify photo matches expected venue → is_match + desc + score
  2) photo-evaluate (wall): identify animal + photo quality → animal + desc + score + style + blurry

This is a lighthearted feature, not a real CV system.
"""

from __future__ import annotations

import base64
import json
import uuid
from datetime import datetime
from typing import Optional

from . import config, data_loader, db, llm_client


CHECKIN_TARGET_STRUCTURE: dict = {
    "is_match": "bool, 照片内容是否与用户当前所在场馆匹配（true/false）",
    "desc": "str, 30 字以内的中文描述，简短说明这张图拍的是什么",
    "score": "int, 0-100, 出片指数（构图、光线、动物状态综合）",
}

CHECKIN_REQUIREMENTS: list[str] = [
    "is_match：照片内容是否与用户当前所在场馆的真实动物匹配，匹配填 true，否则 false",
    "desc 不超过 30 字，简短直接",
    "score 0-100，反映出片指数（构图、光线、动物状态综合）",
]

WALL_TARGET_STRUCTURE: dict = {
    "animal": "str, 识别出的动物名称（如：大熊猫、金丝猴、火烈鸟），无法识别填\"未知动物\"",
    "desc": "str, 30字以内的中文一句话描述",
    "score": "int, 0-100出片指数（构图、光线、清晰度、动物状态综合）",
    "style": "str, 照片风格，从以下选一个：特写/全景/剪影/抓拍/合影/其他",
    "blurry": "str, 清晰度，从以下选一个：清晰/略微模糊/模糊",
}

WALL_REQUIREMENTS: list[str] = [
    "animal 填中文动物名",
    "desc 不超过 30 字，简短直接",
    "score 0-100，反映出片指数",
    "style 和 blurry 必须从给定选项中选",
]


def _suffix_to_mime(suffix: str) -> str:
    suffix = suffix.lower()
    return {
        ".jpg": "image/jpeg",
        ".jpeg": "image/jpeg",
        ".png": "image/png",
        ".webp": "image/webp",
        ".gif": "image/gif",
    }.get(suffix, "image/jpeg")


# ---------------------------------------------------------------------------
# Checkin: verify photo matches expected venue
# ---------------------------------------------------------------------------

async def evaluate_photo_with_expected(
    image_bytes: bytes,
    suffix: str = ".jpg",
    user_id: Optional[int] = None,
    session_id: Optional[str] = None,
    auto_checkin: bool = False,
    expected_venue: Optional[dict] = None,
) -> dict:
    if not expected_venue:
        return _fallback_checkin(reason="no_expected_venue")

    if not llm_client.is_llm_enabled():
        return _fallback_checkin(reason="USE_LLM=false", expected_venue=expected_venue, user_id=user_id, session_id=session_id)

    try:
        result = await _checkin_with_llm(image_bytes, suffix, expected_venue, user_id=user_id, session_id=session_id)
    except BaseException as e:
        return _fallback_checkin(reason=f"LLM error: {e}", expected_venue=expected_venue, user_id=user_id, session_id=session_id)

    result["matched_venue_id"] = expected_venue.get("id", "")
    result["matched_venue_name"] = expected_venue.get("name", "")
    return result


async def _checkin_with_llm(image_bytes: bytes, suffix: str, expected_venue: dict, user_id: Optional[int] = None, session_id: Optional[str] = None) -> dict:
    expected_animals = ", ".join(expected_venue.get("animals", [])) or "该馆常见动物"
    expected_name = expected_venue.get("name", "未指定")
    user_text = (
        f"用户当前正在「{expected_name}」参观（该馆常见动物：{expected_animals}）。\n"
        f"请仔细看照片，判断内容是否与「{expected_name}」匹配，然后给一段简短描述和一个出片评分。"
    )
    data = await _call_llm_photo(
        image_bytes, suffix, user_text,
        target_structure=CHECKIN_TARGET_STRUCTURE,
        background="你是一位动物园'出片点评师'。用户会告诉你他们当前所在场馆，请看照片判断内容是否匹配该场馆，并给出简短描述和出片评分。",
        requirements=CHECKIN_REQUIREMENTS,
    )
    eval_id = uuid.uuid4().hex[:8]
    is_match = bool(data.get("is_match", False))
    result = {
        "evaluation_id": eval_id,
        "is_match": is_match,
        "desc": data.get("desc", "") or "",
        "score": int(data.get("score", 0) or 0),
        "fallback": False,
        "ts": datetime.now().isoformat(timespec="seconds"),
    }
    db.insert_photo_eval(eval_id, result, user_id=user_id, session_id=session_id)
    return result


async def _wall_with_llm(image_bytes: bytes, suffix: str, user_id: Optional[int] = None, session_id: Optional[str] = None) -> dict:
    user_text = "请看这张在动物园拍的照片，分析里面的动物和照片质量。"
    data = await _call_llm_photo(
        image_bytes, suffix, user_text,
        target_structure=WALL_TARGET_STRUCTURE,
        background="你是一位动物园出片点评师，擅长识别动物和评价照片质量。",
        requirements=WALL_REQUIREMENTS,
    )
    eval_id = uuid.uuid4().hex[:8]
    result = {
        "evaluation_id": eval_id,
        "animal": data.get("animal", "") or "未知动物",
        "desc": data.get("desc", "") or "",
        "score": int(data.get("score", 0) or 0),
        "style": data.get("style", "") or "其他",
        "blurry": data.get("blurry", "") or "清晰",
        "fallback": False,
        "ts": datetime.now().isoformat(timespec="seconds"),
    }
    db.insert_photo_eval(eval_id, result)
    return result


def _fallback_checkin(reason: str = "", expected_venue: Optional[dict] = None, user_id: Optional[int] = None, session_id: Optional[str] = None) -> dict:
    eval_id = uuid.uuid4().hex[:8]
    matched_venue = expected_venue
    if not matched_venue:
        must_sees = [v for v in data_loader.get_all_venue_dicts() if v.get("must_see")]
        matched_venue = must_sees[0] if must_sees else data_loader.get_all_venue_dicts()[0]

    meta = data_loader.get_meta()
    venue_captions_raw = meta.get("photo_venue_captions", {})
    short = meta.get("short_name", meta.get("name", "动物园")[:2])
    caption_a, caption_b = venue_captions_raw.get(
        matched_venue["id"], ("定格瞬间", f"你在{short}的某个角落")
    )

    result = {
        "evaluation_id": eval_id,
        "is_match": False,
        "desc": f"{caption_a}｜{caption_b}",
        "score": 0,
        "matched_venue_id": matched_venue.get("id", ""),
        "matched_venue_name": matched_venue.get("name", ""),
        "fallback": True,
        "fallback_reason": reason,
        "ts": datetime.now().isoformat(timespec="seconds"),
    }
    db.insert_photo_eval(eval_id, result, user_id=user_id, session_id=session_id)
    return result


# ---------------------------------------------------------------------------
# Wall: identify animal + evaluate photo quality
# ---------------------------------------------------------------------------

async def evaluate_photo_for_wall(
    image_bytes: bytes,
    suffix: str = ".jpg",
    user_id: Optional[int] = None,
    session_id: Optional[str] = None,
) -> dict:
    """Fun evaluation for photo wall — focus on animal + photo quality."""
    if not llm_client.is_llm_enabled():
        return _fallback_wall(reason="USE_LLM=false", user_id=user_id, session_id=session_id)

    try:
        return await _wall_with_llm(image_bytes, suffix, user_id=user_id, session_id=session_id)
    except BaseException as e:
        return _fallback_wall(reason=f"LLM error: {e}", user_id=user_id, session_id=session_id)


async def _wall_with_llm(image_bytes: bytes, suffix: str, user_id: Optional[int] = None, session_id: Optional[str] = None) -> dict:
    user_text = "请看这张在动物园拍的照片，分析里面的动物和照片质量。"
    data = await _call_llm_photo(
        image_bytes, suffix, user_text,
        target_structure=WALL_TARGET_STRUCTURE,
        background="你是一位动物园出片点评师，擅长识别动物和评价照片质量。",
        requirements=WALL_REQUIREMENTS,
    )
    eval_id = uuid.uuid4().hex[:8]
    result = {
        "evaluation_id": eval_id,
        "animal": data.get("animal", "") or "未知动物",
        "desc": data.get("desc", "") or "",
        "score": int(data.get("score", 0) or 0),
        "style": data.get("style", "") or "其他",
        "blurry": data.get("blurry", "") or "清晰",
        "fallback": False,
        "ts": datetime.now().isoformat(timespec="seconds"),
    }
    db.insert_photo_eval(eval_id, result, user_id=user_id, session_id=session_id)
    return result


def _fallback_wall(reason: str = "", user_id: Optional[int] = None, session_id: Optional[str] = None) -> dict:
    eval_id = uuid.uuid4().hex[:8]
    result = {
        "evaluation_id": eval_id,
        "animal": "未知动物",
        "desc": "定格瞬间｜你在动物园的某个角落",
        "score": 0,
        "style": "其他",
        "blurry": "清晰",
        "fallback": True,
        "fallback_reason": reason,
        "ts": datetime.now().isoformat(timespec="seconds"),
    }
    db.insert_photo_eval(eval_id, result, user_id=user_id, session_id=session_id)
    return result


# ---------------------------------------------------------------------------
# Shared LLM call
# ---------------------------------------------------------------------------

async def _call_llm_photo(
    image_bytes: bytes,
    suffix: str,
    user_text: str,
    target_structure: dict,
    background: str,
    requirements: list[str],
) -> dict:
    b64 = base64.b64encode(image_bytes).decode("ascii")
    mime = _suffix_to_mime(suffix)
    messages = [
        {
            "role": "user",
            "content": [
                {"type": "text", "text": user_text},
                {"type": "image_url", "image_url": {"url": f"data:{mime};base64,{b64}"}},
            ],
        },
    ]
    result = await llm_client.async_chat_json(
        messages=messages,
        target_structure=target_structure,
        background=background,
        requirements=requirements,
        overall_timeout=60.0,
    )
    if result.get("error") or result.get("data") is None:
        raise ValueError(f"LLM error: {result.get('error')} | raw: {(result.get('raw_content') or '')[:200]}")
    return result["data"]


def get_evaluation(eval_id: str) -> Optional[dict]:
    return db.get_photo_eval(eval_id)
