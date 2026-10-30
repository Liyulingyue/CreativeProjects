"""Users, and what each of them may reach.

The service used to have one password, and everyone who had it could
do everything. That is fine for one person on a private network and
not fine for a household: the person who keeps the family's photos
does not need to be able to delete the tax records.

There are two parts to this. A user is a name and a password, and a
grant says that a user may read, or write, one part of the storage.
A grant on a directory covers everything under it, and the most
specific grant for a path is the one that counts -- so a grant on
"photos/2024" beats a grant on "photos".

The administrator is not subject to any of this. One account can always
get to everything, because there has to be a way back in when a grant
is wrong.
"""

from __future__ import annotations

import hashlib
import logging
import secrets
import sqlite3
import threading
import time
from pathlib import Path
from typing import Optional

log = logging.getLogger(__name__)

USERS_DB_NAME = "users.db"

ROLE_ADMIN = "admin"
ROLE_USER = "user"

PERM_READ = "read"
PERM_WRITE = "write"
PERMISSIONS = (PERM_READ, PERM_WRITE)

# A grant on write implies read.
PERMISSION_RANK = {PERM_READ: 1, PERM_WRITE: 2}

# What a person may be kept out of. "none" means they cannot see the
# folder at all; "read" means they can look but not change it.
RESTRICT_NONE = "none"
RESTRICT_READ = "read"
RESTRICTIONS = (RESTRICT_NONE, RESTRICT_READ)

CREATE_USERS = """
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    salt TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user',
    disabled INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
)
"""

# Who may not reach what.
#
# This used to be a list of grants: a path nobody could open until
# somebody was given it. That is the wrong way round for a machine a
# family shares -- everybody has a key to the same house, and a folder
# is private only when somebody says so. So the list is now a list of
# exceptions, and a path with no exception on it is open to everybody.
#
# "none" means the person cannot see the path at all. "read" means
# they can look at it but not change it.
CREATE_RESTRICTIONS = """
CREATE TABLE IF NOT EXISTS restrictions (
    path TEXT NOT NULL,
    user_id TEXT NOT NULL,
    restriction TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (path, user_id)
)
"""

# Session tokens. They used to live in a JSON file that was read whole
# on every request and rewritten whole on every login -- which on a
# service that is polled all day grew into a real cost, and a lost
# update could throw away a token somebody was holding.
CREATE_TOKENS = """
CREATE TABLE IF NOT EXISTS tokens (
    token TEXT PRIMARY KEY,
    username TEXT NOT NULL,
    expires REAL NOT NULL,
    created_at REAL NOT NULL
)
"""


def hash_password(password: str, salt: str) -> str:
    return hashlib.pbkdf2_hmac(
        "sha256", password.encode(), salt.encode(), 100_000
    ).hex()


class UserStore:
    """The users and their grants."""

    def __init__(self, db_path: str):
        self.db_path = db_path
        self._lock = threading.Lock()
        with self._lock:
            conn = self._get_conn()
            conn.execute(CREATE_USERS)
            conn.execute(CREATE_RESTRICTIONS)
            conn.execute(CREATE_TOKENS)
            self._migrate_grants_to_restrictions(conn)
            conn.commit()
            conn.close()

    def _get_conn(self) -> sqlite3.Connection:
        from . import db

        return db.connect(self.db_path)

    # ---- users ----

    def create_user(
        self,
        username: str,
        password: str,
        role: str = ROLE_USER,
    ) -> dict:
        username = (username or "").strip()
        if not username:
            raise UserError("用户名不能为空")
        if len(username) > 64:
            raise UserError("用户名过长")
        if not password or len(password) < 4:
            raise UserError("密码至少 4 个字符")
        if role not in (ROLE_ADMIN, ROLE_USER):
            raise UserError("未知角色")

        user_id = secrets.token_hex(16)
        salt = secrets.token_hex(16)
        with self._lock:
            conn = self._get_conn()
            try:
                existing = conn.execute(
                    "SELECT id FROM users WHERE username = ?", (username,)
                ).fetchone()
                if existing:
                    raise UserError("用户名已存在")
                conn.execute(
                    "INSERT INTO users (id, username, password_hash, salt, role,"
                    " disabled, created_at) VALUES (?, ?, ?, ?, ?, 0, ?)",
                    (user_id, username, hash_password(password, salt), salt, role,
                     int(time.time())),
                )
                conn.commit()
            finally:
                conn.close()
        log.info("created user %s (%s)", username, role)
        return self.get_user(user_id) or {}

    def get_user(self, user_id: str) -> Optional[dict]:
        with self._lock:
            conn = self._get_conn()
            try:
                row = conn.execute(
                    "SELECT id, username, password_hash, salt, role, disabled, created_at"
                    " FROM users WHERE id = ?",
                    (user_id,),
                ).fetchone()
            finally:
                conn.close()
        return self._user(row) if row else None

    def get_by_username(self, username: str) -> Optional[dict]:
        with self._lock:
            conn = self._get_conn()
            try:
                row = conn.execute(
                    "SELECT id, username, password_hash, salt, role, disabled, created_at"
                    " FROM users WHERE username = ?",
                    ((username or "").strip(),),
                ).fetchone()
            finally:
                conn.close()
        return self._user(row) if row else None

    def list_users(self) -> list[dict]:
        with self._lock:
            conn = self._get_conn()
            try:
                rows = conn.execute(
                    "SELECT id, username, password_hash, salt, role, disabled, created_at"
                    " FROM users ORDER BY created_at ASC"
                ).fetchall()
            finally:
                conn.close()
        return [self._user(r) for r in rows]

    def verify(self, username: str, password: str) -> Optional[dict]:
        """The user, if the password is right and the account is usable."""
        user = self.get_by_username(username)
        if user is None or user["disabled"]:
            return None
        digest = hash_password(password, user["salt"])
        if not secrets.compare_digest(digest, user["password_hash"]):
            return None
        return user

    def set_password(self, user_id: str, password: str) -> bool:
        if not password or len(password) < 4:
            raise UserError("密码至少 4 个字符")
        salt = secrets.token_hex(16)
        with self._lock:
            conn = self._get_conn()
            try:
                cur = conn.execute(
                    "UPDATE users SET password_hash = ?, salt = ? WHERE id = ?",
                    (hash_password(password, salt), salt, user_id),
                )
                conn.commit()
                return cur.rowcount > 0
            finally:
                conn.close()

    def set_disabled(self, user_id: str, disabled: bool) -> bool:
        with self._lock:
            conn = self._get_conn()
            try:
                cur = conn.execute(
                    "UPDATE users SET disabled = ? WHERE id = ?",
                    (1 if disabled else 0, user_id),
                )
                conn.commit()
                return cur.rowcount > 0
            finally:
                conn.close()

    def delete_user(self, user_id: str) -> bool:
        with self._lock:
            conn = self._get_conn()
            try:
                cur = conn.execute("DELETE FROM users WHERE id = ?", (user_id,))
                conn.execute("DELETE FROM restrictions WHERE user_id = ?", (user_id,))
                conn.commit()
                return cur.rowcount > 0
            finally:
                conn.close()

    def count_admins(self) -> int:
        with self._lock:
            conn = self._get_conn()
            try:
                return conn.execute(
                    "SELECT COUNT(*) FROM users WHERE role = ? AND disabled = 0",
                    (ROLE_ADMIN,),
                ).fetchone()[0]
            finally:
                conn.close()

    # ---- grants ----

    def restrict(self, path: str, user_id: str, restriction: str) -> dict:
        """Keep somebody out of a folder.

        A folder on a machine a family shares is open to everybody
        until somebody says otherwise. This is the saying otherwise.
        """
        if restriction not in RESTRICTIONS:
            raise UserError("未知的限制类型")
        path = (path or "").strip().strip("/")
        with self._lock:
            conn = self._get_conn()
            try:
                conn.execute(
                    "INSERT INTO restrictions (path, user_id, restriction, created_at)"
                    " VALUES (?, ?, ?, ?)"
                    " ON CONFLICT(path, user_id) DO UPDATE SET restriction = excluded.restriction",
                    (path, user_id, restriction, int(time.time())),
                )
                conn.commit()
            finally:
                conn.close()
        return {"path": path, "user_id": user_id, "restriction": restriction}

    def lift(self, path: str, user_id: str) -> bool:
        """Let somebody back in."""
        path = (path or "").strip().strip("/")
        with self._lock:
            conn = self._get_conn()
            try:
                cur = conn.execute(
                    "DELETE FROM restrictions WHERE path = ? AND user_id = ?", (path, user_id)
                )
                conn.commit()
                return cur.rowcount > 0
            finally:
                conn.close()

    def list_restrictions(self, user_id: Optional[str] = None) -> list[dict]:
        with self._lock:
            conn = self._get_conn()
            try:
                if user_id:
                    rows = conn.execute(
                        "SELECT path, user_id, restriction, created_at FROM restrictions"
                        " WHERE user_id = ? ORDER BY path",
                        (user_id,),
                    ).fetchall()
                else:
                    rows = conn.execute(
                        "SELECT path, user_id, restriction, created_at FROM restrictions"
                        " ORDER BY path"
                    ).fetchall()
            finally:
                conn.close()
        return [
            {"path": r[0], "user_id": r[1], "restriction": r[2], "created_at": r[3]}
            for r in rows
        ]

    def restrictions_for(self, user_id: str) -> list[tuple[str, str]]:
        """What one person may not reach, as (path, restriction) pairs."""
        return [(r["path"], r["restriction"]) for r in self.list_restrictions(user_id)]

    def _migrate_grants_to_restrictions(self, conn: sqlite3.Connection) -> None:
        """Turn the old list of grants into the new list of exceptions.

        A grant said "this folder is closed, and here is the key".
        The new list says "this folder is open, and here is who is shut
        out". So a grant to one person becomes a restriction on
        everybody else -- the same rule, said the other way round.
        """
        tables = {r[0] for r in conn.execute(
            "SELECT name FROM sqlite_master WHERE type='table'"
        )}
        if "acl" not in tables:
            return
        rows = conn.execute("SELECT path, user_id, permission FROM acl").fetchall()
        if not rows:
            return
        everyone = [r[0] for r in conn.execute("SELECT id FROM users")]
        now = int(time.time())
        for path, granted_to, _permission in rows:
            for user_id in everyone:
                if user_id == granted_to:
                    continue
                conn.execute(
                    "INSERT OR IGNORE INTO restrictions (path, user_id, restriction, created_at)"
                    " VALUES (?, ?, 'none', ?)",
                    (path, user_id, now),
                )
        conn.commit()
        log.info(
            "moved %d folder grants into the new list of exceptions", len(rows)
        )

    # ---- session tokens ----

    def store_token(self, token: str, username: str, expires: float) -> None:
        """Remember a session token."""
        with self._lock:
            conn = self._get_conn()
            try:
                conn.execute(
                    "INSERT OR REPLACE INTO tokens (token, username, expires, created_at)"
                    " VALUES (?, ?, ?, ?)",
                    (token, username, float(expires), time.time()),
                )
                conn.commit()
            finally:
                conn.close()

    def token_username(self, token: str) -> Optional[str]:
        """The user a token belongs to, or None if it is expired or unknown."""
        with self._lock:
            conn = self._get_conn()
            try:
                row = conn.execute(
                    "SELECT username, expires FROM tokens WHERE token = ?", (token,)
                ).fetchone()
            finally:
                conn.close()
        if row is None:
            return None
        if time.time() > row[1]:
            self.revoke_token(token)
            return None
        return row[0]

    def revoke_token(self, token: str) -> bool:
        with self._lock:
            conn = self._get_conn()
            try:
                cur = conn.execute("DELETE FROM tokens WHERE token = ?", (token,))
                conn.commit()
                return cur.rowcount > 0
            finally:
                conn.close()

    def prune_tokens(self, keep_per_user: int = 20) -> int:
        """Drop the tokens that have expired, and any beyond the newest few.

        A browser that logs in again and again -- or a script that
        forgets to sign out -- would otherwise leave the table growing
        for as long as the service runs.
        """
        with self._lock:
            conn = self._get_conn()
            try:
                cur = conn.execute("DELETE FROM tokens WHERE expires < ?", (time.time(),))
                removed = cur.rowcount
                rows = conn.execute(
                    "SELECT token, username FROM tokens ORDER BY created_at DESC"
                ).fetchall()
                seen: dict = {}
                doomed = []
                for token, username in rows:
                    count = seen.get(username, 0)
                    if count >= keep_per_user:
                        doomed.append((token,))
                    else:
                        seen[username] = count + 1
                if doomed:
                    conn.executemany("DELETE FROM tokens WHERE token = ?", doomed)
                    removed += len(doomed)
                conn.commit()
                return removed
            finally:
                conn.close()

    def count_tokens(self) -> int:
        with self._lock:
            conn = self._get_conn()
            try:
                row = conn.execute("SELECT COUNT(*) FROM tokens").fetchone()
            finally:
                conn.close()
        return int(row[0]) if row else 0

    def revoke_all_tokens(self) -> int:
        """Sign everybody out. Used when the password changes."""
        with self._lock:
            conn = self._get_conn()
            try:
                cur = conn.execute("DELETE FROM tokens")
                conn.commit()
                return cur.rowcount
            finally:
                conn.close()

    # ---- internals ----

    @staticmethod
    def _user(row) -> dict:
        return {
            "id": row[0],
            "username": row[1],
            "password_hash": row[2],
            "salt": row[3],
            "role": row[4],
            "disabled": bool(row[5]),
            "created_at": row[6],
            "is_admin": row[4] == ROLE_ADMIN,
        }


class UserError(Exception):
    """Something went wrong with a user or a grant."""
