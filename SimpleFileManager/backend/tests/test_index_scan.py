"""What a scan does to the files it finds.

The watcher tells the indexer that something changed; the scan is what
actually makes the new state searchable. It is also the part with the
most branches -- new files, changed files, deleted files, moved files,
files still being written, files that cannot be read at all -- and
until now none of that had a test of its own.

A bug here does not announce itself. The service starts, the pages
work, the search box is there; it just quietly stops finding the file
you wrote ten minutes ago.
"""

import os
import sqlite3
import time

from app.deps import DATA_DIR, get_storage_root, state
from app.indexer import indexer
from app.search_engine import search_engine


def _manifest_paths() -> set[str]:
    conn = sqlite3.connect(str(DATA_DIR / "file_index.db"))
    try:
        return {r[0] for r in conn.execute("SELECT path FROM file_index")}
    finally:
        conn.close()


def _scan_immediately(debounce: int = 0):
    """Scan without waiting out the debounce a fresh file would hit."""
    settings = state.get_settings()
    original = settings.index_debounce_seconds
    settings.index_debounce_seconds = debounce
    try:
        return indexer.scan()
    finally:
        settings.index_debounce_seconds = original


def _write(name: str, text: str):
    path = get_storage_root() / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    return path


def test_a_new_file_becomes_searchable():
    """The plain case: write a file, it can be found."""
    path = _write("scan-new.txt", "scannewword 一段中文")

    _scan_immediately()

    names = [r["name"] for r in search_engine.bm25_search("scannewword")]
    assert names == ["scan-new.txt"]
    assert str(path) in _manifest_paths()


def test_a_changed_file_is_indexed_again():
    """What was written a moment ago is what can be found now."""
    path = _write("scan-changed.txt", "beforeword")
    _scan_immediately()
    assert search_engine.bm25_search("beforeword") != []

    path.write_text("afterword", encoding="utf-8")
    # A change of content changes the size, so the size check alone
    # already catches it. Nothing to hide here.
    _scan_immediately()

    assert search_engine.bm25_search("afterword") != []
    assert search_engine.bm25_search("beforeword") == []


def test_a_deleted_file_leaves_the_index():
    """A file that is gone must stop being found."""
    path = _write("scan-deleted.txt", "goneword")
    _scan_immediately()
    assert search_engine.bm25_search("goneword") != []

    path.unlink()
    _scan_immediately()

    assert search_engine.bm25_search("goneword") == []
    assert str(path) not in _manifest_paths()


def test_a_moved_file_keeps_its_place():
    """A file that moved is still the same file.

    Its content did not change, so it should not be embedded a second
    time; the path it used to have should be gone.
    """
    path = _write("scan-moved-before.txt", "movedword")
    _scan_immediately()
    assert str(path) in _manifest_paths()

    new_path = path.with_name("scan-moved-after.txt")
    path.rename(new_path)
    _scan_immediately()

    paths = _manifest_paths()
    assert str(new_path) in paths
    assert str(path) not in paths
    # Still findable, under its new name.
    assert [r["name"] for r in search_engine.bm25_search("movedword")] == [
        "scan-moved-after.txt"
    ]


def test_a_file_still_being_written_waits():
    """A file the writer has not finished is not indexed yet.

    Indexing half a file leaves half the document searchable and the
    rest missing, and the writer is about to add more. The file waits
    for the debounce window to pass.
    """
    _write("scan-fresh.txt", "freshword")

    summary = _scan_immediately(debounce=3600)

    assert summary.get("pending", 0) >= 1
    assert search_engine.bm25_search("freshword") == []


def test_an_emptied_keyword_index_is_rebuilt():
    """The index can be emptied without the files changing.

    Clearing the search index, restoring a backup, a wiped search.db --
    none of those touch the files, so the usual "did the file change?"
    check says everything is fine, and the keyword channel stays empty
    for ever. The scan notices an empty index with files known, and
    starts again.
    """
    _write("scan-rebuild.txt", "rebuildword")
    _scan_immediately()
    assert search_engine.bm25_search("rebuildword") != []

    search_engine.clear()
    assert search_engine.bm25_search("rebuildword") == []

    _scan_immediately()

    assert search_engine.bm25_search("rebuildword") != []


def test_files_that_cannot_be_indexed_are_reported():
    """A file that is not indexable is not silently ignored.

    A person who indexed a whole library and got no hits for one file
    needs to be told why that file is missing.
    """
    unsupported = _write("scan-thing.exe", "nottext")
    oversized = _write("scan-big.txt", "x" * 500)

    import app.indexer as indexer_module

    original_ceiling = indexer_module.MAX_FILE_BYTES
    indexer_module.MAX_FILE_BYTES = 100
    try:
        _scan_immediately()
        skipped = indexer.skipped_files()
    finally:
        indexer_module.MAX_FILE_BYTES = original_ceiling

    unsupported_rows = [r for r in skipped["unsupported"] if r["path"].endswith("scan-thing.exe")]
    assert unsupported_rows, skipped
    assert unsupported_rows[0]["ext"] == ".exe"

    oversized_rows = [r for r in skipped["oversized"] if r["path"].endswith("scan-big.txt")]
    assert oversized_rows, skipped

    # And neither is searchable.
    assert search_engine.bm25_search("nottext") == []
    assert str(unsupported) not in _manifest_paths()
    assert str(oversized) not in _manifest_paths()


def test_contents_changed_behind_the_index_are_noticed():
    """The same size, the same timestamp, different contents.

    This is what a restore from a backup or an rsync that preserves
    timestamps does: the file's contents are replaced while its
    modification time and size stay exactly where they were. The size
    and time check says "nothing happened", and the index keeps
    describing a file that no longer exists in that form.
    """
    path = _write("scan-behind.txt", "behindword")
    _scan_immediately()
    assert search_engine.bm25_search("behindword") != []

    original = path.stat()
    # Same number of bytes, same modification time, new contents.
    path.write_text("different!", encoding="utf-8")
    os.utime(path, (original.st_atime, original.st_mtime))
    assert path.stat().st_size == original.st_size

    _scan_immediately()

    assert search_engine.bm25_search("different") != []
    assert search_engine.bm25_search("behindword") == []


def test_the_indexing_can_be_paused_and_resumed():
    """Pausing holds everything still; resuming lets it go again."""
    assert indexer.is_paused() is False

    indexer.pause(0)
    assert indexer.is_paused() is True
    assert indexer.pause_remaining() == -1

    indexer.resume()
    assert indexer.is_paused() is False
    assert indexer.pause_remaining() == 0


def test_a_pause_runs_its_course():
    """A pause with a countdown ends by itself."""
    indexer.pause(1)
    assert indexer.is_paused() is True
    assert indexer.pause_remaining() in (0, 1)

    time.sleep(1.1)
    assert indexer.is_paused() is False
