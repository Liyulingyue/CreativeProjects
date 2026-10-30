"""The files somebody wants to keep an eye on.

A star in the corner of a file, and a place to come back to. It is
one person's list -- "the ones I want to find again" is a private
sort of thing -- so the list is kept per person.
"""

from __future__ import annotations

import logging
import sqlite3
import time
from pathlib import Path
from typing import Optional

log = logging.getLogger(__name__)

FAVORITES_DB_NAME = "favorites.db"

CREATE_FAVORITES = """
CREATE TABLE IF NOT EXISTS favorites (
    path TEXT NOT NULL,
    user_id TEXT NOT NULL,
    name TEXT DEFAULT '',
    is_dir INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (path, user_id)
)
"""

CREATE_FAVORITES_INDEX = """
CREATE INDEX IF NOT EXISTS idx_favorites_user ON favorites(user_id, created_at DESC)
"""


class FavoriteStore:
    """Somebody's starred files."""

    def __init__(self, db_path: str):
        self.db_path = db_path
        conn = self._connect()
        try:
            conn.execute(CREATE_FAVORITES)
            conn.execute(CREATE_FAVORITES_INDEX)
            conn.commit()
        finally:
            conn.close()

    def _connect(self) -> sqlite3.Connection:
        from . import db

        return db.connect(self.db_path)

    def add(self, path: str, user_id: str, name: str = "", is_dir: bool = False) -> dict:
        path = (path or "").strip()
        if not path:
            raise ValueError("路径不能为空")
        conn = self._connect()
        try:
            conn.execute(
                "INSERT INTO favorites (path, user_id, name, is_dir, created_at)"
                " VALUES (?, ?, ?, ?, ?)"
                " ON CONFLICT(path, user_id) DO UPDATE SET name = excluded.name,"
                " is_dir = excluded.is_dir",
                (path, user_id, name or Path(path).name, 1 if is_dir else 0, int(time.time())),
            )
            conn.commit()
        finally:
            conn.close()
        return {"path": path, "name": name or Path(path).name, "is_dir": is_dir}

    def remove(self, path: str, user_id: str) -> bool:
        conn = self._connect()
        try:
            cur = conn.execute(
                "DELETE FROM favorites WHERE path = ? AND user_id = ?", (path, user_id)
            )
            conn.commit()
            return cur.rowcount > 0
        finally:
            conn.close()

    def list_for(self, user_id: str) -> list[dict]:
        """Somebody's starred files, newest first.

        A starred file that has since been moved or deleted is left
        out -- there is nothing to open. But it is not forgotten either:
        the entry stays, so if the file comes back it is starred again.
        """
        conn = self._connect()
        try:
            rows = conn.execute(
                "SELECT path, name, is_dir, created_at FROM favorites"
                " WHERE user_id = ? ORDER BY created_at DESC",
                (user_id,),
            ).fetchall()
        finally:
            conn.close()
        return [
            {"path": r[0], "name": r[1], "is_dir": bool(r[2]), "created_at": r[3]}
            for r in rows
        ]

    def paths_for(self, user_id: str) -> set[str]:
        conn = self._connect()
        try:
            rows = conn.execute(
                "SELECT path FROM favorites WHERE user_id = ?", (user_id,)
            ).fetchall()
        finally:
            conn.close()
        return {r[0] for r in rows}

    def is_favorite(self, path: str, user_id: str) -> bool:
        conn = self._connect()
        try:
            row = conn.execute(
                "SELECT 1 FROM favorites WHERE path = ? AND user_id = ?", (path, user_id)
            ).fetchone()
        finally:
            conn.close()
        return row is not None

    def count_for(self, user_id: str) -> int:
        conn = self._connect()
        try:
            row = conn.execute(
                "SELECT COUNT(*) FROM favorites WHERE user_id = ?", (user_id,)
            ).fetchone()
        finally:
            conn.close()
        return int(row[0]) if row else 0
