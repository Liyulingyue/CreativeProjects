"""Many people reaching for the big cores at once.

The pool has one writer at a time and any number of readers. That is a
promise, and a promise that holds only under mild conditions is not a
promise. So: a lot of threads, all at the same time, all reaching for
the pool -- and two things watched throughout.

* A reader and a writer are never in at the same time.
* Two writers are never in at the same time.

The second one matters as much as the first: the picture search and
the picture index both take the pool as a writer, and if two of them
were in together, the "big cores" would be shared after all, which is
the thing the pool exists to prevent.

And the test fails if the pool ever wedges: every thread has to finish.
"""

import threading
import time

import pytest

from app.cpu_pool import CpuPool, CpuPoolBusy


@pytest.fixture
def pool(tmp_path) -> CpuPool:
    return CpuPool(tmp_path / "locks" / "cpu-pool.lock")


class TestThePoolUnderPressure:
    def test_readers_and_writers_never_overlap(self, pool):
        """Many threads, all at once, and the rules still hold."""
        readers = 6
        writers = 3
        violations = []
        finished = []
        barrier = threading.Barrier(readers + writers)

        def watch(stop):
            """Look at the pool's state while the others are in it."""
            while not stop.is_set():
                with pool._cond:
                    if pool._writer and pool._readers:
                        violations.append(f"writer and readers together: "
                                          f"{pool._readers} readers")
                    if pool._writer and pool._fd_mode not in ("exclusive", None):
                        violations.append(f"the file lock says {pool._fd_mode!r} "
                                          f"while a writer is in")
                    if pool._readers and pool._fd_mode not in ("shared", None):
                        violations.append(f"the file lock says {pool._fd_mode!r} "
                                          f"while readers are in")
                time.sleep(0.001)

        def as_reader(i):
            barrier.wait()
            for _ in range(6):
                with pool.hold(timeout=10, shared=True):
                    time.sleep(0.002)
            finished.append(("reader", i))

        def as_writer(i):
            barrier.wait()
            for _ in range(3):
                try:
                    with pool.hold(timeout=10):
                        time.sleep(0.004)
                except CpuPoolBusy:
                    violations.append(f"writer {i} gave up waiting")
            finished.append(("writer", i))

        stop = threading.Event()
        watcher = threading.Thread(target=watch, args=(stop,), daemon=True)
        watcher.start()

        threads = ([threading.Thread(target=as_reader, args=(i,)) for i in range(readers)]
                   + [threading.Thread(target=as_writer, args=(i,)) for i in range(writers)])
        started = time.monotonic()
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=120)
        stop.set()
        watcher.join(timeout=5)

        assert not any(t.is_alive() for t in threads), "the pool wedged"
        assert time.monotonic() - started < 120, "it took far too long"
        assert len(finished) == readers + writers, f"some gave up: {finished}"
        assert not violations, violations[:5]

    def test_the_pool_is_free_afterwards(self, pool):
        """Whatever happened, the pool is usable again."""
        barrier = threading.Barrier(8)

        def use(i):
            barrier.wait()
            for _ in range(5):
                try:
                    with pool.hold(timeout=10, shared=(i % 2 == 0)):
                        time.sleep(0.001)
                except CpuPoolBusy:
                    pass

        threads = [threading.Thread(target=use, args=(i,)) for i in range(8)]
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=60)
        assert not any(t.is_alive() for t in threads), "the pool wedged"
        # No reader or writer left behind.
        with pool._cond:
            assert pool._readers == 0, f"{pool._readers} readers left"
            assert pool._writer is False, "a writer left behind"
        # And it can be taken.
        assert pool.acquire(timeout=5), "the pool would not give itself back"
        pool.release()

    def test_a_writer_that_is_kept_out_gives_up_rather_than_hanging(self, pool):
        """A writer that cannot get in says so within its own deadline.

        Readers have priority on purpose: the model must never be slowed
        down for a picture. So a writer can be kept out -- and when that
        happens the caller has to be told, because the picture index
        puts the picture back in the queue and the picture search tells
        the user the machine is busy. Neither of those can happen if
        the writer waits for ever.
        """
        stop = threading.Event()
        gave_up = []

        def reader():
            while not stop.is_set():
                with pool.hold(timeout=5, shared=True):
                    time.sleep(0.0005)

        def writer():
            try:
                with pool.hold(timeout=1):
                    pass
            except CpuPoolBusy:
                gave_up.append(True)

        readers = [threading.Thread(target=reader, daemon=True) for _ in range(4)]
        for t in readers:
            t.start()
        try:
            w = threading.Thread(target=writer)
            started = time.monotonic()
            w.start()
            w.join(timeout=30)
            waited = time.monotonic() - started
        finally:
            stop.set()
            for t in readers:
                t.join(timeout=5)

        assert not w.is_alive(), "the writer hung instead of giving up"
        assert gave_up, "the writer neither got in nor gave up"
        assert waited < 10, f"it took {waited:.1f}s to give up"

    def test_a_writer_gets_in_when_the_readers_stop(self, pool):
        """When the readers go quiet, the writer gets its turn.

        This is the ordinary case: a conversation is being answered, the
        picture index waits, the conversation ends, and the index takes
        the cores for one picture.
        """
        stop = threading.Event()
        got = []

        def reader():
            while not stop.is_set():
                with pool.hold(timeout=5, shared=True):
                    time.sleep(0.002)
                time.sleep(0.002)   # a moment between requests

        def writer():
            try:
                with pool.hold(timeout=10):
                    got.append(True)
            except CpuPoolBusy:
                pass

        readers = [threading.Thread(target=reader, daemon=True) for _ in range(3)]
        for t in readers:
            t.start()
        try:
            w = threading.Thread(target=writer)
            w.start()
            w.join(timeout=30)
        finally:
            stop.set()
            for t in readers:
                t.join(timeout=5)
        assert not w.is_alive(), "the writer hung"
        assert got, "the writer never got the pool, even with gaps"
