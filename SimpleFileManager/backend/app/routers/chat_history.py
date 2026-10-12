from typing import Optional
from fastapi import APIRouter, Depends, HTTPException
from ..deps import state
from .auth import get_current_auth
from ..models import ChatSession, ChatMessage, ChatHistoryResponse, CreateSessionRequest, AddMessageRequest, UpdateTitleRequest

chat_history_router = APIRouter(tags=["chat_history"])


def _require_session(session_id: str, owner_id: str = "local"):
    session = state.get_chat_history().get_session(session_id, owner_id)
    if not session:
        raise HTTPException(status_code=404, detail="Session not found")
    return session


@chat_history_router.get("/sessions", response_model=ChatHistoryResponse)
def get_sessions(session_type: Optional[str] = None, auth=Depends(get_current_auth)):
    """Get all chat sessions, optionally filtered by type (chat or rag)"""
    chat_svc = state.get_chat_history()
    # This app currently has one shared-password identity, not a users table.
    # Keep one stable owner key across token renewal so a new login does not
    # make all prior sessions disappear.
    sessions = chat_svc.get_sessions(session_type, "local")
    return ChatHistoryResponse(
        sessions=sessions,
        current_session_id=sessions[0].id if sessions else None
    )


@chat_history_router.post("/sessions", response_model=ChatSession)
def create_session(req: CreateSessionRequest, auth=Depends(get_current_auth)):
    """Create a new chat session"""
    chat_svc = state.get_chat_history()
    return chat_svc.create_session(req.session_type, "local")


@chat_history_router.get("/sessions/{session_id}", response_model=ChatSession)
def get_session(session_id: str, auth=Depends(get_current_auth)):
    """Get a specific chat session with messages"""
    return _require_session(session_id, "local")


@chat_history_router.delete("/sessions/{session_id}")
def delete_session(session_id: str, auth=Depends(get_current_auth)):
    """Delete a chat session and its messages"""
    _require_session(session_id, "local")
    chat_svc = state.get_chat_history()
    chat_svc.delete_session(session_id)
    return {"success": True}


@chat_history_router.post("/messages", response_model=ChatMessage)
def add_message(req: AddMessageRequest, auth=Depends(get_current_auth)):
    """Add a message to a chat session"""
    chat_svc = state.get_chat_history()
    _require_session(req.session_id, "local")
    return chat_svc.add_message(
        session_id=req.session_id,
        role=req.role,
        content=req.content,
        sources=req.sources,
        reasoning=getattr(req, "reasoning", "") or "",
        activity=getattr(req, "activity", None) or [],
    )


@chat_history_router.put("/sessions/{session_id}/title")
def update_title(session_id: str, req: UpdateTitleRequest, auth=Depends(get_current_auth)):
    """Update the title of a chat session"""
    chat_svc = state.get_chat_history()
    _require_session(session_id, "local")
    chat_svc.update_session_title(session_id, req.title)
    return {"success": True}
