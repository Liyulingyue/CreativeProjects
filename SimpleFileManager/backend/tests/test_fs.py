"""Tests for the filesystem router (browse, create, move, copy, delete, download, upload, content)."""
from tests.conftest import client, create_test_file, get_storage_root


class TestBrowse:
    def test_browse_root(self):
        create_test_file("test.txt", "hello world")
        res = client.get("/api/fs/browse")
        assert res.status_code == 200
        data = res.json()
        assert data["total_count"] >= 1
        assert any(i["name"] == "test.txt" for i in data["items"])

    def test_browse_nonexistent(self):
        res = client.get("/api/fs/browse", params={"path": "/nonexistent/path"})
        assert res.status_code in (400, 404)

    def test_browse_returns_dirs_first(self):
        create_test_file("z_file.txt")
        root = get_storage_root()
        (root / "a_folder").mkdir(exist_ok=True)
        res = client.get("/api/fs/browse")
        data = res.json()
        first_item = data["items"][0]
        assert first_item["is_dir"] is True

    def test_hidden_files_excluded(self):
        create_test_file(".hidden", "secret")
        res = client.get("/api/fs/browse")
        data = res.json()
        assert not any(i["name"] == ".hidden" for i in data["items"])


class TestCreateFolder:
    def test_create_folder(self):
        res = client.post("/api/fs/create_folder", json={"path": str(get_storage_root()), "name": "newdir"})
        assert res.status_code == 200
        assert (get_storage_root() / "newdir").is_dir()

    def test_create_duplicate_fails(self):
        client.post("/api/fs/create_folder", json={"path": str(get_storage_root()), "name": "dup"})
        res = client.post("/api/fs/create_folder", json={"path": str(get_storage_root()), "name": "dup"})
        assert res.status_code == 409


class TestMove:
    def test_move_file(self):
        create_test_file("source.txt", "content")
        root = get_storage_root()
        res = client.post("/api/fs/move", json={
            "src": str(root / "source.txt"),
            "dest": str(root / "dest.txt"),
        })
        assert res.status_code == 200
        assert not (root / "source.txt").exists()
        assert (root / "dest.txt").exists()


class TestCopy:
    def test_copy_file(self):
        create_test_file("original.txt", "content")
        root = get_storage_root()
        res = client.post("/api/fs/copy", json={
            "src": str(root / "original.txt"),
            "dest": str(root / "copy.txt"),
        })
        assert res.status_code == 200
        assert (root / "original.txt").exists()
        assert (root / "copy.txt").exists()


class TestDelete:
    def test_delete_file(self):
        create_test_file("doomed.txt", "bye")
        root = get_storage_root()
        res = client.post("/api/fs/delete", json={"path": str(root / "doomed.txt")})
        assert res.status_code == 200
        assert not (root / "doomed.txt").exists()

    def test_delete_nonexistent(self):
        res = client.post("/api/fs/delete", json={"path": str(get_storage_root() / "nope.txt")})
        assert res.status_code == 404


class TestDownload:
    def test_download_file(self):
        create_test_file("dl.txt", "download me")
        root = get_storage_root()
        res = client.get("/api/fs/download", params={"path": str(root / "dl.txt")})
        assert res.status_code == 200
        assert "download me" in res.text

    def test_download_nonexistent(self):
        res = client.get("/api/fs/download", params={"path": str(get_storage_root() / "nope.txt")})
        assert res.status_code == 404


class TestContent:
    def test_get_text_content(self):
        create_test_file("readme.md", "# Hello\nWorld")
        root = get_storage_root()
        res = client.get("/api/fs/content", params={"path": str(root / "readme.md")})
        assert res.status_code == 200
        data = res.json()
        assert "# Hello" in data["content"]
        assert data["name"] == "readme.md"

    def test_content_nonexistent(self):
        res = client.get("/api/fs/content", params={"path": str(get_storage_root() / "nope.txt")})
        assert res.status_code == 404


class TestUpload:
    def test_upload_file(self):
        root = get_storage_root()
        res = client.post(
            "/api/fs/upload",
            params={"path": str(root)},
            files=[("files", ("uploaded.txt", b"upload content", "text/plain"))],
        )
        assert res.status_code == 200
        data = res.json()
        assert data["count"] == 1
        assert (root / "uploaded.txt").exists()
        assert (root / "uploaded.txt").read_text() == "upload content"


class TestTree:
    def test_tree_structure(self):
        create_test_file("dir1/file1.txt", "a")
        create_test_file("dir1/file2.txt", "b")
        res = client.get("/api/fs/tree")
        assert res.status_code == 200
        data = res.json()
        assert data["is_dir"] is True
        children = data.get("children", [])
        assert any(c["name"] == "dir1" for c in children)


class TestPathSafety:
    def test_path_outside_root_rejected(self):
        res = client.get("/api/fs/content", params={"path": "../../../etc/passwd"})
        assert res.status_code in (400, 404)
