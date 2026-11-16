"""A history of what a file used to contain.

An upload that replaces a file throws the old one away, and on a NAS
that is sometimes the difference between a good version and a bad
one. The version store keeps what was there before.

The contents are kept as blobs named after their own hash, so two
versions with the same content are stored once, and a file that is
replaced by an identical copy does not fill the disk with copies of
itself. A version is a row saying "this path held this blob, at this
time".

Only changes made through the service are recorded. A file edited
somewhere else -- in an editor on the server, or synced from a
laptop -- is not something the service can snapshot before the change
happens, so those edits are not versioned. The index notices them
later, but by then the old contents are gone.
"""

from __future__ import annotations

import hashlib
import logging
import os
import shutil
import sqlite3
import threading
import time
from pathlib import Path
from typing import Optional

log = logging.getLogger(__name__)

VERSIONS_DIR_NAME = ".versions"
BLOBS_DIR_NAME = "blobs"
VERSIONS_DB_NAME = "versions.db"

# How many versions of one file to keep. Old ones are dropped, so a
# file that is rewritten every day does not fill the disk.
MAX_VERSIONS_PER_FILE = 20

CREATE_TABLE = """
CREATE TABLE IF NOT EXISTS versions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL,
    blob TEXT NOT NULL,
    size INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    note TEXT DEFAULT ''
)
"""


def hash_file(path: Path) -> Optional[str]:
    """The SHA-256 of a file, or None if it cannot be read."""
    h = hashlib.sha256()
    try:
        with open(path, "rb") as f:
            for block in iter(lambda: f.read(1 << 20), b""):
                h.update(block)
    except OSError:
        return None
    return h.hexdigest()


class VersionStore:
    """The versions table and the blobs it points at."""

    def __init__(self, root: Path):
        self.root = Path(root)
        self.blobs = self.root / BLOBS_DIR_NAME
        self.db_path = str(self.root / VERSIONS_DB_NAME)
        self._lock = threading.Lock()
        self.blobs.mkdir(parents=True, exist_ok=True)
        with self._lock:
            conn = self._get_conn()
            conn.execute(CREATE_TABLE)
            conn.commit()
            conn.close()

    def _get_conn(self) -> sqlite3.Connection:
        from . import db

        return db.connect(self.db_path)

    # ---- keeping a version ----

    def snapshot(self, path: Path, note: str = "") -> Optional[dict]:
        """Keep what is at ``path`` before it is replaced.

        The content is stored once, named after its own hash. A file
        that is replaced by an identical copy does not produce a second
        copy of the same bytes.
        """
        path = Path(path)
        if not path.is_file():
            return None
        digest = hash_file(path)
        if not digest:
            return None
        size = path.stat().st_size

        blob = self.blobs / digest
        if not blob.exists():
            tmp = self.blobs / f".{digest}.tmp"
            try:
                shutil.copy2(path, tmp)
                tmp.replace(blob)
            except OSError as e:
                tmp.unlink(missing_ok=True)
                log.warning("could not store a version of %s: %s", path, e)
                return None

        now = int(time.time())
        with self._lock:
            conn = self._get_conn()
            try:
                cur = conn.execute(
                    "INSERT INTO versions (path, blob, size, created_at, note)"
                    " VALUES (?, ?, ?, ?, ?)",
                    (str(path), digest, size, now, note),
                )
                conn.commit()
                version_id = cur.lastrowid
            finally:
                conn.close()

        self._trim(path)
        return self.get(version_id)

    def _trim(self, path: Path) -> None:
        """Drop the oldest versions of a file beyond the limit."""
        with self._lock:
            conn = self._get_conn()
            try:
                rows = conn.execute(
                    "SELECT id FROM versions WHERE path = ? ORDER BY created_at DESC",
                    (str(path),),
                ).fetchall()
                stale = [r[0] for r in rows[MAX_VERSIONS_PER_FILE:]]
                if stale:
                    conn.executemany("DELETE FROM versions WHERE id = ?", [(i,) for i in stale])
                    conn.commit()
            finally:
                conn.close()
        self._collect_blobs()

    def _collect_blobs(self) -> None:
        """Remove blobs that no version refers to any more."""
        with self._lock:
            conn = self._get_conn()
            try:
                referenced = {r[0] for r in conn.execute("SELECT DISTINCT blob FROM versions")}
            finally:
                conn.close()
        try:
            for blob in self.blobs.iterdir():
                if blob.name.startswith("."):
                    continue
                if blob.name not in referenced:
                    blob.unlink(missing_ok=True)
        except OSError:
            pass

    # ---- reading ----

    def list(self, path: Path) -> list[dict]:
        """The versions of one file, newest first."""
        with self._lock:
            conn = self._get_conn()
            try:
                rows = conn.execute(
                    "SELECT id, blob, size, created_at, note FROM versions"
                    " WHERE path = ? ORDER BY created_at DESC",
                    (str(path),),
                ).fetchall()
            finally:
                conn.close()
        return [
            {
                "id": r[0],
                "size": r[2],
                "created_at": r[3],
                "note": r[4],
                "human": time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(r[3])),
            }
            for r in rows
        ]

    def get(self, version_id: int) -> Optional[dict]:
        with self._lock:
            conn = self._get_conn()
            try:
                row = conn.execute(
                    "SELECT id, path, blob, size, created_at, note FROM versions WHERE id = ?",
                    (version_id,),
                ).fetchone()
            finally:
                conn.close()
        if not row:
            return None
        return {
            "id": row[0],
            "path": row[1],
            "blob": row[2],
            "size": row[3],
            "created_at": row[4],
            "note": row[5],
            "human": time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(row[4])),
        }

    def restore(self, version_id: int, target: Optional[Path] = None) -> dict:
        """Put a version back.

        What is there now is itself kept as a version first, so
        restoring is something that can itself be undone.
        """
        version = self.get(version_id)
        if version is None:
            raise VersionError("找不到这个版本")
        blob = self.blobs / version["blob"]
        if not blob.exists():
            raise VersionError("这个版本的内容已经不在了")

        path = Path(target) if target else Path(version["path"])
        if path.exists():
            self.snapshot(path, note="还原前的内容")

        path.parent.mkdir(parents=True, exist_ok=True)
        tmp = path.with_name(f".{path.name}.restoring")
        try:
            shutil.copy2(blob, tmp)
            tmp.replace(path)
        except OSError as e:
            tmp.unlink(missing_ok=True)
            raise VersionError(f"还原失败：{e}") from e

        log.info("restored %s from version %s", path, version_id)
        return {"success": True, "path": str(path), "size": version["size"]}

    def delete(self, version_id: int) -> bool:
        with self._lock:
            conn = self._get_conn()
            try:
                cur = conn.execute("DELETE FROM versions WHERE id = ?", (version_id,))
                conn.commit()
                removed = cur.rowcount > 0
            finally:
                conn.close()
        if removed:
            self._collect_blobs()
        return removed

    def forget(self, path: Path) -> int:
        """Throw away the history of a file that is gone.

        A file that was destroyed for good will not be restored, so its
        old versions are dead weight: the blobs sit on the disk for
        nothing. A directory takes everything under it with it.

        Returns how many versions were removed.
        """
        target = str(Path(path))
        prefix = target.rstrip("/") + "/"
        with self._lock:
            conn = self._get_conn()
            try:
                rows = conn.execute(
                    "SELECT id, path FROM versions WHERE path = ? OR path LIKE ?",
                    (target, prefix + "%"),
                ).fetchall()
                for row in rows:
                    conn.execute("DELETE FROM versions WHERE id = ?", (row[0],))
                conn.commit()
            finally:
                conn.close()
        if rows:
            self._collect_blobs()
        return len(rows)

    def clear(self) -> int:
        """Remove every version, and the blobs that went with them.

        Returns how many versions were removed. Used when the history
        has to start over -- a test that wants a clean slate, or an
        operator who wants the disk space back.
        """
        with self._lock:
            conn = self._get_conn()
            try:
                count = conn.execute("SELECT COUNT(*) FROM versions").fetchone()[0]
                conn.execute("DELETE FROM versions")
                conn.commit()
            finally:
                conn.close()
        self._collect_blobs()
        return count

    def stats(self) -> dict:
        with self._lock:
            conn = self._get_conn()
            try:
                count, total = conn.execute(
                    "SELECT COUNT(*), COALESCE(SUM(size), 0) FROM versions"
                ).fetchone()
            finally:
                conn.close()
        return {"count": count, "size": total}


class VersionError(Exception):
    """Something went wrong with a version."""
