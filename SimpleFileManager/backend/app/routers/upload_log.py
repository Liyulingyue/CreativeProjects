"""Looking at, and clearing, the record of uploads.

Only the administrator may look. The log names paths across the whole
storage, including the folders somebody has been kept out of, and a
list of other people's uploads is not a thing a restricted user
should be able to read. The same dependency that guards the accounts
guards this.
"""

from __future__ import annotations

from pathlib import Path
from typing import Optional

from fastapi import APIRouter, Depends, Query
from pydantic import BaseModel

from ..deps import get_storage_root, state
from ..upload_log import MAX_ROWS, RETENTION_DAYS
from .auth import require_admin

upload_log = APIRouter()


class PruneRequest(BaseModel):
    retention_days: int = RETENTION_DAYS
    max_rows: int = MAX_ROWS


def _relative(path: str) -> str:
    """Show a path as the user knows it, not as the server stores it."""
    try:
        return str(Path(path).relative_to(get_storage_root()))
    except ValueError:
        return path


def _present(entry: dict) -> dict:
    return {
        "id": entry["id"],
        "at": entry["at"],
        "username": entry["username"],
        "path": _relative(entry["path"]),
        "name": entry["name"],
        "size": entry["size"],
        "rel_dir": entry["rel_dir"],
    }


@upload_log.get("")
def list_uploads(
    limit: int = Query(50, ge=1, le=500),
    offset: int = Query(0, ge=0),
    _admin=Depends(require_admin),
) -> dict:
    """The most recent uploads, newest first."""
    store = state.get_upload_log()
    return {
        "entries": [_present(e) for e in store.recent(limit, offset)],
        "count": store.count(),
        "stats": store.stats(),
    }


@upload_log.delete("")
def clear_uploads(_admin=Depends(require_admin)) -> dict:
    """Forget every upload."""
    removed = state.get_upload_log().clear()
    return {"success": True, "removed": removed}


@upload_log.post("/prune")
def prune_uploads(req: PruneRequest, _admin=Depends(require_admin)) -> dict:
    """Drop entries past the retention window, and any beyond the cap."""
    return state.get_upload_log().prune(req.retention_days, req.max_rows)
