"""Tests for the shared embedding-core mutex."""

import os
import subprocess
import sys
import threading
import time
from pathlib import Path

import pytest

from app.cpu_pool import CpuPool, CpuPoolBusy


@pytest.fixture
def pool(tmp_path) -> CpuPool:
    return CpuPool(tmp_path / "locks" / "cpu-pool.lock")


def test_take_and_release(pool):
    assert pool.acquire(timeout=0) is True
    pool.release()


def test_try_hold_reports_busy(pool):
    """Another thread holds the pool."""
    held = threading.Event()
    released = threading.Event()

    def holder():
        with pool.hold():
            held.set()
            released.wait(5)

    t = threading.Thread(target=holder, daemon=True)
    t.start()
    assert held.wait(5)
    try:
        with pool.try_hold(timeout=0) as got:
            assert got is False
    finally:
        released.set()
        t.join(5)


def test_try_hold_succeeds_when_free(pool):
    with pool.try_hold(timeout=0) as got:
        assert got is True


def test_hold_waits_for_the_holder(pool):
    started = threading.Event()
    released = threading.Event()
    took = threading.Event()

    def holder():
        with pool.hold():
            started.set()
            released.wait(5)

    t = threading.Thread(target=holder, daemon=True)
    t.start()
    assert started.wait(5)

    def waiter():
        with pool.hold(timeout=5):
            took.set()

    w = threading.Thread(target=waiter, daemon=True)
    w.start()
    time.sleep(0.2)
    assert not took.is_set(), "the second holder should still be waiting"

    released.set()
    assert took.wait(5), "the second holder should get the pool"
    t.join(5)
    w.join(5)


def test_hold_raises_when_the_deadline_passes(pool):
    held = threading.Event()
    released = threading.Event()

    def holder():
        with pool.hold():
            held.set()
            released.wait(5)

    t = threading.Thread(target=holder, daemon=True)
    t.start()
    assert held.wait(5)
    try:
        with pytest.raises(CpuPoolBusy):
            with pool.hold(timeout=0.2):
                pass
    finally:
        released.set()
        t.join(5)


def test_is_free_tracks_the_holder(pool):
    assert pool.is_free() is True
    with pool.hold():
        assert pool.is_free() is False
    assert pool.is_free() is True


def test_the_lock_is_visible_to_another_process(pool):
    """The encoder lives in another process, so the lock has to be too."""
    script = (
        "import sys, time;"
        "sys.path.insert(0, %r);"
        "from app.cpu_pool import CpuPool;"
        "p = CpuPool(%r);"
        "p.acquire(timeout=5);"
        "print('held', flush=True);"
        "time.sleep(30)"
        % (str(Path(__file__).resolve().parent.parent), str(pool.path))
    )
    proc = subprocess.Popen([sys.executable, "-c", script], stdout=subprocess.PIPE)
    try:
        assert proc.stdout.readline().strip() == b"held"
        assert pool.acquire(timeout=0.5) is False, (
            "the pool should be held by the other process"
        )
    finally:
        proc.terminate()
        proc.wait(5)


def test_a_dead_process_does_not_leave_the_pool_locked(pool):
    """flock is released by the kernel when the holder dies."""
    script = (
        "import sys;"
        "sys.path.insert(0, %r);"
        "from app.cpu_pool import CpuPool;"
        "p = CpuPool(%r);"
        "p.acquire(timeout=5);"
        "print('held', flush=True)"
        % (str(Path(__file__).resolve().parent.parent), str(pool.path))
    )
    proc = subprocess.Popen([sys.executable, "-c", script], stdout=subprocess.PIPE)
    assert proc.stdout.readline().strip() == b"held"
    proc.wait(10)
    assert pool.acquire(timeout=1) is True
    pool.release()


def test_the_lock_file_is_not_world_readable(pool):
    pool.acquire(timeout=0)
    mode = os.stat(pool.path).st_mode & 0o777
    pool.release()
    assert mode == 0o600


def test_the_same_thread_may_not_take_the_pool_twice(pool):
    """The pool is not reentrant, and saying so beats hanging.

    A nested take waits for the outer hold to end. With a deadline it
    fails instead -- a request that hangs for ever is worse than one that
    says the server is busy.
    """
    assert pool.acquire(timeout=0) is True
    try:
        assert pool.acquire(timeout=0.2) is False
    finally:
        pool.release()
    # And the pool is free again afterwards.
    with pool.try_hold(timeout=0) as got:
        assert got is True


def test_the_pool_is_free_again_when_a_hold_ends(pool):
    with pool.hold():
        pass
    with pool.try_hold(timeout=0) as got:
        assert got is True


def test_two_readers_can_hold_the_pool_at_once(pool):
    """The model takes a reader's lease: two chats may generate at once.

    The llama-server on this box was started with --parallel 2, so it can
    serve two conversations at once. The pool does not need to be
    stricter than the server.
    """
    with pool.hold(timeout=5, shared=True):
        with pool.hold(timeout=5, shared=True):
            # Both are in at the same time.
            assert pool._readers == 2


def test_a_writer_keeps_readers_out(pool):
    """A picture search needs the whole box for the moment it encodes."""
    with pool.hold(timeout=5):
        with pytest.raises(CpuPoolBusy):
            with pool.hold(timeout=0.3, shared=True):
                pass


def test_a_reader_keeps_a_writer_out(pool):
    """The model has priority: the index waits for it, not the other way."""
    with pool.hold(timeout=5, shared=True):
        with pytest.raises(CpuPoolBusy):
            with pool.hold(timeout=0.3):
                pass


def test_a_writer_waits_for_the_readers_to_leave(pool):
    """A writer that arrives while readers are in waits for them."""
    order = []

    def reader():
        with pool.hold(timeout=5, shared=True):
            order.append("reader in")
            time.sleep(0.3)
            order.append("reader out")

    def writer():
        with pool.hold(timeout=5):
            order.append("writer in")
            order.append("writer out")

    t = threading.Thread(target=reader)
    t.start()
    time.sleep(0.1)
    w = threading.Thread(target=writer)
    w.start()
    t.join(5)
    w.join(5)
    assert order == ["reader in", "reader out", "writer in", "writer out"], order


def test_a_reader_does_not_wait_for_a_writer_that_is_only_waiting(pool):
    """The model must not be kept out by a writer that has not got in.

    The picture index takes the cores one picture at a time and gives
    them back between pictures, so it is waiting most of the time. A
    conversation that arrives in between should go straight in, or the
    index would keep the model out of its own machine.
    """
    order = []

    def writer_waiting():
        # The writer is held up by a reader that is already in.
        with pool.hold(timeout=5, shared=True):
            order.append("first reader in")
            time.sleep(0.4)
            order.append("first reader out")

    def second_reader():
        time.sleep(0.1)
        with pool.hold(timeout=5, shared=True):
            order.append("second reader in")

    t = threading.Thread(target=writer_waiting)
    t.start()
    time.sleep(0.05)
    # A writer tries to come in and has to wait for the reader.
    w = threading.Thread(target=lambda: _try_writer(pool, order))
    w.start()
    time.sleep(0.05)
    r = threading.Thread(target=second_reader)
    r.start()

    t.join(5)
    w.join(5)
    r.join(5)
    # The second reader got in before the writer did.
    assert order.index("second reader in") < order.index("writer in"), order


def _try_writer(pool, order):
    try:
        with pool.hold(timeout=5):
            order.append("writer in")
    except CpuPoolBusy:
        order.append("writer gave up")


def test_the_pool_is_free_again_after_a_reader_leaves(pool):
    with pool.hold(timeout=5, shared=True):
        pass
    with pool.hold(timeout=1):
        pass


def test_two_processes_share_the_readers_lock(pool):
    """A reader in another process does not shut this one out.

    The lock is a file lock because the work lives in another process; a
    reader's lease has to be a shared one, or a second backend would
    refuse to talk to the model while the first one is answering.
    """
    script = (
        "import sys, time;"
        "sys.path.insert(0, %r);"
        "from app.cpu_pool import CpuPool;"
        "p = CpuPool(%r);"
        "p.acquire(timeout=5, shared=True);"
        "print('held', flush=True);"
        "time.sleep(10)"
        % (str(Path(__file__).resolve().parent.parent), str(pool.path))
    )
    proc = subprocess.Popen([sys.executable, "-c", script], stdout=subprocess.PIPE)
    try:
        assert proc.stdout.readline().strip() == b"held"
        # This process can still take a reader's lease.
        assert pool.acquire(timeout=0.5, shared=True) is True
        pool.release()
        # But not a writer's: the other process is reading.
        assert pool.acquire(timeout=0.3) is False
    finally:
        proc.terminate()
        proc.wait(5)
