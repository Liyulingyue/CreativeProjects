"""Uploads that do not lose files and do not have to fit in one request.

Two problems with uploading a file in a single POST. The file has to
arrive in one piece, so anything bigger than the proxy's limit fails
with no way to resume -- the whole thing starts again. And the
destination is opened for writing straight away, so a file that already
exists is replaced without a word, which on a NAS is how a day of work
disappears.

So an upload is prepared, sent in pieces, and only then put in place.
The pieces are kept until the client says it is finished, which means a
dropped connection resumes rather than restarts. The name is checked
before anything is written, and the client is told what is already
there so it can ask whether to replace it.

The staging area lives under the storage root in a dot-directory, so it
is skipped by the file browser, by the indexer, and by the backup.
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

STAGING_DIR_NAME = ".uploads"
META_NAME = "meta.json"

# A staging area left behind by a client that gave up. Anything older
# than this is removed the next time an upload starts.
STALE_UPLOAD_SECONDS = 24 * 3600

# The default piece size. Small enough that a dropped connection loses
# little, large enough that the request overhead does not dominate.
DEFAULT_CHUNK_SIZE = 4 * 1024 * 1024


class UploadError(Exception):
    """An upload could not go ahead."""


class UploadConflict(UploadError):
    """The name is already taken.

    Carries what is already there so the client can ask the user what
    to do about it.
    """

    def __init__(self, message: str, existing: dict):
        super().__init__(message)
        self.existing = existing


def staging_root(storage_root: Path) -> Path:
    return Path(storage_root) / STAGING_DIR_NAME


class UploadManager:
    """Prepares, receives and finishes uploads."""

    def __init__(self, storage_root: Path):
        self.storage_root = Path(storage_root)
        self.root = staging_root(self.storage_root)

    # ---- internals ----

    def _dir(self, upload_id: str) -> Path:
        # The id is generated here, but it comes back from the client
        # too, so it has to be checked rather than trusted.
        if not upload_id or not all(c in "0123456789abcdef" for c in upload_id) or len(upload_id) != 32:
            raise UploadError("无效的上传编号")
        return self.root / upload_id

    def _meta_path(self, upload_id: str) -> Path:
        return self._dir(upload_id) / META_NAME

    def _read_meta(self, upload_id: str) -> dict:
        try:
            return json.loads(self._meta_path(upload_id).read_text(encoding="utf-8"))
        except (OSError, ValueError):
            raise UploadError("上传不存在或已过期")

    def _write_meta(self, upload_id: str, meta: dict) -> None:
        self._meta_path(upload_id).write_text(
            json.dumps(meta, ensure_ascii=False, indent=2), encoding="utf-8"
        )

    def _sweep_stale(self) -> None:
        """Remove staging areas nobody came back for."""
        if not self.root.exists():
            return
        cutoff = time.time() - STALE_UPLOAD_SECONDS
        for entry in self.root.iterdir():
            try:
                if entry.stat().st_mtime < cutoff:
                    shutil.rmtree(entry, ignore_errors=True)
            except OSError:
                continue

    # ---- operations ----

    def init(self, target_dir: Path, name: str, size: int, total_chunks: int) -> dict:
        """Start an upload and report what is already at the destination.

        Nothing is written to the destination yet. The caller asks the
        user what to do about a name that is taken, and comes back with
        the answer.
        """
        self._sweep_stale()
        self.root.mkdir(parents=True, exist_ok=True)

        target_dir = Path(target_dir)
        dest = target_dir / name
        existing = None
        if dest.exists():
            st = dest.stat()
            existing = {
                "name": name,
                "size": st.st_size,
                "is_dir": dest.is_dir(),
                "modified": time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(st.st_mtime)),
            }

        upload_id = uuid.uuid4().hex
        meta = {
            "id": upload_id,
            "name": name,
            "target_dir": str(target_dir),
            "size": int(size),
            "total_chunks": int(total_chunks),
            "created_at": int(time.time()),
        }
        (self._dir(upload_id)).mkdir(parents=True, exist_ok=True)
        self._write_meta(upload_id, meta)
        return {"upload_id": upload_id, "existing": existing, **meta}

    def chunk_path(self, upload_id: str, index: int) -> Path:
        meta = self._read_meta(upload_id)
        if index < 0 or index >= meta["total_chunks"]:
            raise UploadError("分片序号超出范围")
        return self._dir(upload_id) / f"{index:06d}.part"

    def write_chunk(self, upload_id: str, index: int, data: bytes) -> dict:
        """Store one piece. Writing the same piece twice is fine."""
        path = self.chunk_path(upload_id, index)
        # Written to a temporary name and moved into place, so a chunk
        # that is half-written is not mistaken for one that arrived.
        tmp = path.with_suffix(".part.tmp")
        with open(tmp, "wb") as out:
            out.write(data)
        tmp.replace(path)
        return {"received": self.received(upload_id), "index": index}

    def received(self, upload_id: str) -> list[int]:
        """Which pieces have arrived."""
        meta = self._read_meta(upload_id)
        d = self._dir(upload_id)
        out = []
        for i in range(meta["total_chunks"]):
            if (d / f"{i:06d}.part").exists():
                out.append(i)
        return out

    def status(self, upload_id: str) -> dict:
        meta = self._read_meta(upload_id)
        received = self.received(upload_id)
        return {
            **meta,
            "received": received,
            "received_bytes": sum(
                (self._dir(upload_id) / f"{i:06d}.part").stat().st_size for i in received
            ),
        }

    def complete(self, upload_id: str, overwrite: str = "never") -> dict:
        """Put the uploaded file in place.

        `overwrite` is what to do about a name that is already taken:
        "never" refuses, "replace" replaces it, "rename" writes the
        file under a name that is free.
        """
        meta = self._read_meta(upload_id)
        target_dir = Path(meta["target_dir"])
        name = meta["name"]
        total = meta["total_chunks"]

        received = set(self.received(upload_id))
        missing = [i for i in range(total) if i not in received]
        if missing:
            raise UploadError(f"还有 {len(missing)} 个分片没有上传")

        dest = target_dir / name
        if dest.exists():
            if overwrite == "never":
                st = dest.stat()
                raise UploadConflict(
                    f"{name} 已经存在",
                    {
                        "name": name,
                        "size": st.st_size,
                        "is_dir": dest.is_dir(),
                        "modified": time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(st.st_mtime)),
                    },
                )
            if overwrite == "rename":
                dest = _free_name(target_dir, name)

        # The pieces are joined in a temporary file beside the target and
        # moved into place, so the destination never holds a partial
        # file -- not even if the process dies mid-join.
        tmp = target_dir / f".{name}.uploading"
        total_bytes = 0
        try:
            with open(tmp, "wb") as out:
                for i in range(total):
                    part = self._dir(upload_id) / f"{i:06d}.part"
                    with open(part, "rb") as src:
                        shutil.copyfileobj(src, out, length=1024 * 1024)
                    total_bytes += part.stat().st_size
            if meta["size"] and total_bytes != meta["size"]:
                raise UploadError(
                    f"文件大小不符：预期 {meta['size']} 字节，实际 {total_bytes} 字节"
                )
            tmp.replace(dest)
        except Exception:
            tmp.unlink(missing_ok=True)
            raise

        shutil.rmtree(self._dir(upload_id), ignore_errors=True)
        log.info("uploaded %s (%d bytes)", dest, total_bytes)
        return {
            "success": True,
            "name": dest.name,
            "path": str(dest),
            "size": total_bytes,
            "replaced": overwrite == "replace",
        }

    def abandon(self, upload_id: str) -> dict:
        """Throw away an upload that will not be finished."""
        shutil.rmtree(self._dir(upload_id), ignore_errors=True)
        return {"success": True}


def _free_name(target_dir: Path, name: str) -> Path:
    """A name close to the original that nothing is using."""
    stem = Path(name).stem
    suffix = Path(name).suffix
    n = 1
    while True:
        candidate = target_dir / f"{stem}({n}){suffix}"
        if not candidate.exists():
            return candidate
        n += 1
