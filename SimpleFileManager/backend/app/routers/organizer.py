import os
import json
import uuid
import sqlite3
import hashlib
from pathlib import Path
from typing import Optional
from datetime import datetime, timedelta
from fastapi import APIRouter, HTTPException
from ..models import SnapshotRecord, FileSnapshot, FileChange, Suggestion, CompareResponse
from ..deps import state

organizer = APIRouter(prefix="/api/organizer", tags=["organizer"])


class OrganizerService:
    def __init__(self, db_path: str):
        self.db_path = db_path
        self._init_db()

    def _get_conn(self) -> sqlite3.Connection:
        return sqlite3.connect(self.db_path, check_same_thread=False)

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
        conn.commit()
        conn.close()

    def _scan_directory(self, root_path: str) -> list[dict]:
        files = []
        try:
            for entry in Path(root_path).rglob("*"):
                if ".simplefilemanager" in str(entry):
                    continue
                try:
                    stat = entry.stat()
                    files.append({
                        "path": str(entry),
                        "name": entry.name,
                        "is_dir": entry.is_dir(),
                        "size": stat.st_size if entry.is_file() else 0,
                        "modified": datetime.fromtimestamp(stat.st_mtime).isoformat(),
                    })
                except (PermissionError, OSError):
                    continue
        except Exception:
            pass
        return files

    def take_snapshot(self, root_path: str) -> SnapshotRecord:
        date = datetime.now().strftime("%Y-%m-%d")
        files = self._scan_directory(root_path)

        conn = self._get_conn()
        conn.execute("DELETE FROM file_snapshots WHERE date = ?", (date,))

        snapshots = []
        for f in files:
            sid = uuid.uuid4().hex
            conn.execute(
                "INSERT INTO file_snapshots (id, date, path, name, is_dir, size, modified) VALUES (?, ?, ?, ?, ?, ?, ?)",
                (sid, date, f["path"], f["name"], int(f["is_dir"]), f["size"], f["modified"])
            )
            snapshots.append(FileSnapshot(
                id=sid,
                path=f["path"],
                name=f["name"],
                is_dir=f["is_dir"],
                size=f["size"],
                modified=f["modified"],
                snapshot_date=date
            ))

        conn.commit()
        conn.close()

        return SnapshotRecord(
            id=date,
            date=date,
            total_files=sum(1 for f in files if not f["is_dir"]),
            total_dirs=sum(1 for f in files if f["is_dir"]),
            files=snapshots
        )

    def get_snapshot(self, date: str) -> Optional[SnapshotRecord]:
        conn = self._get_conn()
        rows = conn.execute(
            "SELECT id, date, path, name, is_dir, size, modified FROM file_snapshots WHERE date = ?",
            (date,)
        ).fetchall()
        conn.close()

        if not rows:
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
            files=snapshots
        )

    def get_latest_snapshot(self) -> Optional[SnapshotRecord]:
        conn = self._get_conn()
        row = conn.execute(
            "SELECT DISTINCT date FROM file_snapshots ORDER BY date DESC LIMIT 1"
        ).fetchone()
        conn.close()

        if row:
            return self.get_snapshot(row[0])
        return None

    def list_snapshots(self, limit: int = 30) -> list[dict]:
        conn = self._get_conn()
        rows = conn.execute(
            """SELECT date, COUNT(*) as file_count,
               SUM(CASE WHEN is_dir = 0 THEN 1 ELSE 0 END) as files,
               SUM(CASE WHEN is_dir = 1 THEN 1 ELSE 0 END) as dirs
               FROM file_snapshots GROUP BY date ORDER BY date DESC LIMIT ?""",
            (limit,)
        ).fetchall()
        conn.close()

        return [
            {"date": r[0], "file_count": r[1], "files": r[2], "dirs": r[3]}
            for r in rows
        ]

    def compare_snapshots(self, date_from: str, date_to: str) -> CompareResponse:
        snap_from = self.get_snapshot(date_from)
        snap_to = self.get_snapshot(date_to)

        if not snap_from or not snap_to:
            raise HTTPException(status_code=404, detail="Snapshot not found")

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

        for f in added_files:
            ext = Path(f.path).suffix.lower()

            if ext in [".jpg", ".jpeg", ".png", ".gif", ".webp", ".svg"]:
                suggestions.append(Suggestion(
                    id=uuid.uuid4().hex,
                    type="move",
                    priority="low",
                    message=f"图片文件建议移到 /photos 目录",
                    source_path=f.path,
                    target_path="/photos",
                    reason=f"根据文件类型 ({ext}) 判断"
                ))
            elif ext in [".mp4", ".avi", ".mkv", ".mov", ".wmv"]:
                suggestions.append(Suggestion(
                    id=uuid.uuid4().hex,
                    type="move",
                    priority="low",
                    message=f"视频文件建议移到 /videos 目录",
                    source_path=f.path,
                    target_path="/videos",
                    reason=f"根据文件类型 ({ext}) 判断"
                ))
            elif ext in [".pdf", ".doc", ".docx", ".txt", ".md"]:
                suggestions.append(Suggestion(
                    id=uuid.uuid4().hex,
                    type="move",
                    priority="low",
                    message=f"文档文件建议移到 /documents 目录",
                    source_path=f.path,
                    target_path="/documents",
                    reason=f"根据文件类型 ({ext}) 判断"
                ))
            elif ext in [".mp3", ".wav", ".flac", ".ogg"]:
                suggestions.append(Suggestion(
                    id=uuid.uuid4().hex,
                    type="move",
                    priority="low",
                    message=f"音频文件建议移到 /music 目录",
                    source_path=f.path,
                    target_path="/music",
                    reason=f"根据文件类型 ({ext}) 判断"
                ))
            elif ext in [".zip", ".tar", ".gz", ".rar", ".7z"]:
                suggestions.append(Suggestion(
                    id=uuid.uuid4().hex,
                    type="archive",
                    priority="medium",
                    message=f"压缩文件可能需要归档",
                    source_path=f.path,
                    target_path=None,
                    reason="压缩包建议统一归档管理"
                ))

        return suggestions

    def delete_old_snapshots(self, keep_days: int = 30):
        conn = self._get_conn()
        cutoff = (datetime.now() - timedelta(days=keep_days)).strftime("%Y-%m-%d")
        conn.execute("DELETE FROM file_snapshots WHERE date < ?", (cutoff,))
        conn.commit()
        conn.close()


def get_organizer_service() -> OrganizerService:
    from ..deps import DATA_DIR
    db_path = str(DATA_DIR / "organizer.db")
    return OrganizerService(db_path)


@organizer.post("/snapshot")
def take_snapshot():
    settings = state.get_settings()
    storage_path = settings.storage_path
    base_dir = Path(__file__).resolve().parent.parent.parent
    root_path = str(base_dir / storage_path)

    svc = get_organizer_service()
    return svc.take_snapshot(root_path)


@organizer.get("/snapshots")
def list_snapshots(limit: int = 30):
    svc = get_organizer_service()
    return {"snapshots": svc.list_snapshots(limit)}


@organizer.get("/compare")
def compare_snapshots(date_from: str, date_to: str):
    svc = get_organizer_service()
    return svc.compare_snapshots(date_from, date_to)


@organizer.get("/latest")
def get_latest():
    svc = get_organizer_service()
    snap = svc.get_latest_snapshot()
    if not snap:
        return {"has_snapshot": False}
    return {"has_snapshot": True, "date": snap.date, "files": snap.total_files, "dirs": snap.total_dirs}


@organizer.delete("/snapshots")
def cleanup_snapshots(keep_days: int = 30):
    svc = get_organizer_service()
    svc.delete_old_snapshots(keep_days)
    return {"success": True}
