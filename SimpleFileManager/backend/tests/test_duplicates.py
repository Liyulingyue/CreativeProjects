"""Tests for finding the copies of a file that are here twice.

They are found by what is inside them, so the names do not have to
match. And a copy that somebody may not open is not a copy they can
get rid of, so it is not counted.
"""

import pytest


@pytest.fixture
def finder(tmp_path):
    """A duplicate finder over a small index."""
    import sqlite3

    from app.duplicates import DuplicateFinder

    root = tmp_path / "storage"
    root.mkdir()
    db_path = tmp_path / "file_index.db"

    conn = sqlite3.connect(db_path)
    conn.execute(
        "CREATE TABLE file_index (path TEXT PRIMARY KEY, mtime REAL, size INTEGER,"
        " indexed_at INTEGER, content_hash TEXT)"
    )
    conn.commit()
    conn.close()

    f = DuplicateFinder(root)
    f.index_db_path = str(db_path)
    return f


def _index(finder, path, size, digest):
    import sqlite3

    conn = sqlite3.connect(finder.index_db_path)
    conn.execute(
        "INSERT OR REPLACE INTO file_index (path, mtime, size, indexed_at, content_hash)"
        " VALUES (?, 0, ?, 0, ?)",
        (path, size, digest),
    )
    conn.commit()
    conn.close()


def test_two_copies_of_the_same_file_are_found(finder):
    _index(finder, "a.txt", 2000, "abc")
    _index(finder, "b.txt", 2000, "abc")

    out = finder.find()

    assert out["count"] == 1
    assert out["groups"][0]["copies"] == 2
    assert [f["name"] for f in out["groups"][0]["files"]] == ["a.txt", "b.txt"]


def test_the_room_the_copies_are_wasting_is_worked_out(finder):
    _index(finder, "a.txt", 2000, "abc")
    _index(finder, "b.txt", 2000, "abc")
    _index(finder, "c.txt", 2000, "abc")

    out = finder.find()

    # Every copy but one is space being spent for nothing.
    assert out["wasted"] == 4000
    assert out["groups"][0]["wasted"] == 4000


def test_files_that_differ_are_not_duplicates(finder):
    _index(finder, "a.txt", 2000, "abc")
    _index(finder, "b.txt", 2000, "def")

    assert finder.find()["count"] == 0


def test_a_tiny_file_is_not_worth_grouping(finder):
    _index(finder, "a.txt", 10, "abc")
    _index(finder, "b.txt", 10, "abc")

    assert finder.find()["count"] == 0


def test_a_file_without_a_hash_is_not_in_the_answer(finder):
    _index(finder, "a.txt", 2000, None)
    _index(finder, "b.txt", 2000, None)

    assert finder.find()["count"] == 0
