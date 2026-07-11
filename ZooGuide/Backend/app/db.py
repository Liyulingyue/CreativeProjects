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

-- Activity Achievements: catalog of available achievements (id, name, criteria)
CREATE TABLE IF NOT EXISTS achievements (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    icon TEXT NOT NULL,
    category TEXT NOT NULL,
    criteria_type TEXT NOT NULL,    -- photo_count / checkin_count / best_vibe / consecutive_days / venues_unique
    criteria_threshold INTEGER NOT NULL,
    sort_order INTEGER DEFAULT 0
);

-- Per-user earned achievements
CREATE TABLE IF NOT EXISTS user_achievements (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL,
    achievement_id TEXT NOT NULL,
    earned_at TEXT NOT NULL,
    progress INTEGER DEFAULT 0,  -- 0-100, 100 = fully earned
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (achievement_id) REFERENCES achievements(id) ON DELETE CASCADE,
    UNIQUE(user_id, achievement_id)
);

CREATE INDEX IF NOT EXISTS idx_user_achievements_user ON user_achievements(user_id);

-- GPS check-ins (separate from venue checkins, to track location-based activities)
CREATE TABLE IF NOT EXISTS gps_checkins (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER,
    session_id TEXT,
    lat REAL NOT NULL,
    lon REAL NOT NULL,
    nearest_venue_id TEXT,
    nearest_venue_name TEXT,
    in_park INTEGER DEFAULT 0,
    ts TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL
);
"""


DEFAULT_ACHIEVEMENTS = [
    # Photo achievements
    ("photo_first", "拍照新手", "拍下第一张动物照片", "📷", "photo", "photo_count", 1, 1),
    ("photo_5", "拍照入门", "累计拍 5 张照片", "📸", "photo", "photo_count", 5, 2),
    ("photo_20", "拍照达人", "累计拍 20 张照片", "🎞️", "photo", "photo_count", 20, 3),
    ("photo_perfect", "完美出片", "单张照片评分 90+", "🌟", "photo", "best_vibe", 90, 4),
    ("photo_streak", "连续打卡", "连续 3 天拍照", "🔥", "photo", "consecutive_days", 3, 5),
    # Checkin achievements
    ("checkin_first", "初次打卡", "第一次标记已游览", "🦒", "checkin", "checkin_count", 1, 6),
    ("checkin_5", "小小游客", "游览 5 个不同馆", "🌱", "checkin", "venues_unique", 5, 7),
    ("checkin_10", "资深游客", "游览 10 个不同馆", "🌳", "checkin", "venues_unique", 10, 8),
    ("checkin_all", "红山老炮", "游览全部 23 个馆", "🏆", "checkin", "venues_unique", 23, 9),
    # GPS achievements
    ("gps_first", "找得到路", "第一次 GPS 定位", "📍", "gps", "checkin_count", 1, 10),
    ("gps_5", "GPS 高手", "GPS 定位 5 次", "🛰️", "gps", "checkin_count", 5, 11),
]


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
            # Seed default achievements if empty
            cur = c.execute("SELECT COUNT(*) FROM achievements")
            (count,) = cur.fetchone()
            if count == 0:
                c.executemany(
                    """INSERT INTO achievements
                       (id, name, description, icon, category, criteria_type, criteria_threshold, sort_order)
                       VALUES (?, ?, ?, ?, ?, ?, ?, ?)""",
                    DEFAULT_ACHIEVEMENTS,
                )


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

def insert_gps_checkin(
    lat: float,
    lon: float,
    user_id: Optional[int] = None,
    session_id: Optional[str] = None,
    nearest_venue_id: Optional[str] = None,
    nearest_venue_name: Optional[str] = None,
    in_park: bool = False,
) -> int:
    ts = datetime.utcnow().isoformat(timespec="seconds")
    with _lock:
        with get_conn() as c:
            cur = c.execute(
                """INSERT INTO gps_checkins(user_id, session_id, lat, lon, nearest_venue_id, nearest_venue_name, in_park, ts)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?)""",
                (
                    user_id,
                    session_id,
                    lat,
                    lon,
                    nearest_venue_id,
                    nearest_venue_name,
                    int(in_park),
                    ts,
                ),
            )
            return cur.lastrowid


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
# Achievements
# ---------------------------------------------------------------------------

def list_all_achievements() -> list[dict]:
    """Return all available achievements (catalog)."""
    with get_conn() as c:
        rows = c.execute(
            "SELECT id, name, description, icon, category, criteria_type, criteria_threshold, sort_order "
            "FROM achievements ORDER BY sort_order, id"
        ).fetchall()
        return [dict(r) for r in rows]


def get_user_earned(user_id: int) -> list[dict]:
    """Return achievements earned by a user."""
    with get_conn() as c:
        rows = c.execute(
            """SELECT a.id, a.name, a.description, a.icon, a.category,
                      a.criteria_type, a.criteria_threshold,
                      ua.earned_at, ua.progress
               FROM user_achievements ua
               JOIN achievements a ON a.id = ua.achievement_id
               WHERE ua.user_id = ?
               ORDER BY ua.earned_at DESC""",
            (user_id,),
        ).fetchall()
        return [dict(r) for r in rows]


def grant_achievement(user_id: int, achievement_id: str) -> bool:
    """Mark achievement as earned. Returns True if newly granted, False if already had."""
    with _lock:
        with get_conn() as c:
            cur = c.execute(
                "SELECT id FROM user_achievements WHERE user_id = ? AND achievement_id = ?",
                (user_id, achievement_id),
            )
            if cur.fetchone():
                return False
            c.execute(
                """INSERT INTO user_achievements(user_id, achievement_id, earned_at, progress)
                   VALUES (?, ?, ?, 100)""",
                (user_id, achievement_id, datetime.utcnow().isoformat(timespec="seconds")),
            )
            return True


def get_user_stats_for_achievements(user_id: int) -> dict:
    """Compute stats used to evaluate achievement criteria."""
    with get_conn() as c:
        photo_count = c.execute(
            "SELECT COUNT(*) FROM photo_evals WHERE user_id = ?",
            (user_id,),
        ).fetchone()[0]
        checkin_count = c.execute(
            "SELECT COUNT(*) FROM checkins WHERE user_id = ?",
            (user_id,),
        ).fetchone()[0]
        venues_unique = c.execute(
            "SELECT COUNT(DISTINCT venue_id) FROM checkins WHERE user_id = ?",
            (user_id,),
        ).fetchone()[0]
        best_vibe = c.execute(
            "SELECT MAX(CAST(json_extract(payload_json, '$.vibe_score') AS INTEGER)) FROM photo_evals WHERE user_id = ?",
            (user_id,),
        ).fetchone()[0] or 0
        # Consecutive days with photos
        rows = c.execute(
            "SELECT DISTINCT substr(ts, 1, 10) AS day FROM photo_evals WHERE user_id = ? ORDER BY day DESC",
            (user_id,),
        ).fetchall()
        days = [r[0] for r in rows]
        consecutive_days = 0
        if days:
            from datetime import datetime, timedelta
            try:
                cur_day = datetime.fromisoformat(days[0])
                consecutive_days = 1
                for i in range(1, len(days)):
                    d = datetime.fromisoformat(days[i])
                    if (cur_day - d).days == 1:
                        consecutive_days += 1
                        cur_day = d
                    else:
                        break
            except Exception:
                pass
        gps_count = c.execute(
            "SELECT COUNT(*) FROM gps_checkins WHERE user_id = ?",
            (user_id,),
        ).fetchone()[0]

    return {
        "photo_count": photo_count,
        "checkin_count": checkin_count,
        "venues_unique": venues_unique,
        "best_vibe": best_vibe,
        "consecutive_days": consecutive_days,
        "gps_count": gps_count,
    }


def evaluate_achievements(user_id: int) -> list[str]:
    """Check all criteria and grant newly-earned achievements. Returns list of newly earned IDs."""
    catalog = list_all_achievements()
    stats = get_user_stats_for_achievements(user_id)
    newly_earned = []
    for a in catalog:
        ctype = a["criteria_type"]
        threshold = a["criteria_threshold"]
        current = stats.get(ctype, 0)
        if current >= threshold:
            if grant_achievement(user_id, a["id"]):
                newly_earned.append(a["id"])
    return newly_earned


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