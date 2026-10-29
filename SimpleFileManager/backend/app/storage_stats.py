"""How much room is left, and how big is this directory.

Two numbers the file browser used not to show: the free space on the
disk, and the size of a directory. Both are easy to want and awkward
to get, because computing the second one means walking the whole
subtree -- which is exactly the kind of work that should not happen
on every page load.

So directory sizes are cached for a short while. The answer is a
rounding error for anything the user is about to act on, and the cache
keeps a listing of a large library from turning into a full disk scan.
"""

from __future__ import annotations

import os
import shutil
import threading
import time
from pathlib import Path

# How long a computed directory size is reused. Long enough that
# browsing around does not re-walk the same trees, short enough that a
# file that has just been added shows up.
SIZE_CACHE_TTL = 30.0


class _SizeCache:
    """A small time-to-live cache of directory sizes."""

    def __init__(self):
        self._lock = threading.Lock()
        self._entries: dict[str, tuple[float, int]] = {}

    def get(self, key: str) -> int | None:
        with self._lock:
            entry = self._entries.get(key)
        if entry is None:
            return None
        stamp, size = entry
        if time.monotonic() - stamp > SIZE_CACHE_TTL:
            return None
        return size

    def put(self, key: str, size: int) -> None:
        with self._lock:
            self._entries[key] = (time.monotonic(), size)
            # The cache is only ever a convenience, so it is allowed to
            # forget rather than grow without bound.
            if len(self._entries) > 2000:
                cutoff = time.monotonic() - SIZE_CACHE_TTL * 4
                self._entries = {
                    k: v for k, v in self._entries.items() if v[0] >= cutoff
                }

    def invalidate(self, key: str) -> None:
        with self._lock:
            self._entries.pop(key, None)


_sizes = _SizeCache()


def dir_size(path: Path) -> int:
    """Total bytes under a directory, or the size of a file.

    A file's size comes from its stat and is free. A directory has to
    be walked, so the answer is cached.
    """
    path = Path(path)
    if path.is_file():
        try:
            return path.stat().st_size
        except OSError:
            return 0
    key = str(path)
    cached = _sizes.get(key)
    if cached is not None:
        return cached
    total = 0
    for root, dirs, files in os.walk(path, onerror=lambda e: None):
        for name in files:
            try:
                total += (Path(root) / name).stat().st_size
            except OSError:
                continue
    _sizes.put(key, total)
    return total


def invalidate(path: Path) -> None:
    """Forget the cached size of a path and of its parents.

    Called when something under a directory changes, so the number in
    the browser is not a lie for the next half minute.
    """
    path = Path(path)
    _sizes.invalidate(str(path))
    parent = path.parent
    for _ in range(8):
        _sizes.invalidate(str(parent))
        if parent.parent == parent:
            break
        parent = parent.parent


def disk_info(path: Path) -> dict:
    """Total, used and free space of the disk holding ``path``."""
    try:
        usage = shutil.disk_usage(path)
    except OSError as e:
        return {"ok": False, "detail": str(e)[:120]}
    return {
        "ok": True,
        "total": usage.total,
        "used": usage.used,
        "free": usage.free,
        "used_pct": round(usage.used / usage.total * 100, 1) if usage.total else 0,
    }
