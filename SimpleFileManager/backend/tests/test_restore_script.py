"""The backup and the restore, end to end.

A backup is only worth what it can restore, and the restore is the part
nobody writes until the day it is needed. These tests do the whole
round trip: a storage root with files in it, a backup, a restore into
a fresh directory, and a look at what came back.

They also check the things that make a backup a lie: an archive whose
database does not open, an archive that gets unpacked one level too
deep, and a restore that quietly overwrites what was there.
"""
import importlib.util
import json
import sqlite3
import sys
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

import pytest

SCRIPTS = Path(__file__).resolve().parent.parent.parent / "scripts"


def load_script(name: str):
    """The scripts are tools, not a package; load them where they lie."""
    spec = importlib.util.spec_from_file_location(f"sfm_{name}_script", SCRIPTS / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


@pytest.fixture
def storage(tmp_path):
    """A storage root with the things a real one holds."""
    root = tmp_path / "data"
    (root / ".simplefilemanager").mkdir(parents=True)
    (root / "照片").mkdir()
    (root / "照片" / "猫.jpg").write_bytes(b"jpeg-bytes")
    (root / "说明.txt").write_text("你好", encoding="utf-8")

    db = sqlite3.connect(root / ".simplefilemanager" / "meta.db")
    db.execute("CREATE TABLE t (a TEXT)")
    db.execute("INSERT INTO t VALUES ('hello')")
    db.commit()
    db.close()
    return root


@pytest.fixture
def backup(storage, monkeypatch):
    """The backup script, pointed at the fake root.

    Without this it would archive the real storage root, which holds
    a hundred megabytes of index -- slow enough to look like a hang.
    """
    module = load_script("backup")
    monkeypatch.setattr(module, "SOURCE_DIR", storage)
    return module


@pytest.fixture
def restore(monkeypatch):
    """The restore script, with the running service out of the way."""
    module = load_script("restore")
    monkeypatch.setattr(module, "API_URL", "http://127.0.0.1:1")
    return module


def test_a_backup_comes_back_where_it_belonged(tmp_path, backup, restore):
    """The whole round trip.

    The files come back, the database comes back with its rows, and
    they come back in the storage root itself -- not one level too
    deep, which is where unpacking the tar by hand leaves them.
    """
    archive = tmp_path / "backup.tar.gz"
    build = backup.build_archive(archive)
    assert build.problems == []

    restored = tmp_path / "restored"
    assert restore.main([str(archive), str(restored)]) == 0

    assert (restored / "照片" / "猫.jpg").read_bytes() == b"jpeg-bytes"
    assert (restored / "说明.txt").read_text(encoding="utf-8") == "你好"
    assert (restored / ".simplefilemanager" / "meta.db").exists()
    # Not one level too deep: this is the mistake the old
    # instructions in the backup script led people into.
    assert not (restored / "data").exists()

    conn = sqlite3.connect(restored / ".simplefilemanager" / "meta.db")
    try:
        assert conn.execute("SELECT a FROM t").fetchone()[0] == "hello"
    finally:
        conn.close()


def test_the_archive_is_checked_before_anybody_relies_on_it(tmp_path, backup):
    """A good backup passes the check."""
    archive = tmp_path / "backup.tar.gz"
    backup.build_archive(archive)

    report = backup.verify_archive(archive)
    assert report.problems == []
    assert report.ok is True


def test_a_database_that_does_not_open_is_a_problem_not_a_backup(tmp_path, backup):
    """The check looks inside, not just at the file list."""
    archive = tmp_path / "backup.tar.gz"
    backup.build_archive(archive)

    # A database of rubbish in the place of a good one.
    import io
    import tarfile

    broken = tmp_path / "broken.tar.gz"
    with tarfile.open(archive, "r:gz") as src, tarfile.open(broken, "w:gz") as dst:
        for member in src.getmembers():
            if member.name.endswith("meta.db"):
                payload = b"this is not a database at all, not even close"
                info = tarfile.TarInfo(member.name)
                info.size = len(payload)
                dst.addfile(info, io.BytesIO(payload))
            else:
                dst.addfile(member, src.extractfile(member))

    report = backup.verify_archive(broken)
    assert not report.ok
    assert any("meta.db" in p for p in report.problems)


def test_a_broken_archive_is_not_restored(tmp_path, backup, restore):
    """The restore refuses what the check rejects."""
    archive = tmp_path / "backup.tar.gz"
    backup.build_archive(archive)

    import io
    import tarfile

    broken = tmp_path / "broken.tar.gz"
    with tarfile.open(archive, "r:gz") as src, tarfile.open(broken, "w:gz") as dst:
        for member in src.getmembers():
            if member.name.endswith("meta.db"):
                payload = b"not a database"
                info = tarfile.TarInfo(member.name)
                info.size = len(payload)
                dst.addfile(info, io.BytesIO(payload))
            else:
                dst.addfile(member, src.extractfile(member))

    target = tmp_path / "restored"
    assert restore.main([str(broken), str(target)]) != 0
    assert not target.exists() or not any(target.iterdir())


def test_the_old_data_is_moved_aside_not_deleted(tmp_path, backup, restore):
    """A restore that turns out to be the wrong one has to be undoable."""
    archive = tmp_path / "backup.tar.gz"
    backup.build_archive(archive)

    target = tmp_path / "restored"
    target.mkdir()
    (target / "旧文件.txt").write_text("还在", encoding="utf-8")

    assert restore.main([str(archive), str(target)]) == 0

    # The new data is in place.
    assert (target / "说明.txt").exists()
    # And the old data is not gone.
    asides = list(tmp_path.glob("restored.before-restore-*"))
    assert len(asides) == 1
    assert (asides[0] / "旧文件.txt").read_text(encoding="utf-8") == "还在"


class FakeService(BaseHTTPRequestHandler):
    """A service that is up."""

    def log_message(self, *args):
        pass

    def do_GET(self):
        body = json.dumps({"status": "alive"}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def test_the_backup_tells_the_truth_when_the_snapshot_fails(tmp_path, backup, monkeypatch):
    """A database that could not be snapshotted is reported.

    The old code fell back to copying the live database, said nothing,
    and produced an archive that looked like every other archive.
    """
    monkeypatch.setattr(backup, "snapshot_database", lambda src, dest: False)

    archive = tmp_path / "backup.tar.gz"
    build = backup.build_archive(archive)
    assert build.problems
    assert any("meta.db" in p for p in build.problems)


def test_the_wal_is_not_carried_along_with_the_snapshot(tmp_path, backup):
    """The snapshot already holds everything the log holds."""
    import tarfile

    db = sqlite3.connect(backup.SOURCE_DIR / ".simplefilemanager" / "meta.db")
    db.execute("PRAGMA journal_mode=WAL")
    db.execute("INSERT INTO t VALUES ('second')")
    db.commit()
    db.close()
    # The service holds the database open, so the log stays on disk.
    holder = sqlite3.connect(backup.SOURCE_DIR / ".simplefilemanager" / "meta.db")
    holder.execute("SELECT count(*) FROM t").fetchone()
    assert (backup.SOURCE_DIR / ".simplefilemanager" / "meta.db-wal").exists()

    archive = tmp_path / "backup.tar.gz"
    backup.build_archive(archive)
    with tarfile.open(archive, "r:gz") as tar:
        names = tar.getnames()
    holder.close()
    assert not any(n.endswith("-wal") for n in names)
    assert not any(n.endswith("-shm") for n in names)


def test_a_dry_run_changes_nothing(tmp_path, backup, restore):
    """Looking before you leap."""
    archive = tmp_path / "backup.tar.gz"
    backup.build_archive(archive)

    target = tmp_path / "restored"
    assert restore.main(["--dry-run", str(archive), str(target)]) == 0
    assert not target.exists() or not any(target.iterdir())


def test_the_live_storage_root_is_not_touched_while_the_service_runs(tmp_path, backup, restore, monkeypatch):
    """The service writes to the storage root.

    The check is about the directory being restored over, not about
    the service being up in general: looking at an archive in a
    scratch directory is harmless.
    """
    archive = tmp_path / "backup.tar.gz"
    backup.build_archive(archive)

    server = HTTPServer(("127.0.0.1", 0), FakeService)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        live = tmp_path / "live-root"
        live.mkdir()
        (live / "旧文件.txt").write_text("还在", encoding="utf-8")
        monkeypatch.setattr(restore, "DEFAULT_TARGET", live)
        monkeypatch.setattr(restore, "API_URL", f"http://127.0.0.1:{server.server_address[1]}")

        # No target given: the live root is the one that would be hit.
        assert restore.main([str(archive)]) != 0
        assert (live / "旧文件.txt").exists()

        # A scratch directory is fine even with the service running.
        scratch = tmp_path / "scratch"
        assert restore.main([str(archive), str(scratch)]) == 0
        assert (scratch / "说明.txt").exists()
    finally:
        server.shutdown()
        server.server_close()
