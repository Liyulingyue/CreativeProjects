"""Tests for what the index must never hold.

Two ways a row can outlive its file, both of which leave something
searchable that is not there: indexing the directories that hold
deleted and half-written files, and recording a file that vanishes
between being read and being written down.
"""

import sqlite3

from app.deps import DATA_DIR, get_storage_root, state
from app.indexer import _is_hidden, indexer
from app.search_engine import search_engine


def _manifest_paths() -> set[str]:
    conn = sqlite3.connect(str(DATA_DIR / "file_index.db"))
    try:
        return {r[0] for r in conn.execute("SELECT path FROM file_index")}
    finally:
        conn.close()


def _scan_immediately():
    """Scan without waiting out the debounce a fresh file would hit."""
    settings = state.get_settings()
    original = settings.index_debounce_seconds
    settings.index_debounce_seconds = 0
    try:
        return indexer.scan()
    finally:
        settings.index_debounce_seconds = original


def test_a_hidden_directory_is_hidden_at_any_depth():
    """`.trash/2026/data/a.txt` is a deleted file, not a document.

    Only guarding the last path component let the trash and the
    upload staging area into the index, so a file the user had
    deleted was still returned by search.
    """
    root = get_storage_root()

    assert _is_hidden(root / ".trash" / "x" / "a.txt")
    assert _is_hidden(root / ".uploads" / "x" / "000000.part")
    assert _is_hidden(root / "notes" / ".cache" / "a.txt")
    assert not _is_hidden(root / "notes" / "a.txt")
    assert not _is_hidden(root / "notes" / "file.backup")


def test_deleted_and_staged_files_are_not_indexed():
    """Nothing under a dot-directory ends up searchable."""
    root = get_storage_root()
    visible = root / "hygiene-visible.txt"
    trashed = root / ".trash" / "somewhere" / "hygiene-trashed.txt"
    staged = root / ".uploads" / "abc" / "hygiene-staged.txt"
    for path, text in (
        (visible, "visibleword here"),
        (trashed, "trashedword here"),
        (staged, "stagedword here"),
    ):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    try:
        _scan_immediately()

        paths = _manifest_paths()
        assert str(visible) in paths
        assert str(trashed) not in paths
        assert str(staged) not in paths

        assert [r["name"] for r in search_engine.bm25_search("visibleword")] == [
            "hygiene-visible.txt"
        ]
        assert search_engine.bm25_search("trashedword") == []
        assert search_engine.bm25_search("stagedword") == []
    finally:
        for path in (visible, trashed, staged):
            path.unlink(missing_ok=True)
        for directory in (root / ".trash" / "somewhere", root / ".uploads" / "abc"):
            try:
                directory.rmdir()
            except OSError:
                pass


def test_a_file_that_vanishes_mid_index_leaves_no_keyword_row(monkeypatch):
    """A half-written index is worse than none.

    The keyword row used to be written before the file's timestamp
    was read. A file deleted in that window left a row in the keyword
    index that the manifest had never heard of, so no later scan could
    remove it -- the search kept returning a file that was gone.
    """
    root = get_storage_root()
    path = root / "hygiene-vanishing.txt"
    path.write_text("vanishingword here")

    real_path = str(path)
    original_read = indexer._read_content

    def read_then_vanish(p):
        content = original_read(p)
        if p == real_path:
            # Read, then deleted before the file is written down --
            # the window in which the old code wrote a keyword row
            # first and then failed to record it.
            path.unlink(missing_ok=True)
        return content

    monkeypatch.setattr(indexer, "_read_content", read_then_vanish)
    try:
        _scan_immediately()
    finally:
        monkeypatch.setattr(indexer, "_read_content", original_read)
        path.unlink(missing_ok=True)

    assert real_path not in _manifest_paths()
    assert search_engine.bm25_search("vanishingword") == []
