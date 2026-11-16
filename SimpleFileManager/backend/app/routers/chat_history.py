from typing import Optional
from fastapi import APIRouter, HTTPException
from ..deps import state, session_owner
from ..models import ChatSession, ChatMessage, ChatHistoryResponse, CreateSessionRequest, AddMessageRequest, UpdateTitleRequest

chat_history_router = APIRouter(tags=["chat_history"])


def _require_session(session_id: str):
    """One of this person's conversations.

    A conversation belongs to the person who started it. Asking for
    somebody else's is answered as though it were not there, rather
    than as though it were forbidden -- there is no reason to tell a
    stranger that a conversation exists.
    """
    session = state.get_chat_history().get_session(session_id)
    if not session:
        raise HTTPException(status_code=404, detail="找不到这个会话")
    return session


@chat_history_router.get("/sessions", response_model=ChatHistoryResponse)
def get_sessions(session_type: Optional[str] = None):
    """Get all chat sessions, optionally filtered by type (chat or rag)"""
    chat_svc = state.get_chat_history()
    sessions = chat_svc.get_sessions(session_type)
    return ChatHistoryResponse(
        sessions=sessions,
        current_session_id=sessions[0].id if sessions else None
    )


@chat_history_router.post("/sessions", response_model=ChatSession)
def create_session(req: CreateSessionRequest):
    """Create a new chat session"""
    chat_svc = state.get_chat_history()
    return chat_svc.create_session(req.session_type, owner=session_owner())


@chat_history_router.get("/sessions/{session_id}", response_model=ChatSession)
def get_session(session_id: str):
    """Get a specific chat session with messages"""
    return _require_session(session_id)


@chat_history_router.delete("/sessions/{session_id}")
def delete_session(session_id: str):
    """Delete a chat session and its messages"""
    _require_session(session_id)
    chat_svc = state.get_chat_history()
    chat_svc.delete_session(session_id)
    return {"success": True}


@chat_history_router.post("/messages", response_model=ChatMessage)
def add_message(req: AddMessageRequest):
    """Add a message to a chat session"""
    chat_svc = state.get_chat_history()
    _require_session(req.session_id)
    # use_fast is no longer forwarded: the column stays on disk for
    # back-compat with rows saved before the providers refactor, but
    # new writes take the default (0). The legacy `use_fast` field
    # is still accepted on the request so old clients don't 422.
    return chat_svc.add_message(
        session_id=req.session_id,
        role=req.role,
        content=req.content,
        sources=req.sources,
        reasoning=getattr(req, "reasoning", "") or "",
        activity=getattr(req, "activity", None) or [],
        ttft_ms=getattr(req, "ttft_ms", 0) or 0,
        total_ms=getattr(req, "total_ms", 0) or 0,
        tokens=getattr(req, "tokens", 0) or 0,
        tps=getattr(req, "tps", 0.0) or 0.0,
        provider_id=getattr(req, "provider_id", None),
        plans=getattr(req, "plans", None),
    )


@chat_history_router.put("/sessions/{session_id}/title")
def update_title(session_id: str, req: UpdateTitleRequest):
    """Update the title of a chat session"""
    chat_svc = state.get_chat_history()
    _require_session(session_id)
    chat_svc.update_session_title(session_id, req.title)
    return {"success": True}
