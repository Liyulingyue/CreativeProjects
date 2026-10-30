"""An append-only record of who put what where.

A file on this machine is a plain file. It carries no owner and no
history, and this log does not change that: it records *uploads*, not
files. The difference matters. A file can also arrive by rsync, by
SMB, or by a shell on the same machine, and none of those produce an
event here. So an empty history means "not uploaded through this
service", never "nobody put it there".

The log is therefore informational, not a permission: nobody may
reach a file because their name is on it, and nobody may not because
their name is absent. What it is good for is answering "who put this
here" and "what did someone send me", which a shared machine has no
other way to answer.

It is bounded. A log that grows forever is a disk that fills up, so
entries older than `RETENTION_DAYS` or beyond `MAX_ROWS` are dropped
when a new one is written. The pruning is checked at most once an
hour, so the cost is a single delete per hour rather than per upload.
"""

from __future__ import annotations

import sqlite3
import threading
import time
from typing import Optional

# How long an entry is kept, and how many there may be at most. A
# household does not need years of "who uploaded this", and the
# numbers are generous enough that a busy share does not lose the last
# few months.
RETENTION_DAYS = 90
MAX_ROWS = 100_000
PRUNE_INTERVAL_SECONDS = 3600

CREATE_LOG = """
CREATE TABLE IF NOT EXISTS upload_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    at REAL NOT NULL,
    user_id TEXT NOT NULL DEFAULT '',
    username TEXT NOT NULL DEFAULT '',
    path TEXT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    size INTEGER NOT NULL DEFAULT 0,
    rel_dir TEXT NOT NULL DEFAULT ''
)
"""


class UploadLog:
    """Who uploaded what, when -- and nothing that outlives its use."""

    def __init__(self, db_path: str):
        self.db_path = db_path
        self._lock = threading.Lock()
        self._last_prune = 0.0
        with self._lock:
            conn = self._get_conn()
            try:
                conn.execute(CREATE_LOG)
                conn.execute(
                    "CREATE INDEX IF NOT EXISTS idx_upload_log_at ON upload_log(at)"
                )
                conn.commit()
            finally:
                conn.close()

    def _get_conn(self) -> sqlite3.Connection:
        from . import db

        return db.connect(self.db_path)

    # ---- writing ----

    def record(
        self,
        path: str,
        size: int = 0,
        name: str = "",
        rel_dir: str = "",
        user: Optional[dict] = None,
        at: Optional[float] = None,
    ) -> dict:
        """Note that a file arrived through the service.

        `user` is what the request was served as. None means the
        service is open, which is not the same as an unknown person:
        it means there was no account to record.
        """
        username = ""
        user_id = ""
        if user:
            username = str(user.get("username") or "")
            user_id = str(user.get("id") or "")
        when = time.time() if at is None else at

        with self._lock:
            conn = self._get_conn()
            try:
                cur = conn.execute(
                    "INSERT INTO upload_log (at, user_id, username, path, name, size, rel_dir) "
                    "VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (when, user_id, username, path, name or "", int(size), rel_dir or ""),
                )
                conn.commit()
                entry_id = cur.lastrowid
            finally:
                conn.close()
            self._maybe_prune_locked()

        return {
            "id": entry_id,
            "at": when,
            "user_id": user_id,
            "username": username,
            "path": path,
            "name": name or "",
            "size": int(size),
            "rel_dir": rel_dir or "",
        }

    # ---- reading ----

    def recent(self, limit: int = 100, offset: int = 0) -> list[dict]:
        limit = max(1, min(int(limit), 500))
        offset = max(0, int(offset))
        conn = self._get_conn()
        try:
            rows = conn.execute(
                "SELECT id, at, user_id, username, path, name, size, rel_dir "
                "FROM upload_log ORDER BY id DESC LIMIT ? OFFSET ?",
                (limit, offset),
            ).fetchall()
        finally:
            conn.close()
        return [self._row_to_dict(r) for r in rows]

    def count(self) -> int:
        conn = self._get_conn()
        try:
            return int(conn.execute("SELECT COUNT(*) FROM upload_log").fetchone()[0])
        finally:
            conn.close()

    def stats(self) -> dict:
        conn = self._get_conn()
        try:
            row = conn.execute(
                "SELECT COUNT(*), MIN(at), MAX(at), COALESCE(SUM(size), 0) FROM upload_log"
            ).fetchone()
        finally:
            conn.close()
        return {
            "count": int(row[0]),
            "oldest": row[1],
            "newest": row[2],
            "total_bytes": int(row[3]),
            "retention_days": RETENTION_DAYS,
            "max_rows": MAX_ROWS,
        }

    # ---- cleaning up ----

    def prune(
        self,
        retention_days: int = RETENTION_DAYS,
        max_rows: int = MAX_ROWS,
    ) -> dict:
        """Drop old entries and cap the total. Returns what was removed."""
        cutoff = time.time() - max(0, int(retention_days)) * 86400
        removed_old = 0
        removed_over = 0
        with self._lock:
            conn = self._get_conn()
            try:
                if retention_days > 0:
                    cur = conn.execute("DELETE FROM upload_log WHERE at < ?", (cutoff,))
                    removed_old = cur.rowcount or 0
                # Keep the newest `max_rows`; drop the rest. Done by id,
                # which is the arrival order and so the same as at, but
                # without depending on two entries not sharing a second.
                total = int(conn.execute("SELECT COUNT(*) FROM upload_log").fetchone()[0])
                if max_rows > 0 and total > max_rows:
                    cur = conn.execute(
                        "DELETE FROM upload_log WHERE id IN ("
                        "  SELECT id FROM upload_log ORDER BY id ASC LIMIT ?"
                        ")",
                        (total - max_rows,),
                    )
                    removed_over = cur.rowcount or 0
                conn.commit()
            finally:
                conn.close()
            self._last_prune = time.time()
        return {
            "removed_old": removed_old,
            "removed_over": removed_over,
            "removed": removed_old + removed_over,
        }

    def clear(self) -> int:
        with self._lock:
            conn = self._get_conn()
            try:
                cur = conn.execute("DELETE FROM upload_log")
                conn.commit()
                removed = cur.rowcount or 0
            finally:
                conn.close()
            self._last_prune = time.time()
        return removed

    def _maybe_prune_locked(self) -> None:
        """Prune at most once an hour, while the write lock is held."""
        now = time.time()
        if now - self._last_prune < PRUNE_INTERVAL_SECONDS:
            return
        self._last_prune = now
        cutoff = now - RETENTION_DAYS * 86400
        conn = self._get_conn()
        try:
            conn.execute("DELETE FROM upload_log WHERE at < ?", (cutoff,))
            total = int(conn.execute("SELECT COUNT(*) FROM upload_log").fetchone()[0])
            if total > MAX_ROWS:
                conn.execute(
                    "DELETE FROM upload_log WHERE id IN ("
                    "  SELECT id FROM upload_log ORDER BY id ASC LIMIT ?"
                    ")",
                    (total - MAX_ROWS,),
                )
            conn.commit()
        except Exception:
            # Pruning is housekeeping; it must never fail an upload.
            pass
        finally:
            conn.close()

    @staticmethod
    def _row_to_dict(row) -> dict:
        return {
            "id": row[0],
            "at": row[1],
            "user_id": row[2],
            "username": row[3],
            "path": row[4],
            "name": row[5],
            "size": row[6],
            "rel_dir": row[7],
        }
