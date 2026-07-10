"""SQLite persistence: users, sessions, checkins, photo evals, routes.

Design:
  - Most reads/writes are by user_id OR session_id (anonymous users still get persistence)
  - Token-based auth (UUID stored in DB, revocable)
  - Single-file SQLite DB at Backend/data/zooguide.db
"""

from __future__ import annotations

import json
import sqlite3
import threading
from contextlib import contextmanager
from datetime import datetime, timedelta
from pathlib import Path
from typing import Iterator, Optional

from . import config


DB_PATH = Path(__file__).resolve().parent.parent / "data" / "zooguide.db"
_lock = threading.Lock()


SCHEMA = """
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    display_name TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS auth_tokens (
    token TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_tokens_user ON auth_tokens(user_id);

CREATE TABLE IF NOT EXISTS checkins (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER,
    session_id TEXT NOT NULL,
    venue_id TEXT NOT NULL,
    venue_name TEXT NOT NULL,
    ts TEXT NOT NULL,
    note TEXT,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_checkins_user ON checkins(user_id, ts DESC);
CREATE INDEX IF NOT EXISTS idx_checkins_session ON checkins(session_id, ts DESC);

CREATE TABLE IF NOT EXISTS photo_evals (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    evaluation_id TEXT NOT NULL UNIQUE,
    user_id INTEGER,
    session_id TEXT,
    payload_json TEXT NOT NULL,
    image_path TEXT,
    ts TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_photo_user ON photo_evals(user_id, ts DESC);
CREATE INDEX IF NOT EXISTS idx_photo_session ON photo_evals(session_id, ts DESC);

CREATE TABLE IF NOT EXISTS routes (
    id TEXT PRIMARY KEY,
    user_id INTEGER,
    prefs_json TEXT NOT NULL,
    summary TEXT,
    total_minutes INTEGER,
    stops_count INTEGER,
    llm_used INTEGER,
    fallback INTEGER,
    created_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_routes_user ON routes(user_id, created_at DESC);
"""


@contextmanager
def get_conn() -> Iterator[sqlite3.Connection]:
    DB_PATH.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(str(DB_PATH), timeout=10.0)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA foreign_keys = ON")
    try:
        yield conn
        conn.commit()
    finally:
        conn.close()


def init_db() -> None:
    with _lock:
        with get_conn() as c:
            c.executescript(SCHEMA)


# ---------------------------------------------------------------------------
# Users & tokens
# ---------------------------------------------------------------------------

def create_user(username: str, password_hash: str, display_name: Optional[str] = None) -> int:
    with _lock:
        with get_conn() as c:
            cur = c.execute(
                "INSERT INTO users(username, password_hash, display_name, created_at) VALUES (?, ?, ?, ?)",
                (username, password_hash, display_name or username, datetime.utcnow().isoformat()),
            )
            return cur.lastrowid


def find_user_by_username(username: str) -> Optional[dict]:
    with get_conn() as c:
        row = c.execute("SELECT * FROM users WHERE username = ?", (username,)).fetchone()
        return dict(row) if row else None


def find_user_by_id(user_id: int) -> Optional[dict]:
    with get_conn() as c:
        row = c.execute("SELECT * FROM users WHERE id = ?", (user_id,)).fetchone()
        return dict(row) if row else None


def create_token(user_id: int, days: int = 30) -> str:
    import uuid
    token = uuid.uuid4().hex + uuid.uuid4().hex  # 64 chars
    expires = (datetime.utcnow() + timedelta(days=days)).isoformat()
    with _lock:
        with get_conn() as c:
            c.execute(
                "INSERT INTO auth_tokens(token, user_id, expires_at, created_at) VALUES (?, ?, ?, ?)",
                (token, user_id, expires, datetime.utcnow().isoformat()),
            )
    return token


def find_user_by_token(token: str) -> Optional[dict]:
    with get_conn() as c:
        row = c.execute(
            """
            SELECT u.* FROM users u
            JOIN auth_tokens t ON t.user_id = u.id
            WHERE t.token = ? AND t.expires_at > ?
            """,
            (token, datetime.utcnow().isoformat()),
        ).fetchone()
        return dict(row) if row else None


def delete_token(token: str) -> None:
    with _lock:
        with get_conn() as c:
            c.execute("DELETE FROM auth_tokens WHERE token = ?", (token,))


# ---------------------------------------------------------------------------
# Checkins
# ---------------------------------------------------------------------------

def insert_checkin(
    venue_id: str,
    venue_name: str,
    session_id: str,
    user_id: Optional[int] = None,
    note: Optional[str] = None,
) -> dict:
    ts = datetime.utcnow().isoformat(timespec="seconds")
    with _lock:
        with get_conn() as c:
            cur = c.execute(
                """INSERT INTO checkins(user_id, session_id, venue_id, venue_name, ts, note)
                   VALUES (?, ?, ?, ?, ?, ?)""",
                (user_id, session_id, venue_id, venue_name, ts, note),
            )
            checkin_id = cur.lastrowid
    return {"id": checkin_id, "venue_id": venue_id, "venue_name": venue_name, "ts": ts}


def list_checkins_by_session(session_id: str) -> list[dict]:
    with get_conn() as c:
        rows = c.execute(
            "SELECT * FROM checkins WHERE session_id = ? ORDER BY ts DESC",
            (session_id,),
        ).fetchall()
        return [dict(r) for r in rows]


def list_checkins_by_user(user_id: int, limit: int = 100) -> list[dict]:
    with get_conn() as c:
        rows = c.execute(
            "SELECT * FROM checkins WHERE user_id = ? ORDER BY ts DESC LIMIT ?",
            (user_id, limit),
        ).fetchall()
        return [dict(r) for r in rows]


# ---------------------------------------------------------------------------
# Photo evaluations
# ---------------------------------------------------------------------------

def insert_photo_eval(
    evaluation_id: str,
    payload: dict,
    image_path: Optional[str] = None,
    session_id: Optional[str] = None,
    user_id: Optional[int] = None,
) -> None:
    ts = datetime.utcnow().isoformat(timespec="seconds")
    with _lock:
        with get_conn() as c:
            c.execute(
                """INSERT INTO photo_evals(evaluation_id, user_id, session_id, payload_json, image_path, ts)
                   VALUES (?, ?, ?, ?, ?, ?)""",
                (
                    evaluation_id,
                    user_id,
                    session_id,
                    json.dumps(payload, ensure_ascii=False),
                    image_path,
                    ts,
                ),
            )


def list_photo_evals_by_user(user_id: int, limit: int = 50) -> list[dict]:
    with get_conn() as c:
        rows = c.execute(
            "SELECT evaluation_id, payload_json, image_path, ts FROM photo_evals WHERE user_id = ? ORDER BY ts DESC LIMIT ?",
            (user_id, limit),
        ).fetchall()
        out = []
        for r in rows:
            d = dict(r)
            try:
                d["payload"] = json.loads(d.pop("payload_json"))
            except Exception:
                d["payload"] = {}
            out.append(d)
        return out


# ---------------------------------------------------------------------------
# Routes (history)
# ---------------------------------------------------------------------------

def insert_route(
    route_id: str,
    prefs: dict,
    summary: str,
    total_minutes: int,
    stops_count: int,
    llm_used: bool,
    fallback: bool,
    user_id: Optional[int] = None,
) -> None:
    ts = datetime.utcnow().isoformat(timespec="seconds")
    with _lock:
        with get_conn() as c:
            c.execute(
                """INSERT INTO routes(id, user_id, prefs_json, summary, total_minutes, stops_count, llm_used, fallback, created_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)""",
                (
                    route_id,
                    user_id,
                    json.dumps(prefs, ensure_ascii=False),
                    summary,
                    total_minutes,
                    stops_count,
                    int(llm_used),
                    int(fallback),
                    ts,
                ),
            )


def list_routes_by_user(user_id: int, limit: int = 20) -> list[dict]:
    with get_conn() as c:
        rows = c.execute(
            """SELECT id, prefs_json, summary, total_minutes, stops_count, llm_used, fallback, created_at
               FROM routes WHERE user_id = ? ORDER BY created_at DESC LIMIT ?""",
            (user_id, limit),
        ).fetchall()
        out = []
        for r in rows:
            d = dict(r)
            try:
                d["prefs"] = json.loads(d.pop("prefs_json"))
            except Exception:
                d["prefs"] = {}
            d["llm_used"] = bool(d["llm_used"])
            d["fallback"] = bool(d["fallback"])
            out.append(d)
        return out


def get_route_full(route_id: str, user_id: int) -> Optional[dict]:
    """Reconstruct full route from prefs + summary."""
    with get_conn() as c:
        row = c.execute(
            "SELECT * FROM routes WHERE id = ? AND user_id = ?",
            (route_id, user_id),
        ).fetchone()
        if not row:
            return None
        d = dict(row)
        try:
            d["prefs"] = json.loads(d.pop("prefs_json"))
        except Exception:
            d["prefs"] = {}
        return d