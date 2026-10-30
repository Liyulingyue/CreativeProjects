"""Finding the copies of a file that are on the machine twice.

A NAS accumulates duplicates: the same photo off the phone and off
the laptop, the same film downloaded twice, a document saved over and
over under different names. They cost space nobody is using, and they
are hard to find by eye -- the names do not have to match.

The indexer already works out a SHA-256 of every file it sees, so the
duplicates are already known; this is the part that puts them in
front of somebody and works out how much room they are taking up.
"""

from __future__ import annotations

import logging
import sqlite3
from pathlib import Path
from typing import Optional

log = logging.getLogger(__name__)

# A file smaller than this is not worth grouping -- there are a great
# many of them and they are hardly the reason a disk is full.
MIN_DUPLICATE_SIZE = 1024

# How many copies of one file to name. A group of four hundred copies
# is not something anybody reads.
MAX_COPIES_SHOWN = 50


class DuplicateFinder:
    """The groups of files whose contents are identical."""

    def __init__(self, storage_root: Optional[Path] = None):
        from .deps import DATA_DIR, get_storage_root

        self.index_db_path = str(DATA_DIR / "file_index.db")
        self.storage_root = Path(storage_root) if storage_root else get_storage_root()

    def _connect(self) -> sqlite3.Connection:
        from . import db

        return db.connect(self.index_db_path)

    def _hashes(self) -> dict[str, list[tuple[str, int]]]:
        """content hash -> [(path, size), ...] for the files that have one."""
        with self._connect() as conn:
            try:
                rows = conn.execute(
                    "SELECT path, size, content_hash FROM file_index"
                    " WHERE content_hash IS NOT NULL AND content_hash != ''"
                    " AND size >= ?",
                    (MIN_DUPLICATE_SIZE,),
                ).fetchall()
            except sqlite3.Error:
                return {}
        groups: dict[str, list[tuple[str, int]]] = {}
        for path, size, digest in rows:
            groups.setdefault(digest, []).append((path, int(size or 0)))
        return groups

    def find(self, limit: int = 200) -> dict:
        """The duplicate groups, biggest waste first.

        Only the files this person may open are counted: a duplicate
        they cannot see is not a duplicate they can get rid of.
        """
        from .deps import can_access

        groups = self._hashes()
        found = []
        wasted = 0
        for digest, entries in groups.items():
            if len(entries) < 2:
                continue
            visible = [
                (path, size)
                for path, size in entries
                if can_access(self.storage_root / path, "read")
            ]
            if len(visible) < 2:
                continue
            visible.sort(key=lambda e: e[0])
            size = visible[0][1]
            # Every copy but one is space being spent for nothing.
            waste = size * (len(visible) - 1)
            wasted += waste
            found.append({
                "hash": digest,
                "size": size,
                "copies": len(visible),
                "wasted": waste,
                "files": [
                    {"path": path, "name": Path(path).name, "size": s}
                    for path, s in visible[:MAX_COPIES_SHOWN]
                ],
            })

        found.sort(key=lambda g: g["wasted"], reverse=True)
        found = found[:limit]
        return {
            "groups": found,
            "count": len(found),
            "wasted": wasted,
            "total_wasted": wasted,
        }

    def clean(self, digest: str, keep: str = "") -> dict:
        """Put the extra copies in the recycle bin.

        One copy is kept and the rest go to the bin -- the bin, not
        nowhere, so a mistake here is a thing that can be undone. The
        copy kept is the one named, or else the oldest, which is
        usually the one that was there first and might be the one
        something else refers to.

        Only files the caller may write to are touched. A duplicate
        somebody else keeps is not this person's to throw away.
        """
        from .deps import check_access
        from .trash import TrashError, TrashService

        entries = self._hashes().get(digest)
        if not entries or len(entries) < 2:
            raise ValueError("没有找到这组重复文件")

        candidates = []
        for path, size in entries:
            target = self.storage_root / path
            try:
                check_access(target, "write")
            except Exception:
                continue
            candidates.append((path, size, target))
        if len(candidates) < 2:
            raise ValueError("你没有权限处理这组重复文件")

        if keep:
            kept = next((c for c in candidates if c[0] == keep), None)
            if kept is None:
                raise ValueError("要保留的文件不在这一组里")
        else:
            # The oldest is kept: it is the one that was there first.
            kept = min(
                candidates,
                key=lambda c: (c[2].stat().st_mtime if c[2].exists() else 0, c[0]),
            )

        bin_service = TrashService(self.storage_root)
        trashed = []
        failed = []
        for path, size, target in candidates:
            if target == kept[2]:
                continue
            try:
                bin_service.trash(target)
                trashed.append(path)
            except (TrashError, OSError) as e:
                failed.append({"path": path, "error": str(e)})

        return {
            "kept": kept[0],
            "trashed": trashed,
            "failed": failed,
            "wasted": sum(
                size for path, size, _ in candidates
                if path != kept[0] and path in set(trashed)
            ),
        }

    def scan_for_duplicates(self, budget: int = 2000) -> dict:
        """Work out the hashes of files that do not have one yet.

        The indexer fills these in as it goes, but a NAS that has been
        running for years has files that were indexed before there were
        hashes. This does a bounded pass over them.
        """
        with self._connect() as conn:
            try:
                rows = conn.execute(
                    "SELECT path FROM file_index"
                    " WHERE content_hash IS NULL OR content_hash = ''"
                ).fetchall()
            except sqlite3.Error:
                return {"hashed": 0, "remaining": 0}

        hashed = 0
        for (path,) in rows[:budget]:
            abs_path = self.storage_root / path
            if not abs_path.is_file():
                continue
            digest = _hash_file(abs_path)
            if not digest:
                continue
            with self._connect() as conn:
                try:
                    conn.execute(
                        "UPDATE file_index SET content_hash = ? WHERE path = ?",
                        (digest, path),
                    )
                    conn.commit()
                except sqlite3.Error:
                    continue
            hashed += 1
        return {"hashed": hashed, "remaining": max(0, len(rows) - budget)}


def _hash_file(path: Path) -> Optional[str]:
    """SHA-256 of a file's bytes, or None if it cannot be read."""
    import hashlib

    try:
        h = hashlib.sha256()
        with open(path, "rb") as f:
            for block in iter(lambda: f.read(1 << 20), b""):
                h.update(block)
        return h.hexdigest()
    except OSError:
        return None
