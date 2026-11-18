import os
import json
import logging
import threading
import time
import uuid
import sqlite3
import hashlib
from pathlib import Path
from typing import Optional
from datetime import datetime, timedelta
from fastapi import APIRouter, Depends, HTTPException
from .. import db
from .auth import require_admin
from ..models import SnapshotRecord, FileSnapshot, FileChange, Suggestion, CompareResponse
from ..deps import state, get_storage_root

log = logging.getLogger(__name__)

organizer = APIRouter()

# How many entries one snapshot may hold. A snapshot of a very large
# library would otherwise walk it for minutes and write a row per
# file -- and the page that asks for it uses none of those rows.
#
# A snapshot that hit the limit says so, rather than pretending it
# saw everything: a snapshot that stops half way would otherwise make
# every file it missed look like a deletion.
MAX_SNAPSHOT_ENTRIES = 200_000

# How many rows are written at a time.
INSERT_BATCH = 2000


class OrganizerService:
    def __init__(self, db_path: str):
        self.db_path = db_path
        self._lock = threading.Lock()
        self._init_db()

    def _get_conn(self) -> sqlite3.Connection:
        return db.connect(self.db_path)

    def _init_db(self):
        conn = self._get_conn()
        conn.execute("""
            CREATE TABLE IF NOT EXISTS file_snapshots (
                id TEXT PRIMARY KEY,
                date TEXT NOT NULL,
                path TEXT NOT NULL,
                name TEXT NOT NULL,
                is_dir INTEGER NOT NULL,
                size INTEGER NOT NULL,
                modified TEXT NOT NULL
            )
        """)
        conn.execute("CREATE INDEX IF NOT EXISTS idx_snapshot_date ON file_snapshots(date)")
        conn.execute("CREATE INDEX IF NOT EXISTS idx_snapshot_path ON file_snapshots(path)")
        # The snapshots themselves are stored one row per file, so a
        # snapshot of an empty tree would leave no rows at all -- and
        # then it would not exist. That is exactly the snapshot you
        # want to compare against: the one you took before everything
        # was deleted. So the snapshots are listed in their own table.
        conn.execute("""
            CREATE TABLE IF NOT EXISTS snapshot_meta (
                date TEXT PRIMARY KEY,
                complete INTEGER NOT NULL DEFAULT 1,
                taken_at REAL NOT NULL
            )
        """)
        # Whether the snapshot saw everything. A snapshot that stopped
        # half way would make every file it missed look like a
        # deletion, so the fact is kept with it.
        cols = [r[1] for r in conn.execute("PRAGMA table_info(file_snapshots)")]
        if "complete" not in cols:
            conn.execute(
                "ALTER TABLE file_snapshots ADD COLUMN complete INTEGER NOT NULL DEFAULT 1"
            )
        conn.commit()
        conn.close()

    def _scan_directory(self, root_path: str) -> tuple[list[dict], bool]:
        """Everything under a directory, and whether that was all of it."""
        files = []
        truncated = False
        try:
            for entry in Path(root_path).rglob("*"):
                if ".simplefilemanager" in entry.parts:
                    continue
                if len(files) >= MAX_SNAPSHOT_ENTRIES:
                    truncated = True
                    break
                try:
                    stat = entry.stat()
                    files.append({
                        # Relative to the storage root, like every other path in
                        # the API. Absolute paths leaked the host layout into the
                        # UI and made snapshots non-portable.
                        "path": str(entry.relative_to(root_path)),
                        "name": entry.name,
                        "is_dir": entry.is_dir(),
                        "size": stat.st_size if entry.is_file() else 0,
                        "modified": datetime.fromtimestamp(stat.st_mtime).isoformat(),
                    })
                except (PermissionError, OSError):
                    continue
        except Exception:
            # A tree that cannot be walked whole is still worth a
            # snapshot of the part that could be read.
            truncated = True
        return files, truncated

    def take_snapshot(self, root_path: str) -> SnapshotRecord:
        # Seconds matter: the whole point of a snapshot is to diff two moments,
        # and keying on the day meant a second shot the same day silently
        # overwrote the first, so "snapshot -> add files -> snapshot again" was
        # impossible within one day.
        date = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
        files, truncated = self._scan_directory(root_path)
        total_files = sum(1 for f in files if not f["is_dir"])
        total_dirs = sum(1 for f in files if f["is_dir"])

        with db.write_lock(self._lock, what="快照表"):
            conn = self._get_conn()
            try:
                # Guard against a collision in the same second rather than
                # deleting the previous snapshot outright.
                if conn.execute(
                    "SELECT 1 FROM snapshot_meta WHERE date = ?", (date,)
                ).fetchone():
                    date = f"{date}:{uuid.uuid4().hex[:4]}"

                rows = [
                    (uuid.uuid4().hex, date, f["path"], f["name"],
                     int(f["is_dir"]), f["size"], f["modified"], 0 if truncated else 1)
                    for f in files
                ]
                # In batches: one statement per file is one statement
                # per file, and a library has a lot of files.
                for start in range(0, len(rows), INSERT_BATCH):
                    conn.executemany(
                        "INSERT INTO file_snapshots (id, date, path, name, is_dir, size,"
                        " modified, complete) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                        rows[start:start + INSERT_BATCH],
                    )
                # The snapshot itself is listed even when it has no
                # files -- an empty tree is a moment worth remembering.
                conn.execute(
                    "INSERT OR REPLACE INTO snapshot_meta (date, complete, taken_at)"
                    " VALUES (?, ?, ?)",
                    (date, 0 if truncated else 1, time.time()),
                )
                conn.commit()
            finally:
                conn.close()

        if truncated:
            log.warning("the snapshot of %s stopped at %d entries; it is not complete",
                        root_path, len(files))
        return SnapshotRecord(
            id=date,
            date=date,
            total_files=total_files,
            total_dirs=total_dirs,
            # The list is not returned: the page that asks for a snapshot
            # wants the counts, and the rows are here for the diff later.
            files=[],
            truncated=truncated,
        )

    def get_snapshot(self, date: str) -> Optional[SnapshotRecord]:
        conn = self._get_conn()
        rows = conn.execute(
            "SELECT id, date, path, name, is_dir, size, modified, complete"
            " FROM file_snapshots WHERE date = ?",
            (date,)
        ).fetchall()
        meta = conn.execute(
            "SELECT complete FROM snapshot_meta WHERE date = ?", (date,)
        ).fetchone()
        conn.close()

        if meta is None:
            return None

        snapshots = []
        for row in rows:
            snapshots.append(FileSnapshot(
                id=row[0],
                path=row[2],
                name=row[3],
                is_dir=bool(row[4]),
                size=row[5],
                modified=row[6],
                snapshot_date=row[1]
            ))

        return SnapshotRecord(
            id=date,
            date=date,
            total_files=sum(1 for s in snapshots if not s.is_dir),
            total_dirs=sum(1 for s in snapshots if s.is_dir),
            files=snapshots,
            truncated=not meta[0],
        )

    def get_latest_snapshot(self) -> Optional[SnapshotRecord]:
        """The most recent snapshot, as counts.

        The page that asks wants the counts and the date, so they are
        counted in the database. Reading the whole snapshot and
        throwing all but two numbers away meant a library with fifty
        thousand files built fifty thousand objects for every page
        load.
        """
        conn = self._get_conn()
        row = conn.execute(
            "SELECT m.date,"
            " SUM(CASE WHEN f.is_dir = 0 THEN 1 ELSE 0 END),"
            " SUM(CASE WHEN f.is_dir = 1 THEN 1 ELSE 0 END),"
            " m.complete"
            " FROM snapshot_meta m"
            " LEFT JOIN file_snapshots f ON f.date = m.date"
            " GROUP BY m.date ORDER BY m.taken_at DESC, m.date DESC LIMIT 1"
        ).fetchone()
        conn.close()

        if not row:
            return None
        return SnapshotRecord(
            id=row[0],
            date=row[0],
            total_files=int(row[1] or 0),
            total_dirs=int(row[2] or 0),
            files=[],
            truncated=not row[3],
        )

    def list_snapshots(self, limit: int = 30) -> list[dict]:
        conn = self._get_conn()
        rows = conn.execute(
            """SELECT m.date,
               SUM(CASE WHEN f.is_dir = 0 THEN 1 ELSE 0 END) as files,
               SUM(CASE WHEN f.is_dir = 1 THEN 1 ELSE 0 END) as dirs,
               COUNT(f.id) as file_count
               FROM snapshot_meta m
               LEFT JOIN file_snapshots f ON f.date = m.date
               GROUP BY m.date ORDER BY m.taken_at DESC, m.date DESC LIMIT ?""",
            (limit,)
        ).fetchall()
        conn.close()

        return [
            {"date": r[0], "file_count": r[3], "files": r[1] or 0, "dirs": r[2] or 0}
            for r in rows
        ]

    def compare_snapshots(self, date_from: str, date_to: str) -> CompareResponse:
        snap_from = self.get_snapshot(date_from)
        snap_to = self.get_snapshot(date_to)

        if not snap_from or not snap_to:
            raise HTTPException(status_code=404, detail="找不到这份快照")

        if snap_from.truncated or snap_to.truncated:
            # One of the two did not see everything, so the files it
            # missed look like files that were deleted. Saying so is
            # better than a diff that invents a thousand deletions.
            raise HTTPException(
                status_code=400,
                detail="这两份快照里有一份不是完整的（拍的时候超出了上限），"
                       "比出来的结果不可信，请重新拍一次",
            )

        files_from = {f.path: f for f in snap_from.files}
        files_to = {f.path: f for f in snap_to.files}

        added_files = []
        added_dirs = []
        deleted_files = []
        deleted_dirs = []

        for path, f in files_to.items():
            if path not in files_from:
                change = FileChange(
                    path=path,
                    name=f.name,
                    change_type="added",
                    size=f.size,
                    modified=f.modified
                )
                if f.is_dir:
                    added_dirs.append(change)
                else:
                    added_files.append(change)

        for path, f in files_from.items():
            if path not in files_to:
                change = FileChange(
                    path=path,
                    name=f.name,
                    change_type="deleted",
                    size=f.size,
                    modified=f.modified
                )
                if f.is_dir:
                    deleted_dirs.append(change)
                else:
                    deleted_files.append(change)

        suggestions = self._generate_suggestions(added_files, added_dirs)

        return CompareResponse(
            date_from=date_from,
            date_to=date_to,
            added_files=added_files,
            added_dirs=added_dirs,
            deleted_files=deleted_files,
            deleted_dirs=deleted_dirs,
            suggestions=suggestions
        )

    def _generate_suggestions(self, added_files: list[FileChange], added_dirs: list[FileChange]) -> list[Suggestion]:
        suggestions = []

        # added_dirs was accepted but never iterated, so a brand-new folder
        # produced no suggestion at all -- yet an empty or stale folder is
        # exactly the kind of thing this feature should flag.
        for d in added_dirs:
            suggestions.append(Suggestion(
                id=uuid.uuid4().hex,
                # Deliberately not "move"/"create_folder": the UI turns any
                # non-move type into a create_folder plan action, which would
                # actually create the directory when approved. A new folder
                # needs review, not action.
                type="review",
                priority="medium",
                message=f"新增目录 {d.path}/ 待确认",
                source_path=d.path,
                target_path=None,
                reason="新增目录需要确认用途是否合适",
            ))

        for f in added_files:
            ext = Path(f.path).suffix.lower()

            if ext in [".jpg", ".jpeg", ".png", ".gif", ".webp", ".svg"]:
                suggestions.append(Suggestion(
                    id=uuid.uuid4().hex,
                    type="move",
                    priority="low",
                    message="图片文件建议移到 /photos 目录",
                    source_path=f.path,
                    target_path="/photos",
                    reason=f"根据文件类型 ({ext}) 判断"
                ))
            elif ext in [".mp4", ".avi", ".mkv", ".mov", ".wmv"]:
                suggestions.append(Suggestion(
                    id=uuid.uuid4().hex,
                    type="move",
                    priority="low",
                    message="视频文件建议移到 /videos 目录",
                    source_path=f.path,
                    target_path="/videos",
                    reason=f"根据文件类型 ({ext}) 判断"
                ))
            elif ext in [".pdf", ".doc", ".docx", ".txt", ".md"]:
                suggestions.append(Suggestion(
                    id=uuid.uuid4().hex,
                    type="move",
                    priority="low",
                    message="文档文件建议移到 /documents 目录",
                    source_path=f.path,
                    target_path="/documents",
                    reason=f"根据文件类型 ({ext}) 判断"
                ))
            elif ext in [".mp3", ".wav", ".flac", ".ogg"]:
                suggestions.append(Suggestion(
                    id=uuid.uuid4().hex,
                    type="move",
                    priority="low",
                    message="音频文件建议移到 /music 目录",
                    source_path=f.path,
                    target_path="/music",
                    reason=f"根据文件类型 ({ext}) 判断"
                ))
            elif ext in [".zip", ".tar", ".gz", ".rar", ".7z"]:
                suggestions.append(Suggestion(
                    id=uuid.uuid4().hex,
                    type="archive",
                    priority="medium",
                    message="压缩文件可能需要归档",
                    source_path=f.path,
                    target_path=None,
                    reason="压缩包建议统一归档管理"
                ))

        return suggestions

    def delete_old_snapshots(self, keep_days: int = 30):
        # Snapshots are keyed "YYYY-MM-DD HH:MM:SS" now, so the cutoff has to
        # carry a time too or every snapshot from the cutoff day would be kept
        # (its date string is lexicographically greater than a bare day).
        #
        # The cutoff is the *start* of the day, not the end: with the end
        # of the day, "keep 30 days" threw away the thirtieth day as
        # well, so it kept twenty-nine.
        cutoff = (datetime.now() - timedelta(days=max(0, int(keep_days)))).strftime("%Y-%m-%d 00:00:00")
        with db.write_lock(self._lock, what="快照表"):
            conn = self._get_conn()
            try:
                conn.execute("DELETE FROM file_snapshots WHERE date < ?", (cutoff,))
                conn.execute("DELETE FROM snapshot_meta WHERE date < ?", (cutoff,))
                conn.commit()
            finally:
                conn.close()


def get_organizer_service() -> OrganizerService:
    from ..deps import DATA_DIR
    db_path = str(DATA_DIR / "organizer.db")
    return OrganizerService(db_path)


@organizer.post("/snapshot")
def take_snapshot(_admin=Depends(require_admin)):
    root_path = str(get_storage_root())

    svc = get_organizer_service()
    return svc.take_snapshot(root_path)


@organizer.get("/snapshots")
def list_snapshots(limit: int = 30, _admin=Depends(require_admin)):
    svc = get_organizer_service()
    return {"snapshots": svc.list_snapshots(limit)}


@organizer.get("/compare")
def compare_snapshots(date_from: str, date_to: str, _admin=Depends(require_admin)):
    svc = get_organizer_service()
    return svc.compare_snapshots(date_from, date_to)


@organizer.get("/latest")
def get_latest(_admin=Depends(require_admin)):
    svc = get_organizer_service()
    snap = svc.get_latest_snapshot()
    if not snap:
        return {"has_snapshot": False}
    return {"has_snapshot": True, "date": snap.date, "files": snap.total_files, "dirs": snap.total_dirs}


@organizer.delete("/snapshots")
def cleanup_snapshots(keep_days: int = 30, _admin=Depends(require_admin)):
    svc = get_organizer_service()
    svc.delete_old_snapshots(keep_days)
    return {"success": True}
