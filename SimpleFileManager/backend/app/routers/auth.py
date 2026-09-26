"""Authentication: password-based token auth.

First launch: no password set, auth is optional (requires_auth=False).
Once user sets a password via Settings, all API calls must include
Authorization: Bearer <token> header. Token is a hash of the password.
"""

import hashlib
import secrets
import time
from pathlib import Path

from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.security import HTTPBearer, HTTPAuthorizationCredentials
from pydantic import BaseModel

from ..deps import DATA_DIR, state, _load_json, _save_json
from ..models import LoginRequest, LoginResponse, ChangePasswordRequest, AuthStatus

auth = APIRouter()

AUTH_FILE = DATA_DIR / "auth.json"
bearer_scheme = HTTPBearer(auto_error=False)

# Token expires after 7 days
TOKEN_TTL = 7 * 24 * 3600


def _load_auth() -> dict:
    return _load_json(AUTH_FILE, {"password_hash": None, "salt": None, "tokens": {}})


def _save_auth(data: dict):
    _save_json(AUTH_FILE, data)


def _hash_password(password: str, salt: str) -> str:
    return hashlib.pbkdf2_hmac("sha256", password.encode(), salt.encode(), 100000).hex()


def _generate_token() -> str:
    return secrets.token_hex(32)


def is_auth_enabled() -> bool:
    """Auth is enabled when a password has been set."""
    data = _load_auth()
    return data.get("password_hash") is not None


def set_password(password: str):
    salt = secrets.token_hex(16)
    data = {
        "password_hash": _hash_password(password, salt),
        "salt": salt,
        "tokens": {},
    }
    _save_auth(data)


def verify_password(password: str) -> bool:
    data = _load_auth()
    if not data.get("password_hash"):
        return False
    return _hash_password(password, data["salt"]) == data["password_hash"]


def issue_token() -> str:
    token = _generate_token()
    data = _load_auth()
    data.setdefault("tokens", {})
    data["tokens"][token] = time.time() + TOKEN_TTL
    _save_auth(data)
    return token


def revoke_token(token: str):
    data = _load_auth()
    data.get("tokens", {}).pop(token, None)
    _save_auth(data)


def is_valid_token(token: str) -> bool:
    data = _load_auth()
    tokens = data.get("tokens", {})
    expiry = tokens.get(token)
    if not expiry:
        return False
    if time.time() > expiry:
        tokens.pop(token, None)
        _save_auth(data)
        return False
    return True


def get_current_auth(request: Request, credentials: HTTPAuthorizationCredentials = Depends(bearer_scheme)):
    """Dependency: returns token string if auth is disabled or token is valid.
    Raises 401 if auth is enabled but token is missing/invalid.
    """
    if not is_auth_enabled():
        return None
    if not credentials:
        raise HTTPException(status_code=401, detail="Not authenticated")
    token = credentials.credentials
    if not is_valid_token(token):
        raise HTTPException(status_code=401, detail="Invalid or expired token")
    return token


@auth.get("/status", response_model=AuthStatus)
def auth_status():
    data = _load_auth()
    has_password = data.get("password_hash") is not None
    return AuthStatus(requires_auth=has_password, has_password=has_password)


@auth.post("/login", response_model=LoginResponse)
def login(req: LoginRequest):
    if not is_auth_enabled():
        return LoginResponse(token=_generate_token(), requires_auth=False)
    if not verify_password(req.password):
        raise HTTPException(status_code=401, detail="密码错误")
    token = issue_token()
    return LoginResponse(token=token, requires_auth=True)


@auth.get("/verify")
def verify(credentials: HTTPAuthorizationCredentials = Depends(bearer_scheme)):
    if not is_auth_enabled():
        return {"valid": True, "requires_auth": False}
    if not credentials or not is_valid_token(credentials.credentials):
        raise HTTPException(status_code=401, detail="Not authenticated")
    return {"valid": True, "requires_auth": True}


@auth.post("/logout")
def logout(credentials: HTTPAuthorizationCredentials = Depends(bearer_scheme)):
    if credentials:
        revoke_token(credentials.credentials)
    return {"success": True}


@auth.post("/password")
def change_password(req: ChangePasswordRequest, credentials: HTTPAuthorizationCredentials = Depends(bearer_scheme)):
    if not is_auth_enabled():
        if req.old_password:
            if not verify_password(req.old_password):
                raise HTTPException(status_code=401, detail="旧密码错误")
        set_password(req.new_password)
        token = issue_token()
        return {"success": True, "token": token, "message": "密码已设置"}
    if not credentials or not is_valid_token(credentials.credentials):
        raise HTTPException(status_code=401, detail="Not authenticated")
    if not verify_password(req.old_password):
        raise HTTPException(status_code=401, detail="旧密码错误")
    set_password(req.new_password)
    token = issue_token()
    return {"success": True, "token": token, "message": "密码已更新"}
