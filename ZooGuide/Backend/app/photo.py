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

# In-memory store of evaluations keyed by evaluation_id
_evaluations: dict[str, dict] = {}


PHOTO_TARGET_STRUCTURE: dict = {
    "animal_guess": "str, 推测的动物中文名 (e.g. 大熊猫 / 长颈鹿 / 大猩猩 / 考拉 / 细尾獴)",
    "animal_confidence": "int, 0-100, 识别确信度",
    "matched_venue_id": "str, 推断的最可能场馆 ID（必须是候选 ID 之一，或留空字符串）",
    "caption": "str, 30 字以内的中文配文（活泼、有梗）",
    "vibe_score": "int, 0-100, 整体出片指数",
    "vibe_label": "str, 一个词形容这张照片的氛围 (e.g. 可爱、震撼、治愈、爆笑)",
    "comment": "str, 60-100 字的中文评价，有梗、有细节，不要套话",
    "badge": "str, 一个 4-6 字徽章 (e.g. '野菜F4认证'/'国宝认证'/'最佳拍档')",
    "tips": "list[str], 1-2 条拍摄建议（10-25字）",
}


PHOTO_BACKGROUND: str = (
    "你是一位风趣的动物园'出片点评师'，擅长从一张照片里读出故事。"
    "用户给你一张在南京红山森林动物园拍的照片，请：\n"
    "1. 推测照片里的动物（中文名）\n"
    "2. 推断最可能是在红山哪个场馆拍的（用 matched_venue_id）\n"
    "3. 给这张照片写一段幽默、有梗的中文评价\n"
    "\n"
    "红山的小知识可以点缀进来（不要硬塞）：\n"
    "- 大熊猫馆有3个户外运动场\n"
    "- 大猩猩兄弟团：香椿头/马兰头/小蒜头/枸杞头（南京野菜命名）\n"
    "- 细尾獴网红'站岗'画面\n"
    "- 小熊猫是趋同进化经典案例（与大熊猫远亲但独立演化）\n"
    "- 唐家河展区2025年开放，复刻四川唐家河国家级自然保护区\n"
    "\n"
    "语气：像朋友在朋友圈下面评论，自带梗，不端架子"
)


PHOTO_REQUIREMENTS: list[str] = [
    "animal_guess 用中文常用动物名",
    "matched_venue_id 必须是候选 ID 之一（红山的实际场馆 ID），否则留空字符串",
    "caption 不超过 30 字",
    "comment 60-100 字，要有梗，不要说'这张照片很美'这类空话",
    "vibe_score 0-100，反映'出片'指数（构图、光线、动物状态综合）",
    "badge 用 4-6 字网络梗词或形容词",
    "tips 1-2 条即可",
]


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
        "image_path": str(image_path.relative_to(PHOTO_DIR.parent)),
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
    _evaluations[eval_id] = result
    return result


def _fallback_evaluation(image_path: Path, reason: str = "") -> dict:
    eval_id = uuid.uuid4().hex[:8]
    name = image_path.stem.lower()
    matched_venue = None
    for v in data_loader.get_all_venue_dicts():
        if v["id"] in name or any(a.replace(" ", "") in name for a in v.get("animals", [])):
            matched_venue = v
            break
    if not matched_venue:
        must_sees = [v for v in data_loader.get_all_venue_dicts() if v.get("must_see")]
        idx = int(hashlib.md5(name.encode()).hexdigest(), 16) % len(must_sees)
        matched_venue = must_sees[idx]

    venue_captions = {
        "panda": ("圆滚滚的黑眼圈", "你拍到了国民顶流"),
        "gorilla": ("野菜F4日常出镜", "大猩猩四兄弟同款pose"),
        "koala": ("睡神本神", "今天又是睡饱的一天"),
        "giraffe": ("脖子超长预警", "今天脖子又长了一厘米"),
        "tiger": ("百兽之王的眼神", "虎视眈眈"),
        "tangjiahe": ("2025新开的唐家河", "原生态保护区的日常"),
        "meerkat": ("站岗小哨兵", "网红打卡名场面"),
        "red_panda": ("滚滚本滚不是滚滚", "和小熊猫撞脸"),
    }
    venue_comments = {
        "panda": "这只圆滚滚正专心啃竹子，竹叶从嘴边掉下来都浑然不觉。这不就是上班摸鱼的我吗？建议存下来当表情包。",
        "gorilla": "香椿头/马兰头/小蒜头/枸杞头四兄弟里，你拍到了哪一只？看这个眼神，像不像周一早上的你？",
        "koala": "每天睡20小时的考拉，贡献了本届'最佛系员工'称号。给它一个枕头，它能睡到下一个冰川世纪。",
        "giraffe": "脖子长度 = 你吃自助餐的排队时长。但拍照时别站在它正下方，否则构图全是脖子。",
        "tiger": "百兽之王此刻的表情：'我看到你了，但懒得动。' 这就是顶级捕食者的从容。",
        "tangjiahe": "2025年10月才开放的唐家河展区，把四川的自然保护区搬到了南京。这张是'首发游客'限定款。",
        "meerkat": "它立正站好的样子，让我想起每天早晨地铁里端着手抓饼赶早高峰的自己。",
        "red_panda": "很多人以为小熊猫是熊猫小时候，其实它们和大熊猫是远亲。是趋同进化的经典案例（说人话：撞脸不撞DNA）。",
    }
    badges = {
        "panda": "国宝认证",
        "gorilla": "野菜F4认证",
        "koala": "澳洲睡眠代言",
        "giraffe": "长颈代表",
        "tiger": "百兽之王认证",
        "tangjiahe": "首发游客",
        "meerkat": "站岗小队长",
        "red_panda": "撞脸不撞DNA",
    }

    vid = matched_venue["id"]
    animal = matched_venue["animals"][0] if matched_venue.get("animals") else "未知动物"
    caption_a, caption_b = venue_captions.get(vid, ("定格瞬间", "你在红山的某个角落"))
    comment = venue_comments.get(vid, f"这张照片定格了你在{matched_venue['name']}的某个瞬间。")
    badge = badges.get(vid, "红山留念")

    hash_int = int(hashlib.md5(name.encode()).hexdigest()[:4], 16)
    vibe_score = 70 + (hash_int % 25)
    vibe_labels = ["治愈系", "出片神图", "自然氛围", "氛围感拉满", "小红书素材"]
    vibe_label = vibe_labels[hash_int % len(vibe_labels)]

    result = {
        "evaluation_id": eval_id,
        "image_path": str(image_path.relative_to(PHOTO_DIR.parent)),
        "animal_guess": animal,
        "animal_confidence": 65,
        "matched_venue_id": vid,
        "matched_venue_name": matched_venue["name"],
        "caption": f"{caption_a}｜{caption_b}",
        "vibe_score": vibe_score,
        "vibe_label": vibe_label,
        "comment": comment,
        "badge": badge,
        "tips": [
            "试试用低角度仰拍，让动物'俯视'镜头",
            "手机贴玻璃时关掉闪光灯，避免反光",
        ],
        "fallback": True,
        "fallback_reason": reason,
        "ts": datetime.now().isoformat(timespec="seconds"),
    }
    _evaluations[eval_id] = result
    return result


def get_evaluation(eval_id: str) -> Optional[dict]:
    return _evaluations.get(eval_id)