"""Photo evaluation: multi-modal LLM analyzes animal photos taken at the zoo.

The model:
  1) Identifies the most likely animal in the photo
  2) Maps it to a Hongshan venue (if possible)
  3) Generates a fun evaluation ("你和XX的合照评价") + badge

This is a lighthearted feature, not a real CV system.
"""

from __future__ import annotations

import base64
import hashlib
import json
import uuid
from datetime import datetime
from pathlib import Path
from typing import Optional

from . import config, data_loader, db, llm_client


PHOTO_DIR = Path(__file__).resolve().parent.parent / "data" / "photos"
PHOTO_DIR.mkdir(parents=True, exist_ok=True)


PHOTO_TARGET_STRUCTURE: dict = {
    "animal_guess": "str, 推测的动物中文名 (e.g. 大熊猫 / 长颈鹿 / 大猩猩 / 考拉 / 细尾獴)",
    "animal_confidence": "int, 0-100, 识别确信度",
    "matched_venue_id": "str, 推断的最可能场馆 ID（必须是候选 ID 之一，或留空字符串）",
    "caption": "str, 30 字以内的中文配文（活泼、有梗）",
    "vibe_score": "int, 0-100, 整体出片指数",
    "vibe_label": "str, 一个词形容这张照片的氛围 (e.g. 可爱、震撼、治愈、爆笑)",
    "comment": "str, 60-100 字的中文评价，有梗、有细节，不要套话",
    "badge": "str, 一个{badge_example}",
    "tips": "list[str], 1-2 条拍摄建议（10-25字）",
}


def _build_photo_background() -> str:
    meta = data_loader.get_meta()
    name = meta.get("name", "动物园")
    short = meta.get("short_name", name[:2])
    extras = meta.get("prompt_extras", {})
    template = extras.get("photo_background", "")
    if not template:
        return f"你是一位风趣的动物园'出片点评师'。用户给你一张在{name}拍的照片，请识别动物并评价。"
    fun_facts = meta.get("fun_facts", [])
    if not fun_facts:
        with (Path(__file__).resolve().parent.parent / "data" / "system.json").open(encoding="utf-8") as f:
            sys_cfg = json.load(f)
        fun_facts = sys_cfg.get("fun_facts", [])
    fun_facts_block = "\n".join(f"- {f}" for f in fun_facts) if fun_facts else ""
    return template.format(name=name, short_name=short, fun_facts_block=fun_facts_block)


PHOTO_BACKGROUND: str = "你是一位风趣的动物园'出片点评师'。用户给你一张在动物园拍的照片，请识别动物并评价。"
try:
    PHOTO_BACKGROUND = _build_photo_background()
except Exception:
    pass


def _build_photo_requirements() -> list[str]:
    meta = data_loader.get_meta()
    extras = meta.get("prompt_extras", {})
    venue_rule = extras.get("photo_requirement_venue", "matched_venue_id 必须是候选 ID 之一，否则留空字符串")
    badge_example = extras.get("photo_badge_examples", "4-6字徽章")
    return [
        "animal_guess 用中文常用动物名",
        venue_rule,
        "caption 不超过 30 字",
        "comment 60-100 字，要有梗，不要说'这张照片很美'这类空话",
        "vibe_score 0-100，反映'出片'指数（构图、光线、动物状态综合）",
        f"badge 用 {badge_example}",
        "tips 1-2 条即可",
    ]


PHOTO_REQUIREMENTS: list[str] = [
    "animal_guess 用中文常用动物名",
    "matched_venue_id 必须是候选 ID 之一，否则留空字符串",
    "caption 不超过 30 字",
    "comment 60-100 字，要有梗，不要说'这张照片很美'这类空话",
    "vibe_score 0-100，反映'出片'指数（构图、光线、动物状态综合）",
    "badge 用 4-6字徽章",
    "tips 1-2 条即可",
]
try:
    PHOTO_REQUIREMENTS = _build_photo_requirements()
except Exception:
    pass


def _venues_brief() -> list[dict]:
    """Concise venue list to pass to LLM as candidates."""
    return [
        {"id": v["id"], "name": v["name"], "animals": v.get("animals", [])}
        for v in data_loader.get_all_venue_dicts()
        if v.get("animals")
    ]


def save_photo(file_bytes: bytes, suffix: str) -> Path:
    """Persist photo to disk; returns file path."""
    sid = uuid.uuid4().hex
    path = PHOTO_DIR / f"{sid}{suffix}"
    path.write_bytes(file_bytes)
    return path


def evaluate_photo(
    image_path: Path,
    user_id: Optional[int] = None,
    session_id: Optional[str] = None,
    auto_checkin: bool = True,
) -> dict:
    """Call multi-modal LLM to evaluate a saved photo. Returns evaluation dict."""
    if not llm_client.is_llm_enabled():
        result = _fallback_evaluation(image_path, reason="USE_LLM=false")
    else:
        try:
            result = _evaluate_with_llm(image_path)
        except Exception as e:
            result = _fallback_evaluation(image_path, reason=str(e))

    # Auto checkin: if matched venue, record a checkin
    if auto_checkin and result.get("matched_venue_id"):
        venue = data_loader.get_venue_dict_by_id(result["matched_venue_id"])
        if venue:
            sid = session_id or (str(user_id) if user_id else "anon")
            try:
                checkin = db.insert_checkin(
                    venue_id=venue["id"],
                    venue_name=venue["name"],
                    session_id=sid,
                    user_id=user_id,
                    note=f"auto from photo {result['evaluation_id']}",
                )
                result["auto_checkin"] = checkin
            except Exception:
                pass

    return result


def evaluate_photo_with_expected(
    image_path: Path,
    user_id: Optional[int] = None,
    session_id: Optional[str] = None,
    auto_checkin: bool = False,
    expected_venue: Optional[dict] = None,
) -> dict:
    """Like evaluate_photo but tells LLM which venue to verify against.

    The returned dict has 'matched_venue_id' set to the LLM's best guess.
    The caller (main.py) decides success by comparing to expected_venue_id.
    """
    if not expected_venue:
        return evaluate_photo(image_path, user_id, session_id, auto_checkin)

    if not llm_client.is_llm_enabled():
        return _fallback_evaluation(image_path, reason="USE_LLM=false", expected_venue=expected_venue)

    try:
        result = _evaluate_with_llm_and_expected(image_path, expected_venue)
    except BaseException as e:
        return _fallback_evaluation(image_path, reason=f"LLM error: {e}", expected_venue=expected_venue)
    return result


def _evaluate_with_llm(image_path: Path) -> dict:
    user_prompt = (
        f"请分析这张照片，按 target_structure 输出 JSON。\n"
        f"\n候选场馆（请用 matched_venue_id 匹配其中之一）：\n"
        f"{json.dumps(_venues_brief(), ensure_ascii=False)}"
    )
    with image_path.open("rb") as f:
        b64 = base64.b64encode(f.read()).decode("ascii")
    suffix = image_path.suffix.lower()
    mime = {
        ".jpg": "image/jpeg",
        ".jpeg": "image/jpeg",
        ".png": "image/png",
        ".webp": "image/webp",
        ".gif": "image/gif",
    }.get(suffix, "image/jpeg")
    client = llm_client._get_client()
    messages = [
        {"role": "system", "content": PHOTO_BACKGROUND},
        {
            "role": "user",
            "content": [
                {"type": "text", "text": user_prompt},
                {"type": "image_url", "image_url": {"url": f"data:{mime};base64,{b64}"}},
            ],
        },
    ]
    resp = client.chat.completions.create(
        model=config.MODEL_NAME,
        messages=messages,
        response_format={"type": "json_object"},
        max_tokens=1500,
        timeout=60.0,
    )
    content = resp.choices[0].message.content or "{}"
    if "```" in content:
        for fence in ("```json", "```"):
            if fence in content:
                content = content.split(fence)[1].split("```")[0]
                break
    data = json.loads(content)
    eval_id = uuid.uuid4().hex[:8]
    result = {
        "evaluation_id": eval_id,
        "image_path": str(image_path.relative_to(PHOTO_DIR.parent) if image_path.is_relative_to(PHOTO_DIR.parent) else image_path.name),
        "animal_guess": data.get("animal_guess", ""),
        "animal_confidence": data.get("animal_confidence", 0),
        "matched_venue_id": data.get("matched_venue_id", ""),
        "caption": data.get("caption", ""),
        "vibe_score": data.get("vibe_score", 0),
        "vibe_label": data.get("vibe_label", ""),
        "comment": data.get("comment", ""),
        "badge": data.get("badge", ""),
        "tips": data.get("tips", []),
        "fallback": False,
        "ts": datetime.now().isoformat(timespec="seconds"),
    }
    if result["matched_venue_id"]:
        v = data_loader.get_venue_dict_by_id(result["matched_venue_id"])
        if v:
            result["matched_venue_name"] = v["name"]
    db.insert_photo_eval(eval_id, result)
    return result


def _evaluate_with_llm_and_expected(image_path: Path, expected_venue: dict) -> dict:
    """Like _evaluate_with_llm but tells LLM the expected venue upfront.

    The expected venue is presented as 'the user says they're at this place,
    but verify what's actually in the photo'. LLM still returns its best
    guess for matched_venue_id, which main.py compares.
    """
    expected_animals = ", ".join(expected_venue.get("animals", [])) or "该馆常见动物"
    expected_name = expected_venue.get("name", "未指定")
    user_prompt = (
        f"用户声称在「{expected_name}」（常见动物：{expected_animals}）。\n"
        f"请仔细看照片，识别出实际拍到的动物，"
        f"判断照片内容是否与「{expected_name}」匹配。\n"
        f"\n候选场馆（请用 matched_venue_id 匹配其中之一）：\n"
        f"{json.dumps(_venues_brief(), ensure_ascii=False)}\n"
        f"\n按 target_structure 输出 JSON。"
    )
    with image_path.open("rb") as f:
        b64 = base64.b64encode(f.read()).decode("ascii")
    suffix = image_path.suffix.lower()
    mime = {
        ".jpg": "image/jpeg",
        ".jpeg": "image/jpeg",
        ".png": "image/png",
        ".webp": "image/webp",
        ".gif": "image/gif",
    }.get(suffix, "image/jpeg")
    client = llm_client._get_client()
    messages = [
        {"role": "system", "content": PHOTO_BACKGROUND},
        {
            "role": "user",
            "content": [
                {"type": "text", "text": user_prompt},
                {"type": "image_url", "image_url": {"url": f"data:{mime};base64,{b64}"}},
            ],
        },
    ]
    resp = client.chat.completions.create(
        model=config.MODEL_NAME,
        messages=messages,
        response_format={"type": "json_object"},
        max_tokens=1500,
        timeout=60.0,
    )
    content = resp.choices[0].message.content or "{}"
    if "```" in content:
        for fence in ("```json", "```"):
            if fence in content:
                content = content.split(fence)[1].split("```")[0]
                break
    data = json.loads(content)
    eval_id = uuid.uuid4().hex[:8]
    result = {
        "evaluation_id": eval_id,
        "image_path": str(image_path.relative_to(PHOTO_DIR.parent) if image_path.is_relative_to(PHOTO_DIR.parent) else image_path.name),
        "animal_guess": data.get("animal_guess", ""),
        "animal_confidence": data.get("animal_confidence", 0),
        "matched_venue_id": data.get("matched_venue_id", ""),
        "caption": data.get("caption", ""),
        "vibe_score": data.get("vibe_score", 0),
        "vibe_label": data.get("vibe_label", ""),
        "comment": data.get("comment", ""),
        "badge": data.get("badge", ""),
        "tips": data.get("tips", []),
        "fallback": False,
        "ts": datetime.now().isoformat(timespec="seconds"),
    }
    if result["matched_venue_id"]:
        v = data_loader.get_venue_dict_by_id(result["matched_venue_id"])
        if v:
            result["matched_venue_name"] = v["name"]
    db.insert_photo_eval(eval_id, result)
    return result


def _fallback_evaluation(image_path: Path, reason: str = "", expected_venue: Optional[dict] = None) -> dict:
    """Fallback evaluation when LLM is not available or fails.

    If expected_venue is provided, uses it instead of random selection.
    """
    eval_id = uuid.uuid4().hex[:8]
    name = image_path.stem.lower()
    matched_venue = None

    # Priority 1: Use expected_venue if provided
    if expected_venue:
        matched_venue = expected_venue
    else:
        # Priority 2: Try to match from filename
        for v in data_loader.get_all_venue_dicts():
            if v["id"] in name or any(a.replace(" ", "") in name for a in v.get("animals", [])):
                matched_venue = v
                break

        # Priority 3: Random from must-see venues
        if not matched_venue:
            must_sees = [v for v in data_loader.get_all_venue_dicts() if v.get("must_see")]
            idx = int(hashlib.md5(name.encode()).hexdigest(), 16) % len(must_sees)
            matched_venue = must_sees[idx]

    meta = data_loader.get_meta()
    venue_captions_raw = meta.get("photo_venue_captions", {})
    venue_captions = {k: tuple(v) for k, v in venue_captions_raw.items()}

    animal = matched_venue.get("animals", [""])[0] if matched_venue.get("animals") else ""
    short = meta.get("short_name", meta.get("name", "动物园")[:2])
    caption_a, caption_b = venue_captions.get(matched_venue["id"], ("定格瞬间", f"你在{short}的某个角落"))

    eval_id = uuid.uuid4().hex[:8]
    result = {
        "evaluation_id": eval_id,
        "image_path": str(image_path.relative_to(PHOTO_DIR.parent) if image_path.is_relative_to(PHOTO_DIR.parent) else image_path.name),
        "animal_guess": animal,
        "animal_confidence": 0,
        "matched_venue_id": matched_venue["id"],
        "matched_venue_name": matched_venue["name"],
        "caption": f"{caption_a}｜{caption_b}",
        "vibe_score": 0,
        "vibe_label": "fallback",
        "comment": "LLM不可用，使用规则引擎兜底评价",
        "badge": "规则引擎",
        "tips": [
            "试试用低角度仰拍，让动物更有气势",
            "手机贴玻璃时关掉闪光灯，避免反光",
        ],
        "fallback": True,
        "fallback_reason": reason,
        "ts": datetime.now().isoformat(timespec="seconds"),
    }
    db.insert_photo_eval(eval_id, result)
    return result


def get_evaluation(eval_id: str) -> Optional[dict]:
    return db.get_photo_eval(eval_id)