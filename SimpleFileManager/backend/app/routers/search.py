import os
from datetime import datetime
from pathlib import Path
from typing import Optional

from fastapi import APIRouter, HTTPException, Query
from pydantic import BaseModel
from ..interaction import interaction
from ..deps import state, can_access, get_storage_root
from ..models import FileNode, SearchResult
from ..search_engine import search_engine

search = APIRouter()


class HybridSearchResponse(BaseModel):
    results: list[dict]
    keyword_count: int
    semantic_count: int
    query: str
    # Echoed back so the UI can label the active mode without tracking its own
    # optimistic state.
    mode: str = "hybrid"


@search.post("/hybrid", response_model=HybridSearchResponse)
def hybrid_search(
    query: str,
    top_k: int = Query(10, ge=1, le=100),
    mode: str = "hybrid",
    path_filter: str = "",
) -> HybridSearchResponse:
    # A search embeds its query, so it competes with indexing for the same
    # server; let indexing stand down for the duration. Keyword-only mode does
    # not need the embedding server at all.
    if mode != "keyword":
        interaction.mark_active()
    if not query.strip():
        raise HTTPException(status_code=400, detail="搜索词是空的")
    if mode not in ("hybrid", "keyword", "semantic"):
        raise HTTPException(status_code=400, detail=f"Unknown search mode: {mode}")
    payload = search_engine.hybrid_search(query, top_k, path_filter, mode)

    # The index holds every document on the machine. What this person
    # may not open does not come back to them -- and the count is
    # adjusted so the page does not show a total they never got.
    payload["results"] = _readable_results(payload.get("results", []))
    payload["total"] = len(payload["results"])
    # The engine returns counts, not the mode; echo the requested one so the UI
    # does not have to trust its own local state to label the active channel.
    payload["mode"] = mode
    return HybridSearchResponse(**payload)


def _readable_results(results: list) -> list:
    """The search results this person may open."""
    keep = []
    for r in results:
        path = r.get("path") or ""
        if not path:
            continue
        target = Path(path)
        if not target.is_absolute():
            target = get_storage_root() / path
        if can_access(target, "read"):
            keep.append(r)
    return keep


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
    # A Path has no strftime; the times come off the stat.
    stamp = datetime.fromtimestamp(stat.st_mtime).strftime("%Y-%m-%dT%H:%M:%S")
    created = datetime.fromtimestamp(stat.st_ctime).strftime("%Y-%m-%dT%H:%M:%S")
    return FileNode(
        name=path.name,
        path=str(path),
        is_dir=path.is_dir(),
        size=stat.st_size if path.is_file() else 0,
        modified=stamp,
        created=created,
        extension=path.suffix.lower(),
        mime_type=_get_mime_type(path),
    )


# How many entries one search may look at. A NAS can hold a hundred
# thousand files, and a keyword that matches nothing would otherwise
# mean walking all of them, every time, on a spinning disk.
MAX_SEARCH_ENTRIES = 50_000


@search.get("/query")
def search_files(q: str, path: Optional[str] = None, limit: int = 50) -> SearchResult:
    interaction.mark_active()
    storage_path = state.get_settings().storage_path
    if not q or len(q) < 2:
        raise HTTPException(status_code=400, detail="搜索词太短了")

    search_root = Path(storage_path) if not path else Path(path)
    if not search_root.exists():
        raise HTTPException(status_code=404, detail="找不到这个搜索路径")

    query_lower = q.lower()
    results = []
    examined = 0
    truncated = False

    for entry in search_root.rglob("*"):
        examined += 1
        if examined > MAX_SEARCH_ENTRIES:
            # The walk stops here. Saying so is better than looking like
            # the search had found everything there is.
            truncated = True
            break
        try:
            if query_lower in entry.name.lower():
                # The name of a file this person may not open is not
                # theirs to learn.
                if not can_access(entry, "read"):
                    continue
                results.append(_path_to_filenode(entry))
                if len(results) >= limit:
                    break
        except (PermissionError, OSError):
            continue

    return SearchResult(
        items=results,
        total=len(results),
        query=q,
        truncated=truncated,
    )


@search.get("/suggest")
def suggest(q: str, limit: int = Query(10, ge=1, le=50)) -> list[str]:
    storage_path = state.get_settings().storage_path
    if not q or len(q) < 1:
        return []

    search_root = Path(storage_path)
    suggestions = set()
    query_lower = q.lower()

    examined = 0
    for entry in search_root.rglob("*"):
        examined += 1
        if examined > MAX_SEARCH_ENTRIES:
            break
        try:
            if query_lower in entry.name.lower():
                if not can_access(entry, "read"):
                    continue
                suggestions.add(entry.name)
                if len(suggestions) >= limit:
                    break
        except (PermissionError, OSError):
            continue

    return list(suggestions)[:limit]
