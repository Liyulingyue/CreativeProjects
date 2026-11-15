"""Sharing a file or a directory with someone who has no login.

A NAS is a private thing, but the files on it are not always meant
to stay private -- a photograph for a relative, a document for a
colleague. A share is a link that opens one path, read-only, for a
while, and can be stopped at any moment.

The rules are deliberately narrow, because a share is the one thing
in the service that a stranger can reach:

* one path per link, never a directory above it;
* read-only -- a share can be fetched, never changed;
* an expiry, and the ability to revoke it before then;
* optionally a password, so a link that leaks is not enough on its own;
* optionally a cap on how many times it can be fetched.

The link id is a long random string rather than anything derived from
the path, so it cannot be guessed and it does not leak where the file
lives.
"""

from __future__ import annotations

import hashlib
import logging
import secrets
import sqlite3
import threading
import time
from typing import Optional
from . import db

log = logging.getLogger(__name__)

SHARES_DB_NAME = "shares.db"

# How long a share lasts when the operator does not say.
DEFAULT_EXPIRY_DAYS = 7
# The longest a share can last.
MAX_EXPIRY_DAYS = 365
# A link id is long enough that guessing it is not worth trying.
ID_BYTES = 24

CREATE_TABLE = """
CREATE TABLE IF NOT EXISTS shares (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL,
    is_dir INTEGER NOT NULL DEFAULT 0,
    size INTEGER NOT NULL DEFAULT 0,
    name TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL DEFAULT 0,
    password_hash TEXT,
    max_downloads INTEGER NOT NULL DEFAULT 0,
    downloads INTEGER NOT NULL DEFAULT 0,
    last_access INTEGER NOT NULL DEFAULT 0,
    revoked INTEGER NOT NULL DEFAULT 0
)
"""


def hash_password(password: str, salt: Optional[str] = None) -> str:
    """A salted hash of a share's password."""
    salt = salt or secrets.token_hex(16)
    digest = hashlib.pbkdf2_hmac(
        "sha256", password.encode(), salt.encode(), 100_000
    ).hex()
    return f"{salt}${digest}"


def check_password(password: str, stored: Optional[str]) -> bool:
    """Whether a password matches. A share with no password accepts anything."""
    if not stored:
        return True
    try:
        salt, _digest = stored.split("$", 1)
    except ValueError:
        return False
    return secrets.compare_digest(hash_password(password, salt), stored)


class ShareStore:
    """The shares table."""

    def __init__(self, db_path: str):
        self.db_path = db_path
        self._lock = threading.Lock()
        with self._lock:
            conn = self._get_conn()
            conn.execute(CREATE_TABLE)
            conn.commit()
            conn.close()

    def _get_conn(self) -> sqlite3.Connection:

        return db.connect(self.db_path)

    # ---- creating and listing ----

    def create(
        self,
        path: str,
        is_dir: bool,
        size: int,
        name: str,
        expiry_days: int = DEFAULT_EXPIRY_DAYS,
        password: str = "",
        max_downloads: int = 0,
    ) -> dict:
        share_id = secrets.token_urlsafe(ID_BYTES)
        now = int(time.time())
        days = max(0, min(int(expiry_days or 0), MAX_EXPIRY_DAYS))
        expires_at = now + days * 86400 if days else 0
        # The hash is slow and it is done before the lock is taken.
        secret = hash_password(password) if password else None
        with db.write_lock(self._lock, what="共享表"):
            conn = self._get_conn()
            try:
                conn.execute(
                    "INSERT INTO shares (id, path, is_dir, size, name, created_at, expires_at,"
                    " password_hash, max_downloads, downloads, last_access, revoked)"
                    " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0, 0)",
                    (
                        share_id, path, 1 if is_dir else 0, int(size), name, now,
                        expires_at,
                        secret,
                        int(max_downloads or 0),
                    ),
                )
                conn.commit()
            finally:
                conn.close()
        log.info("share created: %s -> %s (expires %s)", share_id, path, expires_at or "never")
        return self.get(share_id)

    def get(self, share_id: str) -> Optional[dict]:
        conn = self._get_conn()
        try:
            row = conn.execute(
                "SELECT id, path, is_dir, size, name, created_at, expires_at,"
                " password_hash, max_downloads, downloads, last_access, revoked"
                " FROM shares WHERE id = ?",
                (share_id,),
            ).fetchone()
        finally:
            conn.close()
        return self._row(row) if row else None

    def get_password_hash(self, share_id: str) -> Optional[str]:
        """The stored hash of a share's password.

        Kept apart from the listing so the hash is never handed out by
        accident -- the public view reports only whether there is one.
        """
        conn = self._get_conn()
        try:
            row = conn.execute(
                "SELECT password_hash FROM shares WHERE id = ?", (share_id,)
            ).fetchone()
        finally:
            conn.close()
        return row[0] if row else None

    def list(self, only_active: bool = False) -> list[dict]:
        conn = self._get_conn()
        try:
            rows = conn.execute(
                "SELECT id, path, is_dir, size, name, created_at, expires_at,"
                " password_hash, max_downloads, downloads, last_access, revoked"
                " FROM shares ORDER BY created_at DESC"
            ).fetchall()
        finally:
            conn.close()
        out = [self._row(r) for r in rows]
        if only_active:
            out = [s for s in out if s["active"]]
        return out

    def revoke(self, share_id: str) -> bool:
        with db.write_lock(self._lock, what="共享表"):
            conn = self._get_conn()
            try:
                cur = conn.execute(
                    "UPDATE shares SET revoked = 1 WHERE id = ?", (share_id,)
                )
                conn.commit()
                return cur.rowcount > 0
            finally:
                conn.close()

    def set_expiry(self, share_id: str, expiry_days: int) -> bool:
        now = int(time.time())
        days = max(0, min(int(expiry_days or 0), MAX_EXPIRY_DAYS))
        expires_at = now + days * 86400 if days else 0
        with db.write_lock(self._lock, what="共享表"):
            conn = self._get_conn()
            try:
                cur = conn.execute(
                    "UPDATE shares SET expires_at = ? WHERE id = ?", (expires_at, share_id)
                )
                conn.commit()
                return cur.rowcount > 0
            finally:
                conn.close()

    def note_access(self, share_id: str) -> None:
        """Record that a share was fetched."""
        with db.write_lock(self._lock, what="共享表"):
            conn = self._get_conn()
            try:
                conn.execute(
                    "UPDATE shares SET downloads = downloads + 1, last_access = ?"
                    " WHERE id = ?",
                    (int(time.time()), share_id),
                )
                conn.commit()
            finally:
                conn.close()

    def purge_expired(self) -> int:
        """Remove shares that have expired and been left alone."""
        now = int(time.time())
        with db.write_lock(self._lock, what="共享表"):
            conn = self._get_conn()
            try:
                cur = conn.execute(
                    "DELETE FROM shares WHERE expires_at > 0 AND expires_at < ?"
                    " AND last_access < ?",
                    (now, now - 30 * 86400),
                )
                conn.commit()
                return cur.rowcount
            finally:
                conn.close()

    # ---- internals ----

    @staticmethod
    def _row(row) -> dict:
        now = int(time.time())
        expires_at = row[6]
        expired = bool(expires_at) and expires_at < now
        used_up = bool(row[8]) and row[9] >= row[8]
        revoked = bool(row[11])
        return {
            "id": row[0],
            "path": row[1],
            "is_dir": bool(row[2]),
            "size": row[3],
            "name": row[4],
            "created_at": row[5],
            "expires_at": expires_at,
            "has_password": bool(row[7]),
            "max_downloads": row[8],
            "downloads": row[9],
            "last_access": row[10],
            "revoked": revoked,
            "expired": expired,
            "used_up": used_up,
            "active": not (revoked or expired or used_up),
        }
