"""Tests for users and for what they may reach.

The rules that matter: a grant on a folder covers everything under it,
the most specific grant for a path is the one that counts, and the
administrator is not subject to any of it.
"""

import pytest


@pytest.fixture
def store(tmp_path):
    from app.users import UserStore

    return UserStore(str(tmp_path / "users.db"))


def test_a_user_is_created_and_can_log_in(store):
    from app.users import ROLE_USER

    user = store.create_user("家人", "family123", role=ROLE_USER)

    assert user["username"] == "家人"
    assert store.verify("家人", "family123")["id"] == user["id"]
    assert store.verify("家人", "wrong") is None
    assert store.verify("陌生人", "family123") is None


def test_the_same_name_cannot_be_used_twice(store):
    from app.users import UserError

    store.create_user("家人", "family123")
    with pytest.raises(UserError):
        store.create_user("家人", "other123")


def test_a_short_password_is_refused(store):
    from app.users import UserError

    with pytest.raises(UserError):
        store.create_user("家人", "ab")


def test_a_disabled_user_cannot_log_in(store):
    user = store.create_user("家人", "family123")
    store.set_disabled(user["id"], True)

    assert store.verify("家人", "family123") is None


def test_the_most_specific_grant_wins():
    from app.deps import effective_permission

    grants = [("", "read"), ("photos", "write"), ("photos/2024", "read")]

    # A grant on a folder covers everything under it.
    assert effective_permission("photos", grants) == "write"
    assert effective_permission("photos/x.jpg", grants) == "write"
    # The more specific grant beats the broader one.
    assert effective_permission("photos/2024", grants) == "read"
    assert effective_permission("photos/2024/x.jpg", grants) == "read"
    # The root grant covers everything it has no closer claim on.
    assert effective_permission("documents", grants) == "read"
    # Nothing at all.
    assert effective_permission("anything", []) is None


def test_a_grant_below_the_root_does_not_leak_upwards():
    from app.deps import effective_permission

    grants = [("photos", "write")]
    assert effective_permission("documents/a.pdf", grants) is None


def test_grants_are_stored_and_removed(store):
    user = store.create_user("家人", "family123")

    store.grant("photos", user["id"], "write")
    assert store.permissions_for(user["id"]) == [("photos", "write")]

    store.grant("photos/2024", user["id"], "read")
    assert len(store.permissions_for(user["id"])) == 2

    store.revoke("photos/2024", user["id"])
    assert store.permissions_for(user["id"]) == [("photos", "write")]


def test_deleting_a_user_takes_its_grants_with_it(store):
    user = store.create_user("家人", "family123")
    store.grant("photos", user["id"], "write")

    store.delete_user(user["id"])

    assert store.get_user(user["id"]) is None
    assert store.list_grants(user["id"]) == []


def test_a_root_grant_is_the_whole_storage(store):
    user = store.create_user("家人", "family123")
    store.grant("", user["id"], "read")

    assert store.permissions_for(user["id"]) == [("", "read")]
