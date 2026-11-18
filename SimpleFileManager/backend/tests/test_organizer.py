"""The organiser: take a snapshot of the tree, and compare two of them.

The point of a snapshot is to answer "what has appeared since then",
so the two things that matter are that the snapshot sees the tree, and
that the comparison does not invent changes that never happened.
"""

import time

import pytest

from conftest import client

from app.deps import get_storage_root


@pytest.fixture(autouse=True)
def clean_snapshots():
    """The snapshots outlive the test that took them.

    The organiser's database is not in the storage root, so the usual
    cleanup does not touch it -- and a snapshot left behind by one test
    is a snapshot the next one compares against.
    """
    from app.routers.organizer import get_organizer_service

    svc = get_organizer_service()
    conn = svc._get_conn()
    conn.execute("DELETE FROM file_snapshots")
    conn.execute("DELETE FROM snapshot_meta")
    conn.commit()
    conn.close()
    yield


def _file(relative: str, content: str = "x"):
    path = get_storage_root() / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    return path


class TestTakingASnapshot:
    def test_the_snapshot_sees_the_files(self):
        _file("照片/一.jpg")
        _file("照片/二.jpg")
        _file("说明.txt")

        res = client.post("/api/organizer/snapshot")
        assert res.status_code == 200, res.text
        body = res.json()
        assert body["total_files"] == 3, body
        assert body["total_dirs"] == 1, body
        assert not body["truncated"]

    def test_the_snapshot_does_not_bring_the_file_list_with_it(self):
        """The page wants the counts, not every path.

        A library with fifty thousand files would otherwise send fifty
        thousand paths back for a page that shows two numbers.
        """
        for i in range(50):
            _file(f"照片/{i}.jpg")

        res = client.post("/api/organizer/snapshot")
        assert res.status_code == 200
        assert res.json()["files"] == []
        assert res.json()["total_files"] == 50

    def test_the_latest_snapshot_is_cheap(self):
        """Asking for the latest snapshot must not read the whole thing.

        The page that asks wants two numbers. Reading every row of the
        snapshot to produce them meant building an object per file on
        every page load.
        """
        for i in range(50):
            _file(f"照片/{i}.jpg")
        client.post("/api/organizer/snapshot")

        res = client.get("/api/organizer/latest")
        assert res.status_code == 200
        body = res.json()
        assert body["has_snapshot"] is True
        assert body["files"] == 50
        assert body["dirs"] == 1

    def test_the_manager_directory_is_not_part_of_the_snapshot(self):
        """The service's own directory is not the user's files."""
        _file("说明.txt")
        (get_storage_root() / ".simplefilemanager").mkdir(exist_ok=True)
        client.post("/api/organizer/snapshot")
        res = client.get("/api/organizer/latest")
        assert res.json()["files"] == 1, res.json()


class TestComparingTwoSnapshots:
    def test_a_file_that_appeared_is_reported(self):
        _file("原来的.txt")
        first = client.post("/api/organizer/snapshot").json()
        time.sleep(1.1)
        _file("新来的.txt")
        second = client.post("/api/organizer/snapshot").json()

        res = client.get("/api/organizer/compare", params={
            "date_from": first["date"], "date_to": second["date"],
        })
        assert res.status_code == 200, res.text
        body = res.json()
        assert [f["name"] for f in body["added_files"]] == ["新来的.txt"]
        assert body["deleted_files"] == []

    def test_a_file_that_went_away_is_reported(self):
        _file("会消失的.txt")
        first = client.post("/api/organizer/snapshot").json()
        time.sleep(1.1)
        (get_storage_root() / "会消失的.txt").unlink()
        second = client.post("/api/organizer/snapshot").json()

        res = client.get("/api/organizer/compare", params={
            "date_from": first["date"], "date_to": second["date"],
        })
        body = res.json()
        assert [f["name"] for f in body["deleted_files"]] == ["会消失的.txt"]

    def test_a_picture_is_suggested_for_the_photos_folder(self):
        _file("随便放着的.jpg")
        first = client.post("/api/organizer/snapshot").json()
        time.sleep(1.1)
        _file("新来的.jpg")
        second = client.post("/api/organizer/snapshot").json()

        res = client.get("/api/organizer/compare", params={
            "date_from": first["date"], "date_to": second["date"],
        })
        suggestions = res.json()["suggestions"]
        assert any(s["target_path"] == "/photos" for s in suggestions), suggestions

    def test_a_snapshot_that_is_not_complete_is_not_compared(self):
        """A snapshot that stopped half way would invent deletions.

        Everything it missed looks like a file that was deleted, so
        comparing against one of those is worse than saying no.
        """
        _file("说明.txt")
        first = client.post("/api/organizer/snapshot").json()
        time.sleep(1.1)
        _file("新来的.txt")
        second = client.post("/api/organizer/snapshot").json()

        # Say that the second snapshot did not see everything.
        from app.routers.organizer import get_organizer_service

        svc = get_organizer_service()
        conn = svc._get_conn()
        conn.execute("UPDATE snapshot_meta SET complete = 0 WHERE date = ?",
                     (second["date"],))
        conn.commit()
        conn.close()

        res = client.get("/api/organizer/compare", params={
            "date_from": first["date"], "date_to": second["date"],
        })
        assert res.status_code == 400, res.text
        assert "不完整" in res.json()["detail"] or "完整" in res.json()["detail"]


class TestCleaningUp:
    def test_old_snapshots_go_away_and_recent_ones_stay(self):
        """Thirty days means thirty days.

        The cutoff was the end of the day, so the thirtieth day was
        thrown away too -- it really kept twenty-nine.
        """
        from datetime import datetime, timedelta

        from app.routers.organizer import get_organizer_service

        svc = get_organizer_service()
        conn = svc._get_conn()
        old = (datetime.now() - timedelta(days=40)).strftime("%Y-%m-%d %H:%M:%S")
        edge = (datetime.now() - timedelta(days=30)).strftime("%Y-%m-%d %H:%M:%S")
        fresh = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
        for date in (old, edge, fresh):
            conn.execute(
                "INSERT INTO file_snapshots (id, date, path, name, is_dir, size,"
                " modified, complete) VALUES (?, ?, 'x', 'x', 0, 0, '', 1)",
                (date.replace(" ", "-") + date.replace(" ", "-"), date),
            )
        conn.commit()
        conn.close()

        svc.delete_old_snapshots(keep_days=30)

        conn = svc._get_conn()
        left = {r[0] for r in conn.execute("SELECT date FROM file_snapshots")}
        conn.close()
        assert old not in left, "an old snapshot was kept"
        assert edge in left, "the thirtieth day was thrown away"
        assert fresh in left


class TestTheSnapshotOfAnEmptyTree:
    """A snapshot of nothing is still a moment worth remembering.

    The snapshots are stored one row per file, so a snapshot of an
    empty tree left no rows -- and then it did not exist. That is
    exactly the snapshot you want to compare against: the one you took
    before everything was deleted.
    """

    def test_a_snapshot_of_an_empty_tree_exists(self):
        res = client.post("/api/organizer/snapshot")
        assert res.status_code == 200, res.text
        body = res.json()
        assert body["total_files"] == 0
        assert body["date"]

        latest = client.get("/api/organizer/latest")
        assert latest.json()["has_snapshot"] is True
        assert latest.json()["files"] == 0

    def test_the_last_snapshot_survives_the_last_file_being_deleted(self):
        _file("会消失的.txt")
        first = client.post("/api/organizer/snapshot").json()
        time.sleep(1.1)
        (get_storage_root() / "会消失的.txt").unlink()
        second = client.post("/api/organizer/snapshot").json()

        # The second snapshot saw an empty tree, and it is still there.
        res = client.get("/api/organizer/compare", params={
            "date_from": first["date"], "date_to": second["date"],
        })
        assert res.status_code == 200, res.text
        assert [f["name"] for f in res.json()["deleted_files"]] == ["会消失的.txt"]

    def test_the_snapshots_are_listed_even_when_they_are_empty(self):
        client.post("/api/organizer/snapshot")
        res = client.get("/api/organizer/snapshots")
        assert res.status_code == 200
        assert len(res.json()["snapshots"]) == 1


class TestWhichSnapshotIsTheLatest:
    """Two snapshots taken in the same second are two snapshots.

    Their names differ only by a random suffix, so ordering them by
    name picks whichever suffix sorts last -- not whichever was taken
    last. The page asks for "the latest" and means the one that was
    taken last.
    """

    def test_the_latest_is_the_one_that_was_taken_last(self):
        _file("第一个.txt")
        first = client.post("/api/organizer/snapshot").json()
        time.sleep(1.1)
        _file("第二个.txt")
        second = client.post("/api/organizer/snapshot").json()
        time.sleep(1.1)
        _file("第三个.txt")
        third = client.post("/api/organizer/snapshot").json()

        res = client.get("/api/organizer/latest")
        assert res.status_code == 200
        assert res.json()["date"] == third["date"], res.json()

    def test_the_list_is_in_the_order_they_were_taken(self):
        # A second apart, so that the names themselves say which is
        # which. Two snapshots taken in the same second differ only by
        # a random suffix, and the order of those suffixes is not the
        # order they were taken in.
        for i in range(3):
            _file(f"文件{i}.txt")
            client.post("/api/organizer/snapshot")
            time.sleep(1.1)

        res = client.get("/api/organizer/snapshots")
        dates = [s["date"] for s in res.json()["snapshots"]]
        assert len(dates) == 3
        assert dates == sorted(dates, reverse=True), dates
