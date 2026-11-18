#!/usr/bin/env python3
"""Back up the NAS's data.

The storage root holds two very different things: the user's files, and
the index that makes them searchable -- a keyword index, a vector
store, the chat history, the plans, the settings. Losing the second
kind is expensive but survivable; losing the first kind is not.

So the whole root is archived. The vector store is the bulk of it and
is technically rebuildable, but rebuilding it means re-embedding every
document on a CPU, which is hours, so it is kept.

The databases are snapshotted rather than copied. The service has them
open, and copying a file out from under a running SQLite gives an
archive that may not open at all; the backup API takes a consistent
snapshot of a live database instead.

The vector store is a directory that the indexer writes into while it
runs, and copying it out from under a running write can catch a data
file half way. So the indexing is asked to stop first, and only started
again when the archive is done.

Usage:
    scripts/backup.py [destination]     # default: /mnt/backup/sfm

Environment:
    BACKUP_DIR   where the archives go
    KEEP         how many to keep (default 7)
    SOURCE_DIR   what to back up (default: the backend's storage root)
    SFM_API_URL  the running service (default: http://127.0.0.1:8000)
    SFM_PASSWORD the administrator's password, so the indexing can be
                 asked to stop while the vector store is copied

To restore: use the restore script.

    scripts/restore.py <archive> [target]

It checks the archive first, puts the files back where they belong, and
keeps the old data aside instead of deleting it. Unpacking the archive
by hand does not: the archive holds one top-level directory named after
the storage root, so `tar xzf archive -C <root>` leaves the data one
level too deep, in a directory the service never looks at.
"""

import json
import os
import shutil
import sqlite3
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
from collections import namedtuple
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from archive_check import verify_archive  # noqa: E402  (a script, not a package)

PROJECT_DIR = Path(__file__).resolve().parent.parent
SOURCE_DIR = Path(os.getenv("SOURCE_DIR", PROJECT_DIR / "backend" / "data"))
BACKUP_DIR = Path(os.getenv("BACKUP_DIR", sys.argv[1] if len(sys.argv) > 1 else "/mnt/backup/sfm"))
KEEP = int(os.getenv("KEEP", "7"))

# SQLite side files that belong with their database.
DB_SUFFIXES = (".db",)

# The snapshot already holds everything these hold. Carrying them along
# puts a log of a different moment next to a consistent copy, which is
# at best noise and at worst a puzzle nobody can answer later.
DB_SIDE_SUFFIXES = ("-wal", "-shm", "-journal")

# What one build produced: how much went in, and what went wrong.
Build = namedtuple("Build", "files size problems")


def snapshot_database(src: Path, dest: Path) -> bool:
    """Copy a live SQLite database into a consistent snapshot."""
    try:
        src_conn = sqlite3.connect(f"file:{src}?mode=ro", uri=True, timeout=10)
        try:
            dest_conn = sqlite3.connect(dest)
            try:
                src_conn.backup(dest_conn)
            finally:
                dest_conn.close()
        finally:
            src_conn.close()
        return True
    except sqlite3.Error as e:
        print(f"[backup] {src.name} 快照失败：{e}", file=sys.stderr)
        return False


class Indexing:
    """Ask the running service to hold its indexing still.

    The vector store is written while the indexer runs. Copying it in
    the middle of a write can catch a data file half way, and the
    archive then holds a vector store that may not open. The service
    knows how to stop; this asks it to, and to start again afterwards.
    """

    def __init__(self, api_url: str, password: str):
        self.api_url = api_url.rstrip("/")
        self.password = password
        self.token = ""

    def _call(self, path: str, method: str = "POST", body: dict | None = None):
        data = json.dumps(body).encode() if body is not None else None
        headers = {"Content-Type": "application/json"}
        if self.token:
            headers["Authorization"] = "Bearer " + self.token
        req = urllib.request.Request(
            self.api_url + path, data=data, headers=headers, method=method
        )
        with urllib.request.urlopen(req, timeout=30) as resp:
            raw = resp.read()
        return json.loads(raw) if raw else {}

    def stop(self) -> bool:
        """Stop the indexing. Returns whether it worked."""
        try:
            out = self._call("/api/auth/login", body={
                "username": os.getenv("SFM_ADMIN_USER", "admin"),
                "password": self.password,
            })
            self.token = out.get("access_token") or out.get("token") or ""
        except urllib.error.HTTPError as e:
            print(f"[backup] 登录失败：{e}（向量库可能拷得不完整）", file=sys.stderr)
            return False
        except Exception as e:
            print(f"[backup] 连不上服务：{e}（向量库可能拷得不完整）", file=sys.stderr)
            return False
        if not self.token:
            print("[backup] 没有拿到令牌（向量库可能拷得不完整）", file=sys.stderr)
            return False
        try:
            self._call("/api/settings/auto_index/pause", body={"seconds": 0})
            self._call("/api/images/index/pause")
        except Exception as e:
            print(f"[backup] 让索引停下来失败：{e}（向量库可能拷得不完整）", file=sys.stderr)
            return False
        return True

    def start_again(self) -> None:
        try:
            self._call("/api/settings/auto_index/resume")
            self._call("/api/images/index/resume")
        except Exception as e:
            print(f"[backup] 索引没能自动恢复，请手动恢复：{e}", file=sys.stderr)

    def wait_until_idle(self, seconds: float = 30.0) -> None:
        """Give the file that was being written a moment to finish."""
        deadline = time.time() + seconds
        while time.time() < deadline:
            try:
                status = self._call("/api/images/index/status", method="GET")
            except Exception:
                return
            if not status.get("running"):
                return
            time.sleep(1.0)


def build_archive(archive: Path) -> Build:
    """Write the archive.

    Returns what went in and what went wrong along the way. A
    database that could not be snapshotted is still copied, because a
    rough copy beats no copy, but it is reported: the backup is only
    as good as its weakest database, and the operator has to know
    which one that was.
    """
    problems: list[str] = []
    staging = Path(tempfile.mkdtemp(prefix="sfm-backup-"))
    try:
        root_copy = staging / SOURCE_DIR.name
        root_copy.mkdir()

        for path in sorted(SOURCE_DIR.rglob("*")):
            rel = path.relative_to(SOURCE_DIR)
            if any(part.startswith(".") and part != ".simplefilemanager" for part in rel.parts):
                # Hidden things other than our own data directory: a
                # mount point, a partial download, an editor's swap file.
                continue
            if path.is_dir():
                (root_copy / rel).mkdir(parents=True, exist_ok=True)
                continue
            target = root_copy / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            if path.name.endswith(DB_SIDE_SUFFIXES):
                # The snapshot of the database beside it already holds
                # everything this holds.
                continue
            if path.suffix in DB_SUFFIXES:
                if not snapshot_database(path, target):
                    problems.append(f"{path.name} 没有做成一致的快照，只是原样拷了一份")
                    shutil.copy2(path, target)
                continue
            shutil.copy2(path, target)

        files = sum(1 for _ in root_copy.rglob("*") if _.is_file())
        with tarfile.open(archive, "w:gz") as tar:
            tar.add(root_copy, arcname=SOURCE_DIR.name)
        size = archive.stat().st_size
        return Build(files, size, problems)
    finally:
        shutil.rmtree(staging, ignore_errors=True)


def main() -> int:
    if not SOURCE_DIR.is_dir():
        print(f"备份失败：源目录不存在 {SOURCE_DIR}", file=sys.stderr)
        return 1
    BACKUP_DIR.mkdir(parents=True, exist_ok=True)

    stamp = time.strftime("%Y%m%d-%H%M%S")
    archive = BACKUP_DIR / f"sfm-{stamp}.tar.gz"
    tmp = BACKUP_DIR / f".sfm-{stamp}.tar.gz.part"

    print(f"[backup] 源目录 : {SOURCE_DIR}")
    print(f"[backup] 目标   : {archive}")

    # The vector store is written while the indexer runs. Ask it to
    # stop, so the archive holds a store that opens.
    indexing = Indexing(
        os.getenv("SFM_API_URL", "http://127.0.0.1:8000"),
        os.getenv("SFM_PASSWORD", ""),
    )
    paused = False
    if os.getenv("SFM_PASSWORD"):
        paused = indexing.stop()
        if paused:
            indexing.wait_until_idle()
            print("[backup] 索引   : 已暂停")
    else:
        print("[backup] 索引   : 没有密码，索引继续在跑（向量库可能拷得不完整）")

    try:
        build = build_archive(tmp)
        for problem in build.problems:
            print(f"[backup] 有问题 : {problem}", file=sys.stderr)
        # Written to a temporary name and moved into place, so an
        # interrupted backup never leaves a truncated file that looks
        # like a good one.
        tmp.replace(archive)
    except Exception as e:
        tmp.unlink(missing_ok=True)
        print(f"备份失败：{e}", file=sys.stderr)
        return 1
    finally:
        if paused:
            indexing.start_again()
            print("[backup] 索引   : 已恢复")

    print(f"[backup] 完成   : {archive}（{build.files} 个文件，{build.size / 1024 / 1024:.1f} MB）")

    # Check that the archive is not merely readable but restorable. The
    # old check -- that the tar could be opened and its names listed --
    # said nothing about whether the databases inside open, so a broken
    # backup could look exactly like a good one.
    report = verify_archive(archive)
    for note in report.notes:
        print(f"[backup] 没查   : {note}")
    if not report.ok:
        # A backup that cannot be restored must not sit in the rotation
        # looking like one. It is kept under a name the rotation does
        # not match, so it can be looked at but not restored by
        # accident.
        broken = archive.with_suffix(archive.suffix + ".broken")
        archive.replace(broken)
        for problem in report.problems:
            print(f"备份失败：{problem}", file=sys.stderr)
        print(f"备份失败：这个归档还原不了，已改名存为 {broken.name}", file=sys.stderr)
        return 1
    print(f"[backup] 校验   : 还原得了 ✓")

    archives = sorted(BACKUP_DIR.glob("sfm-*.tar.gz"), key=lambda p: p.stat().st_mtime)
    if len(archives) > KEEP:
        for old in archives[: len(archives) - KEEP]:
            old.unlink(missing_ok=True)
            print(f"[backup] 清理   : 删除 {old.name}")
    print(f"[backup] 当前保留 : {len(list(BACKUP_DIR.glob('sfm-*.tar.gz')))} 个归档")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
