"""测评合理性评价（LLM-as-a-Judge）：
1. 确定性体检 —— 不依赖 LLM 的结构化指标（题量、维度覆盖、反向题比例等）
2. LLM 质性评审 —— 内容效度、题目表述、计分逻辑等维度打分与改进建议
"""
from typing import Optional

from pydantic import BaseModel, Field

from .. import llm, store
from ..schemas import Assessment


class CheckItem(BaseModel):
    name: str
    passed: bool
    detail: str


class StructuredAudit(BaseModel):
    question_count: int
    dimension_count: int
    reverse_ratio: float          # 反向题占比
    coverage_ok: bool             # 每个维度都有题目
    items: list[CheckItem] = Field(default_factory=list)


class LlmReview(BaseModel):
    valid: Optional[int] = None   # 内容效度 0-100
    clarity: Optional[int] = None # 表述清晰度 0-100
    scoring_logic: Optional[int] = None  # 计分与结果映射合理性 0-100
    overall: Optional[int] = None
    issues: list[str] = Field(default_factory=list)
    suggestions: list[str] = Field(default_factory=list)
    summary: str = ""


class EvaluationReport(BaseModel):
    assessment_id: str
    assessment_name: str
    structured: StructuredAudit
    llm_review: Optional[LlmReview] = None
    verdict: str = ""             # 一句话结论


def structural_audit(assessment: Assessment) -> StructuredAudit:
    dim_keys = {d.key for d in assessment.dimensions}
    used_keys = {q.dimension for q in assessment.questions}
    reverse_count = sum(1 for q in assessment.questions if q.reverse)
    total = len(assessment.questions)
    ratio = reverse_count / total if total else 0.0
    per_dim_counts = {k: sum(1 for q in assessment.questions if q.dimension == k) for k in sorted(dim_keys)}
    thin_dims = [f"{k}({n}题)" for k, n in per_dim_counts.items() if n < 3]

    items = [
        CheckItem(
            name="题目数量充足",
            passed=total >= 8,
            detail=f"共 {total} 题" + ("，建议至少 8 题" if total < 8 else ""),
        ),
        CheckItem(
            name="维度全覆盖",
            passed=dim_keys == used_keys and not thin_dims,
            detail=("所有维度均有题目" if dim_keys == used_keys else f"未覆盖维度：{sorted(dim_keys - used_keys)}")
            + (f"；题目过少的维度：{', '.join(thin_dims)}" if thin_dims else ""),
        ),
        CheckItem(
            name="反向题比例合理",
            passed=(total > 0 and 0.1 <= ratio <= 0.5) or (total > 0 and ratio == 0 and assessment.report_type == "dimension_profile"),
            detail=f"反向题 {reverse_count}/{total}（{ratio:.0%}），经验区间 10%-50%",
        ),
        CheckItem(
            name="结果模板完备",
            passed=(assessment.report_type == "type_matching" and bool(assessment.types)) or assessment.report_type == "dimension_profile",
            detail=f"report_type={assessment.report_type}，types 数量 {len(assessment.types)}",
        ),
    ]
    if assessment.report_type == "type_matching":
        letters_high = {d.key: d.pole_high[-1] for d in assessment.dimensions}
        letters_low = {d.key: (d.pole_low[-1] if d.pole_low else "") for d in assessment.dimensions}
        items.append(CheckItem(
            name="类型码极性可区分",
            passed=all(letters_high.values()) and all(letters_low.values()) and set(letters_high.values()).isdisjoint(set(letters_low.values())),
            detail=f"高分极字母 {letters_high}，低分极字母 {letters_low}",
        ))
    return StructuredAudit(
        question_count=total,
        dimension_count=len(assessment.dimensions),
        reverse_ratio=round(ratio, 3),
        coverage_ok=dim_keys == used_keys,
        items=items,
    )


def _verdict(structured: StructuredAudit, review: Optional[LlmReview]) -> str:
    failed = [i.name for i in structured.items if not i.passed]
    if review and review.overall is not None:
        grade = "合格" if review.overall >= 70 else "建议修改后使用"
        tail = f"；结构问题：{('、'.join(failed))}" if failed else ""
        return f"LLM 综合评分 {review.overall}/100，{grade}{tail}"
    if failed:
        return f"结构体检未通过：{'、'.join(failed)}"
    return "结构体检通过（未启用 LLM 评审，缺少质性意见）"


async def evaluate_assessment(assessment_id: str, use_llm: bool = True) -> EvaluationReport:
    assessment = store.get_assessment(assessment_id)
    if not assessment:
        raise RuntimeError(f"测评不存在：{assessment_id}")

    structured = structural_audit(assessment)
    review: Optional[LlmReview] = None
    if use_llm and llm.llm_enabled():
        review = await _llm_review(assessment)
    return EvaluationReport(
        assessment_id=assessment.id,
        assessment_name=assessment.name,
        structured=structured,
        llm_review=review,
        verdict=_verdict(structured, review),
    )


async def _llm_review(assessment: Assessment) -> LlmReview:
    q_lines = [f"{i + 1}. [{q.dimension}{'/反向' if q.reverse else ''}] {q.text}" for i, q in enumerate(assessment.questions)]
    d_lines = [f"- {d.key}（{d.name}）：{d.pole_low or '低'} ←→ {d.pole_high}" for d in assessment.dimensions]
    t_lines = [f"- {t.code} {t.name}" for t in assessment.types] or ["（无类型模板，维度画像型报告）"]
    prompt = (
        f"请作为心理测量学评审专家，评审以下测评问卷的合理性。\n\n"
        f"测评名称：{assessment.name}（{assessment.description}）\n"
        f"报告类型：{assessment.report_type}\n"
        f"维度：\n{chr(10).join(d_lines)}\n\n"
        f"题目：\n{chr(10).join(q_lines)}\n\n"
        f"结果模板：\n{chr(10).join(t_lines)}\n\n"
        "请从以下方面评审：1) 内容效度——题目是否真正测到对应维度；2) 表述清晰度——是否存在歧义、双重否定、"
        "一题多问；3) 计分与结果映射逻辑是否自洽。输出 JSON：\n"
        '{"valid": 0-100, "clarity": 0-100, "scoring_logic": 0-100, "overall": 0-100, '
        '"issues": ["问题..."], "suggestions": ["改进建议..."], "summary": "一句话总结"}'
    )
    data = await llm.chat_json(prompt, system="你是一名严格但建设性的心理测量学评审专家。", temperature=0.2)
    return LlmReview.model_validate(data)
