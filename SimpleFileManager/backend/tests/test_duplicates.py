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


def test_the_extra_copies_go_to_the_recycle_bin(finder):
    """Not to nowhere -- to the bin, so a mistake can be undone."""
    from app.trash import trash_root

    root = finder.storage_root
    (root / "a.txt").write_bytes(b"x" * 2000)
    (root / "b.txt").write_bytes(b"x" * 2000)
    (root / "c.txt").write_bytes(b"x" * 2000)
    _index(finder, "a.txt", 2000, "abc")
    _index(finder, "b.txt", 2000, "abc")
    _index(finder, "c.txt", 2000, "abc")

    out = finder.clean("abc")

    assert len(out["trashed"]) == 2
    assert (root / "a.txt").exists()
    assert not (root / "b.txt").exists()
    assert not (root / "c.txt").exists()
    # And it is in the bin.
    assert trash_root(root).exists()
    assert len(list(trash_root(root).iterdir())) == 2


def test_the_copy_that_is_named_is_the_one_kept(finder):
    root = finder.storage_root
    for name in ("a.txt", "b.txt", "c.txt"):
        (root / name).write_bytes(b"x" * 2000)
        _index(finder, name, 2000, "abc")

    out = finder.clean("abc", keep="c.txt")

    assert out["kept"].endswith("c.txt")
    assert (root / "c.txt").exists()
    assert not (root / "a.txt").exists()


def test_nothing_is_thrown_away_that_cannot_be_found_again(finder):
    root = finder.storage_root
    for name in ("a.txt", "b.txt"):
        (root / name).write_bytes(b"x" * 2000)
        _index(finder, name, 2000, "abc")

    out = finder.clean("abc")

    assert out["failed"] == []
    assert out["wasted"] == 2000
