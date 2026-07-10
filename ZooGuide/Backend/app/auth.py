"""Auth helpers: password hashing + token verification."""

from __future__ import annotations

import bcrypt
from fastapi import Header, HTTPException
from typing import Optional

from . import db


def hash_password(password: str) -> str:
    return bcrypt.hashpw(password.encode("utf-8"), bcrypt.gensalt(rounds=10)).decode("utf-8")


def verify_password(password: str, hashed: str) -> bool:
    try:
        return bcrypt.checkpw(password.encode("utf-8"), hashed.encode("utf-8"))
    except Exception:
        return False


async def get_current_user_optional(
    authorization: Optional[str] = Header(default=None),
) -> Optional[dict]:
    """Extract user from Bearer token. Returns None if not authenticated."""
    if not authorization:
        return None
    if not authorization.lower().startswith("bearer "):
        return None
    token = authorization[7:].strip()
    if not token:
        return None
    user = db.find_user_by_token(token)
    return user


async def get_current_user(
    authorization: Optional[str] = Header(default=None),
) -> dict:
    """Require authentication. Raises 401 if missing/invalid."""
    user = await get_current_user_optional(authorization)
    if not user:
        raise HTTPException(status_code=401, detail="需要登录")
    return user