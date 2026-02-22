#!/usr/bin/env python
"""routers/chat.py — Chat and chat history routes."""
from fastapi import APIRouter, HTTPException
from fastapi.responses import StreamingResponse
from typing import Dict, List, Optional
from ..models import ChatRequest, ChatResponse, ChatHistoryResponse, ChatHistoryItem, DeleteResponse, SessionInfo
from ..services.chat_service import ChatService

router = APIRouter(tags=["chat"])
service = ChatService()


@router.post("/chat", response_model=None)
async def chat(request: ChatRequest, stream: bool = False):
    """Send a chat message and get response (supports streaming)."""
    try:
        result = await service.handle_chat(request, stream=stream)
        
        # If streaming, wrap generator in StreamingResponse
        if stream:
            async def event_generator():
                async for event in result:
                    yield event
            
            return StreamingResponse(event_generator(), media_type="text/event-stream")
        
        # Non-streaming returns ChatResponse model
        return result
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/chat/history", response_model=ChatHistoryResponse)
async def get_history(
    session_id: str,
    workspace: Optional[str] = None,
    limit: int = 100,
    offset: int = 0
):
    """Retrieve chat history for a session."""
    try:
        items = service.get_history(session_id, workspace, limit, offset)
        total = service.get_history_count(session_id, workspace)
        
        history_items = [
            ChatHistoryItem(
                id=item["id"],
                session_id=item["session_id"],
                workspace=item["workspace"],
                role=item["role"],
                content=item["content"],
                metadata=item.get("metadata"),
                timestamp=item["timestamp"],
                feedback=item.get("feedback"),
                feedback_comment=item.get("feedback_comment")
            )
            for item in items
        ]
        
        return ChatHistoryResponse(
            items=history_items,
            total=total,
            session_id=session_id,
            workspace=workspace
        )
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.delete("/chat/history", response_model=DeleteResponse)
async def delete_history(
    session_id: str,
    workspace: Optional[str] = None
):
    """Delete chat history for a session."""
    try:
        deleted_count = service.delete_history(session_id, workspace)
        return DeleteResponse(
            message="Chat history deleted",
            deleted_count=deleted_count
        )
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/chat/sessions", response_model=List[SessionInfo])
async def get_sessions():
    """Get list of active session IDs."""
    try:
        return [SessionInfo(**session) for session in service.get_sessions()]
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.post("/chat/feedback")
async def save_feedback(
    message_id: int,
    session_id: str,
    feedback: str,
    comment: Optional[str] = None
):
    """Save user feedback for a message (training data collection).
    
    Args:
        message_id: ID of the chat message being evaluated
        session_id: Session ID for context
        feedback: Feedback string (e.g. '👍', '👎', 'good', 'bad', etc.)
        comment: Optional user comment/explanation for the feedback
    
    Returns:
        Success response with feedback ID for deduplication
    """
    try:
        print(f"[DEBUG] Received feedback request: message_id={message_id}, session_id={session_id}, feedback={feedback}, comment={comment}")
        feedback_id = service.save_feedback(message_id, session_id, feedback, comment)
        print(f"[DEBUG] Saved feedback with ID: {feedback_id}")
        return {
            "success": True,
            "feedback_id": feedback_id,
            "message": f"Feedback '{feedback}' saved for message {message_id}"
        }
    except Exception as e:
        print(f"[DEBUG] Error saving feedback: {e}")
        raise HTTPException(status_code=500, detail=str(e))

