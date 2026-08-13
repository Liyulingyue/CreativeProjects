from typing import Optional
from fastapi import APIRouter, HTTPException
from ..deps import state
from ..models import ChatSession, ChatMessage, ChatHistoryResponse, CreateSessionRequest, AddMessageRequest, UpdateTitleRequest

chat_history_router = APIRouter(tags=["chat_history"])


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
    return chat_svc.create_session(req.session_type)


@chat_history_router.get("/sessions/{session_id}", response_model=ChatSession)
def get_session(session_id: str):
    """Get a specific chat session with messages"""
    chat_svc = state.get_chat_history()
    session = chat_svc.get_session(session_id)
    if not session:
        raise HTTPException(status_code=404, detail="Session not found")
    return session


@chat_history_router.delete("/sessions/{session_id}")
def delete_session(session_id: str):
    """Delete a chat session and its messages"""
    chat_svc = state.get_chat_history()
    chat_svc.delete_session(session_id)
    return {"success": True}


@chat_history_router.post("/messages", response_model=ChatMessage)
def add_message(req: AddMessageRequest):
    """Add a message to a chat session"""
    chat_svc = state.get_chat_history()
    session = chat_svc.get_session(req.session_id)
    if not session:
        raise HTTPException(status_code=404, detail="Session not found")
    return chat_svc.add_message(
        session_id=req.session_id,
        role=req.role,
        content=req.content,
        sources=req.sources
    )


@chat_history_router.put("/sessions/{session_id}/title")
def update_title(session_id: str, req: UpdateTitleRequest):
    """Update the title of a chat session"""
    chat_svc = state.get_chat_history()
    session = chat_svc.get_session(session_id)
    if not session:
        raise HTTPException(status_code=404, detail="Session not found")
    chat_svc.update_session_title(session_id, req.title)
    return {"success": True}
