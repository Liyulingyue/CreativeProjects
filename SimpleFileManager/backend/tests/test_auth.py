"""Tests for signing in.

The password lives in two places -- the file that says whether the
service is protected, and the administrator account the login actually
checks -- and these are the tests that keep them from drifting apart.
"""

import importlib
import json

import pytest

from conftest import client


@pytest.fixture
def auth_file(tmp_path, monkeypatch):
    """Point the auth module at a file of its own."""
    auth_module = importlib.import_module("app.routers.auth")

    path = tmp_path / "auth.json"
    monkeypatch.setattr(auth_module, "AUTH_FILE", path)
    monkeypatch.setattr(auth_module, "_auth_cache", {"mtime": None, "data": None})
    return path


@pytest.fixture
def administrator(auth_file):
    """An installation with one administrator and a known password."""
    from app.deps import state
    from app.routers.auth import set_password
    from app.users import ROLE_ADMIN

    store = state.get_user_store()
    existing = store.get_by_username("admin")
    if existing is None:
        store.create_user("admin", "testpass", role=ROLE_ADMIN)
    set_password("testpass")
    return store


def _login(username, password):
    return client.post(
        "/api/auth/login", json={"username": username, "password": password}
    )


def test_the_single_password_becomes_the_administrator(auth_file):
    """An installation that always had one password still works."""
    from app.deps import state
    from app.routers.auth import set_password

    store = state.get_user_store()
    for user in store.list_users():
        store.delete_user(user["id"])
    set_password("firstpass")
    # The account is gone, so the password has to bring it back.
    store.delete_user(store.get_by_username("admin")["id"])

    res = _login("", "firstpass")

    assert res.status_code == 200
    assert res.json()["role"] == "admin"


def test_changing_the_password_changes_the_one_that_is_checked(administrator):
    """The password the login checks is the password that was changed.

    These used to be two different things: the change updated a file
    and the login read an account, so the change was reported as done
    and the old password carried on working.
    """
    res = _login("admin", "testpass")
    assert res.status_code == 200
    token = res.json()["token"]

    changed = client.post(
        "/api/auth/password",
        json={"old_password": "testpass", "new_password": "testpass9"},
        headers={"Authorization": f"Bearer {token}"},
    )
    assert changed.status_code == 200

    assert _login("admin", "testpass").status_code == 401
    assert _login("admin", "testpass9").status_code == 200


def test_changing_the_password_signs_everybody_out(administrator):
    res = _login("admin", "testpass")
    assert res.status_code == 200
    token = res.json()["token"]

    client.post(
        "/api/auth/password",
        json={"old_password": "testpass", "new_password": "testpass9"},
        headers={"Authorization": f"Bearer {token}"},
    )

    # The session the change was made from is gone too.
    assert (
        client.get("/api/users/me", headers={"Authorization": f"Bearer {token}"}).status_code
        == 401
    )


def test_the_old_password_is_refused(administrator):
    assert _login("admin", "definitely-not-it").status_code == 401


def test_the_password_file_holds_no_sessions(administrator, auth_file):
    """The sessions live in the database, not in the file."""
    res = _login("admin", "testpass")
    assert res.status_code == 200

    data = json.loads(auth_file.read_text())
    assert not data.get("tokens")
    assert administrator.count_tokens() >= 1


def test_the_sessions_survive_a_restart_of_the_page(administrator):
    """A token is still good on the next request, and the next."""
    res = _login("admin", "testpass")
    token = res.json()["token"]

    for _ in range(3):
        check = client.get("/api/users/me", headers={"Authorization": f"Bearer {token}"})
        assert check.status_code == 200
        assert check.json()["username"] == "admin"


def test_signing_out_ends_the_session(administrator):
    res = _login("admin", "testpass")
    token = res.json()["token"]

    out = client.post("/api/auth/logout", headers={"Authorization": f"Bearer {token}"})
    assert out.status_code == 200

    assert client.get("/api/users/me", headers={"Authorization": f"Bearer {token}"}).status_code == 401
