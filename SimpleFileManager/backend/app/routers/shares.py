"""Managing shares: creating them, listing them, stopping them.

These routes require a login, because making a file reachable by
strangers is not something an unauthenticated caller should be able
to do. Reading the shared file is a different matter, and lives in
share_public.
"""

from __future__ import annotations

import time
from pathlib import Path
from typing import Optional

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from ..deps import get_storage_root, resolve_under_root, PathOutsideRoot, state
from ..shares import DEFAULT_EXPIRY_DAYS, MAX_EXPIRY_DAYS

shares = APIRouter()


class CreateShareRequest(BaseModel):
    path: str
    # Days until the link stops working. Zero means it does not expire
    # on its own, though it can still be revoked.
    expiry_days: int = DEFAULT_EXPIRY_DAYS
    # Optional password, so a link that leaks is not enough on its own.
    # It has to be worth something: a link that is out on the internet
    # with a one-letter password is not protected by the password, it
    # is only slowed down.
    password: str = ""
    # Optional cap on how many times it can be fetched. Zero means no cap.
    max_downloads: int = 0


class UpdateShareRequest(BaseModel):
    expiry_days: Optional[int] = None


# The shortest password a share may have. The password is the only
# thing between a link that is out on the internet and the file behind
# it, and a one-letter password is not a password.
MIN_SHARE_PASSWORD = 4


def _store():
    return state.get_share_store()


@shares.post("")
def create_share(req: CreateShareRequest) -> dict:
    """Make a file or directory reachable by link."""
    try:
        target = resolve_under_root(req.path)
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    if not target.exists():
        raise HTTPException(status_code=404, detail="File not found")

    # A share is a window onto one thing. It never reaches above the
    # storage root, and never onto the program's own directory.
    storage = Path(get_storage_root()).resolve()
    if storage == target or target == storage / ".simplefilemanager":
        raise HTTPException(status_code=400, detail="不能分享这个位置")

    if target.is_dir():
        size = 0
        try:
            for p in target.rglob("*"):
                if p.is_file():
                    size += p.stat().st_size
        except OSError:
            pass
    else:
        size = target.stat().st_size

    password = (req.password or "").strip()
    if password and len(password) < MIN_SHARE_PASSWORD:
        raise HTTPException(
            status_code=400,
            detail=f"分享密码至少要 {MIN_SHARE_PASSWORD} 个字符")

    days = int(req.expiry_days or 0)
    if days < 0:
        # A negative number used to be quietly turned into zero, and
        # zero means "never expires". A link that was supposed to have
        # expired yesterday would have lived for ever.
        raise HTTPException(status_code=400, detail="有效期不能是负数")
    days = min(days, MAX_EXPIRY_DAYS)
    share = _store().create(
        path=str(target),
        is_dir=target.is_dir(),
        size=size,
        name=target.name,
        expiry_days=days,
        password=password,
        max_downloads=int(req.max_downloads or 0),
    )
    return {k: v for k, v in share.items() if k != "path"} | {"path": str(target)}


@shares.get("")
def list_shares() -> dict:
    """Every share, newest first."""
    entries = _store().list()
    return {"shares": entries, "count": len(entries)}


@shares.patch("/{share_id}")
def update_share(share_id: str, req: UpdateShareRequest) -> dict:
    """Change when a share stops working."""
    if req.expiry_days is not None:
        if not _store().set_expiry(share_id, req.expiry_days):
            raise HTTPException(status_code=404, detail="分享不存在")
    share = _store().get(share_id)
    if share is None:
        raise HTTPException(status_code=404, detail="分享不存在")
    return share


@shares.delete("/{share_id}")
def revoke_share(share_id: str) -> dict:
    """Stop a share working, now."""
    if not _store().revoke(share_id):
        raise HTTPException(status_code=404, detail="分享不存在")
    return {"success": True, "id": share_id}
