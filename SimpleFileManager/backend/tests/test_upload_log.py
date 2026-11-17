"""Tests for the record of who uploaded what.

The log is a history of events, not a property of a file, so these
tests are about the two things that make such a log safe to keep:
it must be bounded, and it must be clearable.
"""

import time

from conftest import client

from app.deps import state


def _log():
    return state.get_upload_log()


def test_an_upload_is_recorded_with_who_and_what():
    log = _log()
    log.clear()
    try:
        log.record(
            path="/storage/photos/a.jpg",
            size=1234,
            name="a.jpg",
            user={"id": "u1", "username": "amy", "role": "user"},
        )
        log.record(path="/storage/notes.txt", size=9, name="notes.txt", user=None)

        entries = log.recent()
        assert [e["name"] for e in entries] == ["notes.txt", "a.jpg"]
        assert entries[0]["username"] == ""  # open service: no account
        assert entries[1]["username"] == "amy"
        assert entries[1]["size"] == 1234
        assert log.count() == 2
    finally:
        log.clear()


def test_old_entries_are_pruned():
    log = _log()
    log.clear()
    try:
        old = time.time() - 200 * 86400
        log.record(path="/storage/old.txt", size=1, name="old.txt", at=old)
        log.record(path="/storage/new.txt", size=1, name="new.txt")

        removed = log.prune(retention_days=90)
        assert removed["removed_old"] == 1
        assert [e["name"] for e in log.recent()] == ["new.txt"]
    finally:
        log.clear()


def test_the_log_is_capped_by_count():
    """Keeping the newest entries, so a busy share cannot fill the disk."""
    log = _log()
    log.clear()
    try:
        for i in range(10):
            log.record(path=f"/storage/{i}.txt", size=1, name=f"{i}.txt")

        removed = log.prune(retention_days=90, max_rows=3)
        assert removed["removed_over"] == 7
        assert [e["name"] for e in log.recent()] == ["9.txt", "8.txt", "7.txt"]
    finally:
        log.clear()


def test_clearing_empties_the_log():
    log = _log()
    log.clear()
    log.record(path="/storage/a.txt", size=1, name="a.txt")
    assert log.clear() == 1
    assert log.count() == 0


def test_the_recent_list_is_trimmed_to_the_users_own_paths():
    """The API shows paths relative to the storage root."""
    log = _log()
    log.clear()
    try:
        root = str(state.get_settings().storage_path)
        log.record(path=f"{root}/inside.txt", size=1, name="inside.txt")
        body = client.get("/api/uploads").json()
        entry = body["entries"][0]
        assert entry["path"] == "inside.txt"
        assert entry["name"] == "inside.txt"
    finally:
        log.clear()


def test_a_small_upload_is_recorded():
    log = _log()
    log.clear()
    try:
        res = client.post(
            "/api/fs/upload",
            params={"path": ""},
            files={"files": ("recorded.txt", b"hello", "text/plain")},
        )
        assert res.status_code == 200, res.text
        entries = client.get("/api/uploads").json()["entries"]
        assert any(e["name"] == "recorded.txt" for e in entries)
    finally:
        log.clear()


def test_a_chunked_upload_is_recorded():
    log = _log()
    log.clear()
    try:
        payload = b"chunked-body"
        init = client.post(
            "/api/fs/upload/init",
            params={
                "path": "",
                "name": "chunked.txt",
                "size": len(payload),
                "total_chunks": 1,
            },
        )
        assert init.status_code == 200, init.text
        upload_id = init.json()["upload_id"]
        chunk = client.post(
            "/api/fs/upload/chunk",
            params={"upload_id": upload_id, "index": 0},
            content=payload,
        )
        assert chunk.status_code == 200, chunk.text
        done = client.post("/api/fs/upload/complete", params={"upload_id": upload_id})
        assert done.status_code == 200, done.text

        entries = client.get("/api/uploads").json()["entries"]
        assert any(e["name"] == "chunked.txt" for e in entries)
    finally:
        log.clear()


class TestTheLogSurvivesItsOwnHousekeeping:
    """Housekeeping must never fail the thing it is housekeeping for."""

    def test_a_failing_prune_does_not_fail_the_upload(self):
        """When pruning goes wrong, the upload still goes through.

        The prune runs inside the write that records the upload. If the
        prune raises -- and it might, the disk could be full or the
        database locked -- then the upload would fail too, and the
        person uploading a file would be told their upload broke
        because housekeeping broke.
        """
        log = _log()
        log.clear()
        original = log._get_conn

        def broken_conn():
            # The connection the prune opens raises on the DELETE.
            raise OSError("the disk is full")

        try:
            # Only the prune's connection fails; the insert still works.
            calls = {"n": 0}

            def conn():
                calls["n"] += 1
                if calls["n"] > 1:
                    raise OSError("the disk is full")
                return original()

            log._get_conn = conn
            # The prune only runs once an hour, so make it due.
            log._last_prune = 0.0
            try:
                entry = log.record(path="/storage/x.txt", size=1, name="x.txt")
            except OSError as e:
                # The prune's failure must not have escaped as an upload
                # failure. If it did, this test fails with a clear message.
                raise AssertionError(
                    f"the upload failed because of the prune: {e}")
            assert entry["id"], "the upload was not recorded"
        finally:
            log._get_conn = original
        try:
            assert log.count() == 1, "the upload was not recorded"
        finally:
            log.clear()
