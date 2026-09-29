"""Polling-based auto indexer.

Periodically scans the storage root, diffs against a local SQLite manifest,
and pushes new/changed text files into the vector store. Files written
within the debounce window are deferred to the next cycle to avoid indexing
half-written content.
"""

import sqlite3
import threading
import time
import traceback
from datetime import datetime
from pathlib import Path
from typing import Optional

from .deps import DATA_DIR, get_storage_root, state
from .models import AutoIndexStatus
from .search_engine import search_engine

TEXT_EXTENSIONS = {
    ".txt", ".md", ".markdown", ".rst", ".log",
    ".json", ".yaml", ".yml", ".toml", ".ini", ".cfg", ".csv",
    ".py", ".js", ".ts", ".tsx", ".jsx", ".rs", ".go", ".java",
    ".c", ".h", ".cpp", ".hpp", ".cs", ".rb", ".php", ".sh", ".bash",
    ".html", ".css", ".scss", ".vue", ".sql",
}

MAX_FILE_BYTES = 512 * 1024  # skip files larger than 512KB
EMBED_BATCH_SIZE = 16
TICK_SECONDS = 5
MIN_INTERVAL_SECONDS = 30


class AutoIndexer:
    def __init__(self):
        self._stop_event = threading.Event()
        self._thread: Optional[threading.Thread] = None
        self._db_path = str(DATA_DIR / "file_index.db")
        self._last_scan_ts = 0.0
        self._last_scan_time: Optional[str] = None
        self._last_scan_files = 0
        self._pending_changes = 0
        self._init_db()

    # ---- manifest db ----

    def _get_conn(self) -> sqlite3.Connection:
        return sqlite3.connect(self._db_path, check_same_thread=False)

    def _init_db(self):
        conn = self._get_conn()
        conn.execute("""
            CREATE TABLE IF NOT EXISTS file_index (
                path TEXT PRIMARY KEY,
                mtime REAL NOT NULL,
                size INTEGER NOT NULL,
                indexed_at INTEGER NOT NULL
            )
        """)
        conn.commit()
        conn.close()

    def _known_files(self) -> dict[str, tuple[float, int]]:
        conn = self._get_conn()
        rows = conn.execute("SELECT path, mtime, size FROM file_index").fetchall()
        conn.close()
        return {r[0]: (r[1], r[2]) for r in rows}

    def _record_files(self, records: list[tuple[str, float, int]]):
        conn = self._get_conn()
        now = int(time.time())
        conn.executemany(
            "INSERT OR REPLACE INTO file_index (path, mtime, size, indexed_at) VALUES (?, ?, ?, ?)",
            [(p, m, s, now) for p, m, s in records]
        )
        conn.commit()
        conn.close()

    def _remove_files(self, paths: list[str]):
        conn = self._get_conn()
        conn.executemany("DELETE FROM file_index WHERE path = ?", [(p,) for p in paths])
        conn.commit()
        conn.close()

    # ---- lifecycle ----

    def start(self):
        if self._thread and self._thread.is_alive():
            return
        self._stop_event.clear()
        self._thread = threading.Thread(target=self._run, name="auto-indexer", daemon=True)
        self._thread.start()

    def stop(self):
        self._stop_event.set()

    def _run(self):
        while not self._stop_event.wait(TICK_SECONDS):
            try:
                self._tick()
            except Exception:
                traceback.print_exc()

    def _tick(self):
        settings = state.get_settings()
        if not settings.auto_index_enabled:
            self._pending_changes = 0
            return
        if state.get_index_status().is_indexing:
            return
        interval = max(MIN_INTERVAL_SECONDS, settings.index_interval)
        if time.time() - self._last_scan_ts < interval:
            return
        self._last_scan_ts = time.time()
        self.scan()

    # ---- scanning ----

    def _iter_text_files(self, root: Path) -> dict[str, tuple[float, int]]:
        found: dict[str, tuple[float, int]] = {}
        try:
            for entry in root.rglob("*"):
                if ".simplefilemanager" in entry.parts or entry.name.startswith("."):
                    continue
                if not entry.is_file() or entry.suffix.lower() not in TEXT_EXTENSIONS:
                    continue
                try:
                    stat = entry.stat()
                    if stat.st_size > MAX_FILE_BYTES or stat.st_size == 0:
                        continue
                    found[str(entry)] = (stat.st_mtime, stat.st_size)
                except (PermissionError, OSError):
                    continue
        except (PermissionError, OSError):
            pass
        return found

    def _read_content(self, path: str) -> Optional[str]:
        try:
            with open(path, "r", encoding="utf-8", errors="ignore") as f:
                return f.read(MAX_FILE_BYTES)
        except (PermissionError, OSError):
            return None

    def scan(self) -> dict:
        """Run one full incremental indexing pass. Returns a summary dict."""
        root = get_storage_root()
        debounce = max(0, state.get_settings().index_debounce_seconds)
        now = time.time()

        state.set_indexing(True, 0.0, str(root))
        try:
            current = self._iter_text_files(root)
            known = self._known_files()

            to_index = []
            pending = 0
            for path, (mtime, size) in current.items():
                if path in known and known[path][0] == mtime and known[path][1] == size:
                    continue
                if now - mtime < debounce:
                    pending += 1
                    continue
                to_index.append(path)
            self._pending_changes = pending

            removed = [p for p in known if p not in current]

            rag_svc = state.get_rag_service()
            indexed, failed = 0, 0

            for i in range(0, len(to_index), EMBED_BATCH_SIZE):
                batch_paths = to_index[i:i + EMBED_BATCH_SIZE]
                contents = {p: self._read_content(p) for p in batch_paths}
                valid = [(p, contents[p]) for p in batch_paths if contents[p]]
                if not valid:
                    continue
                try:
                    vectors = rag_svc.embedding_service.embed([c for _, c in valid])
                except Exception:
                    failed += len(valid)
                    traceback.print_exc()
                    continue
                records = []
                for (path, content), vec in zip(valid, vectors):
                    try:
                        rag_svc.vector_store.add(vec, {"file_path": path, "content": content})
                        search_engine.upsert_file(path, content)
                        stat = Path(path).stat()
                        records.append((path, stat.st_mtime, stat.st_size))
                        indexed += 1
                    except Exception:
                        failed += 1
                        traceback.print_exc()
                if records:
                    self._record_files(records)

            if removed:
                for p in removed:
                    try:
                        rag_svc.vector_store.delete_by_file(p)
                    except Exception:
                        traceback.print_exc()
                    try:
                        search_engine.remove_file(p)
                    except Exception:
                        traceback.print_exc()
                self._remove_files(removed)

            self._last_scan_time = datetime.now().strftime("%Y-%m-%dT%H:%M:%S")
            self._last_scan_files = len(current)

            state.update_index_stats({
                "indexed_files": self._known_count(),
                "last_index_time": self._last_scan_time,
            })

            return {
                "scanned": len(current),
                "indexed": indexed,
                "failed": failed,
                "removed": len(removed),
                "pending": pending,
            }
        finally:
            state.set_indexing(False, 100.0)

    def _known_count(self) -> int:
        conn = self._get_conn()
        row = conn.execute("SELECT COUNT(*) FROM file_index").fetchone()
        conn.close()
        return row[0] if row else 0

    def status(self) -> AutoIndexStatus:
        settings = state.get_settings()
        return AutoIndexStatus(
            enabled=settings.auto_index_enabled,
            running=bool(self._thread and self._thread.is_alive()),
            interval=max(MIN_INTERVAL_SECONDS, settings.index_interval),
            debounce_seconds=settings.index_debounce_seconds,
            last_scan_time=self._last_scan_time,
            last_scan_files=self._last_scan_files,
            indexed_files=self._known_count(),
            pending_changes=self._pending_changes,
        )


indexer = AutoIndexer()
