"""Tests for the recycle bin's retention window.

The window is the one thing in the bin that removes files without
somebody asking, so the tests care about two things: that it only
removes what it promised to remove, and that it says what it removed.
"""

import json
import time

import pytest

from conftest import create_test_file, get_storage_root


@pytest.fixture
def bin_with_entries():
    """A bin with one fresh entry and one that is 40 days old."""
    from app.trash import TrashService

    root = get_storage_root()
    svc = TrashService(root)

    old_file = root / "old.txt"
    old_file.write_text("old", encoding="utf-8")
    old_meta = svc.trash(old_file)
    # Backdate it: the entry directory is named after the deletion time,
    # but the metadata carries the real timestamp.
    meta_path = svc._entry_dir(old_meta["id"]) / "meta.json"
    meta = json.loads(meta_path.read_text(encoding="utf-8"))
    meta["deleted_at"] = int(time.time()) - 40 * 86400
    meta_path.write_text(json.dumps(meta, ensure_ascii=False), encoding="utf-8")

    new_file = root / "new.txt"
    new_file.write_text("new", encoding="utf-8")
    new_meta = svc.trash(new_file)

    return svc, root, old_meta, new_meta


def test_retention_zero_keeps_everything(bin_with_entries):
    svc, root, old_meta, new_meta = bin_with_entries
    removed = svc.purge_expired(0)
    assert removed == []
    assert len(svc.list()) == 2


def test_purge_expired_only_removes_what_is_past_the_window(bin_with_entries):
    svc, root, old_meta, new_meta = bin_with_entries
    removed = svc.purge_expired(30)
    assert [m["id"] for m in removed] == [old_meta["id"]]
    remaining = svc.list()
    assert [m["id"] for m in remaining] == [new_meta["id"]]
    # The file really is gone from the bin.
    assert not svc._entry_dir(old_meta["id"]).exists()


def test_purge_expired_respects_a_longer_window(bin_with_entries):
    svc, root, old_meta, new_meta = bin_with_entries
    removed = svc.purge_expired(90)
    assert removed == []
    assert len(svc.list()) == 2


def test_entries_say_how_long_they_have_left(bin_with_entries):
    svc, root, old_meta, new_meta = bin_with_entries
    entries = {m["id"]: m for m in svc.list(retention_days=30)}
    # The old one is already past its window.
    assert entries[old_meta["id"]]["expires_in_days"] == 0
    assert entries[new_meta["id"]]["expires_in_days"] >= 29
    assert entries[new_meta["id"]]["expires_at"] > int(time.time())


def test_without_a_window_no_expiry_is_announced(bin_with_entries):
    svc, root, old_meta, new_meta = bin_with_entries
    entries = svc.list()
    assert all("expires_in_days" not in m for m in entries)


def test_cleanup_records_what_it_removed(bin_with_entries, monkeypatch):
    from app.trash_retention import trash_retention

    svc, root, old_meta, new_meta = bin_with_entries
    monkeypatch.setattr(trash_retention, "retention_days", lambda: 30)
    trash_retention.log.clear()

    result = trash_retention.cleanup_now(reason="test")
    assert result["purged"] == 1
    assert result["entries"][0]["id"] == old_meta["id"]

    log = trash_retention.log.list(limit=10)
    assert len(log) == 1
    assert log[0]["entry_id"] == old_meta["id"]
    assert log[0]["reason"] == "test"
    assert log[0]["name"] == old_meta["name"]


def test_cleanup_is_a_no_op_without_a_window(bin_with_entries, monkeypatch):
    from app.trash_retention import trash_retention

    svc, root, old_meta, new_meta = bin_with_entries
    monkeypatch.setattr(trash_retention, "retention_days", lambda: 0)
    result = trash_retention.cleanup_now(reason="test")
    assert result["skipped"] is True
    assert result["purged"] == 0
    assert len(svc.list()) == 2


def test_trash_route_reports_the_retention_window(bin_with_entries, monkeypatch):
    from conftest import client
    from app.trash_retention import trash_retention

    monkeypatch.setattr(trash_retention, "retention_days", lambda: 30)
    res = client.get("/api/fs/trash")
    assert res.status_code == 200
    body = res.json()
    assert body["retention_days"] == 30
    assert "expires_in_days" in body["entries"][0]


def test_purge_expired_route_cleans_and_reports(bin_with_entries, monkeypatch):
    from conftest import client
    from app.trash_retention import trash_retention

    svc, root, old_meta, new_meta = bin_with_entries
    monkeypatch.setattr(trash_retention, "retention_days", lambda: 30)
    trash_retention.log.clear()

    res = client.post("/api/fs/trash/purge-expired")
    assert res.status_code == 200, res.text
    body = res.json()
    assert body["purged"] == 1
    assert body["retention_days"] == 30
    assert len(svc.list()) == 1

    log = client.get("/api/fs/trash/log")
    assert log.status_code == 200
    assert len(log.json()["entries"]) == 1
