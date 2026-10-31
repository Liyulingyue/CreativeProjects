"""Authentication: password-based token auth.

First launch: no password set, auth is optional (requires_auth=False).
Once user sets a password via Settings, all API calls must include
Authorization: Bearer <token> header. Token is a hash of the password.
"""

import hashlib
import hmac
import logging
import secrets
import time
from pathlib import Path
from typing import Optional

from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.security import HTTPBearer, HTTPAuthorizationCredentials
from pydantic import BaseModel

from ..deps import DATA_DIR, state, _load_json, _save_json
from ..models import LoginRequest, LoginResponse, ChangePasswordRequest, AuthStatus

# The account the single password became. A deployment that has one
# password keeps one administrator, so nothing about it changes.
ADMIN_USERNAME = "admin"

auth = APIRouter()

log = logging.getLogger(__name__)

AUTH_FILE = DATA_DIR / "auth.json"
bearer_scheme = HTTPBearer(auto_error=False)

# Token expires after 7 days
TOKEN_TTL = 7 * 24 * 3600

# The password settings are read on every request, so they are kept in
# memory and re-read only when the file has actually changed.
_auth_cache: dict = {"mtime": None, "data": None}


def _load_auth() -> dict:
    """The password settings. Read from disk only when they have changed."""
    try:
        mtime = AUTH_FILE.stat().st_mtime
    except OSError:
        mtime = None
    if _auth_cache["data"] is not None and _auth_cache["mtime"] == mtime:
        return _auth_cache["data"]
    data = _load_json(AUTH_FILE, {"password_hash": None, "salt": None, "tokens": {}})
    _auth_cache["mtime"] = mtime
    _auth_cache["data"] = data
    return data


def _save_auth(data: dict):
    _save_json(AUTH_FILE, data)
    try:
        mtime = AUTH_FILE.stat().st_mtime
    except OSError:
        mtime = None
    _auth_cache["mtime"] = mtime
    _auth_cache["data"] = data


def _hash_password(password: str, salt: str) -> str:
    return hashlib.pbkdf2_hmac("sha256", password.encode(), salt.encode(), 100000).hex()


def _generate_token() -> str:
    return secrets.token_hex(32)


def is_auth_enabled() -> bool:
    """Auth is enabled when a password has been set."""
    data = _load_auth()
    return data.get("password_hash") is not None


def set_password(password: str):
    """Set the service's password.

    There used to be one password in a file, and later an administrator
    account in the database, and the two drifted apart -- changing the
    password updated the file while the login checked the account, so
    the change was reported as done and then did nothing. Both are
    written now, and they are the same password.
    """
    salt = secrets.token_hex(16)
    data = {
        "password_hash": _hash_password(password, salt),
        "salt": salt,
        "tokens": {},
    }
    _save_auth(data)

    # The administrator account is what the login actually checks.
    from ..users import ROLE_ADMIN

    try:
        store = state.get_user_store()
        admin = store.get_by_username(ADMIN_USERNAME)
        if admin is not None:
            store.set_password(admin["id"], password)
        else:
            store.create_user(ADMIN_USERNAME, password, role=ROLE_ADMIN)
    except Exception:
        log.exception("could not update the administrator account")

    # Everyone who was signed in is signed out: the password they were
    # holding a session for is not the password any more.
    state.get_user_store().revoke_all_tokens()


def verify_password(password: str) -> bool:
    data = _load_auth()
    if not data.get("password_hash"):
        return False
    return _hash_password(password, data["salt"]) == data["password_hash"]


def issue_token(username: str = "") -> str:
    """Issue a session token.

    The token carries the name of the user it belongs to, so a request
    can be served as that user rather than as whoever happens to be
    logged in.
    """
    token = _generate_token()
    store = state.get_user_store()
    store.prune_tokens()
    store.store_token(token, username or ADMIN_USERNAME, time.time() + TOKEN_TTL)
    return token


def revoke_token(token: str):
    state.get_user_store().revoke_token(token)


_tokens_migrated = False


def migrate_tokens() -> int:
    """Move the tokens from the old JSON file into the table.

    Before there were accounts, the sessions lived in auth.json --
    a file that was read whole on every request and written whole on
    every login. It held hundreds of them, so the cost was real, and
    two people logging in at the same moment could lose one of the
    two sessions. The table is faster and cannot lose an update.
    """
    global _tokens_migrated
    if _tokens_migrated:
        return 0
    _tokens_migrated = True

    data = _load_auth()
    tokens = data.get("tokens") or {}
    if not tokens:
        return 0

    store = state.get_user_store()
    now = time.time()
    moved = 0
    for token, entry in tokens.items():
        if isinstance(entry, (int, float)):
            expires, username = float(entry), ADMIN_USERNAME
        else:
            expires = float(entry.get("expires", 0) or 0)
            username = entry.get("username") or ADMIN_USERNAME
        if expires > now:
            store.store_token(token, username, expires)
            moved += 1
    data["tokens"] = {}
    _save_auth(data)
    if moved:
        log.info("moved %d session tokens into the database", moved)
    return moved


def is_valid_token(token: str) -> bool:
    return token_user(token) is not None


def token_user(token: str) -> Optional[str]:
    """The user a token belongs to, or None if it is no good.

    Tokens issued before there were users carry only an expiry; they
    belong to the administrator, which is who the single password was.
    """
    store = state.get_user_store()

    username = store.token_username(token)
    if username is not None:
        return username

    # Not in the table. It may still be one of the tokens the old JSON
    # file held, from before tokens were kept in the database.
    data = _load_auth()
    tokens = data.get("tokens", {})
    entry = tokens.get(token)
    if entry is None:
        return None
    if isinstance(entry, (int, float)):
        # The old format: an expiry and nothing else.
        if time.time() > entry:
            tokens.pop(token, None)
            _save_auth(data)
            return None
        return ADMIN_USERNAME
    expires = entry.get("expires", 0)
    if time.time() > expires:
        tokens.pop(token, None)
        _save_auth(data)
        return None
    return entry.get("username") or ADMIN_USERNAME


def _stream_secret() -> bytes:
    """The server's own secret, used to sign short-lived stream tokens.

    Generated once at first use and persisted in auth.json — separate
    from the password salt, so a leaked salt does not let an attacker
    forge stream tokens."""
    import secrets as _secrets
    data = _load_auth()
    secret = data.get("stream_secret")
    if not secret:
        secret = _secrets.token_hex(32)
        data["stream_secret"] = secret
        _save_auth(data)
    return secret.encode()


def issue_stream_token(path: str, ttl: int = 300) -> dict:
    """A short-lived token that lets a media element fetch one file.

    A <video> or <audio> tag cannot send an Authorization header, so
    the file has to be reachable by URL. Putting the session token in
    the URL would put it in the access log and in the browser's
    history, so this issues a token instead: derived from the server's
    secret, valid for one path, and good for a few minutes.
    """
    expires = int(time.time()) + ttl
    signature = hmac.new(
        _stream_secret(), f"{path}:{expires}".encode(), hashlib.sha256
    ).hexdigest()
    return {"token": f"{expires}.{signature}", "expires_at": expires}


def verify_stream_token(path: str, token: str) -> bool:
    """Check a stream token against the path it was issued for."""
    try:
        expires_s, signature = token.split(".", 1)
        expires = int(expires_s)
    except (ValueError, AttributeError):
        return False
    if expires < int(time.time()):
        return False
    expected = hmac.new(
        _stream_secret(), f"{path}:{expires}".encode(), hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(signature, expected)


async def get_current_auth(request: Request, credentials: HTTPAuthorizationCredentials = Depends(bearer_scheme)):
    """Dependency: the user being served, if the token is any good.

    Raises 401 if auth is enabled but the token is missing or invalid.
    """
    from ..deps import set_current_user

    if not is_auth_enabled():
        set_current_user(None)
        return None
    if not credentials:
        raise HTTPException(status_code=401, detail="Not authenticated")
    token = credentials.credentials
    username = token_user(token)
    if username is None:
        raise HTTPException(status_code=401, detail="Invalid or expired token")

    user = _lookup_user(username)
    if user is None:
        raise HTTPException(status_code=401, detail="Invalid or expired token")
    if user.get("disabled"):
        raise HTTPException(status_code=401, detail="这个账号已被停用")
    set_current_user(user)
    return username


async def require_admin(request: Request, credentials: HTTPAuthorizationCredentials = Depends(bearer_scheme)):
    """Dependency: the administrator, or nobody.

    Where the files live, which models the service talks to and the
    keys it uses to talk to them are the administrator's business. A
    person with their own folder does not get to change them.
    """
    from ..deps import current_user

    await get_current_auth(request, credentials)
    user = current_user()
    if user is None:
        # No accounts at all: there is nobody to ask, and an
        # installation with a single password keeps working as it did.
        return None
    if user.get("role") != "admin":
        raise HTTPException(status_code=403, detail="只有管理员可以做这个操作")
    return user


def _single_password_matches(password: str) -> bool:
    """Whether the password is the service's single password."""
    try:
        return verify_password(password)
    except Exception:
        return False


def _ensure_admin(password: str) -> Optional[dict]:
    """Turn the single password into an administrator account.

    A deployment that has had one password since before there were
    users gets an administrator called "admin" with that same password,
    the first time anybody logs in. Nothing about it changes.
    """
    from ..users import ROLE_ADMIN

    try:
        store = state.get_user_store()
        existing = store.get_by_username(ADMIN_USERNAME)
        if existing is not None:
            return existing
        return store.create_user(ADMIN_USERNAME, password, role=ROLE_ADMIN)
    except Exception:
        return None


def _lookup_user(username: str) -> Optional[dict]:
    from ..deps import state

    try:
        return state.get_user_store().get_by_username(username)
    except Exception:
        return None


@auth.get("/status", response_model=AuthStatus)
def auth_status():
    data = _load_auth()
    has_password = data.get("password_hash") is not None
    return AuthStatus(requires_auth=has_password, has_password=has_password)


@auth.post("/login", response_model=LoginResponse)
def login(req: LoginRequest):
    """Log in.

    The username is optional for the administrator, so an installation
    that has only ever had one password still works the way it did.
    """
    username = (getattr(req, "username", "") or "").strip() or ADMIN_USERNAME
    user = None
    try:
        store = state.get_user_store()
        user = store.verify(username, req.password)
        if user is None:
            # The account may not exist yet -- the single password
            # becomes the administrator on first use.
            if not store.list_users() and _single_password_matches(req.password):
                user = _ensure_admin(req.password)
    except Exception:
        user = None
    if user is None:
        raise HTTPException(status_code=401, detail="用户名或密码不正确")
    token = issue_token(user["username"])
    return LoginResponse(
        token=token,
        requires_auth=True,
        username=user["username"],
        role=user["role"],
    )


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
        # Password initialization is a deployment bootstrap operation, not a
        # first-visitor-wins public API. Set INIT_PASSWORD before startup.
        raise HTTPException(status_code=503, detail="Authentication is not initialized; configure INIT_PASSWORD")
    if not credentials or not is_valid_token(credentials.credentials):
        raise HTTPException(status_code=401, detail="Not authenticated")
    if not verify_password(req.old_password):
        raise HTTPException(status_code=401, detail="旧密码错误")
    set_password(req.new_password)
    token = issue_token()
    return {"success": True, "token": token, "message": "密码已更新"}
