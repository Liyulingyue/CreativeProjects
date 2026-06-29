"""
用户认证：注册、登录、token 生成与校验。

设计选择：
- 使用 bcrypt 哈希密码（业界标准）
- Token 是随机生成的 UUID4 字符串（简化版，未来可换 JWT）
- Token 存 DB（便于撤销）
- 默认有效期 30 天

可扩展点（未来）：
- 邮箱验证流程
- 密码重置
- OAuth 第三方登录
- 多设备 session 管理
- RBAC 角色权限细分
"""
import secrets
import sqlite3
import bcrypt
from datetime import datetime, timedelta
from pathlib import Path
from typing import Optional


DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DB_PATH = DATA_DIR / "travelsites.db"


def _get_session_days() -> int:
    """从 config 读 SESSION_DAYS，方便统一管理。"""
    try:
        from app.config import SESSION_DAYS
        return SESSION_DAYS
    except Exception:
        return 30


def _get_admin_defaults() -> dict:
    """从环境变量读默认 admin 配置。"""
    try:
        from app.config import (
            ADMIN_USERNAME, ADMIN_PASSWORD,
            ADMIN_EMAIL, ADMIN_DISPLAY_NAME,
        )
        return {
            "username": ADMIN_USERNAME,
            "password": ADMIN_PASSWORD,
            "email": ADMIN_EMAIL,
            "display_name": ADMIN_DISPLAY_NAME,
        }
    except Exception:
        return {
            "username": "admin",
            "password": "admin123",
            "email": "admin@travelsites.local",
            "display_name": "系统管理员",
        }


# ---------- 密码 ----------

def hash_password(password: str) -> str:
    """bcrypt 哈希密码。"""
    return bcrypt.hashpw(password.encode("utf-8"), bcrypt.gensalt()).decode("utf-8")


def verify_password(password: str, password_hash: str) -> bool:
    """验证密码。"""
    try:
        return bcrypt.checkpw(password.encode("utf-8"), password_hash.encode("utf-8"))
    except Exception:
        return False


# ---------- Token ----------

def generate_token() -> str:
    """生成安全 token（URL-safe）。"""
    return secrets.token_urlsafe(48)


# ---------- 用户 CRUD ----------

def create_user(
    username: str,
    password: str,
    email: Optional[str] = None,
    display_name: Optional[str] = None,
    role: str = "user",
) -> dict:
    """创建用户。返回 dict，成功带 user_id，失败带 error。"""
    username = username.strip()
    if not username or len(username) < 3:
        return {"error": "用户名至少 3 个字符"}
    if len(password) < 6:
        return {"error": "密码至少 6 个字符"}

    conn = sqlite3.connect(str(DB_PATH))
    cur = conn.cursor()
    try:
        cur.execute(
            """INSERT INTO users (username, email, password_hash, role, display_name, created_at)
               VALUES (?, ?, ?, ?, ?, ?)""",
            (username, email, hash_password(password), role, display_name, datetime.now().isoformat()),
        )
        conn.commit()
        return {"user_id": cur.lastrowid, "username": username, "role": role}
    except sqlite3.IntegrityError as e:
        if "username" in str(e):
            return {"error": "用户名已被占用"}
        if "email" in str(e):
            return {"error": "邮箱已被注册"}
        return {"error": str(e)}
    finally:
        conn.close()


def authenticate(username: str, password: str) -> Optional[dict]:
    """校验用户名密码。成功返回 user dict，失败返回 None。"""
    conn = sqlite3.connect(str(DB_PATH))
    conn.row_factory = sqlite3.Row
    row = conn.execute(
        "SELECT id, username, password_hash, role, display_name FROM users WHERE username=?",
        (username,),
    ).fetchone()
    conn.close()

    if not row:
        return None
    if not verify_password(password, row["password_hash"]):
        return None

    return {
        "user_id": row["id"],
        "username": row["username"],
        "role": row["role"],
        "display_name": row["display_name"],
    }


def create_session(user_id: int) -> dict:
    """为用户创建 session，返回 token 和过期时间。"""
    token = generate_token()
    now = datetime.now()
    expires = now + timedelta(days=_get_session_days())

    conn = sqlite3.connect(str(DB_PATH))
    conn.execute(
        "INSERT INTO user_sessions (token, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)",
        (token, user_id, now.isoformat(), expires.isoformat()),
    )
    conn.execute(
        "UPDATE users SET last_login_at=? WHERE id=?",
        (now.isoformat(), user_id),
    )
    conn.commit()
    conn.close()

    return {
        "token": token,
        "expires_at": expires.isoformat(),
    }


def verify_session(token: str) -> Optional[dict]:
    """校验 token，返回 user dict 或 None。"""
    conn = sqlite3.connect(str(DB_PATH))
    conn.row_factory = sqlite3.Row
    row = conn.execute(
        """SELECT u.id, u.username, u.role, u.display_name, s.expires_at
           FROM user_sessions s JOIN users u ON s.user_id = u.id
           WHERE s.token=?""",
        (token,),
    ).fetchone()
    conn.close()

    if not row:
        return None

    expires = datetime.fromisoformat(row["expires_at"])
    if expires < datetime.now():
        return None

    return {
        "user_id": row["id"],
        "username": row["username"],
        "role": row["role"],
        "display_name": row["display_name"],
    }


def delete_session(token: str) -> bool:
    """登出：删除 session。"""
    conn = sqlite3.connect(str(DB_PATH))
    cur = conn.cursor()
    cur.execute("DELETE FROM user_sessions WHERE token=?", (token,))
    deleted = cur.rowcount
    conn.commit()
    conn.close()
    return deleted > 0


def get_user_by_id(user_id: int) -> Optional[dict]:
    """通过 ID 查询用户。"""
    conn = sqlite3.connect(str(DB_PATH))
    conn.row_factory = sqlite3.Row
    row = conn.execute(
        "SELECT id, username, email, role, display_name, created_at, last_login_at FROM users WHERE id=?",
        (user_id,),
    ).fetchone()
    conn.close()
    return dict(row) if row else None


def ensure_admin_user() -> None:
    """
    启动时确保至少有一个 admin 账户。

    同步策略（按环境变量 ADMIN_SYNC 决定）：
    - "true"：.env 配置覆盖 DB（每次启动都同步密码/邮箱）
    - "false"（默认）：DB 已有 admin 时不覆盖
    - DB 无 admin：始终用 .env 创建

    设计说明：Admin 账户是部署级配置，应该由环境变量控制。
    修改 .env → 重启 → 生效。这符合 Docker/K8s 部署习惯。
    """
    import os
    sync = os.getenv("ADMIN_SYNC", "false").lower() in ("true", "1", "yes")
    defaults = _get_admin_defaults()
    target_username = defaults["username"]

    conn = sqlite3.connect(str(DB_PATH))
    cur = conn.cursor()

    existing = cur.execute(
        "SELECT id, username, password_hash, email, display_name FROM users WHERE role='admin' ORDER BY id LIMIT 1"
    ).fetchone()

    if existing is None:
        # DB 无 admin → 用 .env 创建
        result = create_user(
            username=defaults["username"],
            password=defaults["password"],
            email=defaults["email"],
            display_name=defaults["display_name"],
            role="admin",
        )
        if "error" not in result:
            print(f"[auth] 已创建 admin: {defaults['username']} / {defaults['password']}")
            print(f"[auth] ⚠️  生产环境请立即修改默认密码！")
        conn.close()
        return

    # DB 有 admin，检查是否需要更新
    db_id, db_username, db_hash, db_email, db_display = existing

    updates = {}
    new_hash = hash_password(defaults["password"])
    if sync:
        # 强制同步模式：覆盖所有字段
        if db_username != target_username:
            updates["username"] = target_username
        if db_hash != new_hash:
            updates["password_hash"] = new_hash
        if defaults["email"] and db_email != defaults["email"]:
            updates["email"] = defaults["email"]
        if defaults["display_name"] and db_display != defaults["display_name"]:
            updates["display_name"] = defaults["display_name"]
    # 默认（sync=false）：不覆盖 DB

    if updates:
        set_clause = ", ".join(f"{k}=?" for k in updates.keys())
        cur.execute(
            f"UPDATE users SET {set_clause} WHERE id=?",
            (*updates.values(), db_id)
        )
        if "password_hash" in updates:
            cur.execute("DELETE FROM user_sessions WHERE user_id=?", (db_id,))
            print(f"[auth] 密码已更新，所有 session 已失效（请重新登录）")
        conn.commit()
        print(f"[auth] admin 已同步: {list(updates.keys())}")
    conn.close()