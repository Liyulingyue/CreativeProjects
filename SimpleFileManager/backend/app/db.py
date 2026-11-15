"""One place to open a SQLite database.

Every database in this project was opened with the stock settings:
rollback journal, a five second timeout, and no locking of its own.
That combination is the wrong one for this service, because the
indexer writes the file manifest while the digest thread reads it and
the API reads the search index -- a writer blocked every reader, and
a reader that waited long enough gave up with "database is locked".

Three changes:

* WAL, so readers carry on while a writer works. The mode is a
  property of the file, not of the connection, so it only has to be
  set once -- but setting it on every connection is harmless and
  covers databases created by a later version of the code.
* A busy timeout, which turns lock contention into a wait instead of
  an error. The default was five seconds; a full reindex of a large
  book writes thousands of rows in one transaction and needs longer.
* synchronous=NORMAL, which is the right setting once WAL is on: it
  survives a crash of the process, and only the last transaction can
  be lost to a power cut.

The lock itself lives with the service that owns the database -- see
``search_engine`` and ``indexer``, which had none.
"""

from __future__ import annotations

import sqlite3
from contextlib import contextmanager
import threading

# How long a statement waits for a lock before giving up. A reindex of
# a large document holds a write lock for a while, and a reader that
# gives up is a failed request.
BUSY_TIMEOUT_MS = 20_000

# Writing the journal synchronously on every commit costs a disk sync
# per transaction, which the write-ahead log makes unnecessary.
_conn_options = {
    "timeout": BUSY_TIMEOUT_MS / 1000,
    "check_same_thread": False,
}


@contextmanager
def write_lock(lock: threading.Lock, timeout: float = 10.0, what: str = "数据库"):
    """Hold a store's lock for a write, or give up.

    The lock is only for writes. The database is opened in WAL mode, so
    a reader works while a writer works, and there is no reason to make
    a reader wait for a writer -- or a writer wait for a reader.

    The wait is bounded anyway: a lock held by something that will never
    let go would otherwise take every request that has to pass through
    the same door with it, and the service would stop answering without
    saying why.
    """
    if not lock.acquire(timeout=timeout):
        raise TimeoutError(f"{what}忙，{timeout} 秒内没有拿到写锁")
    try:
        yield
    finally:
        lock.release()


def connect(path, **overrides) -> sqlite3.Connection:
    """Open a database with the settings this service needs."""
    options = {**_conn_options, **overrides}
    conn = sqlite3.connect(path, **options)
    conn.execute(f"PRAGMA busy_timeout = {BUSY_TIMEOUT_MS}")
    try:
        conn.execute("PRAGMA journal_mode = WAL")
    except sqlite3.OperationalError:
        # Another connection is in the middle of a transaction. The mode
        # belongs to the file, so whoever gets there first sets it.
        pass
    conn.execute("PRAGMA synchronous = NORMAL")
    return conn


def connect_readonly(path, timeout: float = 2.0) -> sqlite3.Connection:
    """Open a database read-only, for probes that must not create it."""
    conn = sqlite3.connect(
        f"file:{path}?mode=ro", uri=True, timeout=timeout, check_same_thread=False
    )
    conn.execute(f"PRAGMA busy_timeout = {int(timeout * 1000)}")
    return conn


class DatabaseLock:
    """A reentrant lock for one database.

    SQLite serialises writers itself, but it does not stop two threads
    from building conflicting views of the same file -- one deleting
    the rows of a file while another is counting them, say. The lock
    makes the read-modify-write sequences the service performs
    indivisible.
    """

    def __init__(self):
        self._lock = threading.RLock()

    def __enter__(self):
        self._lock.acquire()
        return self

    def __exit__(self, *exc):
        self._lock.release()
        return False
