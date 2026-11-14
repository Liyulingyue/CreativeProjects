"""One owner at a time for the big cores.

The model, the picture search and the picture index all want the same big
cores. Running two of them at once does not split the cores politely: the
model that is mid-request slows down and the other one's tail latency
explodes.

So the pool has exactly one owner at a time:

  * the model holds it while it generates -- including for the whole
    stream, and including while the request is on its way to the server,
    because the server starts computing the moment it receives it;
  * a picture search takes it too, but only if it happens to be free. A
    user waiting for a picture gets an answer now or a clear refusal --
    never a queue;
  * the picture index takes it one picture at a time and gives it back
    between pictures, so a search waits for one picture rather than for a
    whole build.

The text index is not in this pool: bge runs on the little cores, which
the model does not use.

The lock is a file lock (flock) rather than a threading.Lock because the work
itself lives in another process: the backend holds the lock and then talks to
the llama-server that owns the cores. flock is also what makes the lock
survive a backend restart -- the kernel drops it when the process dies, so a
crash mid-index does not leave the pool locked forever.
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


class CpuPoolBusy(Exception):
    """The pool could not be taken within the deadline."""


class CpuPool:
    """A mutex over a set of cores, shared between processes.

    `path` is the lock file. It is created on first use; the directory has to
    exist.
    """

    def __init__(self, path: Path):
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.name = self.path.stem
        # In-process exclusion. A plain Lock, not an RLock: the release has
        # to work from a different thread than the acquire, because a
        # streaming response is consumed by a thread pool that may hand
        # the next chunk to another worker. A plain Lock can be released
        # by any thread; an RLock cannot.
        self._thread_lock = threading.Lock()
        # How many holds are outstanding. The file lock belongs to the
        # outermost one, so it is only given back when the count reaches
        # zero.
        self._holds = 0
        self._fd: Optional[int] = None

    # -- internals ---------------------------------------------------------

    def _open(self) -> int:
        if self._fd is None:
            self._fd = os.open(self.path, os.O_RDWR | os.O_CREAT, 0o600)
        return self._fd

    def _try_flock(self) -> bool:
        fd = self._open()
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
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

    # -- public API --------------------------------------------------------

    def acquire(self, timeout: Optional[float] = None) -> bool:
        """Take the pool.

        `timeout` is a number of seconds to wait, `None` to wait forever,
        `0` to try once. Returns True when the pool was taken.

        The pool is not reentrant. Taking it twice from one thread waits
        for the first hold to end, and with a deadline it fails instead of
        hanging. That is deliberate: the model holds the pool while it
        generates, and a nested hold would mean the cores were promised to
        two things at once.
        """
        deadline = None if timeout is None else time.monotonic() + timeout
        # In-process first: a thread that is waiting for the lock should
        # queue behind the current holder, not race it.
        if timeout is None:
            self._thread_lock.acquire()
        else:
            if not self._thread_lock.acquire(timeout=max(0.0, timeout)):
                return False
        try:
            while True:
                if self._try_flock():
                    break
                if deadline is not None and time.monotonic() >= deadline:
                    self._thread_lock.release()
                    return False
                # Sleep a fraction of the remaining time so the last attempt
                # lands close to the deadline.
                remaining = None if deadline is None else deadline - time.monotonic()
                time.sleep(POLL_INTERVAL if remaining is None
                           else max(0.001, min(POLL_INTERVAL, remaining)))
        except BaseException:
            self._thread_lock.release()
            raise
        self._holds += 1
        return True

    def release(self) -> None:
        self._holds -= 1
        if self._holds <= 0:
            self._holds = 0
            self._release_flock()
        self._thread_lock.release()

    @contextmanager
    def hold(self, timeout: Optional[float] = None) -> Iterator[None]:
        """Hold the pool, waiting up to `timeout` seconds.

        Raises CpuPoolBusy when the deadline passes. Used by the indexer,
        which has to wait: the alternative is dropping work on the floor.
        """
        if not self.acquire(timeout):
            raise CpuPoolBusy(f"CPU 池在 {timeout} 秒内没有被释放")
        try:
            yield
        finally:
            self.release()

    @contextmanager
    def try_hold(self, timeout: float = 0.0) -> Iterator[bool]:
        """Hold the pool if it is free, otherwise yield False.

        Never raises and never waits longer than `timeout`. Used by the
        text-to-image query, where waiting is worse than failing.
        """
        got = self.acquire(timeout)
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


_pool: Optional[CpuPool] = None
_pool_lock = threading.Lock()

# The pools this installation knows about. The names are the file names, so
# a stale lock file from a previous run is picked up rather than ignored.
BIG_POOL = "big-cores"


def get_pool(name: str) -> CpuPool:
    """The pool with the given name, created on first use."""
    global _pool
    with _pool_lock:
        if _pool is None:
            from .deps import DATA_DIR
            _pool = {}
        if name not in _pool:
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

