#!/usr/bin/env python
"""routers/files.py — File listing and reading routes."""
from fastapi import APIRouter, HTTPException
from typing import Optional
from pathlib import Path
from ...core.settings import settings
from ..models import FileListResponse, FileReadResponse, FileItem

router = APIRouter(tags=["files"])


@router.get("/files/list", response_model=FileListResponse)
async def list_files(path: Optional[str] = None):
    """List files in the workspace or at a specific path."""
    # Resolve root
    root_path = settings.WORKSPACE_DIR
    if path:
        root_path = Path(path)
        if not root_path.is_absolute():
            root_path = settings.BASE_DIR / root_path

    if not root_path.exists():
        return FileListResponse(files=[], root=str(root_path))

    try:
        items = []
        for item in sorted(root_path.iterdir(), key=lambda x: (not x.is_dir(), x.name.lower())):
            # Ignore hidden files/folders (except common ones like .env)
            if item.name.startswith('.') and item.name not in ['.env', '.gitignore', '.vscode']:
                continue
            
            is_dir = item.is_dir()
            items.append(FileItem(
                name=item.name,
                path=str(item.relative_to(root_path)).replace('\\', '/'),
                abs_path=str(item.absolute()),
                type="directory" if is_dir else "file"
            ))
        
        return FileListResponse(files=items, root=str(root_path))
    
    except PermissionError:
        raise HTTPException(status_code=403, detail="Permission denied")
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/files/read", response_model=FileReadResponse)
async def read_file_content(path: str):
    """Read content of a file."""
    p = Path(path)
    
    if not p.exists() or not p.is_file():
        raise HTTPException(status_code=404, detail="File not found")
    
    try:
        content = p.read_text(encoding='utf-8', errors='replace')
        return FileReadResponse(content=content, path=str(p))
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Error reading file: {str(e)}")
