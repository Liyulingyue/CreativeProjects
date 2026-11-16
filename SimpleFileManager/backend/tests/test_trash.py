"""Tests for the recycle bin, the storage numbers, and the trash routes.

The bin is the one place where a mistake is unrecoverable, so it gets
its own file rather than being folded into the file-system tests.
"""

import json
from pathlib import Path

import pytest

from tests.conftest import client
from tests.test_permissions import _login, people  # noqa: F401  (the fixture)


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


class TestTheBinRespectsTheHouseRules:
    """The bin is shared, but a shut-out folder is still shut out.

    The name of a file that came out of a private folder, and where it
    came from, are that folder's business.
    """

    def test_a_file_from_a_shut_out_folder_is_not_in_the_list(self, people):
        from app.deps import get_storage_root, state

        root = get_storage_root()
        (root / "私密").mkdir(exist_ok=True)
        (root / "私密" / "日记.txt").write_text("秘密", encoding="utf-8")
        (root / "公开").mkdir(exist_ok=True)
        (root / "公开" / "随手.txt").write_text("随便", encoding="utf-8")
        state.get_user_store().restrict("私密", people["one"]["id"], "none")

        # The admin puts both files in the bin.
        boss = _login("boss", "boss1234")
        admin_headers = {"Authorization": f"Bearer {boss}"}
        client.post("/api/fs/delete", json={"path": str(root / "私密" / "日记.txt")},
                    headers=admin_headers)
        client.post("/api/fs/delete", json={"path": str(root / "公开" / "随手.txt")},
                    headers=admin_headers)

        token = _login("甲", "jia1234")
        res = client.get("/api/fs/trash", headers={"Authorization": f"Bearer {token}"})

        names = [e["name"] for e in res.json()["entries"]]
        assert "随手.txt" in names
        assert "日记.txt" not in names, f"不该看见别人的文件: {names}"

    def test_a_shut_out_person_cannot_restore_it(self, people):
        from app.deps import get_storage_root, state

        root = get_storage_root()
        (root / "私密").mkdir(exist_ok=True)
        (root / "私密" / "日记.txt").write_text("秘密", encoding="utf-8")
        state.get_user_store().restrict("私密", people["one"]["id"], "none")

        boss = _login("boss", "boss1234")
        client.post("/api/fs/delete", json={"path": str(root / "私密" / "日记.txt")},
                    headers={"Authorization": f"Bearer {boss}"})
        entries = client.get("/api/fs/trash",
                              headers={"Authorization": f"Bearer {boss}"}).json()["entries"]
        entry = next(e for e in entries if e["name"] == "日记.txt")

        token = _login("甲", "jia1234")
        res = client.post(f"/api/fs/trash/{entry['id']}/restore",
                          headers={"Authorization": f"Bearer {token}"})

        assert res.status_code == 403
        assert not (root / "私密" / "日记.txt").exists()

    def test_emptying_the_bin_is_the_administrators_call(self, people):
        from app.deps import get_storage_root

        root = get_storage_root()
        (root / "随手.txt").write_text("随便", encoding="utf-8")
        boss = _login("boss", "boss1234")
        client.post("/api/fs/delete", json={"path": str(root / "随手.txt")},
                    headers={"Authorization": f"Bearer {boss}"})

        token = _login("甲", "jia1234")
        res = client.post("/api/fs/trash/empty",
                          headers={"Authorization": f"Bearer {token}"})

        assert res.status_code == 403
        boss = _login("boss", "boss1234")
        still_there = client.get("/api/fs/trash",
                                  headers={"Authorization": f"Bearer {boss}"})
        assert still_there.json()["count"] == 1
