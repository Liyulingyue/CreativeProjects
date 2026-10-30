"""Tests for the files somebody starred.

The list is one person's -- "the ones I want to find again" is a
private sort of thing -- even on a machine the whole family uses. And
a starred file that has since been made private is not shown, because
there is nothing to open and its name is not theirs to read.
"""

import pytest

from conftest import client


@pytest.fixture
def two_people(tmp_path, monkeypatch):
    """Two ordinary people, each with their own list."""
    import importlib

    from app.deps import state

    auth_module = importlib.import_module("app.routers.auth")
    monkeypatch.setattr(auth_module, "AUTH_FILE", tmp_path / "auth.json")
    monkeypatch.setattr(auth_module, "_auth_cache", {"mtime": None, "data": None})

    store = state.get_user_store()
    for user in store.list_users():
        if user["username"] != "admin":
            store.delete_user(user["id"])

    one = store.create_user("甲", "jia1234", role="user")
    two = store.create_user("乙", "yi1234", role="user")
    auth_module.set_password("boss1234")
    return one, two


def _login(username, password):
    res = client.post("/api/auth/login", json={"username": username, "password": password})
    assert res.status_code == 200, res.text
    return res.json()["token"]


def _make(relative: str, content: str = "hello"):
    from app.deps import get_storage_root

    path = get_storage_root() / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    return path


def test_a_file_can_be_starred(two_people):
    _make("全家/照片.jpg")
    token = _login("甲", "jia1234")

    res = client.post("/api/favorites", json={"path": "全家/照片.jpg"},
                      headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 200
    assert res.json()["name"] == "照片.jpg"


def test_the_starred_file_comes_back(two_people):
    _make("全家/照片.jpg")
    token = _login("甲", "jia1234")

    client.post("/api/favorites", json={"path": "全家/照片.jpg"},
                headers={"Authorization": f"Bearer {token}"})

    res = client.get("/api/favorites", headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 200
    assert [f["name"] for f in res.json()["favorites"]] == ["照片.jpg"]


def test_the_list_is_yours_alone(two_people):
    _make("全家/照片.jpg")
    one = _login("甲", "jia1234")
    two = _login("乙", "yi1234")

    client.post("/api/favorites", json={"path": "全家/照片.jpg"},
                headers={"Authorization": f"Bearer {one}"})

    theirs = client.get("/api/favorites", headers={"Authorization": f"Bearer {two}"})

    assert theirs.json()["favorites"] == []


def test_a_star_can_be_taken_off(two_people):
    _make("全家/照片.jpg")
    token = _login("甲", "jia1234")

    client.post("/api/favorites", json={"path": "全家/照片.jpg"},
                headers={"Authorization": f"Bearer {token}"})

    removed = client.delete("/api/favorites", params={"path": "全家/照片.jpg"},
                            headers={"Authorization": f"Bearer {token}"})
    assert removed.status_code == 200

    listed = client.get("/api/favorites", headers={"Authorization": f"Bearer {token}"})
    assert listed.json()["favorites"] == []


def test_a_starred_file_that_has_gone_is_not_shown(two_people):
    """There is nothing to open."""
    _make("全家/照片.jpg")
    token = _login("甲", "jia1234")

    client.post("/api/favorites", json={"path": "全家/照片.jpg"},
                headers={"Authorization": f"Bearer {token}"})

    from app.deps import get_storage_root

    (get_storage_root() / "全家" / "照片.jpg").unlink()

    listed = client.get("/api/favorites", headers={"Authorization": f"Bearer {token}"})

    assert listed.json()["favorites"] == []
    # But the star is still there, so if it comes back it is starred again.
    assert listed.json()["total"] == 1


def test_a_starred_file_made_private_is_not_shown(two_people):
    """Its name is not theirs to read either."""
    from app.deps import state

    _make("私密/日记.txt")
    token = _login("甲", "jia1234")

    client.post("/api/favorites", json={"path": "私密/日记.txt"},
                headers={"Authorization": f"Bearer {token}"})

    state.get_user_store().restrict("私密", two_people[0]["id"], "none")

    listed = client.get("/api/favorites", headers={"Authorization": f"Bearer {token}"})

    assert listed.json()["favorites"] == []


def test_the_listing_says_which_files_are_starred(two_people):
    """So the browser knows without asking again."""
    _make("全家/照片.jpg")
    _make("全家/别的.jpg")
    token = _login("甲", "jia1234")

    client.post("/api/favorites", json={"path": "全家/照片.jpg"},
                headers={"Authorization": f"Bearer {token}"})

    res = client.get("/api/fs/browse", params={"path": "全家"},
                     headers={"Authorization": f"Bearer {token}"})

    starred = res.json()["favorite_paths"]
    assert len(starred) == 1
    assert starred[0].endswith("全家/照片.jpg")
