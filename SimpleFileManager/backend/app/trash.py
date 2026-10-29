"""The recycle bin.

Deleting a file used to mean unlinking it, which on a NAS is a way to
lose something that took an afternoon to write and no way to get it
back. The bin keeps what was deleted until it is emptied.

The layout under the storage root::

    .trash/
      20261029-141233-a1b2c3/
        meta.json      what was deleted, where it came from, when
        data/          the file or directory itself

Each deletion gets its own directory rather than a flat pile, so the
original path is recoverable and two files with the same name never
collide. The directory name sorts by time, so the listing is in
deletion order without reading any metadata.

The bin lives inside the storage root so it moves with the data and
can be reached by whatever else touches the share -- but it is hidden
from the file browser and skipped by the indexer, so nothing that was
deleted keeps showing up in search.
"""

from __future__ import annotations

import json
import logging
import os
import shutil
import time
import uuid
from pathlib import Path
from typing import Optional

log = logging.getLogger(__name__)

TRASH_DIR_NAME = ".trash"
META_NAME = "meta.json"
DATA_DIR_NAME = "data"


class TrashError(Exception):
    """Something went wrong moving a file into or out of the bin."""


def _dir_size(path: Path) -> int:
    """Total bytes of a file, or of everything under a directory."""
    if path.is_file():
        try:
            return path.stat().st_size
        except OSError:
            return 0
    total = 0
    for root, _dirs, files in os.walk(path):
        for name in files:
            try:
                total += (Path(root) / name).stat().st_size
            except OSError:
                continue
    return total


def trash_root(storage_root: Path) -> Path:
    return storage_root / TRASH_DIR_NAME


class TrashService:
    """Moves files into the bin and back out again."""

    def __init__(self, storage_root: Path):
        self.storage_root = Path(storage_root)

    # ---- internals ----

    def _entry_dir(self, entry_id: str) -> Path:
        return trash_root(self.storage_root) / entry_id

    def _read_meta(self, entry_dir: Path) -> Optional[dict]:
        meta_path = entry_dir / META_NAME
        try:
            return json.loads(meta_path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return None

    # ---- operations ----

    def trash(self, target: Path) -> dict:
        """Move a file or directory into the bin.

        Returns the entry that was created. The original path is kept so
        the file can be put back where it came from.
        """
        target = Path(target)
        if not target.exists():
            raise TrashError("路径不存在")
        if trash_root(self.storage_root) in target.parents:
            raise TrashError("回收站里的东西不能再次删除")

        root = trash_root(self.storage_root)
        root.mkdir(parents=True, exist_ok=True)

        entry_id = f"{time.strftime('%Y%m%d-%H%M%S')}-{uuid.uuid4().hex[:6]}"
        entry_dir = self._entry_dir(entry_id)
        data_dir = entry_dir / DATA_DIR_NAME
        data_dir.mkdir(parents=True)

        try:
            # The file keeps its name inside the entry, so a directory
            # that is restored comes back whole rather than as a
            # scattering of loose children.
            shutil.move(str(target), str(data_dir / target.name))
        except OSError as e:
            shutil.rmtree(entry_dir, ignore_errors=True)
            raise TrashError(f"移动到回收站失败: {e}") from e

        meta = {
            "id": entry_id,
            "name": target.name,
            "original_path": str(target),
            "rel_path": str(target.relative_to(self.storage_root)),
            "deleted_at": int(time.time()),
            "deleted_at_human": time.strftime("%Y-%m-%d %H:%M:%S"),
            "is_dir": target.is_dir(),
            "size": _dir_size(data_dir / target.name),
        }
        try:
            (entry_dir / META_NAME).write_text(
                json.dumps(meta, ensure_ascii=False, indent=2), encoding="utf-8"
            )
        except OSError as e:
            # The file is already in the bin; without metadata it can
            # still be emptied, just not restored to its old path.
            log.warning("could not write trash metadata for %s: %s", entry_id, e)

        log.info("trashed %s -> %s", target, entry_id)
        return meta

    def list(self) -> list[dict]:
        """Every entry in the bin, newest first."""
        root = trash_root(self.storage_root)
        if not root.exists():
            return []
        entries = []
        for entry_dir in root.iterdir():
            if not entry_dir.is_dir():
                continue
            meta = self._read_meta(entry_dir)
            if meta is None:
                # A directory without metadata is a leftover from an
                # interrupted delete. It is still shown, with what can
                # be worked out from the directory itself.
                children = list(entry_dir.iterdir())
                data = entry_dir / DATA_DIR_NAME
                inner = list(data.iterdir()) if data.exists() else []
                meta = {
                    "id": entry_dir.name,
                    "name": inner[0].name if inner else entry_dir.name,
                    "original_path": "",
                    "rel_path": "",
                    "deleted_at": int(entry_dir.stat().st_mtime),
                    "deleted_at_human": time.strftime(
                        "%Y-%m-%d %H:%M:%S", time.localtime(entry_dir.stat().st_mtime)
                    ),
                    "is_dir": bool(inner and inner[0].is_dir()),
                    "size": _dir_size(inner[0]) if inner else 0,
                    "incomplete": True,
                }
            entries.append(meta)
        entries.sort(key=lambda m: m.get("deleted_at", 0), reverse=True)
        return entries

    def restore(self, entry_id: str) -> dict:
        """Put a deleted file back where it came from."""
        entry_dir = self._entry_dir(entry_id)
        if not entry_dir.exists():
            raise TrashError("回收站里没有这一项")
        meta = self._read_meta(entry_dir)
        data_dir = entry_dir / DATA_DIR_NAME
        inner = list(data_dir.iterdir()) if data_dir.exists() else []
        if not inner:
            raise TrashError("回收站里这一项没有内容")
        source = inner[0]

        if meta and meta.get("original_path"):
            dest = Path(meta["original_path"])
            if not dest.is_relative_to(self.storage_root):
                raise TrashError("原路径不在存储根目录内")
        else:
            dest = self.storage_root / source.name

        # Restoring over something that took the file's place would
        # destroy it, so the new name is chosen instead.
        if dest.exists():
            stem, suffix = dest.stem, dest.suffix
            n = 1
            while dest.exists():
                dest = dest.with_name(f"{stem}(恢复{n}){suffix}")
                n += 1

        dest.parent.mkdir(parents=True, exist_ok=True)
        try:
            shutil.move(str(source), str(dest))
        except OSError as e:
            raise TrashError(f"还原失败: {e}") from e
        shutil.rmtree(entry_dir, ignore_errors=True)
        log.info("restored %s -> %s", entry_id, dest)
        return {"id": entry_id, "path": str(dest), "renamed": dest.name != (meta or {}).get("name")}

    def purge(self, entry_id: str) -> dict:
        """Delete one entry for good."""
        entry_dir = self._entry_dir(entry_id)
        if not entry_dir.exists():
            raise TrashError("回收站里没有这一项")
        meta = self._read_meta(entry_dir) or {}
        shutil.rmtree(entry_dir, ignore_errors=True)
        log.info("purged %s", entry_id)
        return {"id": entry_id, "name": meta.get("name", "")}

    def empty(self) -> dict:
        """Delete everything in the bin."""
        root = trash_root(self.storage_root)
        if not root.exists():
            return {"removed": 0}
        count = 0
        for entry_dir in list(root.iterdir()):
            if entry_dir.is_dir():
                shutil.rmtree(entry_dir, ignore_errors=True)
                count += 1
        log.info("emptied the trash: %d entries", count)
        return {"removed": count}

    def stats(self) -> dict:
        entries = self.list()
        return {
            "count": len(entries),
            "size": sum(e.get("size", 0) for e in entries),
        }
