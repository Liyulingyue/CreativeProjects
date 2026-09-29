"""自动生成测评：给一个主题（如「领导力」「拖延倾向」），LLM 产出
维度 + 题目 + 计分规则 + 结果模板，通过 schema 校验后入库。"""
import json
import re
import uuid
from typing import Optional

from .. import llm, store
from ..schemas import Assessment

SCHEMA_HINT = """{
  "name": "测评名称",
  "description": "一句话介绍这份测评测什么、适合谁",
  "subject": "human | cat | other",
  "report_type": "type_matching | dimension_profile",
  "dimensions": [
    {"key": "英文短标识如 drive", "name": "维度中文名", "pole_high": "高分极描述", "pole_low": "低分极描述（type_matching 时为对极字母，如 内向 I）", "description": "维度解释"}
  ],
  "questions": [
    {"text": "陈述式题目，第一人称或对对象描述", "dimension": "对应维度 key", "reverse": false}
  ],
  "types": [
    {"code": "类型码（type_matching 时按各维度 pole_high/pole_low 末位字母组合）", "name": "类型名", "keywords": ["关键词"], "description": "类型描述", "advice": "针对建议"}
  ],
  "profile_levels": {"维度key": ["低分档描述", "中分档描述", "高分档描述"]}
}"""


async def generate_assessment(
    topic: str,
    question_count: int = 12,
    dimension_count: int = 4,
    subject: Optional[str] = None,
    extra_requirements: str = "",
) -> Assessment:
    if not llm.llm_enabled():
        raise RuntimeError("LLM 未配置：自动生成测评需要在 backend/.env 中设置 LLM_API_KEY")

    subject_line = f"测评对象是「{subject}」。" if subject else "测评对象请根据主题自行判断（人/宠物/其他）。"
    prompt = (
        f"请设计一份关于「{topic}」的心理测评问卷。\n"
        f"{subject_line}\n"
        f"要求：\n"
        f"- {dimension_count} 个维度，{question_count} 道题，每个维度至少 3 题\n"
        f"- 约 1/4 的题目为反向计分（reverse=true）\n"
        f"- 若 report_type 为 type_matching：dimensions 中 pole_high/pole_low 的末位必须是单个字母（如 外向 E / 内向 I），"
        f"types 覆盖所有维度字母组合（2 维度 4 型、4 维度 16 型）\n"
        f"- 若 report_type 为 dimension_profile：types 留空数组，profile_levels 每个维度给 3 档描述\n"
        f"{('- ' + extra_requirements) if extra_requirements else ''}\n"
        f"严格按照如下 JSON Schema 输出，不要输出任何其他内容：\n{SCHEMA_HINT}"
    )
    data = await llm.chat_json(prompt, system="你是一名资深的心理测量学专家与问卷设计师。")
    data.setdefault("subject", subject or "human")
    data["id"] = f"gen-{uuid.uuid4().hex[:8]}"
    data["source"] = "ai_generated"
    data.setdefault("time_estimate_minutes", max(5, round(question_count * 0.7)))
    # 生成器只给题目文本，选项统一用标准五级 Likert
    for q in data.get("questions", []):
        q.setdefault("reverse", False)
        q.pop("options", None)
    assessment = Assessment.model_validate(data)
    store.save_generated_assessment(assessment)
    return assessment


def sanitize_id(name: str) -> str:
    return re.sub(r"[^a-zA-Z0-9_-]", "-", name.lower()).strip("-")[:40] or "assessment"
