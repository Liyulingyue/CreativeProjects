"""Tests for sharing a link with somebody.

A share is a window onto one thing. It never reaches above the folder
that was shared, it can be locked with a password, it can be stopped,
and it can run out of downloads.
"""

import pytest

from conftest import client


@pytest.fixture
def tree():
    """A folder with a subfolder inside it, and a file in each."""
    from app.deps import get_storage_root

    root = get_storage_root() / "分享树"
    (root / "子目录").mkdir(parents=True, exist_ok=True)
    (root / "更深").mkdir(parents=True, exist_ok=True)
    (root / "顶层.txt").write_text("顶层", encoding="utf-8")
    (root / "子目录" / "里面.txt").write_text("里面", encoding="utf-8")
    (root / "更深" / "更深.txt").write_text("更深", encoding="utf-8")
    return root


@pytest.fixture
def share(tree):
    """A share of that folder."""
    res = client.post("/api/shares", json={"path": str(tree.resolve()), "expiry_days": 1})
    assert res.status_code == 200, res.text
    sid = res.json()["id"]
    yield sid
    client.delete(f"/api/shares/{sid}")


def test_the_link_shows_the_folder(share):
    res = client.get(f"/api/share/{share}")

    assert res.status_code == 200
    assert res.json()["is_dir"] is True


def test_a_visitor_sees_the_files(share):
    res = client.get(f"/api/share/{share}/list")

    assert res.status_code == 200
    assert sorted(i["name"] for i in res.json()["items"]) == ["子目录", "更深", "顶层.txt"]


def test_a_visitor_can_go_into_a_subfolder(share):
    """A shared folder may have folders inside it."""
    from urllib.parse import quote

    res = client.get(f"/api/share/{share}/list?path={quote('子目录')}")

    assert res.status_code == 200
    assert [i["name"] for i in res.json()["items"]] == ["里面.txt"]


def test_a_visitor_can_download_a_file_from_a_subfolder(share):
    from urllib.parse import quote

    res = client.get(f"/api/share/{share}/download?path={quote('子目录/里面.txt')}")

    assert res.status_code == 200
    assert res.content == "里面".encode()


def test_a_visitor_cannot_climb_out_of_the_shared_folder(share):
    from urllib.parse import quote

    for escape in ("../..", "子目录/../../..", "/etc/passwd"):
        listed = client.get(f"/api/share/{share}/list?path={quote(escape)}")
        fetched = client.get(f"/api/share/{share}/download?path={quote(escape)}")
        assert listed.status_code >= 400, escape
        assert fetched.status_code >= 400, escape


def test_a_locked_share_asks_for_the_password():
    from app.deps import get_storage_root

    folder = get_storage_root() / "锁着的"
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "a.txt").write_text("a", encoding="utf-8")
    made = client.post("/api/shares", json={"path": str(folder.resolve()), "password": "letmein"})
    sid = made.json()["id"]

    res = client.get(f"/api/share/{sid}/list")

    assert res.status_code == 401

    client.delete(f"/api/shares/{sid}")


def test_the_right_password_opens_it(share):
    # A fresh share with a password on it.
    from app.deps import get_storage_root

    folder = get_storage_root() / "带密码"
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "a.txt").write_text("a", encoding="utf-8")
    made = client.post("/api/shares", json={"path": str(folder.resolve()), "password": "letmein"})
    sid = made.json()["id"]

    assert client.get(f"/api/share/{sid}/list").status_code == 401

    unlocked = client.post(f"/api/share/{sid}/unlock", json={"password": "letmein"})
    assert unlocked.status_code == 200
    assert client.get(f"/api/share/{sid}/list").status_code == 200

    client.delete(f"/api/shares/{sid}")


def test_the_wrong_password_does_not_open_it(share):
    from app.deps import get_storage_root

    folder = get_storage_root() / "带密码2"
    folder.mkdir(parents=True, exist_ok=True)
    made = client.post("/api/shares", json={"path": str(folder.resolve()), "password": "letmein"})
    sid = made.json()["id"]

    assert client.post(f"/api/share/{sid}/unlock", json={"password": "nope"}).status_code == 401
    assert client.get(f"/api/share/{sid}/list").status_code == 401

    client.delete(f"/api/shares/{sid}")


def test_a_stopped_link_stops_working(share):
    client.delete(f"/api/shares/{share}")

    assert client.get(f"/api/share/{share}").status_code == 410
    assert client.get(f"/api/share/{share}/list").status_code == 410


def test_a_link_that_was_never_made_is_not_found():
    assert client.get("/api/share/nosuchlink").status_code == 404


class TestALinkFollowsItsFile:
    """A share opens one path. The file behind it can move."""

    def test_a_link_still_opens_the_file_after_it_moves(self):
        from app.deps import get_storage_root

        root = get_storage_root()
        (root / "分享的文件.txt").write_text("内容", encoding="utf-8")

        created = client.post("/api/shares", json={
            "path": str(root / "分享的文件.txt"),
            "password": "abcd",
            "expiry_days": 7,
        })
        assert created.status_code == 200, created.text
        share_id = created.json()["id"]

        moved = client.post("/api/fs/move", json={
            "src": str(root / "分享的文件.txt"), "dest": str(root / "搬走的文件.txt"),
        })
        assert moved.status_code == 200

        public = client.get(f"/api/share/{share_id}")
        assert public.status_code == 200, public.text
        assert public.json()["name"] == "搬走的文件.txt"
