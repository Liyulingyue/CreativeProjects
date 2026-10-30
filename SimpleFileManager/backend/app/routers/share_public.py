"""Reading a shared file.

These routes are deliberately open -- the whole point of a share is
that someone without a login can open it. Everything that could make
that unsafe is handled by the rules on the share itself: one path, read
only, an expiry, and optionally a password.

The link id is long and random, so it cannot be guessed. The path it
points at is never returned, only the name of the file, so the link
does not say where the file lives on the NAS.
"""

from __future__ import annotations

import time
from pathlib import Path
from typing import Optional

from fastapi import APIRouter, HTTPException, Request, Response
from fastapi.responses import FileResponse
from pydantic import BaseModel

from ..shares import check_password
from ..routers.fs import _get_mime_type

share_public = APIRouter()

# How long a visitor stays unlocked after typing the password.
UNLOCK_TTL = 3600
UNLOCK_COOKIE = "sfm_share"


def _store():
    from ..deps import state

    return state.get_share_store()


def _public_view(share: dict) -> dict:
    """What a visitor is allowed to know about a share."""
    return {
        "name": share["name"],
        "is_dir": share["is_dir"],
        "size": share["size"],
        "has_password": share["has_password"],
        "expires_at": share["expires_at"],
        "downloads": share["downloads"],
        "max_downloads": share["max_downloads"],
    }


def _resolve(share_id: str) -> tuple[dict, Optional[Path]]:
    """The share and its path, if the share is still working."""
    share = _store().get(share_id)
    if share is None:
        raise HTTPException(status_code=404, detail="链接不存在")
    if share["revoked"]:
        raise HTTPException(status_code=410, detail="这个链接已被停止")
    if share["expired"]:
        raise HTTPException(status_code=410, detail="这个链接已经过期")
    if share["used_up"]:
        raise HTTPException(status_code=410, detail="这个链接的下载次数已用完")
    target = Path(share["path"])
    if not target.exists():
        raise HTTPException(status_code=404, detail="文件已被删除")
    return share, target


def _is_unlocked(request: Request, share: dict) -> bool:
    """Whether the visitor has already typed the password."""
    if not share["has_password"]:
        return True
    raw = request.cookies.get(UNLOCK_COOKIE, "")
    if not raw:
        return False
    try:
        share_id, expires, sig = raw.split(".", 2)
    except ValueError:
        return False
    if share_id != share["id"] or int(expires) < int(time.time()):
        return False
    from ..routers.auth import _stream_secret

    import hashlib
    import hmac

    expected = hmac.new(
        _stream_secret(), f"{share_id}:{expires}".encode(), hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(sig, expected)


@share_public.get("/{share_id}")
def share_info(share_id: str, request: Request):
    """What a visitor sees when they open the link."""
    share, _target = _resolve(share_id)
    view = _public_view(share)
    view["locked"] = not _is_unlocked(request, share)
    return view


class UnlockRequest(BaseModel):
    password: str = ""


@share_public.post("/{share_id}/unlock")
def unlock(share_id: str, req: UnlockRequest, response: Response):
    """Check the password and let the visitor through."""
    share, _target = _resolve(share_id)
    if not share["has_password"]:
        return {"ok": True}
    # The hash is never handed out by the public view, so it is read
    # here, only for the moment it takes to check it.
    raw_hash = _store().get_password_hash(share_id)
    if raw_hash is None:
        raise HTTPException(status_code=500, detail="分享信息不完整")
    if not check_password(req.password, raw_hash):
        raise HTTPException(status_code=401, detail="密码不正确")

    expires = int(time.time()) + UNLOCK_TTL
    import hashlib
    import hmac

    from ..routers.auth import _stream_secret

    sig = hmac.new(
        _stream_secret(), f"{share_id}:{expires}".encode(), hashlib.sha256
    ).hexdigest()
    response.set_cookie(
        UNLOCK_COOKIE,
        f"{share_id}.{expires}.{sig}",
        max_age=UNLOCK_TTL,
        httponly=True,
        samesite="lax",
    )
    return {"ok": True}


def _inside(share_root: Path, relative: str) -> Path:
    """A path inside the shared folder, or nothing.

    The visitor names a file by where it is under the folder they were
    given. That name comes from outside, so it is checked against the
    folder before it is used: no climbing out, no absolute paths, no
    following a symlink out of the tree.
    """
    relative = (relative or "").strip().strip("/")
    if not relative:
        return share_root
    parts = []
    for part in relative.replace("\\", "/").split("/"):
        if part in ("", "."):
            continue
        if part == "..":
            raise HTTPException(status_code=400, detail="无效的路径")
        parts.append(part)
    candidate = share_root.joinpath(*parts)
    try:
        resolved = candidate.resolve()
        if not resolved.is_relative_to(share_root.resolve()):
            raise HTTPException(status_code=400, detail="无效的路径")
    except OSError:
        raise HTTPException(status_code=404, detail="文件不存在")
    return resolved


@share_public.get("/{share_id}/list")
def share_list(share_id: str, request: Request):
    """The contents of a shared directory, or of a folder inside it."""
    share, target = _resolve(share_id)
    if not share["is_dir"]:
        raise HTTPException(status_code=400, detail="这不是一个文件夹")
    if not _is_unlocked(request, share):
        raise HTTPException(status_code=401, detail="需要密码")

    relative = request.query_params.get("path", "")
    here = _inside(target, relative)
    if not here.exists():
        raise HTTPException(status_code=404, detail="文件不存在")
    if not here.is_dir():
        raise HTTPException(status_code=400, detail="这不是一个文件夹")

    items = []
    try:
        for entry in sorted(here.iterdir(), key=lambda p: (not p.is_dir(), p.name.lower())):
            if entry.name.startswith("."):
                continue
            try:
                st = entry.stat()
                items.append({
                    "name": entry.name,
                    "is_dir": entry.is_dir(),
                    "size": st.st_size if entry.is_file() else 0,
                    "modified": time.strftime("%Y-%m-%d %H:%M", time.localtime(st.st_mtime)),
                })
            except OSError:
                continue
    except OSError as e:
        raise HTTPException(status_code=500, detail=str(e))
    _store().note_access(share_id)
    return {
        "name": share["name"],
        "path": relative,
        "items": items,
        "count": len(items),
    }


@share_public.get("/{share_id}/download")
def share_download(share_id: str, request: Request):
    """Fetch the shared file, or a file inside a shared directory."""
    share, target = _resolve(share_id)
    if not _is_unlocked(request, share):
        raise HTTPException(status_code=401, detail="需要密码")

    # A file inside a shared directory may be asked for by name, and
    # by where it is under the folder -- a subfolder is somewhere a
    # visitor can go.
    name = request.query_params.get("name")
    relative = request.query_params.get("path", "")
    if name and not relative:
        if "/" in name or "\\" in name or name.startswith("."):
            raise HTTPException(status_code=400, detail="无效的文件名")
        relative = name
    if relative:
        candidate = _inside(target, relative)
        if not candidate.is_file():
            raise HTTPException(status_code=404, detail="文件不存在")
        target = candidate

    if target.is_dir():
        raise HTTPException(status_code=400, detail="请指定要下载的文件")

    _store().note_access(share_id)
    size = target.stat().st_size
    headers = {"Accept-Ranges": "bytes"}
    return FileResponse(
        str(target),
        filename=target.name,
        media_type=_get_mime_type(target),
        headers=headers,
    )
