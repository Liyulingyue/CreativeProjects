"""测评会话流程：开始 -> 逐题作答 -> 完成 -> 报告（可选 LLM 深度解读）。"""
import uuid

from fastapi import HTTPException

from .. import llm, store
from ..schemas import Answer, Assessment, Report, SessionState
from .scoring import build_report, now_iso

MAX_ANSWER = 5
MIN_ANSWER = 1


def _validate_value(assessment: Assessment, value: int) -> None:
    if not (MIN_ANSWER <= value <= MAX_ANSWER):
        raise HTTPException(status_code=422, detail=f"作答值需在 {MIN_ANSWER}-{MAX_ANSWER} 之间")


def start_session(assessment_id: str) -> SessionState:
    assessment = store.get_assessment(assessment_id)
    if not assessment:
        raise HTTPException(status_code=404, detail=f"测评不存在：{assessment_id}")
    session = SessionState(session_id=uuid.uuid4().hex[:12], assessment_id=assessment_id, started_at=now_iso())
    store.save_session(session)
    return session


def submit_answer(session_id: str, question_id: str, value: int) -> SessionState:
    session = store.get_session(session_id)
    if not session:
        raise HTTPException(status_code=404, detail=f"会话不存在：{session_id}")
    if session.status == "finished":
        raise HTTPException(status_code=409, detail="该测评已完成")

    assessment = store.get_assessment(session.assessment_id)
    if not assessment:
        raise HTTPException(status_code=404, detail="测评数据缺失")
    qmap = {q.id: q for q in assessment.questions}
    if question_id not in qmap:
        raise HTTPException(status_code=422, detail=f"题目不存在：{question_id}")
    if any(a.question_id == question_id for a in session.answers):
        raise HTTPException(status_code=409, detail="该题已作答")
    _validate_value(assessment, value)

    session.answers.append(Answer(question_id=question_id, value=value))
    session.current_index = len(session.answers)
    if len(session.answers) >= len(assessment.questions):
        session.status = "finished"
        session.finished_at = now_iso()
    store.save_session(session)
    return session


def get_report(session_id: str, use_llm: bool = True) -> Report:
    session = store.get_session(session_id)
    if not session:
        raise HTTPException(status_code=404, detail=f"会话不存在：{session_id}")
    assessment = store.get_assessment(session.assessment_id)
    if not assessment:
        raise HTTPException(status_code=404, detail="测评数据缺失")
    if session.status != "finished":
        raise HTTPException(status_code=409, detail="测评尚未完成，无法生成报告")

    narrative = ""
    if use_llm and llm.llm_enabled():
        narrative = _llm_narrative(assessment, session)
    return build_report(assessment, session, narrative=narrative)


def _llm_narrative(assessment: Assessment, session: SessionState) -> str:
    """把结构化得分交给 LLM，产出更个性化的深度解读；失败则回退模板文案。"""
    from .scoring import score_dimensions

    scores = score_dimensions(assessment, session.answers)
    lines = [f"- {s.name}: {s.raw}/5 ({s.percent}%, 倾向 {s.pole})" for s in scores]
    subject = {("cat"): "猫咪", ("human"): "被测者"}.get(assessment.subject, "被测者")
    prompt = (
        f"这是一份「{assessment.name}」的测评结果（对象：{subject}）。\n"
        f"维度得分：\n" + "\n".join(lines) + "\n\n"
        "请写一段 200-300 字的深度解读，先概括整体特点，再针对每个维度给出具体、"
        "可操作的建议，语气亲切专业，不要复述分数本身。直接输出正文。"
    )
    try:
        return llm.chat(prompt, system="你是一名温和、专业的心理测评解读师。", temperature=0.6)
    except Exception:
        return ""
