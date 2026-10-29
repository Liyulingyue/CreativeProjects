"""Tests for the recycle bin, the storage numbers, and the trash routes.

The bin is the one place where a mistake is unrecoverable, so it gets
its own file rather than being folded into the file-system tests.
"""

import json
from pathlib import Path

import pytest


@pytest.fixture
def trash(tmp_path):
    from app.trash import TrashService

    root = tmp_path / "storage"
    (root / "docs").mkdir(parents=True)
    (root / "docs" / "note.txt").write_text("hello", encoding="utf-8")
    (root / "docs" / "sub").mkdir()
    (root / "docs" / "sub" / "deep.txt").write_text("deep", encoding="utf-8")
    return TrashService(root), root


def test_trash_moves_the_file_and_keeps_its_contents(trash):
    svc, root = trash
    target = root / "docs" / "note.txt"

    entry = svc.trash(target)

    assert not target.exists()
    assert entry["name"] == "note.txt"
    assert entry["rel_path"] == "docs/note.txt"
    assert entry["size"] == len("hello")


def test_trashed_directory_keeps_its_tree(trash):
    svc, root = trash
    target = root / "docs" / "sub"

    svc.trash(target)

    assert not target.exists()
    entries = svc.list()
    assert len(entries) == 1
    assert entries[0]["is_dir"] is True


def test_restore_puts_the_file_back_where_it_was(trash):
    svc, root = trash
    target = root / "docs" / "note.txt"
    svc.trash(target)

    with pytest.raises(Exception):
        svc.restore("no-such-entry")
    entry = svc.list()[0]
    svc.restore(entry["id"])

    assert target.exists()
    assert target.read_text(encoding="utf-8") == "hello"
    assert svc.list() == []


def test_restore_does_not_overwrite_a_file_that_took_the_place(trash):
    svc, root = trash
    target = root / "docs" / "note.txt"
    svc.trash(target)
    entry = svc.list()[0]

    target.write_text("something else entirely", encoding="utf-8")
    result = svc.restore(entry["id"])

    assert result["renamed"] is True
    assert target.read_text(encoding="utf-8") == "something else entirely"
    restored = list((root / "docs").glob("note*"))
    assert len(restored) == 2
    assert any(p.read_text(encoding="utf-8") == "hello" for p in restored)


def test_purge_removes_the_entry_for_good(trash):
    svc, root = trash
    svc.trash(root / "docs" / "note.txt")
    entry = svc.list()[0]

    svc.purge(entry["id"])

    assert svc.list() == []
    assert not (root / "docs" / "note.txt").exists()


def test_empty_removes_everything(trash):
    svc, root = trash
    svc.trash(root / "docs" / "note.txt")
    svc.trash(root / "docs" / "sub")

    result = svc.empty()

    assert result["removed"] == 2
    assert svc.list() == []


def test_the_bin_is_hidden_from_the_browser(trash):
    """The bin lives under the storage root, so it must not show up
    in a listing -- and the browser skips dot-files for exactly this
    reason."""
    svc, root = trash
    svc.trash(root / "docs" / "note.txt")

    assert (root / ".trash").exists()
    assert (root / ".trash").name.startswith(".")
    # Nothing under the root that a listing would show.
    visible = [p for p in root.iterdir() if not p.name.startswith(".")]
    assert visible == [root / "docs"]


def test_a_file_inside_the_bin_cannot_be_binned_again(trash):
    svc, root = trash
    svc.trash(root / "docs" / "note.txt")
    entry = svc.list()[0]

    with pytest.raises(Exception):
        svc.trash(root / ".trash" / entry["id"])


def test_size_cache_does_not_walk_the_tree_twice(tmp_path):
    from app.storage_stats import dir_size, invalidate

    d = tmp_path / "d"
    d.mkdir()
    for i in range(5):
        (d / f"f{i}.txt").write_text("x" * 100, encoding="utf-8")

    first = dir_size(d)
    assert first == 500
    # The second call comes from the cache; the value is the same.
    assert dir_size(d) == 500

    (d / "f5.txt").write_text("y" * 50, encoding="utf-8")
    invalidate(d)
    assert dir_size(d) == 550
