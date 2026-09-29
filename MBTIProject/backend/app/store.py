"""基于 JSON 文件的轻量存储：内置测评 + AI 生成的测评 + 测评会话。"""
import json
import threading
from pathlib import Path
from typing import Optional

from .config import settings
from .schemas import Assessment, SessionState

_lock = threading.Lock()

BUILTIN_DIR = Path(__file__).resolve().parent / "data"          # app/data：内置测评
GENERATED_DIR = settings.store_dir / "generated"                # data/generated：AI 生成
SESSION_DIR = settings.store_dir / "sessions"                   # data/sessions：会话


def _read(path: Path) -> Optional[dict]:
    if not path.exists():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def _write(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")


def list_assessments() -> list[Assessment]:
    out: list[Assessment] = []
    for directory in (BUILTIN_DIR, GENERATED_DIR):
        if not directory.exists():
            continue
        for path in sorted(directory.glob("*.json")):
            raw = _read(path)
            if raw:
                out.append(Assessment.model_validate(raw))
    return out


def get_assessment(assessment_id: str) -> Optional[Assessment]:
    for directory in (BUILTIN_DIR, GENERATED_DIR):
        if not directory.exists():
            continue
        # 先按文件名快速命中，未命中再按文件内容里的 id 匹配
        raw = _read(directory / f"{assessment_id}.json")
        if raw:
            return Assessment.model_validate(raw)
        for path in directory.glob("*.json"):
            raw = _read(path)
            if raw and raw.get("id") == assessment_id:
                return Assessment.model_validate(raw)
    return None


def save_generated_assessment(assessment: Assessment) -> None:
    with _lock:
        _write(GENERATED_DIR / f"{assessment.id}.json", assessment.model_dump())


def save_session(session: SessionState) -> None:
    with _lock:
        _write(SESSION_DIR / f"{session.session_id}.json", session.model_dump())


def get_session(session_id: str) -> Optional[SessionState]:
    raw = _read(SESSION_DIR / f"{session_id}.json")
    return SessionState.model_validate(raw) if raw else None
