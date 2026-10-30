"""Somebody's starred files.

A star in the corner of a file, and a place to come back to. The list
is one person's: "the ones I want to find again" is a private sort of
thing even on a machine the whole family uses.

A starred file may since have been moved, deleted, or put somewhere
only its owner cannot see. So the list is checked against the files as
they are now -- somebody is not shown the name of a file they may not
open, even if they starred it before it was made private.
"""

from __future__ import annotations

import logging
from pathlib import Path
from typing import Optional

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from ..deps import (
    PathOutsideRoot, can_access, check_access, current_user, get_storage_root,
    resolve_under_root, state,
)

log = logging.getLogger(__name__)

favorites = APIRouter(prefix="/api/favorites", tags=["favorites"])


class FavoriteRequest(BaseModel):
    path: str
    name: str = ""
    is_dir: bool = False


def _user_id() -> str:
    """Who is asking. Without accounts there is nobody to keep a list for."""
    user = current_user()
    if user is None:
        return ""
    return str(user.get("id") or user.get("username") or "")


@favorites.get("")
def list_favorites() -> dict:
    """The files this person starred."""
    user_id = _user_id()
    store = state.get_favorite_store()
    entries = store.list_for(user_id)

    root = get_storage_root()
    visible = []
    for entry in entries:
        target = root / entry["path"]
        # The rest of the file API speaks in full paths, and so does
        # this: a path that has to be translated in the browser is a
        # path that can be got wrong.
        # A starred file that has since been moved or deleted is left
        # out: there is nothing to open. One that has been made private
        # is left out too, for the same reason.
        if not target.exists():
            continue
        if user_id and not can_access(target, "read"):
            continue
        stat = target.stat()
        visible.append({
            **entry,
            "path": str(target),
            "size": stat.st_size if target.is_file() else 0,
            "modified": int(stat.st_mtime),
            "exists": True,
        })

    return {"favorites": visible, "count": len(visible), "total": len(entries)}


@favorites.post("")
def add_favorite(req: FavoriteRequest) -> dict:
    """Star a file."""
    user_id = _user_id()
    try:
        target = resolve_under_root(req.path)
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    if not target.exists():
        raise HTTPException(status_code=404, detail="File not found")

    try:
        return state.get_favorite_store().add(
            str(target.relative_to(get_storage_root())),
            user_id,
            name=req.name or target.name,
            is_dir=target.is_dir(),
        )
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))


@favorites.delete("")
def remove_favorite(path: str = "") -> dict:
    """Unstar a file."""
    user_id = _user_id()
    try:
        target = resolve_under_root(path)
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    relative = str(target.relative_to(get_storage_root()))
    if not state.get_favorite_store().remove(relative, user_id):
        raise HTTPException(status_code=404, detail="没有收藏这个文件")
    return {"success": True}


@favorites.get("/check")
def is_favorite(path: str = "") -> dict:
    """Whether one file is starred."""
    user_id = _user_id()
    try:
        target = resolve_under_root(path)
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    relative = str(target.relative_to(get_storage_root()))
    return {"favorite": state.get_favorite_store().is_favorite(relative, user_id)}
