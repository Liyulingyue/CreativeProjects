#!/usr/bin/env python
"""models.py — Pydantic models for API requests and responses."""
from pydantic import BaseModel
from typing import List, Optional, Dict
from datetime import datetime


class ChatRequest(BaseModel):
    """Chat message request."""
    prompt: str
    history: Optional[List[Dict]] = None
    session_id: Optional[str] = None
    workspace: Optional[str] = None
    # Whether to create a PTY terminal session for this chat
    # Home route sets to False (pure LLM), Workbench sets to True (can execute code)
    # Defaults to True for backward compatibility
    needs_terminal: Optional[bool] = True


class ChatResponse(BaseModel):
    """Chat message response."""
    response: str
    history: List[Dict]


class ChatHistoryItem(BaseModel):
    """A single chat message in history."""
    id: int
    session_id: str
    workspace: Optional[str]
    role: str
    content: str
    metadata: Optional[str] = None
    timestamp: str
    feedback: Optional[str] = None  # 👍 or 👎 rating
    feedback_comment: Optional[str] = None  # User's comment on this message


class ChatHistoryResponse(BaseModel):
    """Response for history queries."""
    items: List[ChatHistoryItem]
    total: int
    session_id: str
    workspace: Optional[str] = None


class DeleteResponse(BaseModel):
    """Response for delete operations."""
    message: str
    deleted_count: int


class FileItem(BaseModel):
    """A file or directory item."""
    name: str
    path: str
    abs_path: str
    type: str  # "file" or "directory"


class FileListResponse(BaseModel):
    """Response for file listing."""
    files: List[FileItem]
    root: str


class FileReadResponse(BaseModel):
    """Response for file reading."""
    content: str
    path: str


class WorkspaceResolveResponse(BaseModel):
    """Response for workspace path resolution."""
    absolute: str


class SessionInfo(BaseModel):
    session_id: str
    workspace: Optional[str] = None
    last_activity: str
    good_count: int = 0  # Number of positive feedbacks
    bad_count: int = 0   # Number of negative feedbacks
