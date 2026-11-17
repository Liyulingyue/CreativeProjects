"""Tests for what one person may reach.

This is one household's machine: everybody has a key to the same
house. Everybody can put things in it and everybody can look at what
is there. A folder becomes private only when somebody says so, and
that is the only thing these tests are about -- the list of
exceptions, and the rule that the most specific one for a path is the
one that counts.

Nothing -- not the file list, not the search, not the index -- may show
a person what they have been shut out of.
"""

import pytest

from conftest import client


@pytest.fixture
def people(tmp_path, monkeypatch):
    """An administrator, and two ordinary people.

    Auth is turned on: without a password the service is open to
    anybody, and an open service cannot keep anything private.
    """
    import importlib

    from app.deps import state

    auth_module = importlib.import_module("app.routers.auth")
    monkeypatch.setattr(auth_module, "AUTH_FILE", tmp_path / "auth.json")
    monkeypatch.setattr(auth_module, "_auth_cache", {"mtime": None, "data": None})

    store = state.get_user_store()
    for user in store.list_users():
        if user["username"] != "admin":
            store.delete_user(user["id"])

    admin = store.create_user("boss", "boss1234", role="admin")
    one = store.create_user("甲", "jia1234", role="user")
    two = store.create_user("乙", "yi1234", role="user")
    auth_module.set_password("boss1234")

    return {"admin": admin, "one": one, "two": two}


def _login(username, password):
    res = client.post("/api/auth/login", json={"username": username, "password": password})
    assert res.status_code == 200
    return res.json()["token"]


def _make(relative: str, content: str = "hello"):
    from app.deps import get_storage_root

    path = get_storage_root() / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    return path


# ---- the ordinary case: everybody sees everything ----


def test_everybody_sees_the_whole_storage(people):
    """One household, one pile of files, everybody in it."""
    _make("全家/照片.jpg")
    token = _login("甲", "jia1234")

    res = client.get("/api/fs/browse", params={"path": ""},
                     headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 200
    assert "全家" in [i["name"] for i in res.json()["items"]]


def test_everybody_can_upload(people):
    _make("全家/占位.txt")
    token = _login("甲", "jia1234")

    res = client.post("/api/fs/create_folder",
                      json={"path": "全家", "name": "甲放的"},
                      headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 200


def test_two_people_see_the_same_files(people):
    """A file one of them put there is the other's to see."""
    _make("全家/甲传的.txt", "甲传的")
    token = _login("乙", "yi1234")

    res = client.get("/api/fs/download", params={"path": "全家/甲传的.txt"},
                     headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 200
    assert res.content == "甲传的".encode()


# ---- the exception: a folder somebody is shut out of ----


def test_a_person_shut_out_of_a_folder_cannot_copy_it_out(people):
    """Copying reads the file.

    A file somebody is not allowed to read must not become readable by
    copying it to somewhere else.
    """
    from app.deps import state

    _make("私密/日记.txt", "秘密")
    _make("公开/放得下吗.txt", "")
    state.get_user_store().restrict("私密", people["one"]["id"], "none")

    token = _login("甲", "jia1234")
    res = client.post("/api/fs/copy", json={
        "src": "私密/日记.txt",
        "dest": "公开/日记.txt",
    }, headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 403
    from app.deps import get_storage_root
    assert not (get_storage_root() / "公开" / "日记.txt").exists()


def test_a_person_shut_out_of_a_folder_cannot_move_a_file_into_it(people):
    from app.deps import state

    _make("私密/日记.txt", "秘密")
    _make("别处/随手.txt", "")
    state.get_user_store().restrict("私密", people["one"]["id"], "none")

    token = _login("甲", "jia1234")
    res = client.post("/api/fs/move", json={
        "src": "别处/随手.txt",
        "dest": "私密/随手.txt",
    }, headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 403
    from app.deps import get_storage_root
    assert (get_storage_root() / "别处" / "随手.txt").exists()


def test_a_person_shut_out_of_a_folder_cannot_move_a_batch_into_it(people):
    """The batch endpoint moves a lot of files at once.

    It has to ask about the destination just like the single move
    does, or a restricted folder can be filled up from the file
    list.
    """
    from app.deps import state

    _make("私密/日记.txt", "秘密")
    _make("别处/随手.txt", "")
    # Read-only, not shut out: this person may look in there, but
    # putting something in it is not theirs to do.
    state.get_user_store().restrict("私密", people["one"]["id"], "read")

    token = _login("甲", "jia1234")
    res = client.post("/api/fs/batch", json={
        "action": "move",
        "paths": ["别处/随手.txt"],
        "dest": "私密",
    }, headers={"Authorization": f"Bearer {token}"})

    assert res.json()["failed_count"] == 1
    from app.deps import get_storage_root
    assert (get_storage_root() / "别处" / "随手.txt").exists()


def test_only_the_administrator_can_empty_the_index(people):
    """The index is hours of work to rebuild.

    Throwing it away is the administrator's decision, not anybody's.
    """
    token = _login("甲", "jia1234")
    res = client.delete("/api/rag/clear", headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 403


def test_only_the_administrator_can_drop_a_file_out_of_the_index(people):
    token = _login("甲", "jia1234")
    res = client.delete("/api/rag/files/全家/照片.jpg",
                        headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 403


def test_the_administrator_can_empty_the_index(people):
    boss = _login("boss", "boss1234")
    res = client.delete("/api/rag/clear", headers={"Authorization": f"Bearer {boss}"})

    assert res.status_code == 200


def test_a_person_shut_out_of_a_folder_cannot_open_it(people):
    from app.deps import state

    _make("私密/日记.txt", "秘密")
    state.get_user_store().restrict("私密", people["one"]["id"], "none")

    token = _login("甲", "jia1234")
    res = client.get("/api/fs/download", params={"path": "私密/日记.txt"},
                     headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 403


def test_a_person_shut_out_does_not_see_the_folder_in_the_listing(people):
    from app.deps import state

    _make("私密/日记.txt")
    _make("全家/照片.jpg")
    state.get_user_store().restrict("私密", people["one"]["id"], "none")

    token = _login("甲", "jia1234")
    res = client.get("/api/fs/browse", params={"path": ""},
                     headers={"Authorization": f"Bearer {token}"})

    names = [i["name"] for i in res.json()["items"]]
    assert "全家" in names
    assert "私密" not in names


def test_the_rest_of_the_family_still_sees_it(people):
    """One person's private folder is not the whole family's."""
    from app.deps import state

    _make("私密/日记.txt")
    state.get_user_store().restrict("私密", people["one"]["id"], "none")

    token = _login("乙", "yi1234")
    res = client.get("/api/fs/download", params={"path": "私密/日记.txt"},
                     headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 200


def test_a_read_only_folder_can_be_looked_at_but_not_changed(people):
    from app.deps import state

    _make("全家/笔记.txt")
    state.get_user_store().restrict("全家", people["one"]["id"], "read")

    token = _login("甲", "jia1234")
    read = client.get("/api/fs/download", params={"path": "全家/笔记.txt"},
                      headers={"Authorization": f"Bearer {token}"})
    assert read.status_code == 200

    wrote = client.post("/api/fs/create_folder",
                        json={"path": "全家", "name": "新文件夹"},
                        headers={"Authorization": f"Bearer {token}"})
    assert wrote.status_code == 403


def test_the_administrator_is_not_shut_out(people):
    from app.deps import state

    _make("私密/日记.txt")
    state.get_user_store().restrict("私密", people["one"]["id"], "none")
    state.get_user_store().restrict("私密", people["admin"]["id"], "none")

    token = _login("boss", "boss1234")
    res = client.get("/api/fs/download", params={"path": "私密/日记.txt"},
                     headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 200


def test_the_search_does_not_show_what_a_person_is_shut_out_of(people):
    """The search covers the whole machine. What comes back does not."""
    from app.deps import state

    _make("私密/日记.txt", "unique-marker-secret")
    state.get_user_store().restrict("私密", people["one"]["id"], "none")

    token = _login("甲", "jia1234")
    res = client.get("/api/search/query", params={"q": "日记"},
                     headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 200
    assert res.json()["items"] == []


def test_the_search_still_finds_what_they_may_open(people):
    _make("全家/日记.txt")
    token = _login("甲", "jia1234")

    res = client.get("/api/search/query", params={"q": "日记"},
                     headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 200
    assert [i["name"] for i in res.json()["items"]] == ["日记.txt"]


def test_the_agent_cannot_read_what_a_person_is_shut_out_of(people):
    import importlib

    from app.deps import set_current_user, state

    agent = importlib.import_module("app.routers.agent")

    _make("私密/日记.txt", "秘密")
    state.get_user_store().restrict("私密", people["one"]["id"], "none")

    set_current_user(people["one"])
    try:
        out = agent.tool_read_file("私密/日记.txt")
    finally:
        set_current_user(None)

    assert out["success"] is False
    assert "权限" in out["error"]


def test_the_agent_cannot_find_what_a_person_is_shut_out_of(people):
    import importlib

    from app.deps import set_current_user, state

    agent = importlib.import_module("app.routers.agent")

    _make("私密/日记.txt")
    _make("全家/照片.txt")
    state.get_user_store().restrict("私密", people["one"]["id"], "none")

    set_current_user(people["one"])
    try:
        out = agent.tool_find_files("日记")
    finally:
        set_current_user(None)

    assert out["success"] is True
    assert out["matches"] == []


# ---- the service's own settings are still the administrator's ----


def test_the_service_settings_are_the_administrators_business(people):
    """A person does not change how the machine is set up."""
    token = _login("甲", "jia1234")

    assert client.post("/api/settings", json={"index_interval": 60},
                        headers={"Authorization": f"Bearer {token}"}).status_code == 403
    assert client.get("/api/providers",
                      headers={"Authorization": f"Bearer {token}"}).status_code == 403
    assert client.post("/api/settings/auto_index/pause", json={"paused": True},
                       headers={"Authorization": f"Bearer {token}"}).status_code == 403


def test_the_administrator_can_change_the_settings(people):
    token = _login("boss", "boss1234")

    assert client.post("/api/settings", json={"index_interval": 300},
                        headers={"Authorization": f"Bearer {token}"}).status_code == 200
    assert client.get("/api/providers",
                      headers={"Authorization": f"Bearer {token}"}).status_code == 200


# ---- a plan still cannot do what the person asking may not do ----


def test_a_plan_cannot_do_what_the_person_asking_it_may_not_do(people):
    from app.deps import state

    _make("私密/机密.txt")
    _make("全家/笔记.txt")
    state.get_user_store().restrict("私密", people["one"]["id"], "none")

    token = _login("甲", "jia1234")
    made = client.post(
        "/api/plans",
        json={
            "title": "偷一个文件",
            "summary": "",
            "source": "agent",
            "actions": [{
                "action_type": "move",
                "source_path": "私密/机密.txt",
                "target_path": "全家/偷来的.txt",
                "reason": "",
            }],
        },
        headers={"Authorization": f"Bearer {token}"},
    )

    assert made.status_code == 403


def test_a_plan_for_a_folder_you_may_reach_is_allowed(people):
    _make("全家/笔记.txt")
    token = _login("甲", "jia1234")

    made = client.post(
        "/api/plans",
        json={
            "title": "整理",
            "summary": "",
            "source": "agent",
            "actions": [{
                "action_type": "create_folder",
                "source_path": "",
                "target_path": "全家/新文件夹",
                "reason": "",
            }],
        },
        headers={"Authorization": f"Bearer {token}"},
    )

    assert made.status_code == 200


def test_a_plan_is_yours_to_see(people):
    token = _login("甲", "jia1234")
    made = client.post(
        "/api/plans",
        json={
            "title": "我的计划",
            "summary": "",
            "source": "agent",
            "actions": [{
                "action_type": "create_folder",
                "source_path": "",
                "target_path": "全家/新文件夹",
                "reason": "",
            }],
        },
        headers={"Authorization": f"Bearer {token}"},
    )
    pid = made.json()["id"]

    listed = client.get("/api/plans", headers={"Authorization": f"Bearer {token}"})
    assert pid in [p["id"] for p in listed.json()["plans"]]

    # Somebody else's is not.
    admin = _login("boss", "boss1234")
    admin_plan = client.post(
        "/api/plans",
        json={
            "title": "管理员的计划",
            "summary": "",
            "source": "agent",
            "actions": [{
                "action_type": "create_folder",
                "source_path": "",
                "target_path": "全家/管理员建的",
                "reason": "",
            }],
        },
        headers={"Authorization": f"Bearer {admin}"},
    )
    admin_pid = admin_plan.json()["id"]

    assert client.get(f"/api/plans/{admin_pid}",
                      headers={"Authorization": f"Bearer {token}"}).status_code == 404


# ---- paths themselves ----


def test_a_relative_path_is_taken_from_the_root(people):
    _make("全家/笔记.txt")
    token = _login("甲", "jia1234")

    res = client.get("/api/fs/browse", params={"path": "全家"},
                     headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 200
    assert [i["name"] for i in res.json()["items"]] == ["笔记.txt"]


def test_a_path_outside_the_root_is_refused(people):
    token = _login("甲", "jia1234")

    res = client.get("/api/fs/browse", params={"path": "../../etc"},
                     headers={"Authorization": f"Bearer {token}"})

    assert res.status_code == 400
