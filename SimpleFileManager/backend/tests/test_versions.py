"""Tests for the version history.

The point of the store is that replacing a file can be taken back, so
the tests are about what survives a replacement and what a restore
does to the thing that was there.
"""

import pytest


@pytest.fixture
def store(tmp_path):
    from app.versions import VersionStore

    return VersionStore(tmp_path / ".versions"), tmp_path


def test_a_replaced_file_keeps_its_old_contents(store):
    svc, root = store
    target = root / "a.txt"
    target.write_text("first", encoding="utf-8")

    svc.snapshot(target, note="replaced")

    target.write_text("second", encoding="utf-8")
    versions = svc.list(target)
    assert len(versions) == 1
    assert versions[0]["note"] == "replaced"


def test_restoring_puts_the_old_contents_back(store):
    svc, root = store
    target = root / "a.txt"
    target.write_text("first", encoding="utf-8")
    version = svc.snapshot(target)

    target.write_text("second", encoding="utf-8")
    svc.restore(version["id"])

    assert target.read_text(encoding="utf-8") == "first"


def test_restoring_keeps_what_was_there(store):
    """A restore can itself be undone, so what it replaced is kept."""
    svc, root = store
    target = root / "a.txt"
    target.write_text("first", encoding="utf-8")
    version = svc.snapshot(target)

    target.write_text("second", encoding="utf-8")
    svc.restore(version["id"])

    versions = svc.list(target)
    assert len(versions) == 2
    assert any(v["note"] == "还原前的内容" for v in versions)


def test_the_same_content_is_stored_once(store):
    svc, root = store
    target = root / "a.txt"
    target.write_text("same", encoding="utf-8")
    svc.snapshot(target)
    svc.snapshot(target)

    blobs = list(svc.blobs.iterdir())
    assert len(blobs) == 1
    assert len(svc.list(target)) == 2


def test_a_missing_file_has_no_versions(store):
    svc, root = store
    assert svc.list(root / "nothing.txt") == []
    assert svc.snapshot(root / "nothing.txt") is None


def test_old_versions_are_dropped(store):
    svc, root = store
    from app.versions import MAX_VERSIONS_PER_FILE

    target = root / "a.txt"
    for i in range(MAX_VERSIONS_PER_FILE + 5):
        target.write_text(f"version {i}", encoding="utf-8")
        svc.snapshot(target)

    assert len(svc.list(target)) == MAX_VERSIONS_PER_FILE


def test_deleting_a_version_removes_its_blob(store):
    svc, root = store
    target = root / "a.txt"
    target.write_text("content", encoding="utf-8")
    version = svc.snapshot(target)

    svc.delete(version["id"])

    assert svc.list(target) == []
    assert list(svc.blobs.iterdir()) == []


def test_restoring_a_version_that_is_gone_fails(store):
    svc, root = store
    from app.versions import VersionError

    with pytest.raises(VersionError):
        svc.restore(9999)
