"""The backup script: what it copies, and what it leaves alone.

A backup is only worth what it can restore. The databases are
snapshotted rather than copied, the vector store is copied while the
indexing is standing still, and the staging area is left out.
"""
import importlib.util
import json
import sqlite3
import sys
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parent.parent.parent / "scripts" / "backup.py"


def load_script():
    """The script is a tool, not a package; load it where it lies."""
    spec = importlib.util.spec_from_file_location("sfm_backup_script", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    sys.modules["sfm_backup_script"] = module
    spec.loader.exec_module(module)
    return module


class FakeService(BaseHTTPRequestHandler):
    """A stand-in for the running service.

    It answers the three things the backup asks: the login, the pause
    and the resume.
    """

    def log_message(self, *args):
        pass

    def _read(self):
        length = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(length) if length else b""
        try:
            return json.loads(raw) if raw else {}
        except ValueError:
            return {}

    def _reply(self, payload, code=200):
        body = json.dumps(payload).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        body = self._read()
        if self.path.endswith("/auth/login"):
            self._reply({"access_token": "tok-123"})
        elif self.path.endswith("/auto_index/pause"):
            self.server.calls.append(("text-pause", body))
            self._reply({"paused": True, "indefinite": True})
        elif self.path.endswith("/auto_index/resume"):
            self.server.calls.append(("text-resume", body))
            self._reply({"paused": False})
        elif self.path.endswith("/index/pause"):
            self.server.calls.append(("image-pause", body))
            self._reply({"paused": True})
        elif self.path.endswith("/index/resume"):
            self.server.calls.append(("image-resume", body))
            self._reply({"paused": False})
        else:
            self._reply({"detail": "没有这个接口"}, code=404)

    def do_GET(self):
        if self.path.endswith("/index/status"):
            self._reply({"running": False})
        else:
            self._reply({"detail": "没有这个接口"}, code=404)


@pytest.fixture
def server():
    server = HTTPServer(("127.0.0.1", 0), FakeService)
    server.calls = []
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    yield server
    server.shutdown()
    server.server_close()


def test_the_indexing_is_asked_to_stop_and_then_to_start_again(server):
    """The vector store is written while the indexer runs.

    Copying it in the middle of a write catches a data file half way,
    and the archive then holds a store that may not open.
    """
    module = load_script()
    url = f"http://127.0.0.1:{server.server_address[1]}"
    indexing = module.Indexing(url, "secret")

    assert indexing.stop() is True
    assert ("text-pause", {"seconds": 0}) in server.calls
    assert ("image-pause", {}) in server.calls

    indexing.start_again()
    assert ("text-resume", {}) in server.calls
    assert ("image-resume", {}) in server.calls


def test_a_service_that_is_not_there_is_not_a_disaster():
    """No service means no pause -- the backup still happens."""
    module = load_script()
    indexing = module.Indexing("http://127.0.0.1:1", "secret")
    assert indexing.stop() is False


def test_the_archive_holds_the_databases_and_leaves_the_rest_out(tmp_path, monkeypatch):
    """A restored archive has to open.

    The databases are snapshotted, the hidden directories are left out,
    and the files a person cares about are all there.
    """
    module = load_script()
    source = tmp_path / "data"
    (source / ".simplefilemanager").mkdir(parents=True)
    (source / "photos").mkdir()
    (source / "photos" / "cat.jpg").write_bytes(b"jpeg")

    # A database that is open, the way it is while the service runs.
    db = sqlite3.connect(source / ".simplefilemanager" / "meta.db")
    db.execute("CREATE TABLE t (a TEXT)")
    db.execute("INSERT INTO t VALUES ('hello')")
    db.commit()

    # Things that should not be in the archive.
    (source / ".uploads" / "abc").mkdir(parents=True)
    (source / ".uploads" / "abc" / "000000.part").write_bytes(b"half")
    (source / ".trash").mkdir()
    (source / ".trash" / "old.txt").write_text("gone", encoding="utf-8")

    monkeypatch.setattr(module, "SOURCE_DIR", source)
    archive = tmp_path / "out.tar.gz"
    build = module.build_archive(archive)
    assert build.files > 0
    assert build.problems == []
    assert archive.exists()

    import tarfile
    with tarfile.open(archive, "r:gz") as tar:
        names = tar.getnames()
        tar.extractall(tmp_path / "restored")

    assert any("photos/cat.jpg" in n for n in names)
    assert not any(".uploads" in n for n in names)
    assert not any(".trash" in n for n in names)

    # The snapshot opens and holds the row that was written.
    restored = tmp_path / "restored" / "data" / ".simplefilemanager" / "meta.db"
    conn = sqlite3.connect(restored)
    try:
        assert conn.execute("SELECT a FROM t").fetchone()[0] == "hello"
    finally:
        conn.close()
