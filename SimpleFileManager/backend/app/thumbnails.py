"""Small pictures of big ones.

A NAS is mostly photographs, and a file browser that shows the same
icon for every one of them is no use for finding anything. These are
the little pictures -- a few hundred pixels across, made once and
kept, so scrolling a folder of photographs does not make the machine
work hard.

The pictures are made from the file itself, never from anything the
file says about itself, so a picture with an image extension that is
not an image simply does not get one.

Pillow is the tool for it. When it is not installed the browser is
left with the icon it would have had anyway.
"""

from __future__ import annotations

import hashlib
import io
import logging
import os
import threading
from pathlib import Path
from typing import Optional

log = logging.getLogger(__name__)

THUMBNAIL_DIR_NAME = ".thumbnails"

# The size of the little picture. Big enough to recognise, small
# enough that a folder of them costs nothing to scroll past.
DEFAULT_SIZE = 256

# Files bigger than this are not thumbnailed. A very large image is
# usually a very large thing that is not a photograph.
MAX_SOURCE_BYTES = 64 * 1024 * 1024

# The formats a browser can show as a picture. Anything else is left
# to the icon.
IMAGE_EXTENSIONS = {
    ".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".tif", ".tiff",
}


def _pillow():
    """Pillow, if it is here."""
    try:
        from PIL import Image

        return Image
    except Exception:
        return None


class ThumbnailStore:
    """The little pictures, and where they are kept."""

    def __init__(self, storage_root: Path, cache_root: Optional[Path] = None):
        self.storage_root = Path(storage_root)
        if cache_root is None:
            from .deps import DATA_DIR

            cache_root = DATA_DIR / THUMBNAIL_DIR_NAME
        self.cache_root = Path(cache_root)
        # One lock per picture being made, so a page full of pictures
        # does not have to wait for each other.
        self._locks: dict[str, threading.Lock] = {}
        self._locks_lock = threading.Lock()

    def cache_path(self, target: Path, size: int) -> Path:
        """Where the little picture for a file is kept.

        The name carries the file's size and the moment it was last
        changed, so a picture that has been replaced is not served
        from the cache in place of the new one.
        """
        try:
            stat = target.stat()
            stamp = f"{stat.st_size}-{int(stat.st_mtime)}-{size}"
        except OSError:
            stamp = "missing"
        key = hashlib.sha1(f"{target}:{stamp}".encode()).hexdigest()
        return self.cache_root / f"{key}.jpg"

    def get(self, target: Path, size: int = DEFAULT_SIZE) -> Optional[bytes]:
        """The little picture, or None if there is not one to be had."""
        if target.suffix.lower() not in IMAGE_EXTENSIONS:
            return None
        try:
            if target.stat().st_size > MAX_SOURCE_BYTES:
                return None
        except OSError:
            return None

        cached = self.cache_path(target, size)
        try:
            return cached.read_bytes()
        except OSError:
            pass

        Image = _pillow()
        if Image is None:
            return None

        # One lock per picture, not one for the whole cache. Decoding a
        # photograph takes about a tenth of a second, and a page of
        # pictures asks for two dozen of them at once: a single lock
        # would make them all queue up and fill the thread pool.
        key = str(cached)
        with self._lock_for(key):
            # Another thread may have made it while we waited.
            try:
                return cached.read_bytes()
            except OSError:
                pass
            try:
                data = self._make(target, size, Image)
            except Exception as e:
                log.debug("no thumbnail for %s: %s", target, e)
                return None
            if data is None:
                return None
            try:
                self.cache_root.mkdir(parents=True, exist_ok=True)
                # Written under a name of its own and then moved into
                # place, so a half-written picture is never served. The
                # thread is in the name so two threads working on the
                # same picture do not write the same temporary file.
                tmp = cached.with_suffix(f".tmp{os.getpid()}-{threading.get_ident()}")
                tmp.write_bytes(data)
                tmp.replace(cached)
            except OSError:
                pass
            return data

    def _lock_for(self, key: str) -> threading.Lock:
        """The lock for one picture."""
        with self._locks_lock:
            lock = self._locks.get(key)
            if lock is None:
                lock = threading.Lock()
                self._locks[key] = lock
            return lock

    def _make(self, target: Path, size: int, Image) -> Optional[bytes]:
        """The little picture, made from the file."""
        try:
            with Image.open(target) as img:
                # Some images come in with a mode that cannot be saved
                # as a JPEG (a palette with transparency, say).
                if img.mode not in ("RGB", "L"):
                    img = img.convert("RGBA")
                img.thumbnail((size, size), Image.LANCZOS)
                buffer = io.BytesIO()
                img.convert("RGB").save(buffer, format="JPEG", quality=80)
                return buffer.getvalue()
        except Exception:
            return None

    def clear(self) -> int:
        """Throw the little pictures away. They are made again on demand."""
        removed = 0
        if not self.cache_root.exists():
            return 0
        for entry in self.cache_root.iterdir():
            try:
                entry.unlink()
                removed += 1
            except OSError:
                continue
        return removed
