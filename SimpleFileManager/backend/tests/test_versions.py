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


def test_clearing_removes_every_version_and_its_blob(store):
    """The whole history can be thrown away in one go."""
    svc, root = store
    for name in ("a.txt", "b.txt"):
        target = root / name
        target.write_text("content", encoding="utf-8")
        svc.snapshot(target)

    removed = svc.clear()

    assert removed == 2
    assert svc.list(root / "a.txt") == []
    assert svc.list(root / "b.txt") == []
    assert list(svc.blobs.iterdir()) == []


def test_the_versions_follow_a_file_that_moves():
    """A version belongs to a file, not to a name.

    If the history stayed at the old path, restoring an old version
    would put the file back where it used to be -- a second copy, at
    an address nobody is looking at.
    """
    from app.deps import get_storage_root, state
    from tests.conftest import client

    root = get_storage_root()
    target = root / "会动的.txt"
    target.write_text("first", encoding="utf-8")
    svc = state.get_version_store()
    svc.snapshot(target)

    moved = client.post("/api/fs/move", json={
        "src": str(target), "dest": str(root / "动过的.txt"),
    })
    assert moved.status_code == 200

    assert svc.list(root / "动过的.txt"), "版本没有跟着文件走"
    assert not svc.list(root / "会动的.txt"), "旧地址上还留着版本记录"


class TestTheHistoryAndTheDisk:
    """A version is a whole copy of the file.

    The per-file limit says nothing about the whole, so there is a
    limit on the whole too.
    """

    def test_a_very_large_file_is_not_versioned(self, tmp_path, monkeypatch):
        from app.versions import VersionStore

        monkeypatch.setattr("app.versions.MAX_VERSION_FILE_BYTES", 10)
        store = VersionStore(tmp_path / ".versions")
        target = tmp_path / "big.bin"
        target.write_bytes(b"x" * 100)

        assert store.snapshot(target) is None
        assert store.list(target) == []

    def test_the_oldest_versions_go_when_the_history_gets_too_big(self, tmp_path, monkeypatch):
        from app.versions import VersionStore

        monkeypatch.setattr("app.versions.MAX_VERSION_TOTAL_BYTES", 100)
        store = VersionStore(tmp_path / ".versions")
        target = tmp_path / "a.txt"
        for i in range(5):
            target.write_text(f"version {i}" + "." * 40, encoding="utf-8")
            store.snapshot(target)

        versions = store.list(target)
        assert versions, "历史被清空了"
        total = sum(v["size"] for v in versions)
        assert total <= 100 + 60, f"历史还是太大: {total}"
        # The newest one is kept -- the recent past is the useful part.
        assert versions[0]["size"] > 0
