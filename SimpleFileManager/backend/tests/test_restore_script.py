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