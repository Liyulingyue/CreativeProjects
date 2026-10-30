"""Small pictures, and the tokens that let a browser ask for them.

A picture element cannot send an Authorization header, so the little
pictures are fetched by URL like any other image on a page. That needs
a key of its own -- one that opens the little pictures and nothing
else.

The key names a directory. A picture of a file in that directory can
be fetched with it; a file cannot be downloaded with it, and a
directory outside it cannot be looked at. It lasts a few minutes, and
it is given out with the listing of the directory it belongs to, so
the browser has one key rather than one per picture.
"""

from __future__ import annotations

import hashlib
import hmac
import time
from pathlib import Path
from typing import Optional

from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import Response
from fastapi.security import HTTPBearer, HTTPAuthorizationCredentials

from ..deps import get_storage_root, resolve_under_root, PathOutsideRoot, can_access
from ..thumbnails import DEFAULT_SIZE, ThumbnailStore

thumbnails = APIRouter(prefix="/api/thumbnails", tags=["thumbnails"])

# How long a key for a directory's pictures is good for. Long enough
# that a page left open still works, short enough that it is not
# something anybody keeps.
TOKEN_TTL = 600


def _secret() -> bytes:
    from ..routers.auth import _stream_secret

    return _stream_secret()


def issue_directory_token(directory: str, ttl: int = TOKEN_TTL) -> dict:
    """A key for the little pictures of one directory."""
    expires = int(time.time()) + ttl
    signature = hmac.new(
        _secret(), f"thumbs:{directory}:{expires}".encode(), hashlib.sha256
    ).hexdigest()
    return {"token": f"{expires}.{signature}", "expires_at": expires}


def verify_directory_token(path: str, token: str) -> bool:
    """Whether the key opens the little picture of this path.

    The key names a directory, and the picture asked for has to be in
    that directory -- not above it, not below it, and not somewhere
    else entirely.
    """
    try:
        expires_s, signature = token.split(".", 1)
        expires = int(expires_s)
    except (ValueError, AttributeError):
        return False
    if expires < int(time.time()):
        return False
    directory = str(Path(path).parent)
    expected = hmac.new(
        _secret(), f"thumbs:{directory}:{expires}".encode(), hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(signature, expected)


bearer_scheme = HTTPBearer(auto_error=False)


@thumbnails.get("")
async def get_thumbnail(
    request: Request,
    path: str,
    size: int = DEFAULT_SIZE,
    tt: str = "",
    credentials: HTTPAuthorizationCredentials = Depends(bearer_scheme),
):
    """A little picture of a big one.

    Fetched with the session token, or with a key for the directory the
    picture is in -- a picture element cannot send a header, so the
    key is in the URL. Either way the answer is a picture and nothing
    else: this opens no file.
    """
    try:
        target = resolve_under_root(path)
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    if not target.exists() or not target.is_file():
        raise HTTPException(status_code=404, detail="File not found")

    if tt:
        if not verify_directory_token(str(target), tt):
            raise HTTPException(status_code=401, detail="Not authorized")
    else:
        # No key: the session has to say so.
        from ..routers.auth import get_current_auth

        await get_current_auth(request, credentials)
        if not can_access(target, "read"):
            raise HTTPException(status_code=403, detail="没有访问这个位置的权限")

    size = max(32, min(int(size or DEFAULT_SIZE), 1024))
    data = ThumbnailStore(get_storage_root()).get(target, size)
    if not data:
        raise HTTPException(status_code=404, detail="No thumbnail")
    return Response(
        content=data,
        media_type="image/jpeg",
        headers={"Cache-Control": "private, max-age=86400"},
    )
