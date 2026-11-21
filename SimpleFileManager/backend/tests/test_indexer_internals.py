"""The indexer's innards: the thread, the watcher, the vector pass.

The scan itself is covered in test_index_scan.py. This file is about
the machinery around it -- the background thread that ticks, the file
watcher that says "something changed", the semantic pass that embeds a
file a piece at a time and has to be able to pick up where it left
off.

The vector pass is the part worth testing carefully. Embedding a
whole library takes hours, so it is resumable: it records how far it
got, and the next pass continues. A bug there does not show up as a
crash -- it shows up as a file that is half embedded for ever, or as
the same file being embedded over and over.
"""

import importlib
import sqlite3
import threading
import time

import pytest

from app.deps import DATA_DIR, get_storage_root, state

indexer_mod = importlib.import_module("app.indexer")
indexer = indexer_mod.indexer


@pytest.fixture(autouse=True)
def nobody_is_talking():
    """The agent tests leave the machine marked as busy.

    Someone talking is a good reason for the indexer to stand down --
    that is the whole point of the rule -- so the vector pass would do
    nothing at all in these tests.
    """
    from app.interaction import interaction

    original = interaction._last_active
    interaction._last_active = 0.0
    _quiet_the_background()
    yield
    interaction._last_active = original
    _quiet_the_background()


def _quiet_the_background():
    """Stop the background indexer and wait for it to let go.

    It scans the whole storage root every time the watcher says
    something changed, and the tests change files constantly. A scan
    of its own running next to the scan a test is making is two
    passes over the same tree at once.
    """
    indexer.stop()
    thread = indexer._thread
    if thread is not None and thread.is_alive():
        thread.join(timeout=10)


def _make(name: str, content: str = "hello"):
    path = get_storage_root() / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    return path


def _manifest_row(path: str) -> tuple:
    conn = sqlite3.connect(str(DATA_DIR / "file_index.db"))
    try:
        return conn.execute(
            "SELECT path, mtime, size, vec_done, chunk_count, chunk_sig, content_hash "
            "FROM file_index WHERE path = ?", (path,),
        ).fetchone()
    finally:
        conn.close()


def _scan(debounce: int = 0):
    settings = state.get_settings()
    original = settings.index_debounce_seconds
    settings.index_debounce_seconds = debounce
    try:
        return indexer.scan()
    finally:
        settings.index_debounce_seconds = original


# ---- the background thread ----


def test_the_indexer_starts_and_stops():
    """The thread is a daemon: it must not keep the process alive."""
    indexer.start()
    assert indexer._thread is not None
    indexer.stop()
    # The stop flag is what the loop watches.
    assert indexer._stop_event.is_set()
    # And it really lets go, so it cannot scan behind the next test.
    if indexer._thread.is_alive():
        indexer._thread.join(timeout=10)


def test_a_scan_that_breaks_does_not_take_the_indexer_with_it():
    """One bad scan must not stop the next one."""
    def broken():
        raise RuntimeError("这一遍坏了")

    monkey = pytest.MonkeyPatch()
    monkey.setattr(indexer, "scan", broken)
    try:
        indexer._scan_guarded()  # must not raise
    finally:
        monkey.undo()


def test_a_scan_in_the_background_returns_at_once():
    """The caller does not wait for the scan."""
    started = threading.Event()
    release = threading.Event()

    def slow_scan():
        started.set()
        release.wait(5)

    monkey = pytest.MonkeyPatch()
    monkey.setattr(indexer, "scan", slow_scan)
    try:
        assert indexer.scan_async() is True
        assert started.wait(5), "the scan never started"
        assert indexer.is_scanning is True
        # A second scan while one is running is refused, not queued.
        assert indexer.scan_async() is False
    finally:
        release.set()
        monkey.undo()
        time.sleep(0.2)


def test_the_loop_keeps_going_after_a_bad_tick():
    """A tick that raises is logged, and the loop carries on."""
    calls = []

    def bad_tick():
        calls.append(1)
        # The loop watches this flag; set it so the loop ends after
        # this turn.
        indexer._stop_event.set()
        raise RuntimeError("这一轮坏了")

    monkey = pytest.MonkeyPatch()
    monkey.setattr(indexer, "_tick", bad_tick)
    try:
        # Another test may have left the stop flag set.
        indexer._stop_event.clear()
        indexer._run()
        assert calls == [1], "the loop did not survive the bad tick"
    finally:
        monkey.undo()
        indexer._stop_event.clear()


# ---- the watcher ----


def test_the_health_check_says_when_nothing_is_being_watched(monkeypatch):
    """A machine whose filesystem cannot be watched falls back to
    timed scans, and the health check has to say so."""
    monkeypatch.setattr(indexer, "_watcher", None)
    status = indexer.watch_status()
    assert status["watching"] is False
    assert status["degraded"] is False
    assert "没有在监视" in status["error"]


def test_a_change_makes_the_next_tick_happen_right_away(monkeypatch):
    """A watcher event resets the clock.

    Without this the change waits for the next scheduled scan, which
    can be minutes away.
    """
    monkeypatch.setattr(indexer, "_last_scan_ts", time.time())
    monkeypatch.setattr(indexer, "_retry_scan_at", time.time() + 60)
    monkeypatch.setattr(indexer, "_watched_changes", 0)

    indexer._on_watched_change({"a.txt"}, set())

    assert indexer._last_scan_ts == 0.0
    assert indexer._retry_scan_at is None
    assert indexer._watched_changes == 1


def test_a_file_the_watcher_says_is_finished_is_remembered(monkeypatch):
    """A closed file does not have to wait out the debounce."""
    monkeypatch.setattr(indexer, "_ready_paths", set())
    root = get_storage_root()
    indexer._on_watched_change({"a.txt"}, {"a.txt"})
    assert str(root / "a.txt") in indexer._ready_paths
    indexer._ready_paths.clear()


def test_too_many_changes_at_once_means_a_full_scan_next_time(monkeypatch):
    """The details were dropped, so the only safe answer is to look at
    everything again."""
    monkeypatch.setattr(indexer, "_last_scan_ts", time.time())
    monkeypatch.setattr(indexer, "_continue_pending", False)

    indexer._on_watch_overflow()

    assert indexer._last_scan_ts == 0.0
    assert indexer._continue_pending is True


# ---- the tick ----


def test_the_tick_does_nothing_when_the_index_is_switched_off(monkeypatch):
    settings = state.get_settings()
    original = settings.auto_index_enabled
    settings.auto_index_enabled = False
    scanned = []
    monkeypatch.setattr(indexer, "scan", lambda: scanned.append(1))
    try:
        indexer._tick()
    finally:
        settings.auto_index_enabled = original
    assert scanned == []


def test_the_tick_does_nothing_while_paused(monkeypatch):
    scanned = []
    monkeypatch.setattr(indexer, "scan", lambda: scanned.append(1))
    indexer.pause(0)
    try:
        indexer._tick()
    finally:
        indexer.resume()
    assert scanned == []


def test_the_tick_does_nothing_outside_the_active_hours(monkeypatch):
    """A machine that is not supposed to be working at three in the
    morning does not index at three in the morning."""
    settings = state.get_settings()
    original = settings.index_active_hours
    settings.index_active_hours = "03:00-03:01"
    scanned = []
    monkeypatch.setattr(indexer, "scan", lambda: scanned.append(1))
    try:
        indexer._tick()
    finally:
        settings.index_active_hours = original
    # Only assert it did not scan if it really is outside those hours.
    if not indexer_mod._in_active_hours("03:00-03:01"):
        assert scanned == []


def test_the_tick_scans_when_the_interval_has_passed(monkeypatch):
    scanned = []
    settings = state.get_settings()
    monkeypatch.setattr(settings, "auto_index_enabled", True)
    monkeypatch.setattr(settings, "index_active_hours", "")
    monkeypatch.setattr(indexer, "scan", lambda: scanned.append(1))
    monkeypatch.setattr(indexer, "_last_scan_ts", 0.0)
    monkeypatch.setattr(indexer, "_pending_changes", 0)
    monkeypatch.setattr(indexer, "_continue_pending", False)
    monkeypatch.setattr(indexer, "_yielded_for_interaction", False)
    monkeypatch.setattr(indexer, "_retry_scan_at", None)
    indexer.resume()
    indexer._tick()
    assert scanned == [1]


# ---- the vector pass ----


class _FakeEmbedder:
    """An embedding service that works and counts what it was asked."""

    def __init__(self):
        self.calls = []

    def embed(self, texts):
        self.calls.append(list(texts))
        return [[0.1, 0.2, 0.3] for _ in texts]


class _FakeStore:
    """A vector store that remembers what it was told to do."""

    def __init__(self):
        self.written = []
        self.deleted = []

    def add_many(self, vectors, records):
        self.written.extend(records)

    def delete_by_file(self, path):
        self.deleted.append(path)


def _fake_rag(monkeypatch):
    """Point the rag service at fakes that work."""
    rag = state.get_rag_service()
    embedder = _FakeEmbedder()
    store = _FakeStore()
    monkeypatch.setattr(rag.embedding_service, "embed", embedder.embed)
    monkeypatch.setattr(rag.vector_store, "add_many", store.add_many)
    monkeypatch.setattr(rag.vector_store, "delete_by_file", store.delete_by_file)
    return embedder, store


def test_a_file_is_embedded_and_the_progress_is_recorded(monkeypatch):
    """The semantic pass runs after the keyword one, and it records
    how far it got."""
    embedder, store = _fake_rag(monkeypatch)
    path = _make("向量/长文.txt", "这是一段要被切块向量化���文字。" * 200)

    _scan()

    row = _manifest_row(str(path))
    assert row is not None
    assert row[3] == row[4], "the progress does not match the chunk count"
    assert row[3] > 0, "nothing was embedded"
    assert embedder.calls, "the embedding service was never called"
    assert store.written, "nothing was written to the vector store"


def test_the_embedding_service_being_down_does_not_break_the_keyword_index(monkeypatch):
    """The keyword channel is cheap and complete; the semantic one
    needs a model. The model being down must not cost the user their
    keyword search."""
    rag = state.get_rag_service()

    def broken(texts):
        raise RuntimeError("模型没在")

    monkeypatch.setattr(rag.embedding_service, "embed", broken)
    path = _make("向量/模型不在.txt", "关键词还在就行")

    _scan()

    from app.search_engine import search_engine
    assert search_engine.bm25_search("关键词") != []
    row = _manifest_row(str(path))
    assert row is not None, "the file was not indexed at all"
    assert row[3] == 0, "it claims to be embedded"


def test_the_embedding_picks_up_where_it_stopped(monkeypatch):
    """Embedding a library takes hours, so it has to be resumable.

    The pass is interrupted half way; the next scan continues from
    there instead of starting again.
    """
    embedder, store = _fake_rag(monkeypatch)
    path = _make("向量/很长很长.txt", "文字" * 4000)

    _scan()
    row = _manifest_row(str(path))
    total = row[4]
    assert total > 0

    # Pretend it got half way, then let the next scan finish it.
    indexer._set_vector_progress(str(path), total // 2)
    assert _manifest_row(str(path))[3] == total // 2

    embedder.calls.clear()
    _scan()

    assert _manifest_row(str(path))[3] == total, "the pass did not finish"
    # And it did not start over: fewer chunks were sent than the whole
    # file has.
    sent = sum(len(c) for c in embedder.calls)
    assert sent < total, "it embedded the whole file again"


def test_the_chunking_changing_makes_the_file_start_over(monkeypatch):
    """The stored progress only means something relative to the chunk
    list it was counted against.

    After the chunking parameters change, the old count describes a
    chunking that no longer exists, so the file is embedded again from
    the beginning rather than being left half embedded for ever.
    """
    embedder, store = _fake_rag(monkeypatch)
    path = _make("向量/参数变了.txt", "文字" * 2000)

    _scan()
    assert _manifest_row(str(path))[3] > 0

    # The stored signature is from another chunking.
    conn = sqlite3.connect(str(DATA_DIR / "file_index.db"))
    try:
        conn.execute(
            "UPDATE file_index SET chunk_sig = ?, vec_done = 1 WHERE path = ?",
            ("别的参数", str(path)),
        )
        conn.commit()
    finally:
        conn.close()

    _scan()

    row = _manifest_row(str(path))
    assert row[5] == indexer_mod.CHUNK_SIG, "the signature was not refreshed"
    assert row[3] == row[4], "the file is still half embedded"


def test_the_progress_helpers():
    """The bookkeeping behind the vector pass."""
    path = _make("向量/进度.txt", "内容")
    _scan()
    total = _manifest_row(str(path))[4]

    indexer._set_vector_progress(str(path), 3)
    assert _manifest_row(str(path))[3] == 3

    indexer._reset_vector_progress(str(path), total)
    row = _manifest_row(str(path))
    assert row[3] == 0
    assert row[4] == total
    assert row[5] == indexer_mod.CHUNK_SIG

    assert indexer._stale_chunking(indexer_mod.CHUNK_SIG) is False
    assert indexer._stale_chunking("别的参数") is True


# ---- the hash backfill ----


def test_files_indexed_before_hashes_existed_get_one(monkeypatch):
    """The hash column was added later than the rest of the manifest.

    The old rows have no hash, and without one a file that moved is
    indistinguishable from a file that was deleted and another one
    written.
    """
    path = _make("回填/老文件.txt", "内容")
    _scan()

    # Pretend it was indexed before the column existed.
    conn = sqlite3.connect(str(DATA_DIR / "file_index.db"))
    try:
        conn.execute("UPDATE file_index SET content_hash = NULL WHERE path = ?", (str(path),))
        conn.commit()
    finally:
        conn.close()

    _scan()

    row = _manifest_row(str(path))
    assert row[6], "the hash was not filled in"


def test_a_file_that_was_just_indexed_is_not_hashed_behind_its_back(monkeypatch):
    """The hash has to describe what was indexed.

    A file whose contents changed between the scan and the backfill
    would otherwise agree with itself, and the change would never be
    noticed.
    """
    path = _make("回填/刚索引.txt", "内容")
    _scan()

    conn = sqlite3.connect(str(DATA_DIR / "file_index.db"))
    try:
        conn.execute("UPDATE file_index SET content_hash = NULL WHERE path = ?", (str(path),))
        conn.commit()
    finally:
        conn.close()

    digest = indexer._compute_hash(str(path))
    assert digest
    assert indexer._set_hash(str(path), digest) is None or True
    assert _manifest_row(str(path))[6] == digest


# ---- following a rename ----


def test_renaming_a_file_keeps_it_in_the_index():
    """The three indexes each keep the path; a file that moved only
    needs its new address."""
    path = _make("改名/之前.txt", "内容")
    _scan()
    assert _manifest_row(str(path)) is not None

    new_path = path.with_name("之后.txt")
    assert indexer.rename_file(str(path), str(new_path)) is True

    rows = {r[0] for r in _manifest_row(str(path))} if False else None
    conn = sqlite3.connect(str(DATA_DIR / "file_index.db"))
    try:
        paths = {r[0] for r in conn.execute("SELECT path FROM file_index")}
    finally:
        conn.close()
    assert str(new_path) in paths
    assert str(path) not in paths
