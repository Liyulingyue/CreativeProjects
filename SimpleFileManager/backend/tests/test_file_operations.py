"""Moving, copying and deleting: what may be overwritten, and what follows.

These are the operations that move a person's files around. Two things
matter here and both were broken at some point: a file that is already
there must not be replaced without asking, and the indexes have to
follow the file to its new address -- otherwise a search keeps offering
a path that is no longer there.
"""
import pytest

from tests.conftest import client, create_test_file, get_storage_root


def move(src, dest, **extra):
    body = {"src": str(src), "dest": str(dest)}
    body.update(extra)
    return client.post("/api/fs/move", json=body)


def copy(src, dest, **extra):
    body = {"src": str(src), "dest": str(dest)}
    body.update(extra)
    return client.post("/api/fs/copy", json=body)


class TestCopy:
    def test_a_file_is_copied(self):
        root = get_storage_root()
        create_test_file("a.txt", "内容")
        res = copy(root / "a.txt", root / "b.txt")
        assert res.status_code == 200
        assert (root / "b.txt").read_text() == "内容"
        assert (root / "a.txt").exists(), "原件不该没了"

    def test_an_existing_file_is_not_replaced_without_asking(self):
        root = get_storage_root()
        create_test_file("a.txt", "新的")
        create_test_file("b.txt", "旧的")
        res = copy(root / "a.txt", root / "b.txt")
        assert res.status_code == 409
        assert (root / "b.txt").read_text() == "旧的", "旧文件被悄悄盖掉了"
        assert res.json()["detail"]["existing"]["name"] == "b.txt"

    def test_replace_puts_the_new_file_in_place_and_keeps_the_old_one(self):
        root = get_storage_root()
        create_test_file("a.txt", "新的")
        create_test_file("b.txt", "旧的")
        res = copy(root / "a.txt", root / "b.txt", overwrite="replace")
        assert res.status_code == 200
        assert (root / "b.txt").read_text() == "新的"
        # The old one can be got back.
        versions = client.get("/api/fs/versions", params={"path": str(root / "b.txt")})
        assert versions.json()["versions"]

    def test_rename_writes_beside_the_original(self):
        root = get_storage_root()
        create_test_file("a.txt", "内容")
        create_test_file("b.txt", "旧的")
        res = copy(root / "a.txt", root / "b.txt", overwrite="rename")
        assert res.status_code == 200
        assert res.json()["path"].endswith("b(1).txt")
        assert (root / "b.txt").read_text() == "旧的"
        assert (root / "b(1).txt").read_text() == "内容"

    def test_an_unknown_way_of_overwriting_does_not_overwrite(self):
        root = get_storage_root()
        create_test_file("a.txt", "新的")
        create_test_file("b.txt", "旧的")
        res = copy(root / "a.txt", root / "b.txt", overwrite="yes-please")
        assert res.status_code == 400
        assert (root / "b.txt").read_text() == "旧的"


class TestMove:
    def test_a_file_is_moved(self):
        root = get_storage_root()
        create_test_file("a.txt", "内容")
        res = move(root / "a.txt", root / "b.txt")
        assert res.status_code == 200
        assert (root / "b.txt").read_text() == "内容"
        assert not (root / "a.txt").exists()

    def test_an_existing_file_is_not_replaced_without_asking(self):
        root = get_storage_root()
        create_test_file("a.txt", "新的")
        create_test_file("b.txt", "旧的")
        res = move(root / "a.txt", root / "b.txt")
        assert res.status_code == 409
        assert (root / "b.txt").read_text() == "旧的"
        assert (root / "a.txt").exists(), "源文件不该动"

    def test_a_folder_cannot_be_moved_into_itself(self):
        root = get_storage_root()
        (root / "文件夹").mkdir()
        (root / "文件夹" / "里面.txt").write_text("x", encoding="utf-8")
        res = move(root / "文件夹", root / "文件夹" / "自己")
        assert res.status_code == 400
        assert (root / "文件夹" / "里面.txt").exists()


class TestTheIndexFollows:
    def test_a_moved_file_is_found_under_its_new_name(self):
        """A search that keeps offering the old path is a search that
        sends the user to a file that is not there."""
        from app.search_engine import search_engine

        root = get_storage_root()
        create_test_file("原来.txt", "zidiaozaizheli")
        # Index it the way the indexer does.
        search_engine.upsert_file(str(root / "原来.txt"), "zidiaozaizheli")

        res = move(root / "原来.txt", root / "后来.txt")
        assert res.status_code == 200

        hits = [h["path"] for h in search_engine.bm25_search("zidiao")]
        assert str(root / "后来.txt") in hits, f"索引没跟上: {hits}"
        assert str(root / "原来.txt") not in hits, f"索引里还留着旧路径: {hits}"

    def test_a_batch_move_also_makes_the_index_follow(self):
        from app.search_engine import search_engine

        root = get_storage_root()
        (root / "去的地方").mkdir()
        create_test_file("甲.txt", "zidiaozaizheli")
        search_engine.upsert_file(str(root / "甲.txt"), "zidiaozaizheli")

        res = client.post("/api/fs/batch", json={
            "action": "move",
            "paths": [str(root / "甲.txt")],
            "dest": str(root / "去的地方"),
        })
        assert res.status_code == 200
        assert res.json()["failed_count"] == 0

        hits = [h["path"] for h in search_engine.bm25_search("zidiao")]
        assert str(root / "去的地方" / "甲.txt") in hits, f"索引没跟上: {hits}"


class TestTheIndexFollowsARestore:
    def test_a_restored_file_that_came_back_under_another_name_is_still_findable(self):
        """Something else took its place while it was in the bin.

        The file comes back under a different name, and the indexes
        have to know the new name -- otherwise a search keeps offering
        a path that is not there.
        """
        from app.search_engine import search_engine

        root = get_storage_root()
        create_test_file("原来.txt", "zidiaozaizheli")
        search_engine.upsert_file(str(root / "原来.txt"), "zidiaozaizheli")

        res = client.post("/api/fs/delete", json={"path": str(root / "原来.txt")})
        assert res.status_code == 200
        # Something else takes the name.
        create_test_file("原来.txt", "别人的文件")

        entries = client.get("/api/fs/trash").json()["entries"]
        entry = next(e for e in entries if e["name"] == "原来.txt")
        restored = client.post(f"/api/fs/trash/{entry['id']}/restore")
        assert restored.status_code == 200
        new_path = restored.json()["path"]
        assert new_path.endswith("原来(恢复1).txt")

        hits = [h["path"] for h in search_engine.bm25_search("zidiao")]
        assert new_path in hits, f"索引没跟上: {hits}"


class TestBatch:
    def test_a_batch_copy_does_not_overwrite(self):
        root = get_storage_root()
        create_test_file("a.txt", "新的")
        create_test_file("b.txt", "旧的")
        res = client.post("/api/fs/batch", json={
            "action": "copy",
            "paths": [str(root / "a.txt")],
            "dest": str(root),
        })
        assert res.status_code == 200
        assert res.json()["failed_count"] == 1
        assert "已经存在" in res.json()["failed"][0]["error"]
        assert (root / "b.txt").read_text() == "旧的"

    def test_a_batch_failure_says_what_happened_in_chinese(self):
        root = get_storage_root()
        create_test_file("a.txt", "内容")
        res = client.post("/api/fs/batch", json={
            "action": "copy",
            "paths": [str(root / "a.txt")],
            "dest": str(root / "没有这个目录"),
        })
        assert res.status_code == 200
        error = res.json()["failed"][0]["error"]
        assert "目标不是一个目录" in error
        assert "Error" not in error and "Traceback" not in error


class TestPermanentDelete:
    def test_the_old_versions_of_a_destroyed_file_go_with_it(self):
        """A version that cannot be restored is dead weight on the disk."""
        root = get_storage_root()
        create_test_file("a.txt", "第一版")
        from app.deps import state

        state.get_version_store().snapshot(root / "a.txt")
        assert state.get_version_store().list(root / "a.txt")

        res = client.post("/api/fs/delete", json={"path": str(root / "a.txt"), "permanent": True})
        assert res.status_code == 200
        assert not state.get_version_store().list(root / "a.txt")
