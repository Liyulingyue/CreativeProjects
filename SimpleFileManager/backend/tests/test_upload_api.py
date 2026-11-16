"""The upload API: what the endpoints accept, refuse and promise.

The pieces themselves are tested in test_uploads.py. What is missing
there is the layer the client actually talks to: the limits it
enforces, the words it uses when it says no, and the file it leaves
behind when something goes wrong half way.
"""
from pathlib import Path

import pytest

from tests.conftest import client, get_storage_root


def start_upload(name="a.txt", size=4, total_chunks=1, rel_dir="", fingerprint="4:100"):
    """Begin an upload through the API and return its answer."""
    root = get_storage_root()
    res = client.post("/api/fs/upload/init", params={
        "path": str(root), "name": name, "size": size,
        "total_chunks": total_chunks, "rel_dir": rel_dir,
        "fingerprint": fingerprint,
    })
    assert res.status_code == 200, res.text
    return res.json()


def send_chunk(upload_id, index, data):
    return client.post(
        "/api/fs/upload/chunk",
        params={"upload_id": upload_id, "index": index},
        content=data,
        headers={"Content-Type": "application/octet-stream"},
    )


class TestChunkedUpload:
    def test_a_file_arrives_in_pieces(self):
        info = start_upload(size=6, total_chunks=2)
        upload_id = info["upload_id"]
        assert send_chunk(upload_id, 0, b"abc").status_code == 200
        assert send_chunk(upload_id, 1, b"def").status_code == 200

        status = client.get("/api/fs/upload/status", params={"upload_id": upload_id})
        assert status.status_code == 200
        assert status.json()["received"] == [0, 1]
        assert status.json()["received_bytes"] == 6

        done = client.post("/api/fs/upload/complete", params={"upload_id": upload_id})
        assert done.status_code == 200
        assert (get_storage_root() / "a.txt").read_bytes() == b"abcdef"

    def test_the_status_of_an_upload_nobody_started_is_a_404(self):
        res = client.get("/api/fs/upload/status", params={"upload_id": "0" * 32})
        assert res.status_code == 404

    def test_a_piece_of_a_upload_that_does_not_exist_is_refused(self):
        res = send_chunk("0" * 32, 0, b"abc")
        assert res.status_code == 404
        assert "上传" in res.json()["detail"]

    def test_a_piece_that_is_too_big_is_refused(self):
        from app.uploads import UploadManager

        info = start_upload(size=10, total_chunks=1)
        too_big = b"x" * (UploadManager.MAX_CHUNK_BYTES + 1)
        res = send_chunk(info["upload_id"], 0, too_big)
        assert res.status_code == 413
        assert "MB" in res.json()["detail"]

    def test_the_file_is_not_put_in_place_until_every_piece_is_there(self):
        info = start_upload(size=6, total_chunks=2)
        send_chunk(info["upload_id"], 0, b"abc")
        done = client.post("/api/fs/upload/complete", params={"upload_id": upload_id_of(info)})
        assert done.status_code == 400
        assert "分片" in done.json()["detail"]
        assert not (get_storage_root() / "a.txt").exists()


def upload_id_of(info):
    return info["upload_id"]


class TestResume:
    def test_the_same_file_picks_up_where_it_left_off(self):
        first = start_upload(size=6, total_chunks=2)
        send_chunk(first["upload_id"], 0, b"abc")
        second = start_upload(size=6, total_chunks=2)
        assert second["upload_id"] == first["upload_id"]
        assert second["received"] == [0]

    def test_a_different_file_does_not_inherit_the_pieces(self):
        first = start_upload(size=6, total_chunks=2)
        send_chunk(first["upload_id"], 0, b"abc")
        second = start_upload(size=6, total_chunks=2, fingerprint="6:999")
        assert second["upload_id"] != first["upload_id"]
        assert second["received"] == []


class TestOverwrite:
    def _existing(self):
        root = get_storage_root()
        (root / "a.txt").write_text("old", encoding="utf-8")
        return root

    def test_a_name_that_is_taken_is_refused_with_the_details(self):
        root = self._existing()
        info = start_upload(name="a.txt", size=3, total_chunks=1)
        send_chunk(info["upload_id"], 0, b"new")
        res = client.post("/api/fs/upload/complete", params={"upload_id": info["upload_id"]})
        assert res.status_code == 409
        detail = res.json()["detail"]
        assert detail["message"] == "a.txt 已经存在"
        assert detail["existing"]["size"] == 3
        # The old file is still the old file.
        assert (root / "a.txt").read_text() == "old"

    def test_replace_puts_the_new_file_in_place(self):
        root = self._existing()
        info = start_upload(name="a.txt", size=3, total_chunks=1)
        send_chunk(info["upload_id"], 0, b"new")
        res = client.post(
            "/api/fs/upload/complete",
            params={"upload_id": info["upload_id"], "overwrite": "replace"},
        )
        assert res.status_code == 200
        assert (root / "a.txt").read_text() == "new"

    def test_an_unknown_way_of_overwriting_does_not_overwrite(self):
        root = self._existing()
        info = start_upload(name="a.txt", size=3, total_chunks=1)
        send_chunk(info["upload_id"], 0, b"new")
        res = client.post(
            "/api/fs/upload/complete",
            params={"upload_id": info["upload_id"], "overwrite": "yes-please"},
        )
        assert res.status_code == 400
        assert (root / "a.txt").read_text() == "old"

    def test_rename_writes_beside_the_original(self):
        root = self._existing()
        info = start_upload(name="a.txt", size=3, total_chunks=1)
        send_chunk(info["upload_id"], 0, b"new")
        res = client.post(
            "/api/fs/upload/complete",
            params={"upload_id": info["upload_id"], "overwrite": "rename"},
        )
        assert res.status_code == 200
        assert res.json()["name"] == "a(1).txt"
        assert (root / "a(1).txt").read_text() == "new"
        assert (root / "a.txt").read_text() == "old"


class TestAFolderInTheWay:
    def test_a_folder_with_the_same_name_is_not_replaced(self):
        root = get_storage_root()
        (root / "folder").mkdir()
        info = start_upload(name="folder", size=3, total_chunks=1)
        send_chunk(info["upload_id"], 0, b"new")
        res = client.post(
            "/api/fs/upload/complete",
            params={"upload_id": info["upload_id"], "overwrite": "replace"},
        )
        assert res.status_code == 400
        assert "文件夹" in res.json()["detail"]
        assert (root / "folder").is_dir()

    def test_the_one_shot_upload_says_the_same(self):
        root = get_storage_root()
        (root / "folder").mkdir()
        res = client.post(
            "/api/fs/upload",
            params={"path": str(root), "overwrite": "replace"},
            files=[("files", ("folder", b"new", "text/plain"))],
        )
        assert res.status_code == 409
        assert (root / "folder").is_dir()


class TestInit:
    def test_a_chunk_count_of_zero_is_refused(self):
        res = client.post("/api/fs/upload/init", params={
            "path": str(get_storage_root()), "name": "a.txt",
            "size": 4, "total_chunks": 0,
        })
        assert res.status_code == 400
        assert res.json()["detail"] == "分片数量不合法"

    def test_a_name_that_is_only_dots_is_refused(self):
        res = client.post("/api/fs/upload/init", params={
            "path": str(get_storage_root()), "name": "..",
            "size": 4, "total_chunks": 1,
        })
        assert res.status_code == 400
        assert res.json()["detail"] == "文件名不合法"

    def test_a_folder_cannot_climb_out_of_the_target(self):
        res = client.post("/api/fs/upload/init", params={
            "path": str(get_storage_root()), "name": "a.txt",
            "size": 4, "total_chunks": 1, "rel_dir": "../../etc",
        })
        assert res.status_code == 400
        assert "之外" in res.json()["detail"]

    def test_the_staging_area_is_not_in_the_file_browsing(self):
        start_upload(size=4, total_chunks=1)
        res = client.get("/api/fs/browse", params={"path": str(get_storage_root())})
        names = [i["name"] for i in res.json()["items"]]
        assert ".uploads" not in names


class TestAbandon:
    def test_giving_up_throws_the_pieces_away(self):
        info = start_upload(size=6, total_chunks=2)
        send_chunk(info["upload_id"], 0, b"abc")
        res = client.delete("/api/fs/upload", params={"upload_id": info["upload_id"]})
        assert res.status_code == 200
        status = client.get("/api/fs/upload/status", params={"upload_id": info["upload_id"]})
        assert status.status_code == 404
        assert not (get_storage_root() / "a.txt").exists()

    def test_giving_up_does_not_leave_an_empty_folder_behind(self):
        info = start_upload(name="a.txt", size=4, total_chunks=1, rel_dir="照片/2024")
        client.delete("/api/fs/upload", params={"upload_id": info["upload_id"]})
        assert not (get_storage_root() / "照片" / "2024").exists()


class TestOneShotUpload:
    def test_a_file_arrives(self):
        root = get_storage_root()
        res = client.post(
            "/api/fs/upload",
            params={"path": str(root)},
            files=[("files", ("one.txt", b"hello", "text/plain"))],
        )
        assert res.status_code == 200
        assert (root / "one.txt").read_bytes() == b"hello"

    def test_a_failed_upload_leaves_no_half_file_behind(self, monkeypatch):
        root = get_storage_root()
        real_open = open

        def explode(self, *args, **kwargs):
            f = real_open(self, *args, **kwargs)
            if str(self).endswith(".uploading"):
                raise OSError("the disk gave up")
            return f

        monkeypatch.setattr("builtins.open", explode)
        with pytest.raises(Exception):
            client.post(
                "/api/fs/upload",
                params={"path": str(root)},
                files=[("files", ("one.txt", b"hello", "text/plain"))],
            )
        monkeypatch.undo()
        # Nothing half-written is sitting under the name.
        assert not (root / "one.txt").exists()
        leftovers = [p.name for p in root.iterdir() if p.name.endswith(".uploading")]
        assert leftovers == []

    def test_the_name_may_not_escape_the_destination(self):
        root = get_storage_root()
        res = client.post(
            "/api/fs/upload",
            params={"path": str(root)},
            files=[("files", ("../escaped.txt", b"no", "text/plain"))],
        )
        assert res.status_code == 400
        assert not (root.parent / "escaped.txt").exists()

    def test_the_saying_is_in_chinese(self):
        root = get_storage_root()
        res = client.post(
            "/api/fs/upload",
            params={"path": str(root)},
            files=[("files", ("../escaped.txt", b"no", "text/plain"))],
        )
        detail = res.json()["detail"]
        assert detail == "文件名不合法"
