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
    """Put a file in the index, the way the indexer would.

    The file is really there, and the index records the time and the
    size it had. The finder needs those to tell a copy from a file
    that has been changed since -- an index that says a file is a
    duplicate is only true of the file as it was when it was looked at.
    """
    import sqlite3

    target = finder.storage_root / path
    if not target.exists():
        target.write_bytes(b"x" * size)
    st = target.stat()
    conn = sqlite3.connect(finder.index_db_path)
    conn.execute(
        "INSERT OR REPLACE INTO file_index (path, mtime, size, indexed_at, content_hash)"
        " VALUES (?, ?, ?, 0, ?)",
        (path, st.st_mtime, st.st_size, digest),
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


class TestAFileThatHasChanged:
    """The index's hash is true of the file as it was when it was looked at.

    A file somebody edited after the index last saw it is not a copy of
    anything, and putting it in the bin throws the edit away.
    """

    def test_a_changed_file_is_not_offered_as_a_duplicate(self, finder):
        """The page must not offer a file that is no longer a copy."""
        _index(finder, "a.txt", 2000, "abc")
        _index(finder, "b.txt", 2000, "abc")

        # b.txt is edited. The index has not been rebuilt yet.
        (finder.storage_root / "b.txt").write_bytes(b"y" * 2000)

        out = finder.find()
        assert out["count"] == 0, f"a changed file was offered as a copy: {out}"

    def test_a_changed_file_is_not_thrown_away(self, finder):
        """The clean-up leaves it alone, and says why."""
        _index(finder, "a.txt", 2000, "abc")
        _index(finder, "b.txt", 2000, "abc")

        (finder.storage_root / "b.txt").write_bytes(b"y" * 2000)

        out = finder.clean("abc", keep="a.txt")
        assert out["trashed"] == [], f"a changed file was thrown away: {out}"
        assert (finder.storage_root / "b.txt").exists()
        assert out["failed"], "it should say why it left the file alone"
        assert "变" in out["failed"][0]["error"]

    def test_it_is_the_contents_that_decide_and_not_the_history(self, finder):
        """A file that was edited and edited back is still a copy.

        What matters is what is in the file now. The time it was
        touched is only a hint that something might have changed --
        when it has, the file is read and hashed again, and that hash
        is the answer.
        """
        import hashlib

        content = b"x" * 2000
        digest = hashlib.sha256(content).hexdigest()
        (finder.storage_root / "a.txt").write_bytes(content)
        (finder.storage_root / "b.txt").write_bytes(content)
        _index(finder, "a.txt", 2000, digest)
        _index(finder, "b.txt", 2000, digest)

        # b.txt is edited, and then edited back.
        (finder.storage_root / "b.txt").write_bytes(b"y" * 2000)
        (finder.storage_root / "b.txt").write_bytes(content)

        out = finder.clean(digest, keep="a.txt")
        assert len(out["trashed"]) == 1, out
        assert not (finder.storage_root / "b.txt").exists()

    def test_a_file_that_kept_its_size_but_not_its_contents_is_spared(self, finder):
        """Same size, different contents: only the hash can tell.

        The cheap check -- size and time -- would pass if the time
        were the same, so the file is read and hashed again.
        """
        import hashlib

        content = b"x" * 2000
        digest = hashlib.sha256(content).hexdigest()
        (finder.storage_root / "a.txt").write_bytes(content)
        (finder.storage_root / "b.txt").write_bytes(content)
        _index(finder, "a.txt", 2000, digest)
        _index(finder, "b.txt", 2000, digest)

        # b.txt is edited. The size is the same; the time is not.
        (finder.storage_root / "b.txt").write_bytes(b"y" * 2000)

        out = finder.clean(digest, keep="a.txt")
        assert out["trashed"] == [], f"a changed file was thrown away: {out}"
        assert (finder.storage_root / "b.txt").exists()
        assert (finder.storage_root / "b.txt").read_bytes() == b"y" * 2000
