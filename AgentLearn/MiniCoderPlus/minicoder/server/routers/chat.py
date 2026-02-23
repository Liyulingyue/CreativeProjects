#!/usr/bin/env python
"""routers/chat.py — Chat and chat history routes."""
from fastapi import APIRouter, HTTPException, Body
from fastapi.responses import StreamingResponse
from typing import Dict, List, Optional
from ..models import ChatRequest, ChatResponse, ChatHistoryResponse, ChatHistoryItem, DeleteResponse, SessionInfo, FeedbackRequest
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
async def save_feedback(request: FeedbackRequest):
    """Save user feedback for a message (training data collection).
    
    Args:
        request: Feedback data containing message_id, session_id, feedback, comment, and context_snapshot
    
    Returns:
        Success response with feedback ID for deduplication
    """
    try:
        feedback_id = service.save_feedback(
            request.message_id, 
            request.session_id, 
            request.feedback, 
            request.comment, 
            request.context_snapshot
        )
        return {
            "success": True,
            "feedback_id": feedback_id,
            "message": f"Feedback '{request.feedback}' saved for message {request.message_id}"
        }
    except Exception as e:
        print(f"[ERROR] Error saving feedback: {e}")
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/chat/feedbacks")
async def get_feedbacks(
    feedback_type: Optional[str] = None,
    session_id: Optional[str] = None,
    limit: int = 100,
    offset: int = 0,
    order_by: str = 'timestamp DESC'
):
    """Get all feedbacks with optional filtering for analysis.
    
    Args:
        feedback_type: Filter by '👍', '👎' or None for all
        session_id: Filter by specific session or None for all
        limit: Number of results per page
        offset: Pagination offset
        order_by: Sort order (e.g., 'timestamp DESC', 'feedback ASC')
    
    Returns:
        List of feedbacks with message content and metadata
    """
    try:
        print(f"[DEBUG] get_feedbacks called: feedback_type={feedback_type}, session_id={session_id}, limit={limit}, offset={offset}, order_by={order_by}")
        items = service.get_feedbacks(feedback_type, session_id, limit, offset, order_by)
        total = service.get_feedbacks_count(feedback_type, session_id)
        
        print(f"[DEBUG] Got {len(items)} feedbacks, total={total}")
        return {
            "items": items,
            "total": total,
            "limit": limit,
            "offset": offset,
            "filters": {
                "feedback_type": feedback_type,
                "session_id": session_id
            }
        }
    except Exception as e:
        import traceback
        print(f"[ERROR] Failed to get feedbacks: {str(e)}")
        print(traceback.format_exc())
        raise HTTPException(status_code=500, detail=str(e))


@router.delete("/chat/feedback/{feedback_id}", response_model=DeleteResponse)
async def delete_feedback(feedback_id: int):
    """Delete a feedback record."""
    try:
        success = service.delete_feedback(feedback_id)
        if not success:
            raise HTTPException(status_code=404, detail="Feedback not found")
        return DeleteResponse(
            message="Feedback record deleted",
            deleted_count=1
        )
    except HTTPException:
        raise
    except Exception as e:
        print(f"[ERROR] Failed to delete feedback {feedback_id}: {str(e)}")
        raise HTTPException(status_code=500, detail=str(e))


@router.delete("/chat/feedbacks/batch", response_model=DeleteResponse)
async def delete_feedbacks_batch(feedback_ids: List[int] = Body(...)):
    """Delete multiple feedback records."""
    try:
        deleted_count = service.delete_feedbacks_batch(feedback_ids)
        return DeleteResponse(
            message=f"Deleted {deleted_count} feedback records",
            deleted_count=deleted_count
        )
    except Exception as e:
        print(f"[ERROR] Failed to batch delete feedbacks: {str(e)}")
        raise HTTPException(status_code=500, detail=str(e))

