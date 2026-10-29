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

Usage:
    scripts/backup.py [destination]     # default: /mnt/backup/sfm

Environment:
    BACKUP_DIR   where the archives go
    KEEP         how many to keep (default 7)
    SOURCE_DIR   what to back up (default: the backend's storage root)

To restore: untar the archive over the storage root. It is a plain
tar.gz, so `tar xzf <archive> -C <storage root>` puts everything back
where it was, including the hidden .simplefilemanager directory.
"""

import os
import shutil
import sqlite3
import sys
import tarfile
import tempfile
import time
from pathlib import Path

PROJECT_DIR = Path(__file__).resolve().parent.parent
SOURCE_DIR = Path(os.getenv("SOURCE_DIR", PROJECT_DIR / "backend" / "data"))
BACKUP_DIR = Path(os.getenv("BACKUP_DIR", sys.argv[1] if len(sys.argv) > 1 else "/mnt/backup/sfm"))
KEEP = int(os.getenv("KEEP", "7"))

# SQLite side files that belong with their database.
DB_SUFFIXES = (".db",)


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


def build_archive(archive: Path) -> tuple[int, int]:
    """Write the archive. Returns (file count, byte count)."""
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
            if path.suffix in DB_SUFFIXES:
                if not snapshot_database(path, target):
                    shutil.copy2(path, target)
                continue
            shutil.copy2(path, target)

        files = sum(1 for _ in root_copy.rglob("*") if _.is_file())
        with tarfile.open(archive, "w:gz") as tar:
            tar.add(root_copy, arcname=SOURCE_DIR.name)
        size = archive.stat().st_size
        return files, size
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
    try:
        files, size = build_archive(tmp)
        # Written to a temporary name and moved into place, so an
        # interrupted backup never leaves a truncated file that looks
        # like a good one.
        tmp.replace(archive)
    except Exception as e:
        tmp.unlink(missing_ok=True)
        print(f"备份失败：{e}", file=sys.stderr)
        return 1

    print(f"[backup] 完成   : {archive}（{files} 个文件，{size / 1024 / 1024:.1f} MB）")

    # Check that the archive is readable. A backup that cannot be read
    # is worse than no backup, because it looks like one.
    try:
        with tarfile.open(archive, "r:gz") as tar:
            tar.getnames()
        print("[backup] 校验   : 归档可读 ✓")
    except Exception as e:
        print(f"备份失败：归档无法读取：{e}", file=sys.stderr)
        return 1

    archives = sorted(BACKUP_DIR.glob("sfm-*.tar.gz"), key=lambda p: p.stat().st_mtime)
    if len(archives) > KEEP:
        for old in archives[: len(archives) - KEEP]:
            old.unlink(missing_ok=True)
            print(f"[backup] 清理   : 删除 {old.name}")
    print(f"[backup] 当前保留 : {len(list(BACKUP_DIR.glob('sfm-*.tar.gz')))} 个归档")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
