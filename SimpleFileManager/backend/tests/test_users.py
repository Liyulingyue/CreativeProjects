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


def test_the_most_specific_restriction_wins():
    from app.deps import effective_restriction

    restrictions = [("", "read"), ("photos", "none"), ("photos/2024", "read")]

    # A restriction on a folder covers everything under it.
    assert effective_restriction("photos", restrictions) == "none"
    assert effective_restriction("photos/x.jpg", restrictions) == "none"
    # The more specific one beats the broader one.
    assert effective_restriction("photos/2024", restrictions) == "read"
    assert effective_restriction("photos/2024/x.jpg", restrictions) == "read"
    # The root restriction covers everything it has no closer claim on.
    assert effective_restriction("documents", restrictions) == "read"
    # Nothing at all means open.
    assert effective_restriction("anything", []) is None


def test_a_restriction_below_the_root_does_not_leak_upwards():
    from app.deps import effective_restriction

    restrictions = [("photos", "none")]
    assert effective_restriction("documents/a.pdf", restrictions) is None


def test_restrictions_are_stored_and_removed(store):
    user = store.create_user("家人", "family123")

    store.restrict("photos", user["id"], "none")
    assert store.restrictions_for(user["id"]) == [("photos", "none")]

    store.restrict("photos/2024", user["id"], "read")
    assert len(store.restrictions_for(user["id"])) == 2

    store.lift("photos/2024", user["id"])
    assert store.restrictions_for(user["id"]) == [("photos", "none")]


def test_deleting_a_user_takes_its_restrictions_with_it(store):
    user = store.create_user("家人", "family123")
    store.restrict("photos", user["id"], "none")

    store.delete_user(user["id"])

    assert store.get_user(user["id"]) is None
    assert store.list_restrictions(user["id"]) == []


def test_a_root_restriction_is_the_whole_storage(store):
    user = store.create_user("家人", "family123")
    store.restrict("", user["id"], "none")

    assert store.restrictions_for(user["id"]) == [("", "none")]


def test_the_old_list_of_grants_becomes_a_list_of_exceptions(store):
    """A grant said "this folder is shut, and here is the key".

    The new list says "this folder is open, and here is who is shut
    out". The same rule, said the other way round.
    """
    import sqlite3

    key = store.create_user("持钥匙的人", "key1234")
    everyone_else = store.create_user("其他人", "other1234")

    # A folder that only the person with the key could open.
    conn = sqlite3.connect(store.db_path)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS acl (path TEXT, user_id TEXT, permission TEXT,"
        " created_at INTEGER)"
    )
    conn.execute(
        "INSERT INTO acl (path, user_id, permission, created_at) VALUES (?, ?, ?, 0)",
        ("photos", key["id"], "write"),
    )
    conn.commit()
    conn.close()

    store._migrate_grants_to_restrictions(
        sqlite3.connect(store.db_path)
    )

    # The person with the key is not shut out...
    assert store.restrictions_for(key["id"]) == []
    # ...and everybody else is.
    assert store.restrictions_for(everyone_else["id"]) == [("photos", "none")]


def test_a_token_belongs_to_the_user_it_was_issued_for(store):
    store.create_user("家人", "family123")

    store.store_token("abc", "家人", 9_999_999_999)

    assert store.token_username("abc") == "家人"


def test_an_unknown_token_belongs_to_nobody(store):
    assert store.token_username("nope") is None


def test_an_expired_token_is_no_good(store):
    store.store_token("old", "家人", 1)

    assert store.token_username("old") is None


def test_a_token_can_be_thrown_away(store):
    store.store_token("abc", "家人", 9_999_999_999)

    assert store.revoke_token("abc") is True
    assert store.token_username("abc") is None


def test_expired_tokens_are_swept_away(store):
    store.store_token("old", "家人", 1)
    store.store_token("new", "家人", 9_999_999_999)

    assert store.prune_tokens() >= 1
    assert store.count_tokens() == 1


def test_a_user_does_not_accumulate_sessions_for_ever(store):
    for i in range(30):
        store.store_token(f"t{i}", "家人", 9_999_999_999)

    store.prune_tokens()

    assert store.count_tokens() <= 20


def test_signing_everybody_out(store):
    store.store_token("a", "家人", 9_999_999_999)
    store.store_token("b", "admin", 9_999_999_999)

    assert store.revoke_all_tokens() == 2
    assert store.count_tokens() == 0
