"""The public share page: a window onto a file that needs no login.

This is the second way into the machine. Everything here is reachable
by anybody who has the link, so the boundaries matter more than
anywhere else: the path may not climb out of the shared folder, the
password may not be guessed quickly, and a link that was revoked or
that ran out of downloads has to stop working.
"""

import time

import pytest
from fastapi.testclient import TestClient

from app.main import app


@pytest.fixture
def client():
    return TestClient(app)


@pytest.fixture
def storage(tmp_path, monkeypatch):
    """A storage root with a file in it, and a share pointing at it."""
    from app.deps import DATA_DIR, get_storage_root, state

    root = get_storage_root()
    folder = root / "公开测试"
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "a.txt").write_text("公开内容", encoding="utf-8")
    (folder / "sub").mkdir(exist_ok=True)
    (folder / "sub" / "b.txt").write_text("子目录", encoding="utf-8")

    store = state.get_share_store()

    def make(name="a.txt", password="", is_dir=False, expiry_days=0, max_downloads=0):
        target = folder / name if name else folder
        if is_dir:
            target = folder
        return store.create(
            path=str(target),
            is_dir=bool(is_dir),
            size=target.stat().st_size if target.is_file() else 0,
            name=target.name,
            expiry_days=expiry_days,
            password=password,
            max_downloads=max_downloads,
        )

    yield {"make": make, "folder": folder, "store": store}
    # Leave nothing behind.
    for entry in list(store.list()):
        try:
            store.revoke(entry["id"])
        except Exception:
            pass


def test_a_shared_file_can_be_fetched_without_a_login(client, storage):
    share = storage["make"]()
    r = client.get(f"/api/share/{share['id']}")
    assert r.status_code == 200, r.text
    assert r.json()["name"] == "a.txt"

    r = client.get(f"/api/share/{share['id']}/download")
    assert r.status_code == 200, r.text
    assert r.content == "公开内容".encode()


def test_the_password_is_asked_for(client, storage):
    share = storage["make"](password="good-password")
    r = client.get(f"/api/share/{share['id']}")
    assert r.status_code == 200
    assert r.json()["locked"] is True

    r = client.get(f"/api/share/{share['id']}/download")
    assert r.status_code == 401

    r = client.post(f"/api/share/{share['id']}/unlock", json={"password": "wrong"})
    assert r.status_code == 401

    r = client.post(f"/api/share/{share['id']}/unlock", json={"password": "good-password"})
    assert r.status_code == 200, r.text
    # The cookie lets the visitor through.
    r = client.get(f"/api/share/{share['id']}/download")
    assert r.status_code == 200, r.text
    assert r.content == "公开内容".encode()


def test_guessing_the_password_is_slowed_down(client, storage):
    """A wrong password five times, and then the door is shut.

    The password is the only thing between a link that is out on the
    internet and the file behind it.
    """
    share = storage["make"](password="good-password")
    for _ in range(5):
        r = client.post(f"/api/share/{share['id']}/unlock", json={"password": "nope"})
        assert r.status_code == 401
    r = client.post(f"/api/share/{share['id']}/unlock", json={"password": "good-password"})
    assert r.status_code == 429, r.text
    assert "等" in r.json()["detail"]


def test_a_revoked_link_stops_working(client, storage):
    share = storage["make"]()
    storage["store"].revoke(share["id"])
    r = client.get(f"/api/share/{share['id']}")
    assert r.status_code == 410


def test_an_expired_link_stops_working(client, storage):
    """A link whose time is up says so, and does not serve the file."""
    share = storage["make"](expiry_days=1)
    # Move the end of its life into the past.
    import sqlite3

    from app.deps import DATA_DIR
    conn = sqlite3.connect(str(DATA_DIR / "shares.db"))
    conn.execute("UPDATE shares SET expires_at = ? WHERE id = ?",
                 (int(time.time()) - 10, share["id"]))
    conn.commit()
    conn.close()

    r = client.get(f"/api/share/{share['id']}")
    assert r.status_code == 410
    r = client.get(f"/api/share/{share['id']}/download")
    assert r.status_code == 410


def test_a_link_that_was_asked_to_expire_yesterday_is_refused(client, storage):
    """A negative number used to mean "never expires".

    Somebody asking for a link that had expired yesterday would have
    got one that lived for ever.
    """
    r = client.post("/api/shares", json={"path": "公开测试/a.txt", "expiry_days": -1})
    assert r.status_code == 400, r.text
    assert "负数" in r.json()["detail"]


def test_a_link_that_ran_out_of_downloads_stops_working(client, storage):
    share = storage["make"](max_downloads=1)
    assert client.get(f"/api/share/{share['id']}/download").status_code == 200
    r = client.get(f"/api/share/{share['id']}/download")
    assert r.status_code == 410


def test_the_visitor_cannot_climb_out_of_the_shared_folder(client, storage):
    """A name is checked against the folder it was given."""
    share = storage["make"](name="", is_dir=True)
    for attempt in ("../secret.txt", "../../etc/passwd", "sub/../../secret.txt",
                    "/etc/passwd", "..%2fsecret.txt"):
        r = client.get(f"/api/share/{share['id']}/list",
                       params={"path": attempt})
        assert r.status_code in (400, 404), f"{attempt} -> {r.status_code}"
        assert "secret" not in r.text


def test_a_shared_folder_lists_only_what_is_inside_it(client, storage):
    share = storage["make"](name="", is_dir=True)
    r = client.get(f"/api/share/{share['id']}/list")
    assert r.status_code == 200, r.text
    names = {i["name"] for i in r.json()["items"]}
    assert "a.txt" in names
    assert "sub" in names


def test_a_hidden_file_is_not_listed(client, storage):
    share = storage["make"](name="", is_dir=True)
    (storage["folder"] / ".hidden").write_text("x", encoding="utf-8")
    r = client.get(f"/api/share/{share['id']}/list")
    names = {i["name"] for i in r.json()["items"]}
    assert ".hidden" not in names


def test_the_password_of_a_share_has_to_be_worth_something(client, storage):
    """A one-letter password on a public link is not a password."""
    r = client.post("/api/shares", json={"path": "公开测试/a.txt", "password": "a"})
    assert r.status_code == 400, r.text
    assert "至少" in r.json()["detail"]


def test_the_public_page_does_not_hold_the_path_of_the_file(client, storage):
    """What a visitor is told is the name, not where it is."""
    share = storage["make"]()
    r = client.get(f"/api/share/{share['id']}")
    body = r.text
    assert "storage" not in body.lower()
    assert "/home/" not in body


def test_only_the_administrator_makes_a_share(client, storage, monkeypatch):
    """A share is a door left open, and it is the administrator's to open.

    The link works for anybody who has it -- including people with no
    account here -- and it is the one thing that lets a file leave the
    permission model altogether. An ordinary person may read a folder;
    that is not a reason to publish it to the internet.
    """
    import app.deps as deps_mod

    # The check asks the module for the user, so patching it there is
    # what the request will see.
    monkeypatch.setattr(deps_mod, "current_user", lambda: {
        "id": "u1", "username": "someone", "role": "user",
    })
    r = client.post("/api/shares", json={"path": "公开测试/a.txt"})
    assert r.status_code in (401, 403), r.text


def test_the_administrator_makes_a_share(client, storage, monkeypatch):
    import app.deps as deps_mod

    monkeypatch.setattr(deps_mod, "current_user", lambda: {
        "id": "u0", "username": "admin", "role": "admin",
    })
    r = client.post("/api/shares", json={"path": "公开测试/a.txt"})
    assert r.status_code == 200, r.text
    assert r.json()["id"]
