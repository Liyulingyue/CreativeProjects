"""Managing users and what they may reach.

Everything here needs the administrator. A user who can grant access
to files can grant access to everything, so this is one door that has
to be kept shut.
"""

from __future__ import annotations

from typing import Optional

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel

from ..deps import current_user, state
from ..users import PERMISSIONS, ROLE_ADMIN, ROLE_USER, UserError

users = APIRouter()


def _require_admin():
    user = current_user()
    if user is None:
        # The service is open, or there are no users yet. Either way
        # there is nothing to protect.
        return
    if user.get("role") != ROLE_ADMIN:
        raise HTTPException(status_code=403, detail="只有管理员可以管理用户")


class CreateUserRequest(BaseModel):
    username: str
    password: str
    role: str = ROLE_USER


class SetPasswordRequest(BaseModel):
    password: str


class SetDisabledRequest(BaseModel):
    disabled: bool


class GrantRequest(BaseModel):
    path: str = ""
    user_id: str
    permission: str = "read"


@users.get("")
def list_users() -> dict:
    """Every account, and what each one may reach."""
    _require_admin()
    store = state.get_user_store()
    accounts = []
    for u in store.list_users():
        accounts.append({
            "id": u["id"],
            "username": u["username"],
            "role": u["role"],
            "disabled": u["disabled"],
            "created_at": u["created_at"],
            "grants": [
                {"path": g["path"], "permission": g["permission"]}
                for g in store.list_grants(u["id"])
            ],
        })
    return {"users": accounts, "count": len(accounts)}


@users.post("")
def create_user(req: CreateUserRequest) -> dict:
    """Make an account."""
    _require_admin()
    try:
        user = state.get_user_store().create_user(req.username, req.password, req.role)
    except UserError as e:
        raise HTTPException(status_code=400, detail=str(e))
    return {
        "id": user["id"],
        "username": user["username"],
        "role": user["role"],
        "disabled": user["disabled"],
        "created_at": user["created_at"],
        "grants": [],
    }


@users.post("/{user_id}/password")
def set_password(user_id: str, req: SetPasswordRequest) -> dict:
    """Change somebody's password."""
    _require_admin()
    try:
        ok = state.get_user_store().set_password(user_id, req.password)
    except UserError as e:
        raise HTTPException(status_code=400, detail=str(e))
    if not ok:
        raise HTTPException(status_code=404, detail="用户不存在")
    return {"success": True}


@users.post("/{user_id}/disabled")
def set_disabled(user_id: str, req: SetDisabledRequest) -> dict:
    """Stop somebody logging in, or let them back in."""
    _require_admin()
    store = state.get_user_store()
    user = store.get_user(user_id)
    if user is None:
        raise HTTPException(status_code=404, detail="用户不存在")
    if user["role"] == ROLE_ADMIN and req.disabled:
        # The last administrator cannot lock themselves out.
        if store.count_admins() <= 1:
            raise HTTPException(status_code=400, detail="至少要保留一个可用的管理员")
    store.set_disabled(user_id, req.disabled)
    return {"success": True, "disabled": req.disabled}


@users.delete("/{user_id}")
def delete_user(user_id: str) -> dict:
    """Remove an account and everything it was allowed to reach."""
    _require_admin()
    store = state.get_user_store()
    user = store.get_user(user_id)
    if user is None:
        raise HTTPException(status_code=404, detail="用户不存在")
    if user["role"] == ROLE_ADMIN and store.count_admins() <= 1:
        raise HTTPException(status_code=400, detail="至少要保留一个管理员")
    store.delete_user(user_id)
    return {"success": True}


@users.post("/grants")
def grant(req: GrantRequest) -> dict:
    """Let somebody reach a part of the storage.

    The path is relative to the storage root and empty means the root
    itself. A grant on a directory covers everything under it.
    """
    _require_admin()
    if req.permission not in PERMISSIONS:
        raise HTTPException(status_code=400, detail="未知权限")
    store = state.get_user_store()
    if store.get_user(req.user_id) is None:
        raise HTTPException(status_code=404, detail="用户不存在")
    try:
        return store.grant(req.path, req.user_id, req.permission)
    except UserError as e:
        raise HTTPException(status_code=400, detail=str(e))


@users.delete("/grants")
def revoke(path: str = "", user_id: str = "") -> dict:
    """Take a permission away."""
    _require_admin()
    ok = state.get_user_store().revoke(path, user_id)
    if not ok:
        raise HTTPException(status_code=404, detail="没有这条权限")
    return {"success": True}


@users.get("/me")
def who_am_i(request: Request) -> dict:
    """Who the current request is being served for."""
    user = current_user()
    if user is None:
        return {"username": "", "role": "", "is_admin": True, "authenticated": False}
    return {
        "username": user["username"],
        "role": user["role"],
        "is_admin": user.get("role") == ROLE_ADMIN,
        "authenticated": True,
    }
