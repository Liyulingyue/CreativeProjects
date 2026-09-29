"""确定性计分引擎：同一份作答无论有没有 LLM，都能产出稳定的分数与报告。"""
import datetime

from ..schemas import Answer, Assessment, DimensionScore, Question, Report, SessionState


def _raw_score(q: Question, a: Answer) -> float:
    v = a.value
    if q.reverse:
        v = 6 - v  # 1-5 Likert 反向
    return float(v)


def score_dimensions(assessment: Assessment, answers: list[Answer]) -> list[DimensionScore]:
    """按维度求均分（1-5），归一化到 0-100。"""
    qmap = {q.id: q for q in assessment.questions}
    buckets: dict[str, list[float]] = {}
    for a in answers:
        q = qmap.get(a.question_id)
        if q:
            buckets.setdefault(q.dimension, []).append(_raw_score(q, a))

    scores: list[DimensionScore] = []
    for dim in assessment.dimensions:
        vals = buckets.get(dim.key, [])
        raw = sum(vals) / len(vals) if vals else 3.0
        percent = round((raw - 1) / 4 * 100)
        if assessment.report_type == "type_matching":
            pole = dim.pole_high if percent >= 50 else (dim.pole_low or dim.pole_high)
        else:
            levels = assessment.profile_levels.get(dim.key, ["偏低", "中等", "较高"])
            idx = 0 if percent < 34 else (1 if percent < 67 else 2)
            pole = levels[min(idx, len(levels) - 1)]
        scores.append(DimensionScore(key=dim.key, name=dim.name, raw=round(raw, 2), percent=percent, pole=pole))
    return scores


def _type_code(assessment: Assessment, scores: list[DimensionScore]) -> str:
    """type_matching：各轴按极性字母拼类型码（如 E + N + T + P）。"""
    letters = []
    for s in scores:
        dim = assessment.dimension_map()[s.key]
        letters.append(dim.pole_high[-1] if s.percent >= 50 else (dim.pole_low[-1] if dim.pole_low else dim.pole_high[-1]))
    return "".join(letters)


def _template_text(assessment: Assessment, type_code: str, scores: list[DimensionScore]) -> tuple[str, str]:
    t = assessment.type_by_code(type_code)
    desc = t.description if t else "暂无该类型的描述模板。"
    advice = t.advice if t else "暂无该类型的建议模板。"
    profile = "；".join(f"{s.name}（{s.percent} 分，{s.pole}）" for s in scores)
    return desc + f"\n\n你的维度画像：{profile}。", advice


def build_report(assessment: Assessment, session: SessionState, narrative: str = "") -> Report:
    scores = score_dimensions(assessment, session.answers)
    report = Report(
        session_id=session.session_id,
        assessment_id=assessment.id,
        assessment_name=assessment.name,
        dimension_scores=scores,
        narrative=narrative,
        narrative_source="llm" if narrative else "template",
    )
    if assessment.report_type == "type_matching":
        code = _type_code(assessment, scores)
        t = assessment.type_by_code(code)
        report.type_code = code
        report.type_name = t.name if t else code
        report.keywords = t.keywords if t else []
        report.type_description, report.advice = _template_text(assessment, code, scores)
    else:
        report.type_description = "；".join(f"{s.name}：{s.pole}（{s.percent} 分）" for s in scores)
        report.advice = "维度画像仅供参考，建议结合具体情境解读。"
    if not report.narrative:
        report.narrative = report.type_description
    return report


def now_iso() -> str:
    return datetime.datetime.now().isoformat(timespec="seconds")
