"""Noticing when files change, without looking for them.

The index used to walk the whole storage every five minutes to find
out what had changed. That works, and it is cheap -- an idle scan of a
few hundred files takes ten milliseconds -- but it means a file you
have just put there is not searchable for a couple of minutes, and on
a machine with a hundred thousand files in it, the walk is not free
either.

Linux will tell us when a file changes, if we ask. The kernel's
inotify says "this directory changed" for every change, and it costs
nothing when nothing is happening. Three things have to be got right:

* inotify watches one directory, not a tree. A new directory needs a
  watch of its own, and a directory that is renamed keeps its watch
  (which now points at the old name). So the set of watches is kept
  in step with the tree, and the tree is walked to find out which
  directories there are.

* The queue of events is finite. If more changes arrive than have
  been read, the kernel drops the details and says only that the
  queue overflowed. The answer to an overflow is a full rescan --
  there is no way to know what was missed.

* inotify only works on local filesystems. On a network mount (NFS,
  SMB) it does not fail -- it simply never says anything, which is
  the worst kind of failure: the service would look healthy and the
  index would quietly go stale. So the filesystem is asked what it is
  before the watcher is started, and if it is not one that can talk,
  the polling that was always there carries on alone.

Everything here is best-effort. If any of it goes wrong the index
still gets built by the periodic scan, just later.
"""

from __future__ import annotations

import ctypes
import ctypes.util
import errno
import logging
import os
import select
import struct
import threading
import time
from pathlib import Path
from typing import Callable, Optional

log = logging.getLogger(__name__)

# The events worth hearing about. IN_CLOSE_WRITE is the one that
# matters most: a file is only interesting once it has been written
# and closed, not while it is still being written.
IN_ACCESS = 0x00000001
IN_MODIFY = 0x00000002
IN_ATTRIB = 0x00000004
IN_CLOSE_WRITE = 0x00000008
IN_MOVED_FROM = 0x00000040
IN_MOVED_TO = 0x00000080
IN_CREATE = 0x00000100
IN_DELETE = 0x00000200
IN_DELETE_SELF = 0x00000400
IN_MOVE_SELF = 0x00000800
IN_UNMOUNT = 0x00002000
IN_Q_OVERFLOW = 0x00004000
IN_IGNORED = 0x00008000
IN_ISDIR = 0x40000000

WATCH_MASK = (
    IN_MODIFY | IN_ATTRIB | IN_CLOSE_WRITE
    | IN_CREATE | IN_DELETE
    | IN_MOVED_FROM | IN_MOVED_TO
    | IN_DELETE_SELF | IN_MOVE_SELF
)

# The filesystem types that can actually say what they are doing.
# Everything else -- NFS, SMB, CIFS, FUSE -- cannot, and asking it
# would only produce silence.
LOCAL_FILESYSTEMS = {
    "ext2", "ext3", "ext4", "xfs", "btrfs", "zfs", "jfs", "reiserfs",
    "f2fs", "overlay", "overlayfs", "tmpfs", "ramfs", "aufs", "nilfs2",
    "bcachefs", "erofs", "squashfs",
}

# The kernel's own limit on how many watches one user may hold.
def _max_user_watches() -> int:
    try:
        with open("/proc/sys/fs/inotify/max_user_watches") as f:
            return int(f.read().strip())
    except (OSError, ValueError):
        return 8192


class InotifyUnavailable(Exception):
    """This machine cannot tell us about file changes."""


class StorageWatcher:
    """Watches the storage root and says when something changed.

    `on_change` is called with the paths that changed, relative to the
    storage root, and the subset of them that are known to be complete
    -- the kernel saw the writer close the file, or the file was moved
    in whole. The complete ones can be indexed at once; the rest have
    to wait out the debounce, because they may still be being written.

    It is called from the watcher's own thread, so it should not do
    much.
    """

    def __init__(
        self,
        storage_root: Path,
        on_change: Callable[[set[str], set[str]], None],
        on_overflow: Optional[Callable[[], None]] = None,
    ):
        self.root = Path(storage_root)
        self.on_change = on_change
        self.on_overflow = on_overflow
        self._fd = -1
        self._thread: Optional[threading.Thread] = None
        self._stop = threading.Event()
        self._lock = threading.Lock()
        # watch descriptor -> (path relative to the root, inode)
        self._watches: dict[int, tuple[str, int]] = {}
        self._events = 0
        self._overflows = 0
        self._error: Optional[str] = None
        self._started_at = 0.0
        self._degraded = False
        self._dir_cache: list[Path] | None = None

    # ---- whether this can work here at all ----

    @staticmethod
    def filesystem_of(path: Path) -> str:
        """The kind of filesystem a path is on.

        Read from /proc, which is where the kernel says what it has
        mounted. A path that is not on any mount of its own is on the
        root filesystem, which is what "/" is.
        """
        target = str(Path(path).resolve())
        best = ""
        best_len = -1
        try:
            with open("/proc/self/mountinfo") as f:
                for line in f:
                    # ... - fstype source super_options
                    parts = line.split(" - ")
                    if len(parts) < 2:
                        continue
                    mount_point = parts[0].split()[4]
                    fstype = parts[1].split()[0]
                    if (target == mount_point or target.startswith(mount_point.rstrip("/") + "/")) \
                            and len(mount_point) > best_len:
                        best = fstype
                        best_len = len(mount_point)
        except OSError:
            return ""
        return best or "unknown"

    @classmethod
    def can_watch(cls, path: Path) -> tuple[bool, str]:
        """Whether files on this path can be watched, and why not."""
        fstype = cls.filesystem_of(path)
        if fstype in LOCAL_FILESYSTEMS:
            return True, fstype
        return False, fstype

    # ---- the watcher itself ----

    def start(self) -> bool:
        """Start watching. Returns False if it cannot be done here."""
        if self._thread and self._thread.is_alive():
            return True
        ok, fstype = self.can_watch(self.root)
        if not ok:
            self._error = f"{fstype} 不会报告文件变化，改用定时扫描"
            log.info("not watching %s: %s", self.root, self._error)
            return False

        libc = ctypes.CDLL(ctypes.util.find_library("c") or "libc.so.6", use_errno=True)
        # IN_NONBLOCK so the read never sits waiting; the select below
        # is what waits.
        fd = libc.inotify_init1(os.O_NONBLOCK | os.O_CLOEXEC)
        if fd < 0:
            self._error = f"inotify 初始化失败: {os.strerror(ctypes.get_errno())}"
            log.warning(self._error)
            return False
        self._libc = libc
        self._fd = fd

        try:
            self._sync_watches()
        except Exception as e:
            self._error = f"无法监视目录: {e}"
            log.warning(self._error)
            self._close()
            return False

        self._stop.clear()
        self._started_at = time.time()
        self._thread = threading.Thread(target=self._run, name="storage-watcher", daemon=True)
        self._thread.start()
        log.info("watching %s (%s)", self.root, fstype)
        return True

    def stop(self):
        self._stop.set()
        thread = self._thread
        if thread and thread.is_alive():
            thread.join(timeout=3)
        self._close()

    def _close(self):
        if self._fd >= 0:
            try:
                os.close(self._fd)
            except OSError:
                pass
            self._fd = -1
        self._watches.clear()

    # ---- keeping the set of watches in step with the tree ----

    @staticmethod
    def _relative(directory: Path, root: Path) -> str:
        """A directory's path relative to the root; "" for the root itself."""
        if directory == root:
            return ""
        try:
            return str(directory.relative_to(root))
        except ValueError:
            return ""

    @staticmethod
    def _join(directory: str, name: str) -> str:
        return os.path.join(directory, name) if directory else name

    def _directories(self, force: bool = False) -> list[Path]:
        """Every directory in the tree, the root first.

        Result is cached because every event triggers a sync, and a full
        walk on each one stalls the watcher thread on large trees."""
        if not force and self._dir_cache is not None:
            return self._dir_cache
        found = [self.root]
        try:
            for base, dirs, _files in os.walk(self.root):
                base_path = Path(base)
                for name in dirs:
                    if name.startswith("."):
                        continue
                    found.append(base_path / name)
        except OSError:
            pass
        self._dir_cache = found
        return found

    def _sync_watches(self, mark_ready: bool = False) -> set[str]:
        """Make the set of watches match the tree.

        A directory that has been renamed keeps its watch -- the
        kernel watches the inode, not the name -- so a watch is
        matched up by inode rather than by path, and only renamed
        paths are rewritten.

        A directory that is newly watched may already hold files that
        were written before the watch existed -- `mv some/dir .` or a
        copy that outran the watcher. Those files produced no events
        and would otherwise wait out the debounce, so when `mark_ready`
        is set they are listed as complete and returned. It is only
        asked for during event handling, never for the initial walk:
        at startup the debounce is the right thing to trust.
        """
        by_inode = {info[1]: wd for wd, info in self._watches.items()}
        seen = set()
        new_dirs: list[Path] = []
        limit = _max_user_watches()

        for directory in self._directories():
            try:
                inode = directory.stat().st_ino
            except OSError:
                continue
            wd = by_inode.get(inode)
            if wd is not None and wd in self._watches:
                relative = self._relative(directory, self.root)
                if self._watches[wd][0] != relative:
                    self._watches[wd] = (relative, inode)
                seen.add(wd)
                continue
            if len(self._watches) >= limit:
                if not self._degraded:
                    self._degraded = True
                    log.warning(
                        "too many directories to watch (%d); falling back to polling for the rest",
                        limit,
                    )
                continue
            wd = self._add_watch(directory)
            if wd is not None:
                seen.add(wd)
                new_dirs.append(directory)

        for wd in list(self._watches):
            if wd not in seen:
                self._watches.pop(wd, None)

        if new_dirs:
            self._dir_cache = None

        ready: set[str] = set()
        if mark_ready:
            for directory in new_dirs:
                base = self._relative(directory, self.root)
                try:
                    with os.scandir(directory) as entries:
                        for entry in entries:
                            if entry.name.startswith("."):
                                continue
                            try:
                                if not entry.is_file(follow_symlinks=False):
                                    continue
                            except OSError:
                                continue
                            ready.add(self._join(base, entry.name))
                except OSError:
                    pass
        return ready

    def _add_watch(self, directory: Path) -> Optional[int]:
        """Watch one directory."""
        try:
            relative = self._relative(directory, self.root)
            inode = directory.stat().st_ino
        except (OSError, ValueError):
            return None
        wd = self._libc.inotify_add_watch(
            self._fd, str(directory).encode(), WATCH_MASK
        )
        if wd < 0:
            err = ctypes.get_errno()
            if err == errno.ENOSPC:
                self._degraded = True
                log.warning("ran out of inotify watches; some directories are not watched")
            else:
                log.debug("cannot watch %s: %s", directory, os.strerror(err))
            return None
        self._watches[wd] = (relative, inode)
        return wd

    # ---- reading the events ----

    def _run(self):
        while not self._stop.is_set():
            try:
                ready, _, _ = select.select([self._fd], [], [], 1.0)
            except (OSError, ValueError):
                break
            if not ready:
                continue
            try:
                data = os.read(self._fd, 64 * 1024)
            except BlockingIOError:
                continue
            except OSError:
                break
            if not data:
                break
            try:
                self._handle(data)
            except Exception:
                log.exception("failed to read a file change")

    def _handle(self, data: bytes):
        """Read a batch of events."""
        changed: set[str] = set()
        ready: set[str] = set()
        directories_touched = False
        overflowed = False

        offset = 0
        while offset < len(data):
            wd, mask, _cookie, length = struct.unpack_from("iIII", data, offset)
            offset += 16
            name = data[offset:offset + length].split(b"\0")[0].decode(errors="replace")
            offset += length

            self._events += 1

            if mask & IN_Q_OVERFLOW:
                # The kernel dropped the details. There is no way to
                # know what was missed, so the only safe answer is to
                # look at everything again.
                overflowed = True
                continue

            if mask & IN_IGNORED:
                # The watch was removed by the kernel -- the directory
                # went away.
                self._watches.pop(wd, None)
                continue

            info = self._watches.get(wd)
            if info is None:
                continue
            directory = info[0]

            if not name:
                continue
            if mask & IN_ISDIR:
                # A directory is not something to index; what matters
                # is that it needs a watch of its own. Only directory
                # changes need the tree walked -- doing it for every
                # file created would be a full walk per upload.
                if mask & (IN_CREATE | IN_DELETE | IN_MOVED_FROM | IN_MOVED_TO):
                    directories_touched = True
                continue

            relative = self._join(directory, name)

            if mask & (IN_DELETE | IN_MOVED_FROM):
                # The path the entry left behind. Whether it was really
                # deleted or moved somewhere still-watched is for the
                # scan to work out from what is actually on disk.
                changed.add(relative)

            if mask & (IN_MODIFY | IN_ATTRIB | IN_CREATE):
                changed.add(relative)

            if mask & (IN_CLOSE_WRITE | IN_MOVED_TO):
                # The writer closed it, or it arrived whole. Safe to
                # index now rather than waiting out the debounce.
                changed.add(relative)
                ready.add(relative)

        if overflowed:
            self._overflows += 1
            log.warning("the file change queue overflowed; a full rescan is needed")
            if self.on_overflow:
                try:
                    self.on_overflow()
                except Exception:
                    log.exception("the overflow handler failed")
            return

        if directories_touched:
            # The tree itself changed: a directory was added, removed
            # or renamed, and the watches have to follow. Anything
            # already sitting in a directory that has only just been
            # watched is treated as complete.
            ready |= self._sync_watches(mark_ready=True)

        if changed or ready:
            # `ready` can be non-empty on its own: a directory that has
            # just been watched may already hold files whose creation
            # events arrived before there was a watch to see them.
            try:
                self.on_change(changed, ready)
            except Exception:
                log.exception("the change handler failed")

    # ---- what the service can be told about ----

    def status(self) -> dict:
        return {
            "watching": bool(self._thread and self._thread.is_alive()),
            "filesystem": self.filesystem_of(self.root),
            "directories": len(self._watches),
            "events": self._events,
            "overflows": self._overflows,
            "degraded": self._degraded,
            "error": self._error,
            "since": int(self._started_at) if self._started_at else None,
        }
