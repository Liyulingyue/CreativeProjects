"""Tests for what one person may reach.

The rules: a grant on a folder covers everything under it, the most
specific grant for a path is the one that counts, and the administrator
is not subject to any of it. And nothing -- not the file list, not the
search, not the index -- may show a person what they may not open.
"""

import pytest

from conftest import client


@pytest.fixture
def people(tmp_path, monkeypatch):
    """An administrator, and somebody who may only read one folder.

    Auth is turned on for these tests. Without a password the service
    is open to anybody, and an open service has nobody to refuse --
    a permission only means something once somebody is signed in.
    """
    import importlib

    from app.deps import state

    auth_module = importlib.import_module("app.routers.auth")
    path = tmp_path / "auth.json"
    monkeypatch.setattr(auth_module, "AUTH_FILE", path)
    monkeypatch.setattr(auth_module, "_auth_cache", {"mtime": None, "data": None})
    auth_module.set_password("boss1234")

    store = state.get_user_store()
    for user in store.list_users():
        if user["username"] != "admin":
            store.delete_user(user["id"])

    admin = store.create_user("boss", "boss1234", role="admin")
    other = store.create_user("other", "other1234", role="user")
    store.grant("我的", other["id"], "read")

    return {"admin": admin, "other": other}


def _login(client, username, password):
    res = client.post("/api/auth/login", json={"username": username, "password": password})
    assert res.status_code == 200, res.text
    return res.json()["token"]


def _make(relative: str, content: str = "hello"):
    from app.deps import get_storage_root

    path = get_storage_root() / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    return path


def test_a_person_may_open_what_they_were_granted(people):
    _make("我的/笔记.txt", "这是我的笔记")
    token = _login(client, "other", "other1234")

    res = client.get("/api/fs/download", params={"path": "我的/笔记.txt"},
                     headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 200


def test_a_person_may_not_open_what_they_were_not_granted(people):
    _make("别人的/机密.txt", "机密")
    token = _login(client, "other", "other1234")

    res = client.get("/api/fs/download", params={"path": "别人的/机密.txt"},
                     headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 403


def test_a_person_sees_only_what_they_were_granted_in_a_listing(people):
    _make("我的/笔记.txt")
    _make("别人的/机密.txt")
    token = _login(client, "other", "other1234")

    res = client.get("/api/fs/browse", params={"path": ""},
                     headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 200
    assert [i["name"] for i in res.json()["items"]] == ["我的"]


def test_a_person_may_not_write_to_what_they_may_only_read(people):
    _make("我的/笔记.txt")
    token = _login(client, "other", "other1234")

    res = client.post("/api/fs/create_folder",
                      json={"path": "我的", "name": "新文件夹"},
                      headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 403


def test_a_relative_path_is_taken_from_the_root(people):
    """A relative path and an absolute one mean the same thing."""
    _make("我的/笔记.txt")
    token = _login(client, "other", "other1234")

    res = client.get("/api/fs/browse", params={"path": "我的"},
                     headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 200
    assert [i["name"] for i in res.json()["items"]] == ["笔记.txt"]


def test_a_path_outside_the_root_is_refused(people):
    token = _login(client, "other", "other1234")

    res = client.get("/api/fs/browse", params={"path": "../../etc"},
                     headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 400


def test_the_search_does_not_show_what_a_person_may_not_open(people):
    """The search covers the whole machine. What comes back does not."""
    _make("别人的/机密.txt", "unique-marker-secret")
    token = _login(client, "other", "other1234")

    res = client.get("/api/search/query", params={"q": "机密"},
                     headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 200
    assert res.json()["items"] == []


def test_the_search_still_finds_what_they_may_open(people):
    _make("我的/笔记.txt", "内容")
    token = _login(client, "other", "other1234")

    res = client.get("/api/search/query", params={"q": "笔记"},
                     headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 200
    assert [i["name"] for i in res.json()["items"]] == ["笔记.txt"]


def test_the_administrator_sees_everything(people):
    _make("别人的/机密.txt")
    token = _login(client, "boss", "boss1234")

    res = client.get("/api/search/query", params={"q": "机密"},
                     headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 200
    assert [i["name"] for i in res.json()["items"]] == ["机密.txt"]


def test_the_agent_cannot_list_a_folder_it_may_not_reach(people):
    """The agent's tools go through the same checks."""
    import importlib

    agent = importlib.import_module("app.routers.agent")

    _make("别人的/机密.txt")
    token = _login(client, "other", "other1234")

    # The check needs the request's user to be the one asking.
    from app.deps import set_current_user

    set_current_user(people["other"])
    try:
        out = agent.tool_list_directory("别人的")
    finally:
        set_current_user(None)

    assert out["success"] is False
    assert "权限" in out["error"]


def test_the_agent_cannot_find_a_file_it_may_not_reach(people):
    import importlib

    agent = importlib.import_module("app.routers.agent")
    from app.deps import set_current_user

    _make("别人的/机密.txt", "unique-marker-secret")
    _make("我的/笔记.txt", "内容")

    set_current_user(people["other"])
    try:
        out = agent.tool_find_files("机密")
    finally:
        set_current_user(None)

    assert out["success"] is True
    assert out["matches"] == []
