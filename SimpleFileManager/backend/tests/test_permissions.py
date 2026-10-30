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


def test_a_stream_token_opens_the_file_it_was_issued_for(people):
    """A media element cannot send an Authorization header."""
    _make("我的/笔记.txt", "内容")
    token = _login(client, "boss", "boss1234")

    issued = client.post(
        "/api/fs/stream-token",
        json={"path": "我的/笔记.txt"},
        headers={"Authorization": f"Bearer {token}"},
    )
    assert issued.status_code == 200
    url = issued.json()["url"]

    res = client.get(url)

    assert res.status_code == 200
    assert res.content == "内容".encode()


def test_a_stream_token_does_not_open_another_file(people):
    _make("我的/笔记.txt", "内容")
    _make("别人的/机密.txt", "机密")
    token = _login(client, "boss", "boss1234")

    issued = client.post(
        "/api/fs/stream-token",
        json={"path": "我的/笔记.txt"},
        headers={"Authorization": f"Bearer {token}"},
    )
    token_value = issued.json()["token"]

    from urllib.parse import quote

    url = f"/api/fs/download?path={quote('别人的/机密.txt')}&st={quote(token_value)}"

    assert client.get(url).status_code == 401


def test_a_stream_token_is_not_good_for_ever(people):
    """A token is a key to one file, and it runs out."""
    _make("我的/笔记.txt", "内容")
    token = _login(client, "boss", "boss1234")

    issued = client.post(
        "/api/fs/stream-token",
        json={"path": "我的/笔记.txt", "ttl": 60},
        headers={"Authorization": f"Bearer {token}"},
    )
    expires = issued.json()["expires_at"]
    import time

    assert expires <= time.time() + 61


def test_the_service_settings_are_the_administrators_business(people):
    """A person with a folder of their own does not change the service."""
    token = _login(client, "other", "other1234")

    assert client.post("/api/settings", json={"index_interval": 60},
                        headers={"Authorization": f"Bearer {token}"}).status_code == 403
    assert client.get("/api/providers",
                      headers={"Authorization": f"Bearer {token}"}).status_code == 403
    assert client.post("/api/settings/auto_index/pause", json={"paused": True},
                       headers={"Authorization": f"Bearer {token}"}).status_code == 403


def test_the_administrator_can_change_the_settings(people):
    token = _login(client, "boss", "boss1234")

    assert client.post("/api/settings", json={"index_interval": 300},
                        headers={"Authorization": f"Bearer {token}"}).status_code == 200
    assert client.get("/api/providers",
                      headers={"Authorization": f"Bearer {token}"}).status_code == 200


def test_a_plan_cannot_do_what_the_person_asking_it_may_not_do(people):
    """A plan moves real files, so it is checked before it is made."""
    _make("我的/笔记.txt")
    _make("别人的/机密.txt")
    token = _login(client, "other", "other1234")

    made = client.post(
        "/api/plans",
        json={
            "title": "偷一个文件",
            "summary": "",
            "source": "agent",
            "actions": [{
                "action_type": "move",
                "source_path": "别人的/机密.txt",
                "target_path": "我的/偷来的.txt",
                "reason": "",
            }],
        },
        headers={"Authorization": f"Bearer {token}"},
    )

    assert made.status_code == 403


def test_a_plan_for_your_own_folder_is_allowed(people):
    _make("我的/笔记.txt")
    token = _login(client, "other", "other1234")

    made = client.post(
        "/api/plans",
        json={
            "title": "整理",
            "summary": "",
            "source": "agent",
            "actions": [{
                "action_type": "create_folder",
                "source_path": "",
                "target_path": "我的/新文件夹",
                "reason": "",
            }],
        },
        headers={"Authorization": f"Bearer {token}"},
    )

    assert made.status_code == 200


def test_a_plan_is_yours_to_see(people):
    token = _login(client, "other", "other1234")
    made = client.post(
        "/api/plans",
        json={
            "title": "我的计划",
            "summary": "",
            "source": "agent",
            "actions": [{
                "action_type": "create_folder",
                "source_path": "",
                "target_path": "我的/新文件夹",
                "reason": "",
            }],
        },
        headers={"Authorization": f"Bearer {token}"},
    )
    pid = made.json()["id"]

    # It is in their list.
    listed = client.get("/api/plans", headers={"Authorization": f"Bearer {token}"})
    assert pid in [p["id"] for p in listed.json()["plans"]]

    # Somebody else's is not.
    other = people["other"]["id"]
    assert other is not None
    admin = _login(client, "boss", "boss1234")
    admin_plan = client.post(
        "/api/plans",
        json={
            "title": "管理员的计划",
            "summary": "",
            "source": "agent",
            "actions": [{
                "action_type": "create_folder",
                "source_path": "",
                "target_path": "我的/管理员建的",
                "reason": "",
            }],
        },
        headers={"Authorization": f"Bearer {admin}"},
    )
    admin_pid = admin_plan.json()["id"]

    assert client.get(f"/api/plans/{admin_pid}",
                      headers={"Authorization": f"Bearer {token}"}).status_code == 404
