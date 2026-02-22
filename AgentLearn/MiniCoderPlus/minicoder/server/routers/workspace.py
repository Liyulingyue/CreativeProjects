#!/usr/bin/env python
"""routers/workspace.py — Workspace resolution routes."""
from fastapi import APIRouter
from typing import Optional
from pathlib import Path
from ...core.settings import settings
from ..models import WorkspaceResolveResponse

router = APIRouter(tags=["workspace"])


@router.get("/workspace/resolve", response_model=WorkspaceResolveResponse)
async def resolve_workspace(path: Optional[str] = None):
    """Resolve a workspace identifier to an absolute path."""
    # Default to configured workspace_dir
    if not path or path.lower() == "none":
        return WorkspaceResolveResponse(absolute=str(settings.WORKSPACE_DIR))

    candidate = Path(path)
    if not candidate.is_absolute():
        candidate = settings.BASE_DIR / candidate
    
    return WorkspaceResolveResponse(absolute=str(candidate))
