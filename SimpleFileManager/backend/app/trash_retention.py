"""Keeping the recycle bin from growing forever.

The bin holds deleted files until somebody empties it. On a family NAS
that is usually the behaviour you want -- the afternoon you spent on a
video is still there -- but it also means the bin quietly eats disk
space that nothing reports.

So there is a retention window. When one is set, entries older than it
are removed for good, and *what was removed is written down*: an
automatic deletion that leaves no trace is the kind of thing that
turns into "where did my file go" a month later.

The window is deliberately off by default (0 days = keep everything).
"""

from __future__ import annotations

import logging
import sqlite3
import threading
import time
from pathlib import Path
from typing import Optional

from .deps import DATA_DIR, get_storage_root, state
from .trash import TrashService

log = logging.getLogger(__name__)

DB_NAME = "trash.db"
# How often the scheduler looks for expired entries. Hourly is fine:
# a file that is a minute past its window is not a problem, and the
# check is cheap.
CHECK_INTERVAL_SECONDS = 3600


class TrashLog:
    """What the retention window removed, and when."""

    def __init__(self, db_path: str):
        self.db_path = db_path
        self._lock = threading.Lock()
        self._init_db()

    def _get_conn(self) -> sqlite3.Connection:
        return sqlite3.connect(self.db_path)

    def _init_db(self):
        with self._lock:
            conn = self._get_conn()
            conn.execute("""
                CREATE TABLE IF NOT EXISTS trash_log (
                    id TEXT PRIMARY KEY,
                    ts INTEGER,
                    entry_id TEXT,
                    name TEXT,
                    rel_path TEXT,
                    size INTEGER DEFAULT 0,
                    reason TEXT DEFAULT ''
                )
            """)
            conn.commit()
            conn.close()

    def record(self, entries: list[dict], reason: str = "retention") -> int:
        """Write down a batch of removed entries."""
        if not entries:
            return 0
        import uuid
        now = int(time.time())
        with self._lock:
            conn = self._get_conn()
            for e in entries:
                conn.execute(
                    "INSERT INTO trash_log (id, ts, entry_id, name, rel_path, size, reason) "
                    "VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (uuid.uuid4().hex, now, e.get("id", ""), e.get("name", ""),
                     e.get("rel_path", ""), int(e.get("size", 0) or 0), reason),
                )
            conn.commit()
            conn.close()
        return len(entries)

    def list(self, limit: int = 50) -> list[dict]:
        with self._lock:
            conn = self._get_conn()
            rows = conn.execute(
                "SELECT id, ts, entry_id, name, rel_path, size, reason FROM trash_log "
                "ORDER BY ts DESC LIMIT ?", (int(limit),),
            ).fetchall()
            conn.close()
        return [
            {"id": r[0], "ts": r[1], "entry_id": r[2], "name": r[3],
             "rel_path": r[4], "size": r[5], "reason": r[6]}
            for r in rows
        ]

    def clear(self) -> int:
        with self._lock:
            conn = self._get_conn()
            count = conn.execute("SELECT COUNT(*) FROM trash_log").fetchone()[0]
            conn.execute("DELETE FROM trash_log")
            conn.commit()
            conn.close()
        return int(count)


class TrashRetention:
    """The scheduler that enforces the retention window."""

    def __init__(self):
        self._stop_event = threading.Event()
        self._thread: Optional[threading.Thread] = None
        self._log: Optional[TrashLog] = None
        self._last_run_ts: float = 0.0

    # ---- the record of what was removed ----

    @property
    def log(self) -> TrashLog:
        if self._log is None:
            self._log = TrashLog(str(DATA_DIR / DB_NAME))
        return self._log

    # ---- scheduler ----

    def start(self):
        if self._thread and self._thread.is_alive():
            return
        self._stop_event.clear()
        self._thread = threading.Thread(target=self._run, name="trash-retention", daemon=True)
        self._thread.start()

    def stop(self):
        self._stop_event.set()

    def _run(self):
        # A short first wait, so a restart (or a changed retention
        # window) takes effect without waiting a whole hour for it.
        if self._stop_event.wait(60):
            return
        while True:
            try:
                self.cleanup_now(reason="scheduled")
            except Exception:
                log.exception("trash retention cleanup failed")
            if self._stop_event.wait(CHECK_INTERVAL_SECONDS):
                return

    # ---- the work ----

    def retention_days(self) -> int:
        try:
            return max(0, int(state.get_settings().trash_retention_days or 0))
        except Exception:
            return 0

    def cleanup_now(self, reason: str = "manual") -> dict:
        """Remove whatever has outlived the window.

        Returns a summary so the caller can tell the user what went.
        """
        days = self.retention_days()
        if days <= 0:
            return {"retention_days": days, "purged": 0, "entries": [], "skipped": True}
        svc = TrashService(Path(get_storage_root()))
        removed = svc.purge_expired(days)
        if removed:
            self.log.record(removed, reason=reason)
            log.info(
                "trash retention removed %d entries older than %d days (%s)",
                len(removed), days, reason,
            )
        self._last_run_ts = time.time()
        return {
            "retention_days": days,
            "purged": len(removed),
            "entries": removed,
            "skipped": False,
        }


trash_retention = TrashRetention()
