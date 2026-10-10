"""Polling-based auto indexer.

Periodically scans the storage root, diffs against a local SQLite manifest,
and pushes new/changed text files into the vector store. Files written
within the debounce window are deferred to the next cycle to avoid indexing
half-written content. PDF and DOCX are parsed by doc_extract rather than read
as text.
"""

import logging
import sqlite3
import threading
import time
import traceback
from datetime import datetime
from pathlib import Path
from typing import Optional

from .chunking import chunk_file
from .interaction import interaction
from .deps import DATA_DIR, EMBED_CHUNK_BATCH, get_storage_root, state
from .doc_extract import DOCUMENT_EXTENSIONS, MAX_DOCUMENT_BYTES, ExtractionError, extract_text
from .models import AutoIndexStatus
from .search_engine import search_engine

log = logging.getLogger(__name__)


def _display_path(path: Path) -> str:
    """Path relative to the storage root, for log lines and API responses that
    must not leak absolute filesystem layout."""
    try:
        return str(path.relative_to(get_storage_root()))
    except (ValueError, OSError):
        return path.name


def _is_hidden(entry: Path) -> bool:
    return ".simplefilemanager" in entry.parts or entry.name.startswith(".")

TEXT_EXTENSIONS = {
    ".txt", ".md", ".markdown", ".rst", ".log",
    ".json", ".yaml", ".yml", ".toml", ".ini", ".cfg", ".csv",
    ".py", ".js", ".ts", ".tsx", ".jsx", ".rs", ".go", ".java",
    ".c", ".h", ".cpp", ".hpp", ".cs", ".rb", ".php", ".sh", ".bash",
    ".html", ".css", ".scss", ".vue", ".sql",
} | DOCUMENT_EXTENSIONS

INDEXABLE_EXTENSIONS = TEXT_EXTENSIONS

MAX_FILE_BYTES = 10 * 1024 * 1024  # plain-text ceiling before a file is skipped
EMBED_BATCH_SIZE = 16
# Chunks per file sent to the embedding service. The whole file is still
# indexed for keyword search; this only bounds semantic coverage.
# Chunks embedded per file per scan. The semantic pass resumes from the stored
# position every scan, so this only bounds how long one scan takes — it is not
# a cap on total coverage.
EMBED_CHUNKS_PER_PASS = 16
TICK_SECONDS = 5
MIN_INTERVAL_SECONDS = 30
# After standing down for the user, retry this soon once they go idle. A full
# index_interval wait would make the resume feel broken.
YIELD_RETRY_SECONDS = 45


class AutoIndexer:
    def __init__(self):
        self._stop_event = threading.Event()
        self._thread: Optional[threading.Thread] = None
        self._db_path = str(DATA_DIR / "file_index.db")
        self._last_scan_ts = 0.0
        self._last_scan_time: Optional[str] = None
        self._last_scan_files = 0
        self._pending_changes = 0
        # Files present in storage that never reach the index: unsupported
        # extension vs. recognised but unparseable. Surfaced in the UI so a
        # missing search hit is explainable.
        self._unsupported: dict[str, str] = {}
        self._oversized: dict[str, str] = {}
        # path -> number of chunks currently in the vector store
        self._vector_partial: dict[str, int] = {}
        self._pending_vector_files = 0
        # True when the last scan stood down because the user was interacting
        self._yielded_for_interaction = False
        self._failed: dict[str, str] = {}
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
        # How many chunks of each file are already in the vector store. Kept in
        # the manifest so a slow embedding pass survives restarts and resumes
        # where it stopped instead of restarting from zero every scan.
        cols = [r[1] for r in conn.execute("PRAGMA table_info(file_index)")]
        if "vec_done" not in cols:
            conn.execute("ALTER TABLE file_index ADD COLUMN vec_done INTEGER NOT NULL DEFAULT 0")
        if "chunk_count" not in cols:
            conn.execute("ALTER TABLE file_index ADD COLUMN chunk_count INTEGER NOT NULL DEFAULT 0")
        conn.commit()
        conn.close()

    def _known_files(self) -> dict[str, tuple[float, int]]:
        conn = self._get_conn()
        rows = conn.execute("SELECT path, mtime, size FROM file_index").fetchall()
        conn.close()
        return {r[0]: (r[1], r[2]) for r in rows}

    def _vector_progress(self) -> dict[str, tuple[int, int]]:
        """path -> (chunks already embedded, total chunks)."""
        conn = self._get_conn()
        rows = conn.execute("SELECT path, vec_done, chunk_count FROM file_index").fetchall()
        conn.close()
        return {r[0]: (r[1], r[2]) for r in rows}

    def _record_files(self, records: list[tuple[str, float, int, int]]):
        conn = self._get_conn()
        now = int(time.time())
        conn.executemany(
            "INSERT OR REPLACE INTO file_index (path, mtime, size, indexed_at, chunk_count) "
            "VALUES (?, ?, ?, ?, ?)",
            # vec_done is not carried on a re-index: a changed file restarts its
            # vector pass from zero.
            [(p, m, s, now, n) for p, m, s, n in records]
        )
        conn.commit()
        conn.close()

    def _set_vector_progress(self, path: str, done: int):
        conn = self._get_conn()
        conn.execute("UPDATE file_index SET vec_done = ? WHERE path = ?", (done, path))
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
        # A scan that stood down for the user is not really done. Waiting out
        # the full interval would leave "resumes when idle" feeling broken, so
        # retry as soon as the idle window has passed instead.
        if self._yielded_for_interaction:
            if interaction.is_busy() or time.time() - self._last_scan_ts < YIELD_RETRY_SECONDS:
                return
        elif time.time() - self._last_scan_ts < interval:
            return
        self._last_scan_ts = time.time()
        self.scan()

    # ---- scanning ----

    def _iter_text_files(self, root: Path) -> dict[str, tuple[float, int]]:
        found: dict[str, tuple[float, int]] = {}
        unsupported: dict[str, str] = {}
        oversized: dict[str, str] = {}
        try:
            for entry in root.rglob("*"):
                if _is_hidden(entry):
                    continue
                if not entry.is_file():
                    continue
                if entry.suffix.lower() not in INDEXABLE_EXTENSIONS:
                    # Files we simply cannot parse. Counted so the user can see
                    # why a document they stored never turns up in search.
                    unsupported[_display_path(entry)] = entry.suffix.lower()
                    continue
                try:
                    stat = entry.stat()
                    # PDFs are mostly images, so the plain-text byte ceiling
                    # would reject a small file full of readable content.
                    limit = MAX_DOCUMENT_BYTES if entry.suffix.lower() in DOCUMENT_EXTENSIONS else MAX_FILE_BYTES
                    if stat.st_size == 0:
                        continue
                    if stat.st_size > limit:
                        # Reported, not silently dropped: a user who indexed a
                        # whole library and got no hits needs to know why.
                        oversized[_display_path(entry)] = f"{stat.st_size // 1024 // 1024}MB"
                        continue
                    found[str(entry)] = (stat.st_mtime, stat.st_size)
                except (PermissionError, OSError):
                    continue
        except (PermissionError, OSError):
            pass
        self._unsupported = unsupported
        self._oversized = oversized
        return found

    def _read_content(self, path: str) -> Optional[str]:
        target = Path(path)
        try:
            extracted = extract_text(target)
        except ExtractionError as e:
            # Recognised format but unreadable (scanned, encrypted, corrupt).
            # Tracked so the UI can report it; skipping keeps the scan alive
            # because one bad file must not stall the whole pass.
            rel = _display_path(target)
            self._failed[rel] = str(e)
            log.info("skipping unextractable document %s: %s", rel, e)
            return None
        except Exception as e:
            rel = _display_path(target)
            self._failed[rel] = f"解析异常：{e}"
            log.warning("unexpected extraction failure for %s", rel, exc_info=True)
            return None

        if extracted is not None:
            return extracted

        try:
            with open(target, "r", encoding="utf-8", errors="ignore") as f:
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
            self._failed = {}
            self._oversized = {}
            self._vector_partial = {}
            self._yielded_for_interaction = False
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
            records: list[tuple[str, float, int, int]] = []

            # Phase 1 — keyword channel. Cheap and complete: FTS only needs the
            # file text, so every file becomes searchable immediately. Done
            # before any embedding so a slow or unreachable embedding service
            # can never block keyword indexing.
            for i in range(0, len(to_index), EMBED_BATCH_SIZE):
                for path in to_index[i:i + EMBED_BATCH_SIZE]:
                    content = self._read_content(path)
                    if not content:
                        continue
                    try:
                        pieces = chunk_file(content)
                        search_engine.upsert_file(path, content)
                        stat = Path(path).stat()
                        records.append((path, stat.st_mtime, stat.st_size, len(pieces)))
                        indexed += 1
                    except Exception:
                        failed += 1
                        traceback.print_exc()
            if records:
                self._record_files(records)

            # Phase 2 — semantic channel, resumable. Progress lives in the
            # manifest, so a file needing hours to vectorize converges over
            # repeated scans and survives a restart, instead of being abandoned
            # half-done or re-embedded from zero. Driven off every known file,
            # not just this pass's, so earlier passes resume correctly.
            progress = self._vector_progress()
            pending_vectors = [
                p for p in current
                if progress.get(p, (0, 1))[0] < progress.get(p, (0, 1))[1]
                and progress.get(p, (0, 0))[1] > 0
            ]
            self._pending_vector_files = len(pending_vectors)

            for path in pending_vectors:
                # Chat and embedding contend for the same four fast cores: a
                # chat request measures ~27 tok/s idle but ~4 tok/s while this
                # runs. Indexing is resumable, so it stands down and resumes on
                # a later tick instead of degrading the conversation.
                if interaction.is_busy():
                    self._yielded_for_interaction = True
                    break
                done, total = progress[path]
                content = self._read_content(path)
                if not content:
                    continue
                pieces = chunk_file(content)
                batch = pieces[done: done + EMBED_CHUNKS_PER_PASS]
                if not batch:
                    continue
                try:
                    chunk_records = [(path, idx, text) for idx, (_off, text) in enumerate(batch)]
                    for j in range(0, len(chunk_records), EMBED_CHUNK_BATCH):
                        # Checked per batch, not just per file: one file is
                        # ~16 chunks, so a file-level check could sit on the
                        # cores for a minute after the user started talking.
                        if interaction.is_busy():
                            self._set_vector_progress(path, done + j)
                            self._vector_partial[path] = done + j
                            self._yielded_for_interaction = True
                            break
                        part = chunk_records[j: j + EMBED_CHUNK_BATCH]
                        vectors = rag_svc.embedding_service.embed([c for _, _, c in part])
                        rag_svc.vector_store.add_many(
                            vectors,
                            [{"file_path": p, "content": c, "chunk_idx": i} for p, i, c in part],
                        )
                    new_done = min(done + len(batch), total)
                    self._set_vector_progress(path, new_done)
                    self._vector_partial[path] = new_done
                except Exception:
                    traceback.print_exc()

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

    def skipped_files(self) -> dict:
        """Files in storage that are absent from the index, split by cause.

        `unsupported` maps path -> extension, `failed` maps path -> reason,
        `partial_vector` maps path -> chunks embedded so far. Paths are relative
        to the storage root.
        """
        progress = self._vector_progress()
        partial = []
        for path, (done, total) in sorted(progress.items()):
            if total > 0 and done < total:
                partial.append({
                    "path": _display_path(Path(path)),
                    "done": done,
                    "total": total,
                    "percent": round(done * 100 / total),
                })
        return {
            "unsupported": [
                {"path": p, "ext": ext or ""} for p, ext in sorted(self._unsupported.items())
            ],
            "failed": [
                {"path": p, "reason": reason} for p, reason in sorted(self._failed.items())
            ],
            "oversized": [
                {"path": p, "size": size} for p, size in sorted(self._oversized.items())
            ],
            "partial_vector": partial,
        }

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
            pending_vector_files=self._pending_vector_files,
            yielded_for_interaction=self._yielded_for_interaction,
        )


indexer = AutoIndexer()
