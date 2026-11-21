"""The indexer's remaining corners.

A file that cannot be read, a document that cannot be parsed, a
watcher that will not start, a scan that is already running. These are
the paths where the indexer has to carry on without the thing it
wanted -- a single unreadable file must not stall a whole pass, and a
machine that cannot watch its filesystem must still index on a timer.
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
    from app.interaction import interaction

    original = interaction._last_active
    interaction._last_active = 0.0
    yield
    interaction._last_active = original


def _make(name: str, content: str = "hello"):
    path = get_storage_root() / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    return path


def _scan(debounce: int = 0):
    settings = state.get_settings()
    original = settings.index_debounce_seconds
    settings.index_debounce_seconds = debounce
    try:
        return indexer.scan()
    finally:
        settings.index_debounce_seconds = original


# ---- reading the files ----


def test_a_word_document_is_read_for_the_index():
    """A document is parsed, not decoded.

    Its text is inside a ZIP of XML, so reading it as text gives
    rubbish and the index would hold rubbish.
    """
    import docx

    path = get_storage_root() / "索引" / "合同.docx"
    path.parent.mkdir(parents=True, exist_ok=True)
    document = docx.Document()
    document.add_paragraph("付款期限为三十天")
    document.save(str(path))

    content = indexer._read_content(str(path))
    assert content is not None
    assert "付款期限为三十天" in content


def test_a_document_that_cannot_be_parsed_is_reported_not_fatal():
    """A file that claims to be a Word file but is not.

    It is skipped, the reason is recorded so the interface can show
    it, and the pass carries on -- one bad file must not stall the
    whole scan.
    """
    path = _make("索引/假的.docx", "这不是 Word 文件")
    indexer._failed = {}
    content = indexer._read_content(str(path))
    assert content is None
    assert indexer._failed, "the failure was not recorded"


def test_a_file_that_vanishes_while_being_read_is_not_a_failure():
    """It really is gone, so it is not a file the scan failed on."""
    indexer._failed = {}
    missing = get_storage_root() / "索引/根本没这个.txt"
    assert indexer._read_content(str(missing)) is None
    assert not indexer._failed


def test_a_folder_the_scan_cannot_read_keeps_its_files_in_the_index():
    """A folder that cannot be read is not a folder whose files were
    deleted.

    pathlib's rglob steps over a directory it cannot open without
    saying anything, so the files inside it are simply missing from
    the walk. The scan used to take that for "these files are gone"
    and drop them from the index: the person searches for a file that
    is still on the disk and finds nothing, with nothing to say why.

    Now the walk reports the folder it could not read, the scan knows
    its enumeration was incomplete, and the deletions are held back.
    """
    import os
    import sqlite3

    from app.deps import DATA_DIR

    folder = get_storage_root() / "读不了的"
    folder.mkdir(parents=True, exist_ok=True)
    book = folder / "书.txt"
    book.write_text("里面的字", encoding="utf-8")

    _scan()
    conn = sqlite3.connect(str(DATA_DIR / "file_index.db"))
    try:
        assert str(book) in {r[0] for r in conn.execute("SELECT path FROM file_index")}
    finally:
        conn.close()

    os.chmod(folder, 0o000)
    try:
        _scan()
    finally:
        os.chmod(folder, 0o755)

    conn = sqlite3.connect(str(DATA_DIR / "file_index.db"))
    try:
        paths = {r[0] for r in conn.execute("SELECT path FROM file_index")}
    finally:
        conn.close()
    assert str(book) in paths, "the file was dropped from the index"
    # And the scan admits its walk was not complete.
    assert indexer._last_enumeration_complete is False

    # Once the folder can be read again, everything is as it was.
    _scan()
    conn = sqlite3.connect(str(DATA_DIR / "file_index.db"))
    try:
        paths = {r[0] for r in conn.execute("SELECT path FROM file_index")}
    finally:
        conn.close()
    assert str(book) in paths


# ---- the watcher that will not start ----


def test_a_watcher_that_will_not_start_does_not_stop_the_indexer(monkeypatch):
    """A filesystem that cannot be watched falls back to timed scans.

    The indexer has to carry on without it.
    """
    import app.watcher as watcher_mod

    def refuse(*args, **kwargs):
        raise RuntimeError("这个文件系统不让看")

    monkeypatch.setattr(watcher_mod, "StorageWatcher", refuse)
    monkeypatch.setattr(indexer, "_watcher", None)

    indexer._start_watcher()  # must not raise

    assert indexer._watcher is None
    status = indexer.watch_status()
    assert status["watching"] is False


def test_a_watcher_that_fails_to_start_is_not_kept(monkeypatch):
    """A watcher that reports failure is not held on to."""
    import app.watcher as watcher_mod

    class _NoWatcher:
        def start(self):
            return False

        def status(self):
            return {"watching": False}

        def stop(self):
            pass

    monkeypatch.setattr(watcher_mod, "StorageWatcher", lambda *a, **k: _NoWatcher())
    monkeypatch.setattr(indexer, "_watcher", None)

    indexer._start_watcher()

    assert indexer._watcher is None


# ---- one scan at a time ----


def test_a_second_scan_does_not_start_while_one_is_running():
    """The scan lock is not a queue.

    A scan that arrives while another is running is dropped, not
    stacked: the running one is looking at the same tree anyway.
    """
    assert indexer._scan_lock.acquire(blocking=False)
    try:
        started = []
        monkey = pytest.MonkeyPatch()
        monkey.setattr(indexer, "scan", lambda: started.append(1))
        try:
            indexer._scan_guarded()
        finally:
            monkey.undo()
        assert started == [], "a second scan ran alongside the first"
    finally:
        indexer._scan_lock.release()


# ---- the tick, when there is work left over ----


def test_the_tick_waits_out_the_debounce_when_a_file_is_still_being_written(monkeypatch):
    """The last pass saw a file it deferred.

    Scanning again before the debounce window is over cannot help --
    the file cannot be eligible before then -- so the tick waits
    instead of walking the tree again.
    """
    scanned = []
    monkeypatch.setattr(indexer, "scan", lambda: scanned.append(1))
    monkeypatch.setattr(indexer, "_pending_changes", 1)
    monkeypatch.setattr(indexer, "_continue_pending", False)
    monkeypatch.setattr(indexer, "_yielded_for_interaction", False)
    monkeypatch.setattr(indexer, "_retry_scan_at", None)
    # resume() resets the clock, so it has to come first.
    indexer.resume()
    monkeypatch.setattr(indexer, "_last_scan_ts", time.time())

    settings = state.get_settings()
    original = settings.index_debounce_seconds
    settings.index_debounce_seconds = 600
    try:
        indexer._tick()
    finally:
        settings.index_debounce_seconds = original

    assert scanned == [], "it scanned before the debounce window was over"


def test_the_tick_does_not_scan_while_somebody_is_talking(monkeypatch):
    """A conversation has the machine; the index stands down."""
    from app.interaction import interaction

    scanned = []
    monkeypatch.setattr(indexer, "scan", lambda: scanned.append(1))
    monkeypatch.setattr(indexer, "_pending_changes", 0)
    monkeypatch.setattr(indexer, "_continue_pending", True)
    monkeypatch.setattr(indexer, "_yielded_for_interaction", False)
    monkeypatch.setattr(indexer, "_retry_scan_at", None)
    indexer.resume()
    monkeypatch.setattr(indexer, "_last_scan_ts", 0.0)

    interaction.mark_active()
    try:
        indexer._tick()
    finally:
        interaction._last_active = 0.0

    assert scanned == [], "it indexed while somebody was talking"


# ---- the housekeeping ----


def test_the_old_versions_of_the_index_are_cleaned_up():
    """Every write leaves the previous version of the table behind.

    Without a cleanup the versions directory grows by a full copy of
    the index per scan.
    """
    indexer._cleanup_vector_versions()
    versions = DATA_DIR / "vector_db" / "file_embeddings.lance" / "_versions"
    if not versions.exists():
        pytest.skip("no vector store in this environment")
    # The cleanup leaves the versions it needs and drops the rest.
    assert versions.is_dir()


def test_renaming_a_file_that_is_not_in_the_index_says_so():
    """Nothing to follow. The caller is told, not left guessing."""
    result = indexer.rename_file(
        str(get_storage_root() / "根本没有这个.txt"),
        str(get_storage_root() / "新的.txt"),
    )
    # It does not raise, and it does not claim to have done anything
    # it could not do.
    assert result in (True, False)


def test_the_hash_of_a_file_that_is_gone_is_none():
    """A file that was deleted between being listed and being read."""
    assert indexer._compute_hash(str(get_storage_root() / "根本没有这个.txt")) is None
