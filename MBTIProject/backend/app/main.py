"""MBTIProject 测评平台 API。"""
from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

from . import store
from .schemas import Assessment, SessionState
from .services import evaluator, generator, session as session_svc


@asynccontextmanager
async def lifespan(_: FastAPI):
    yield


app = FastAPI(title="MBTIProject 测评平台", version="0.1.0", lifespan=lifespan)
app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:5173", "http://127.0.0.1:5173"],
    allow_methods=["*"],
    allow_headers=["*"],
)


class AnswerIn(BaseModel):
    question_id: str
    value: int = Field(ge=1, le=5)


class GenerateIn(BaseModel):
    topic: str = Field(min_length=2, max_length=100)
    question_count: int = Field(default=12, ge=4, le=40)
    dimension_count: int = Field(default=4, ge=2, le=8)
    subject: str = ""
    extra_requirements: str = ""


@app.get("/api/health")
def health():
    return {"status": "ok"}


# ---------- 测评定义 ----------
@app.get("/api/assessments", response_model=list[Assessment])
def list_assessments():
    return store.list_assessments()


@app.get("/api/assessments/{assessment_id}", response_model=Assessment)
def get_assessment(assessment_id: str):
    assessment = store.get_assessment(assessment_id)
    if not assessment:
        from fastapi import HTTPException
        raise HTTPException(status_code=404, detail=f"测评不存在：{assessment_id}")
    return assessment


# ---------- 会话与报告 ----------
@app.post("/api/assessments/{assessment_id}/sessions", response_model=SessionState)
def start_session(assessment_id: str):
    return session_svc.start_session(assessment_id)


@app.get("/api/sessions/{session_id}", response_model=SessionState)
def get_session(session_id: str):
    s = store.get_session(session_id)
    if not s:
        from fastapi import HTTPException
        raise HTTPException(status_code=404, detail=f"会话不存在：{session_id}")
    return s


@app.post("/api/sessions/{session_id}/answers", response_model=SessionState)
def submit_answer(session_id: str, payload: AnswerIn):
    return session_svc.submit_answer(session_id, payload.question_id, payload.value)


@app.get("/api/sessions/{session_id}/report")
def get_report(session_id: str, use_llm: bool = True):
    return session_svc.get_report(session_id, use_llm=use_llm)


# ---------- AI 生成测评 ----------
@app.post("/api/assessments/generate", response_model=Assessment)
async def generate_assessment(payload: GenerateIn):
    try:
        return await generator.generate_assessment(
            topic=payload.topic,
            question_count=payload.question_count,
            dimension_count=payload.dimension_count,
            subject=payload.subject or None,
            extra_requirements=payload.extra_requirements,
        )
    except (RuntimeError, ValueError) as e:
        from fastapi import HTTPException
        raise HTTPException(status_code=503, detail=str(e))


# ---------- 测评合理性评价 ----------
@app.post("/api/assessments/{assessment_id}/evaluation")
async def evaluate_assessment(assessment_id: str, use_llm: bool = True):
    try:
        return await evaluator.evaluate_assessment(assessment_id, use_llm=use_llm)
    except RuntimeError as e:
        from fastapi import HTTPException
        raise HTTPException(status_code=503, detail=str(e))
