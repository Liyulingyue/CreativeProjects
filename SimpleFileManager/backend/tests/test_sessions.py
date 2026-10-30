"""Tests for whose conversations are whose.

A conversation belongs to the person who started it. On a machine
with more than one person that has to be true, or one person's
questions are another's to read.
"""

import pytest

from conftest import client


@pytest.fixture
def two_people(tmp_path, monkeypatch):
    """Two ordinary people, each with an account.

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

    if store.get_by_username("admin") is None:
        store.create_user("admin", "testpass", role="admin")

    one = store.create_user("甲", "jia1234")
    two = store.create_user("乙", "yi1234")

    # A password on the service, so a request carries who it is.
    auth_module.set_password("testpass")

    return one, two


def _login(username, password):
    res = client.post("/api/auth/login", json={"username": username, "password": password})
    assert res.status_code == 200, res.text
    return res.json()["token"]


def test_a_conversation_belongs_to_the_person_who_started_it(two_people):
    token = _login("甲", "jia1234")

    made = client.post(
        "/api/chat_history/sessions",
        json={"session_type": "chat"},
        headers={"Authorization": f"Bearer {token}"},
    )
    assert made.status_code == 200
    sid = made.json()["id"]

    # Somebody else's list does not have it.
    other = _login("乙", "yi1234")
    listed = client.get(
        "/api/chat_history/sessions", headers={"Authorization": f"Bearer {other}"}
    )
    assert sid not in [s["id"] for s in listed.json()["sessions"]]

    # And they cannot open it by asking for it directly.
    opened = client.get(
        f"/api/chat_history/sessions/{sid}", headers={"Authorization": f"Bearer {other}"}
    )
    assert opened.status_code == 404


def test_the_person_who_started_it_can_open_it(two_people):
    token = _login("甲", "jia1234")

    made = client.post(
        "/api/chat_history/sessions",
        json={"session_type": "chat"},
        headers={"Authorization": f"Bearer {token}"},
    )
    sid = made.json()["id"]

    opened = client.get(
        f"/api/chat_history/sessions/{sid}", headers={"Authorization": f"Bearer {token}"}
    )
    assert opened.status_code == 200


def test_somebody_else_cannot_delete_it(two_people):
    token = _login("甲", "jia1234")
    made = client.post(
        "/api/chat_history/sessions",
        json={"session_type": "chat"},
        headers={"Authorization": f"Bearer {token}"},
    )
    sid = made.json()["id"]

    other = _login("乙", "yi1234")
    removed = client.delete(
        f"/api/chat_history/sessions/{sid}", headers={"Authorization": f"Bearer {other}"}
    )
    assert removed.status_code == 404

    # It is still there for the person it belongs to.
    still = client.get(
        f"/api/chat_history/sessions/{sid}", headers={"Authorization": f"Bearer {token}"}
    )
    assert still.status_code == 200


def test_somebody_else_cannot_write_in_it(two_people):
    token = _login("甲", "jia1234")
    made = client.post(
        "/api/chat_history/sessions",
        json={"session_type": "chat"},
        headers={"Authorization": f"Bearer {token}"},
    )
    sid = made.json()["id"]

    other = _login("乙", "yi1234")
    spoken = client.post(
        "/api/chat_history/messages",
        json={"session_id": sid, "role": "user", "content": "偷看"},
        headers={"Authorization": f"Bearer {other}"},
    )
    assert spoken.status_code == 404


def test_the_administrator_sees_everybody(two_people):
    token = _login("甲", "jia1234")
    made = client.post(
        "/api/chat_history/sessions",
        json={"session_type": "chat"},
        headers={"Authorization": f"Bearer {token}"},
    )
    sid = made.json()["id"]

    admin = _login("admin", "testpass")
    opened = client.get(
        f"/api/chat_history/sessions/{sid}", headers={"Authorization": f"Bearer {admin}"}
    )
    assert opened.status_code == 200
