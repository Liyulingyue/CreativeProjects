import os
from pathlib import Path
from typing import Optional

from fastapi import APIRouter, HTTPException
from ..deps import state
from ..models import FileNode, SearchResult

search = APIRouter()


def _get_mime_type(path: Path) -> str:
    ext = path.suffix.lower()
    mime_types = {
        ".txt": "text/plain",
        ".md": "text/markdown",
        ".json": "application/json",
        ".jpg": "image/jpeg",
        ".png": "image/png",
        ".pdf": "application/pdf",
    }
    return mime_types.get(ext, "application/octet-stream")


def _path_to_filenode(path: Path) -> FileNode:
    stat = path.stat()
    return FileNode(
        name=path.name,
        path=str(path),
        is_dir=path.is_dir(),
        size=stat.st_size if path.is_file() else 0,
        modified=path.strftime("%Y-%m-%dT%H:%M:%S"),
        created=path.strftime("%Y-%m-%dT%H:%M:%S"),
        extension=path.suffix.lower(),
        mime_type=_get_mime_type(path),
    )


@search.get("/query")
def search_files(q: str, path: Optional[str] = None, limit: int = 50) -> SearchResult:
    storage_path = state.get_settings().storage_path
    if not q or len(q) < 2:
        raise HTTPException(status_code=400, detail="Query too short")

    search_root = Path(storage_path) if not path else Path(path)
    if not search_root.exists():
        raise HTTPException(status_code=404, detail="Search path not found")

    query_lower = q.lower()
    results = []

    for entry in search_root.rglob("*"):
        try:
            if query_lower in entry.name.lower():
                results.append(_path_to_filenode(entry))
                if len(results) >= limit:
                    break
        except (PermissionError, OSError):
            continue

    return SearchResult(
        items=results,
        total=len(results),
        query=q,
    )


@search.get("/suggest")
def suggest(q: str, limit: int = 10) -> list[str]:
    storage_path = state.get_settings().storage_path
    if not q or len(q) < 1:
        return []

    search_root = Path(storage_path)
    suggestions = set()
    query_lower = q.lower()

    for entry in search_root.rglob("*"):
        try:
            if query_lower in entry.name.lower():
                suggestions.add(entry.name)
                if len(suggestions) >= limit:
                    break
        except (PermissionError, OSError):
            continue

    return list(suggestions)[:limit]
