"""Who gets the big cores.

The model, the picture search and the picture index all want the same big
cores. Running two of them at once does not split the cores politely: the
model that is mid-request slows down and the other one's tail latency
explodes.

So the pool has one writer at a time and any number of readers:

  * the model holds it while it generates -- as a *reader*, because the
    llama-server on this box was started with --parallel 2 and can serve
    two conversations at once. The server does the queueing; the pool
    does not need to be stricter than the server. Two chats therefore
    no longer lock each other out, and the second one waits inside the
    server instead of failing after a minute;
  * a picture search takes it as a *writer*: it needs the whole box for
    the moment it encodes the query. A user waiting for a picture gets
    an answer now or a clear refusal -- never a queue;
  * the picture index takes it as a writer too, one picture at a time, and
    gives it back between pictures, so a search waits for one picture
    rather than for a whole build.

The model has priority: a reader that arrives while a writer is waiting
joins the readers that are already in, rather than making the writer
wait for a turn it may not get for a long time.

The text index is not in this pool: bge runs on the little cores, which
the model does not use.

The lock is a file lock (flock) rather than a threading.Lock because the work
itself lives in another process: the backend holds the lock and then talks to
the llama-server that owns the cores. flock is also what makes the lock
survive a backend restart -- the kernel drops it when the process dies, so a
crash mid-index does not leave the pool locked forever.

Readers take the file lock in shared mode and writers in exclusive mode, so
a second backend on the same machine is held off just as firmly.
"""

import errno
import fcntl
import os
import threading
import time
from contextlib import contextmanager
from pathlib import Path
from typing import Iterator, Optional


# How long to wait between attempts while polling for the lock. Short enough
# that a free pool is picked up promptly, long enough not to spin.
POLL_INTERVAL = 0.05

# How long the file lock may be polled while the in-process lock is held.
# The in-process lock must not be held for long: a thread that wants the
# pool with a short deadline (a picture search) would otherwise be stuck
# behind a thread that is waiting for another process.
FLOCK_POLL_WINDOW = 0.5


class CpuPoolBusy(Exception):
    """The pool could not be taken within the deadline."""


class CpuPool:
    """A lock over a set of cores: many readers, one writer.

    `path` is the lock file. It is created on first use; the directory has to
    exist.

    A reader may hold the pool together with other readers. A writer holds
    it alone: no reader and no other writer.
    """

    def __init__(self, path: Path):
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.name = self.path.stem
        # In-process state. A Condition rather than a Lock: the readers
        # have to be able to be in at the same time, and the release has
        # to work from a different thread than the acquire, because a
        # streaming response is consumed by a thread pool that may hand
        # the next chunk to another worker.
        self._cond = threading.Condition()
        # How many readers are in, and whether a writer is.
        self._readers = 0
        self._writer = False
        # How many writers are waiting. A reader that arrives while no
        # reader is in waits for them: otherwise a stream of readers
        # would keep a writer out for ever.
        self._writers_waiting = 0
        self._fd: Optional[int] = None
        # Whether the file lock is held in shared or exclusive mode. The
        # mode always matches the in-process state: a writer only enters
        # when no reader is in, so the file lock never has to be upgraded
        # while it is held (which flock would refuse anyway).
        self._fd_mode: Optional[str] = None

    # -- internals ---------------------------------------------------------

    def _open(self) -> int:
        if self._fd is None:
            self._fd = os.open(self.path, os.O_RDWR | os.O_CREAT, 0o600)
        return self._fd

    def _try_flock(self, mode: str) -> bool:
        """Take the file lock, or say that somebody else has it."""
        fd = self._open()
        op = fcntl.LOCK_SH if mode == "shared" else fcntl.LOCK_EX
        try:
            fcntl.flock(fd, op | fcntl.LOCK_NB)
            self._fd_mode = mode
            return True
        except OSError as e:
            if e.errno in (errno.EACCES, errno.EAGAIN):
                return False
            raise

    def _release_flock(self) -> None:
        if self._fd is not None:
            try:
                fcntl.flock(self._fd, fcntl.LOCK_UN)
            except OSError:
                pass
            self._fd_mode = None

    def _poll_flock(self, mode: str, deadline: Optional[float]) -> bool:
        """Try for the file lock for a short while.

        The wait is bounded so that the in-process lock is not held for
        long: another thread with a short deadline is waiting on the same
        condition variable.
        """
        window = time.monotonic() + FLOCK_POLL_WINDOW
        while True:
            if self._try_flock(mode):
                return True
            if deadline is not None and time.monotonic() >= deadline:
                return False
            if time.monotonic() >= window:
                return False
            time.sleep(POLL_INTERVAL)

    def _may_enter(self, shared: bool) -> bool:
        """Whether this kind of holder may come in right now."""
        if self._writer:
            return False
        if shared:
            # A reader may join readers that are already in. If nobody is
            # in and a writer is waiting, the writer goes first.
            return self._readers > 0 or self._writers_waiting == 0
        # A writer waits for the readers to leave.
        return self._readers == 0

    # -- public API --------------------------------------------------------

    def acquire(self, timeout: Optional[float] = None, shared: bool = False) -> bool:
        """Take the pool.

        `timeout` is a number of seconds to wait, `None` to wait forever,
        `0` to try once. Returns True when the pool was taken.

        `shared` asks for a reader's lease: other readers may hold it at
        the same time, but no writer may.

        The pool is not reentrant. Taking it twice from one thread waits
        for the first hold to end, and with a deadline it fails instead of
        hanging. That is deliberate: a nested hold would mean the cores
        were promised to two things at once.
        """
        deadline = None if timeout is None else time.monotonic() + timeout
        while True:
            with self._cond:
                if not shared:
                    self._writers_waiting += 1
                try:
                    while not self._may_enter(shared):
                        if deadline is not None and time.monotonic() >= deadline:
                            return False
                        remaining = None if deadline is None else deadline - time.monotonic()
                        self._cond.wait(POLL_INTERVAL if remaining is None
                                       else max(0.001, min(POLL_INTERVAL, remaining)))
                    # The in-process turn is ours. Now the file lock.
                    if shared:
                        if self._readers == 0:
                            # The first reader speaks for the process.
                            if self._fd_mode != "shared":
                                self._release_flock()
                                if not self._poll_flock("shared", deadline):
                                    # Another process has the cores. Give
                                    # the in-process lock back and try
                                    # again, so that a thread with a short
                                    # deadline is not stuck behind us.
                                    if deadline is not None and time.monotonic() >= deadline:
                                        return False
                                    self._cond.wait(POLL_INTERVAL)
                                    continue
                        self._readers += 1
                    else:
                        self._writer = True
                        if self._fd_mode != "exclusive":
                            # No reader is in, so the shared lock (if any)
                            # has already been given back.
                            self._release_flock()
                            if not self._poll_flock("exclusive", deadline):
                                self._writer = False
                                self._cond.notify_all()
                                if deadline is not None and time.monotonic() >= deadline:
                                    return False
                                self._cond.wait(POLL_INTERVAL)
                                continue
                finally:
                    if not shared:
                        self._writers_waiting -= 1
                        self._cond.notify_all()
                return True

    def release(self) -> None:
        """Give the pool back.

        Whether this was a reader or a writer is read off the state: a
        writer and a reader are never in at the same time, so there is no
        ambiguity -- and the release may come from another thread, so it
        cannot be read off the thread either.
        """
        with self._cond:
            if self._writer:
                self._writer = False
                self._release_flock()
            else:
                self._readers = max(0, self._readers - 1)
                if self._readers == 0:
                    self._release_flock()
            self._cond.notify_all()

    @contextmanager
    def hold(self, timeout: Optional[float] = None, shared: bool = False) -> Iterator[None]:
        """Hold the pool, waiting up to `timeout` seconds.

        Raises CpuPoolBusy when the deadline passes. Used by the indexer,
        which has to wait: the alternative is dropping work on the floor.
        """
        if not self.acquire(timeout, shared=shared):
            raise CpuPoolBusy(f"CPU 池在 {timeout} 秒内没有被释放")
        try:
            yield
        finally:
            self.release()

    @contextmanager
    def try_hold(self, timeout: float = 0.0, shared: bool = False) -> Iterator[bool]:
        """Hold the pool if it is free, otherwise yield False.

        Never raises and never waits longer than `timeout`. Used by the
        text-to-image query, where waiting is worse than failing.
        """
        got = self.acquire(timeout, shared=shared)
        try:
            yield got
        finally:
            if got:
                self.release()

    def is_free(self) -> bool:
        """Whether the pool could be taken right now.

        Does not take the lock: the answer is a snapshot, and the pool may be
        busy by the time the caller acts on it.

        The probe uses its own file description. flock belongs to the open
        file description, so re-using the lock's own fd would not see that
        fd's lock as busy -- and releasing it afterwards would drop a lock
        this process believes it holds.
        """
        fd = os.open(self.path, os.O_RDWR | os.O_CREAT, 0o600)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            fcntl.flock(fd, fcntl.LOCK_UN)
            return True
        except OSError:
            return False
        finally:
            os.close(fd)


_pool = None
_pool_lock = threading.Lock()

# The pools this installation knows about. The names are the file names, so
# a stale lock file from a previous run is picked up rather than ignored.
BIG_POOL = "big-cores"


def get_pool(name: str) -> CpuPool:
    """The pool with the given name, created on first use."""
    global _pool
    with _pool_lock:
        if _pool is None:
            _pool = {}
        if name not in _pool:
            from .deps import DATA_DIR
            _pool[name] = CpuPool(DATA_DIR / "locks" / f"{name}.lock")
        return _pool[name]


def get_big_pool() -> CpuPool:
    """The pool of cores the model, the picture search and the picture index
    all compete for.

    The model is running there, and it is the one thing that must never be
    slowed down, so the other two wait for it -- a multimodal search gets a
    few seconds and then gives up rather than queueing behind a build.
    """
    return get_pool(BIG_POOL)
