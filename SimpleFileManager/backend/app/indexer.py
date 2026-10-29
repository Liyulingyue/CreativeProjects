"""Polling-based auto indexer.

Periodically scans the storage root, diffs against a local SQLite manifest,
and pushes new/changed text files into the vector store. Files written
within the debounce window are deferred to the next cycle to avoid indexing
half-written content. PDF and DOCX are parsed by doc_extract rather than read
as text.
"""

import logging
import hashlib
import os
import sqlite3
import threading
import time
from datetime import datetime
from pathlib import Path
from typing import Optional

from . import db
from .chunking import DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP, chunk_file
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


# Windows-Update style scheduling: an active-hours window during which
# background indexing may run. "all" means no restriction. The window may wrap
# past midnight (e.g. 23:00-07:00), which is the common case for a NAS that
# should stay quiet during the day.
ACTIVE_HOURS_ALL = "all"


def _in_active_hours(window: str, now: Optional[datetime] = None) -> bool:
    if not window or window == ACTIVE_HOURS_ALL:
        return True
    try:
        start_s, end_s = window.split("-")
        sh, sm = (int(x) for x in start_s.strip().split(":"))
        eh, em = (int(x) for x in end_s.strip().split(":"))
    except (ValueError, AttributeError):
        log.warning("invalid index active hours %r, treating as all-day", window)
        return True

    t = (now or datetime.now())
    cur = t.hour * 60 + t.minute
    start = sh * 60 + sm
    end = eh * 60 + em
    if start == end:
        return True
    if start < end:
        return start <= cur < end
    # wraps past midnight
    return cur >= start or cur < end


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
# Chunks embedded per scan before yielding to the tick. The pass size is not a
# throughput knob (batching already amortizes the per-call cost), it only sets
# how much work happens between re-checks of the pause/interaction state.
EMBED_CHUNKS_PER_PASS = 48

# Stored alongside each file so a change to the chunking parameters invalidates
# prior progress instead of silently disagreeing with it.
CHUNK_SIG = f"{DEFAULT_CHUNK_SIZE}-{DEFAULT_OVERLAP}"
TICK_SECONDS = 5
MIN_INTERVAL_SECONDS = 30
# After standing down for the user, retry this soon once they go idle. A full
# index_interval wait would make the resume feel broken.
YIELD_RETRY_SECONDS = 45

# How many unchanged-looking files to hash per scan, to check whether
# their contents were replaced behind the indexer's back. Hashing reads
# the whole file, so this is bounded and the rest are checked on later
# scans.
VERIFY_BUDGET = 50


class AutoIndexer:
    def __init__(self):
        self._stop_event = threading.Event()
        self._thread: Optional[threading.Thread] = None
        self._db_path = str(DATA_DIR / "file_index.db")
        self._last_scan_ts = 0.0
        self._last_scan_time: Optional[str] = None
        self._last_scan_files = 0
        # Set when a pass ended with chunks still to embed, so the next tick
        # picks straight back up instead of waiting out index_interval.
        self._continue_pending = False
        self._scan_lock = threading.Lock()
        self._last_enumeration_complete = True
        self._present_paths: set[str] = set()
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
        self._scan_thread: Optional[threading.Thread] = None
        # Manual pause, set by the API. `None` means not paused, an int is a
        # unix timestamp after which indexing resumes on its own, and -1 is a
        # pause with no automatic expiry.
        self._paused_until: Optional[float] = None
        self._failed: dict[str, str] = {}
        # The manifest is written by the scanner and read by the digest
        # service and the API. WAL keeps the readers out of the writer's
        # way; this lock keeps two scans from writing it at once.
        self._db_lock = db.DatabaseLock()
        self._init_db()

    # ---- manifest db ----

    def _get_conn(self) -> sqlite3.Connection:
        return db.connect(self._db_path)

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
        if "chunk_sig" not in cols:
            conn.execute("ALTER TABLE file_index ADD COLUMN chunk_sig TEXT NOT NULL DEFAULT ''")
        # SHA-256 of file content, populated lazily on scan. Used by the
        # move-detection pass to recognise a file that has been renamed to
        # a different path: instead of dropping the old index and re-embedding
        # the new one, we just rewrite the path in the manifest, the FTS
        # table, and the vector store. NULL is allowed and means "not yet
        # computed"; the next scan fills it in.
        if "content_hash" not in cols:
            conn.execute("ALTER TABLE file_index ADD COLUMN content_hash TEXT")
        conn.commit()
        conn.close()

    def _known_files(self) -> dict[str, tuple[float, int, Optional[str], int]]:
        """path -> (mtime, size, content_hash, indexed_at).

        content_hash is the SHA-256 of the file at last indexing, used by
        the move-detection pass to recognise that a file has been renamed
        rather than deleted-and-added. Hashes are populated lazily on
        first scan after the column was added.

        indexed_at is when the file was last indexed. It is what makes a
        file whose contents changed behind the indexer's back
        detectable: mtime and size survive a restore, so the timestamp
        of the inode does not.
        """
        with self._db_lock:
            conn = self._get_conn()
            try:
                rows = conn.execute(
                    "SELECT path, mtime, size, content_hash, indexed_at FROM file_index"
                ).fetchall()
            finally:
                conn.close()
        return {r[0]: (r[1], r[2], r[3], r[4]) for r in rows}

    def _compute_hash(self, abs_path: str) -> Optional[str]:
        """SHA-256 of file content, or None on read failure.

        We hash the raw bytes (not the extracted text) so two files that
        parse to the same text but differ in binary form — e.g. an updated
        PDF — still hash differently. Files that fail to open return None;
        the caller treats that as 'no hash yet' and falls back to treating
        the file as new.
        """
        try:
            h = hashlib.sha256()
            with open(abs_path, "rb") as f:
                for block in iter(lambda: f.read(1 << 20), b""):
                    h.update(block)
            return h.hexdigest()
        except OSError:
            return None

    def _set_hash(self, path: str, content_hash: Optional[str]):
        with self._db_lock:
            conn = self._get_conn()
            try:
                conn.execute(
                    "UPDATE file_index SET content_hash = ? WHERE path = ?",
                    (content_hash, path),
                )
                conn.commit()
            finally:
                conn.close()

    def _cleanup_vector_versions(self):
        """Drop the vector table's superseded versions.

        LanceDB needs an extra package for this; without it the versions
        simply keep piling up, so the failure is logged and the scan
        carries on rather than being abandoned.
        """
        try:
            rag_svc = state.get_rag_service()
            result = rag_svc.vector_store.cleanup_old_versions(older_than_days=1)
            if not result.get("ok"):
                log.warning("could not clean up old vector versions: %s", result.get("detail"))
        except Exception:
            log.warning("could not clean up old vector versions", exc_info=True)

    def _touch_indexed_at(self, path: str):
        """Record that a file was checked and found unchanged.

        Without this the file is hashed again on every scan, because its
        inode timestamp is still newer than the one recorded at indexing.
        """
        with self._db_lock:
            conn = self._get_conn()
            try:
                conn.execute(
                    "UPDATE file_index SET indexed_at = ? WHERE path = ?",
                    (int(time.time()), path),
                )
                conn.commit()
            finally:
                conn.close()

    def _rename_manifest(self, old: str, new: str):
        """Rewrite path in the manifest. PK makes this atomic.

        vec_done / chunk_count / chunk_sig / content_hash are kept — the
        file at the new path is bit-identical to the one at the old path,
        so all the indexing work still applies.
        """
        with self._db_lock:
            conn = self._get_conn()
            try:
                conn.execute(
                    "UPDATE file_index SET path = ? WHERE path = ?",
                    (new, old),
                )
                conn.commit()
            finally:
                conn.close()

    def _backfill_hashes(self, current: dict[str, tuple[float, int, float]],
                          known: dict[str, tuple[float, int, Optional[str], int]],
                          indexing: set[str] | None = None):
        """Populate NULL content_hash entries on demand.

        Walks every path in `current`, computes SHA-256 if the stored hash
        is missing, and updates both the manifest row and the in-memory
        `known` cache. The pass is bounded — a file with a stored hash is
        a single cheap SELECT — so the steady-state cost is one disk read
        per NULL row until none remain.
        """
        # Cap backfill per scan so a freshly-upgraded system with thousands
        # of un-hashed files doesn't spend an entire scan on hashing. The
        # remainder picks up on the next pass.
        budget = 200
        to_update: list[tuple[str, str]] = []
        indexing = indexing or set()
        for path, (mtime, size, _ctime) in current.items():
            if budget <= 0:
                break
            if path in indexing:
                # About to be indexed, and phase 1 writes the hash of
                # what it indexed. Storing the current hash here would
                # make a file whose contents had already drifted agree
                # with itself, and the drift would never be noticed.
                continue
            prev = known.get(path)
            if prev is not None and prev[2]:
                continue
            digest = self._compute_hash(path)
            if not digest:
                continue
            to_update.append((digest, path))
            known[path] = (mtime, size, digest, prev[3] if prev else 0)
            budget -= 1
        if to_update:
            with self._db_lock:
                conn = self._get_conn()
                try:
                    conn.executemany(
                        "UPDATE file_index SET content_hash = ? WHERE path = ?",
                        to_update,
                    )
                    conn.commit()
                finally:
                    conn.close()

    def _vector_progress(self) -> dict[str, tuple[int, int]]:
        """path -> (chunks already embedded, total chunks)."""
        with self._db_lock:
            conn = self._get_conn()
            try:
                rows = conn.execute(
                    "SELECT path, vec_done, chunk_count, chunk_sig FROM file_index"
                ).fetchall()
            finally:
                conn.close()
        return {r[0]: (r[1], r[2], r[3]) for r in rows}

    def _stale_chunking(self, sig: str | None) -> bool:
        """True when the file was chunked with different parameters.

        vec_done is only meaningful relative to the chunk list it was counted
        against. After a parameter change the stored total describes a chunking
        that no longer exists, and clamping progress against it both hides
        unembedded chunks and lets coverage exceed 100%.
        """
        return sig != CHUNK_SIG

    def _record_files(self, records: list[tuple[str, float, int, int]],
                      hashes: dict[str, Optional[str]] | None = None):
        """Record that files were indexed.

        The content hash is written alongside, because it has to describe
        the contents that were just indexed. Leaving it to the next scan's
        backfill pass would store the hash of whatever the file happens to
        contain then, which is how a stale index stays stale: the hash
        would agree with the file even though the index does not.

        REPLACE would wipe the column outright, since it replaces the
        whole row, so the update is spelled out.
        """
        hashes = hashes or {}
        with self._db_lock:
            conn = self._get_conn()
            now = int(time.time())
            try:
                conn.executemany(
                    "INSERT INTO file_index (path, mtime, size, indexed_at, chunk_count, chunk_sig, content_hash) "
                    "VALUES (?, ?, ?, ?, ?, ?, ?) "
                    "ON CONFLICT(path) DO UPDATE SET "
                    "mtime = excluded.mtime, size = excluded.size, "
                    "indexed_at = excluded.indexed_at, "
                    "chunk_count = excluded.chunk_count, chunk_sig = excluded.chunk_sig, "
                    "content_hash = excluded.content_hash",
                    # vec_done is not carried on a re-index: a changed file restarts its
                    # vector pass from zero.
                    [
                        (p, m, s, now, n, CHUNK_SIG, hashes.get(p))
                        for p, m, s, n in records
                    ],
                )
                conn.commit()
            finally:
                conn.close()

    def _set_vector_progress(self, path: str, done: int):
        with self._db_lock:
            conn = self._get_conn()
            try:
                conn.execute(
                    "UPDATE file_index SET vec_done = ? WHERE path = ?", (done, path)
                )
                conn.commit()
            finally:
                conn.close()

    def _reset_vector_progress(self, path: str, chunk_count: int):
        with self._db_lock:
            conn = self._get_conn()
            try:
                conn.execute(
                    "UPDATE file_index SET vec_done = 0, chunk_count = ?, chunk_sig = ? WHERE path = ?",
                    (chunk_count, CHUNK_SIG, path),
                )
                conn.commit()
            finally:
                conn.close()

    def _remove_files(self, paths: list[str]):
        with self._db_lock:
            conn = self._get_conn()
            try:
                conn.executemany(
                    "DELETE FROM file_index WHERE path = ?", [(p,) for p in paths]
                )
                conn.commit()
            finally:
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

    @property
    def is_scanning(self) -> bool:
        return bool(self._scan_thread and self._scan_thread.is_alive())

    def scan_async(self):
        """Start a scan in the background and return immediately."""
        if self.is_scanning:
            return False
        self._scan_thread = threading.Thread(target=self._scan_guarded, name="indexer-scan", daemon=True)
        self._scan_thread.start()
        return True

    def _scan_guarded(self):
        if not self._scan_lock.acquire(blocking=False):
            self._scan_thread = None
            return
        try:
            self.scan()
        except Exception:
            log.exception("indexer tick failed")
        finally:
            self._scan_lock.release()
            self._scan_thread = None

    def _run(self):
        while not self._stop_event.wait(TICK_SECONDS):
            try:
                self._tick()
            except Exception:
                log.exception("indexer scan failed")

    def pause(self, seconds: int = 0):
        """Pause indexing.

        seconds=0 pauses until resumed manually; a positive value resumes on
        its own once the countdown elapses. Kept separate from
        auto_index_enabled, which is the master switch.
        """
        self._paused_until = -1.0 if seconds <= 0 else time.time() + seconds
        log.info("indexing paused (%s)", "manual" if seconds <= 0 else f"{seconds}s")
        return self.pause_remaining()

    def resume(self):
        self._paused_until = None
        # Reset the interval clock so a resume does not wait out the previous
        # tick's elapsed time before doing anything.
        self._last_scan_ts = 0.0
        log.info("indexing resumed")
        return self.pause_remaining()

    def pause_remaining(self) -> int:
        if self._paused_until is None:
            return 0
        if self._paused_until < 0:
            return -1  # indefinite
        return max(0, int(self._paused_until - time.time()))

    def is_paused(self) -> bool:
        if self._paused_until is None:
            return False
        if self._paused_until < 0:
            return True
        if time.time() >= self._paused_until:
            self._paused_until = None  # elapsed; resume on its own
            # Reset the clock so the countdown actually delivers: without this
            # the next scan waits out whatever is left of index_interval, which
            # is up to five minutes after a five-minute pause.
            self._last_scan_ts = 0.0
            log.info("indexing pause elapsed; resuming")
            return False
        return True

    def _tick(self):
        settings = state.get_settings()
        if not settings.auto_index_enabled:
            self._pending_changes = 0
            return
        if self.is_paused():
            return
        if not _in_active_hours(settings.index_active_hours):
            return
        if state.get_index_status().is_indexing:
            return
        interval = max(MIN_INTERVAL_SECONDS, settings.index_interval)
        # Work left over from the previous pass continues straight away. The
        # interval exists to throttle empty scans, not to pace real work: with
        # only EMBED_CHUNKS_PER_PASS chunks per pass, gating on it capped
        # indexing at 16 chunks / 5 min, which put a full-text corpus weeks out.
        # Interaction yielding is handled separately and still applies.
        if self._continue_pending:
            if interaction.is_busy():
                return
        elif self._yielded_for_interaction:
            if interaction.is_busy() or time.time() - self._last_scan_ts < YIELD_RETRY_SECONDS:
                return
        elif time.time() - self._last_scan_ts < interval:
            return
        self._last_scan_ts = time.time()
        self._continue_pending = False
        if self._scan_lock.acquire(blocking=False):
            try:
                self.scan()
            finally:
                self._scan_lock.release()

    # ---- scanning ----

    def _iter_text_files(self, root: Path) -> dict[str, tuple[float, int, float]]:
        found: dict[str, tuple[float, int]] = {}
        present: set[str] = set()
        unsupported: dict[str, str] = {}
        oversized: dict[str, str] = {}
        scan_complete = True
        try:
            for entry in root.rglob("*"):
                if _is_hidden(entry):
                    continue
                if not entry.is_file():
                    continue
                try:
                    stat = entry.stat()
                    present.add(str(entry))
                    if entry.suffix.lower() not in INDEXABLE_EXTENSIONS:
                        # Files we simply cannot parse. Counted so the user can
                        # see why they are absent from search. They still count
                        # as present: a file becoming unsupported must not be
                        # mistaken for deletion of its previous index.
                        unsupported[_display_path(entry)] = entry.suffix.lower()
                        continue
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
                    # ctime is when the inode last changed, which is the
                    # only cheap signal that a file's contents were
                    # replaced without its mtime moving -- a restore from
                    # a backup, or an rsync that preserves timestamps.
                    found[str(entry)] = (stat.st_mtime, stat.st_size, stat.st_ctime)
                except (PermissionError, OSError):
                    scan_complete = False
                    continue
        except (PermissionError, OSError):
            scan_complete = False
        self._last_enumeration_complete = scan_complete
        self._present_paths = present
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
            if not self._last_enumeration_complete:
                # A partial directory walk cannot distinguish deleted files
                # from temporarily inaccessible ones. Indexing what we did see
                # is safe, but removals are suppressed for this scan.
                log.warning("directory scan was incomplete; suppressing index deletions")
            known = self._known_files()

            to_index = []
            changed_files = set()
            pending = 0
            # Files that look untouched but whose inode was touched since
            # they were last indexed. Their contents may have been
            # replaced while their mtime and size stayed put, which is
            # exactly what a restore from a backup or an rsync that
            # preserves timestamps does. They are hashed to find out.
            verify: list[str] = []
            for path, (mtime, size, ctime) in current.items():
                if path in known:
                    k_mtime, k_size, k_hash, k_indexed_at = known[path]
                    if k_mtime == mtime and k_size == size:
                        if k_hash and ctime > k_indexed_at:
                            verify.append(path)
                        continue
                changed_files.add(path)
                if now - mtime < debounce:
                    pending += 1
                    continue
                to_index.append(path)

            # Hash the suspicious ones and re-index any whose contents
            # really did change. The budget keeps a large library from
            # hashing its whole tree in one pass; the rest come round on
            # a later scan.
            if verify:
                budget = VERIFY_BUDGET
                for path in verify:
                    if budget <= 0:
                        break
                    budget -= 1
                    k_hash = known[path][2]
                    digest = self._compute_hash(path)
                    if not digest or digest == k_hash:
                        # The same contents: the file was only touched,
                        # not changed. Record the newer timestamp so it
                        # is not hashed again next scan.
                        self._touch_indexed_at(path)
                        continue
                    log.info("content changed behind the index: %s", path)
                    changed_files.add(path)
                    to_index.append(path)

            # Phase 1 is driven purely by file mtime/size, so anything that
            # empties the FTS index without touching the files -- clearing the
            # search index, restoring a backup, a wiped search.db -- leaves the
            # keyword channel permanently empty while every file still looks
            # "unchanged". Vector progress is tracked separately, which is why
            # this went unnoticed: the semantic side looked complete.
            if search_engine.count() == 0 and current:
                log.info("keyword index is empty but %d files are known; rebuilding FTS", len(current))
                to_index = list(current)
            self._pending_changes = pending

            removed = (
                [p for p in known if p not in self._present_paths]
                if self._last_enumeration_complete else []
            )

            # ---- hash backfill ----
            # The content_hash column was added later than the rest of the
            # manifest, so files indexed before the migration have NULL
            # hashes. Compute and store them lazily on the first scan that
            # sees each one — read I/O is bounded, hashing is cheap, and a
            # NULL hash is the only reason a real move wouldn't be detected
            # by the pass below. Skip files whose mtime/size already match
            # known AND have a hash; they're already populated.
            self._backfill_hashes(current, known, set(to_index))

            # ---- move detection ----
            # The plain delete/add diff can't tell a rename apart from a
            # genuine delete-then-add. With content_hash in the manifest,
            # pair up files that disappeared at one path and reappeared at
            # another with identical content; rewrite the path in place
            # across the manifest, the FTS index, and the vector store.
            # Both sides stay in the scan's working set, just with the
            # old path dropped from `removed` and the new path dropped from
            # `to_index` so neither re-embedding nor a delete runs.
            #
            # Hashes are only computed for files that need them — new files
            # (not in known) and files whose mtime/size changed. A file
            # already in known with matching mtime/size has a valid stored
            # hash from a previous scan, and a moved file must keep its
            # mtime to qualify anyway.
            moves: list[tuple[str, str]] = []
            if removed and self._last_enumeration_complete:
                to_index_set = set(to_index)
                # Index the current side by hash. For each new/changed file,
                # hash on demand — only files we'd otherwise re-index.
                added_paths = [p for p in to_index_set if p not in known]
                new_hashes: dict[str, Optional[str]] = {}
                for p in added_paths:
                    new_hashes[p] = self._compute_hash(p)
                # Match against stored hashes of files that disappeared.
                removed_by_hash: dict[str, str] = {}
                for old in removed:
                    h = known[old][2]
                    if h:
                        removed_by_hash.setdefault(h, old)
                # Build deterministic pairs: each removed hash claims the
                # lexicographically smallest new path that matches it.
                # Anything left over is a real delete (no new home) or a
                # real add (no old home) and stays in the diff sets.
                used_new: set[str] = set()
                used_old: set[str] = set()
                for h, old in removed_by_hash.items():
                    candidates = sorted(
                        np for np, nh in new_hashes.items()
                        if nh == h and np not in used_new
                    )
                    if not candidates:
                        continue
                    new = candidates[0]
                    moves.append((old, new))
                    used_new.add(new)
                    used_old.add(old)
                if moves:
                    # Apply the path rewrite across the manifest, FTS, and
                    # vector store before phase 1 runs. Phase 1 will then
                    # skip these because the cached `known` view shows
                    # `new` with the old mtime/size/hash — the file at the
                    # new path is bit-identical so nothing changed.
                    rag_svc = state.get_rag_service()
                    for old, new in moves:
                        old_mtime, old_size, old_hash, old_indexed_at = known[old]
                        try:
                            self._rename_manifest(old, new)
                        except Exception:
                            log.exception("keyword indexing failed for a file")
                            continue
                        try:
                            search_engine.rename_file(old, new)
                        except Exception:
                            log.exception("vectorising a file failed")
                        try:
                            rag_svc.vector_store.rename_file(old, new)
                        except Exception:
                            log.exception("indexing a file failed")
                        # Reflect the rename in the in-memory cache so the
                        # rest of the scan sees `new` as up-to-date.
                        known.pop(old, None)
                        known[new] = (old_mtime, old_size, old_hash, old_indexed_at)
                    # Remove moved entries from `removed` and `to_index` so
                    # the rest of the pass treats them as unchanged.
                    removed = [p for p in removed if p not in used_old]
                    to_index = [p for p in to_index if p not in used_new]
                    changed_files -= used_new
                    log.info("detected %d file move(s), skipped re-embedding", len(moves))
            # -----------------------

            rag_svc = state.get_rag_service()
            indexed, failed = 0, 0
            records: list[tuple[str, float, int, int]] = []
            indexed_hashes: dict[str, Optional[str]] = {}

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
                        if path in changed_files:
                            # Only discard old vectors after the keyword update
                            # succeeded. If FTS fails, leave the prior semantic
                            # index intact and retry the whole changed file.
                            rag_svc.vector_store.delete_by_file(path)
                        stat = Path(path).stat()
                        records.append((path, stat.st_mtime, stat.st_size, len(pieces)))
                        # The hash of what was just indexed, so the next
                        # scan can tell whether the file still holds it.
                        indexed_hashes[path] = self._compute_hash(path)
                        indexed += 1
                    except Exception:
                        failed += 1
                        log.exception("indexing a file failed")
            if records:
                self._record_files(records, indexed_hashes)
                # Every write leaves the previous version of the table on
                # disk, and a scan writes on every pass. Without this the
                # versions directory grows by a full copy of the index per
                # scan -- 240MB for four books, of which the live data was
                # a fraction.
                self._cleanup_vector_versions()

            # Phase 2 — semantic channel, resumable. Progress lives in the
            # manifest, so a file needing hours to vectorize converges over
            # repeated scans and survives a restart, instead of being abandoned
            # half-done or re-embedded from zero. Driven off every known file,
            # not just this pass's, so earlier passes resume correctly.
            progress = self._vector_progress()
            pending_vectors = [
                p for p in current
                if self._stale_chunking(progress.get(p, (0, 0, None))[2])
                or (progress.get(p, (0, 1, None))[0] < progress.get(p, (0, 1, None))[1]
                    and progress.get(p, (0, 0, None))[1] > 0)
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
                done, stored_total, sig = progress.get(path, (0, 0, None))
                content = self._read_content(path)
                if not content:
                    continue
                pieces = chunk_file(content)
                # The chunk list is the only trustworthy count. chunk_count was
                # written when the keyword pass ran, so a change to the chunking
                # parameters since then leaves it stale -- and vec_done is
                # clamped against it, which makes progress over-report and hides
                # unembedded chunks behind a false "complete". Wipe and
                # re-embed when the two disagree.
                total = len(pieces)
                if total != stored_total or self._stale_chunking(sig):
                    log.info(
                        "chunk count drift on %s: manifest %d, actual %d; re-embedding",
                        os.path.basename(path), stored_total, total,
                    )
                    try:
                        rag_svc.vector_store.delete_by_file(path)
                    except Exception:
                        log.exception("vectorising a file failed")
                    self._reset_vector_progress(path, total)
                    progress[path] = (0, total)
                    done = 0
                batch = pieces[done: done + EMBED_CHUNKS_PER_PASS]
                if not batch:
                    continue
                stop_outer = False
                try:
                    chunk_records = [
                        (path, done + offset, text)
                        for offset, (_off, text) in enumerate(batch)
                    ]
                    for j in range(0, len(chunk_records), EMBED_CHUNK_BATCH):
                        # Checked per batch, not just per file: one file is
                        # ~16 chunks, so a file-level check could sit on the
                        # cores for a minute after the user started talking.
                        # A manual pause also has to take effect mid-scan.
                        if interaction.is_busy() or self.is_paused() or not _in_active_hours(
                                state.get_settings().index_active_hours):
                            self._set_vector_progress(path, done + j)
                            self._vector_partial[path] = done + j
                            if self.is_paused():
                                # Paused is not the same as deferring to the
                                # user: flagging it as an interaction yield
                                # would schedule a fast retry we do not want.
                                stop_outer = True
                            else:
                                self._yielded_for_interaction = True
                            break
                        part = chunk_records[j: j + EMBED_CHUNK_BATCH]
                        vectors = rag_svc.embedding_service.embed([c for _, _, c in part])
                        rag_svc.vector_store.add_many(
                            vectors,
                            [{"file_path": p, "content": c, "chunk_idx": i} for p, i, c in part],
                        )
                    if not stop_outer:
                        new_done = min(done + len(batch), total)
                        self._set_vector_progress(path, new_done)
                        self._vector_partial[path] = new_done
                except Exception:
                    log.exception("reading a file for indexing failed")
                if stop_outer:
                    break

            if removed:
                for p in removed:
                    try:
                        rag_svc.vector_store.delete_by_file(p)
                    except Exception:
                        log.exception("writing the index manifest failed")
                    try:
                        search_engine.remove_file(p)
                    except Exception:
                        log.exception("removing a file from the index failed")
                self._remove_files(removed)

            self._last_scan_time = datetime.now().strftime("%Y-%m-%dT%H:%M:%S")
            self._last_scan_files = len(current)
            # Anything left over means this pass stopped on a budget, not on
            # completion, so the next tick should continue without waiting.
            self._continue_pending = bool(pending_vectors) and not self._yielded_for_interaction

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
        for path, (done, total, _sig) in sorted(progress.items()):
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
            paused_remaining=self.pause_remaining(),
            paused_indefinitely=self._paused_until == -1.0,
            active_hours=settings.index_active_hours,
            in_active_hours=_in_active_hours(settings.index_active_hours),
        )


indexer = AutoIndexer()
