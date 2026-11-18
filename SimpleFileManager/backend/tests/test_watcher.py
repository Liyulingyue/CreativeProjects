"""Tests for noticing changes instead of looking for them.

The watcher is best-effort by design: the periodic scan is the thing
the index depends on, and the watcher only makes it faster. So these
tests are about the parts that decide whether it is safe to take the
faster path -- which paths count as complete, which do not, and what
happens when too much changes at once.
"""

import struct

import pytest

from app.watcher import (
    IN_CLOSE_WRITE,
    IN_CREATE,
    IN_ISDIR,
    IN_MODIFY,
    IN_MOVED_TO,
    IN_Q_OVERFLOW,
    StorageWatcher,
    _max_user_watches,
)


def _event(wd: int, mask: int, name: str, cookie: int = 0) -> bytes:
    """One inotify_event, the way the kernel lays it out."""
    raw = name.encode()
    return struct.pack("iIII", wd, mask, cookie, len(raw)) + raw


class Harness:
    """A watcher with the kernel taken out of it."""

    def __init__(self, tmp_path):
        self.changed = []
        self.ready = []
        self.overflows = 0
        self.watcher = StorageWatcher(
            tmp_path,
            on_change=self._on_change,
            on_overflow=self._on_overflow,
        )
        # A watch on the root, as if it had been added for real.
        self.watcher._watches = {1: ("", 1)}

    def _on_change(self, changed, ready):
        self.changed.append(changed)
        self.ready.append(ready)

    def _on_overflow(self):
        self.overflows += 1

    def feed(self, *events):
        self.watcher._handle(b"".join(events))

    @property
    def all_changed(self):
        return set().union(*self.changed) if self.changed else set()

    @property
    def all_ready(self):
        return set().union(*self.ready) if self.ready else set()


def test_a_closed_file_is_ready_and_a_modified_one_is_not(tmp_path):
    """A writer closing a file is the signal that it is complete.

    A plain modify is not: it may be one write in the middle of a
    copy, and indexing it then would put half a file in the index.
    """
    h = Harness(tmp_path)
    h.feed(_event(1, IN_CLOSE_WRITE, "finished.txt"))
    h.feed(_event(1, IN_MODIFY, "still-writing.txt"))

    assert "finished.txt" in h.all_ready
    assert "still-writing.txt" in h.all_changed
    assert "still-writing.txt" not in h.all_ready


def test_a_file_moved_in_whole_is_ready(tmp_path):
    """A rename into the tree arrives complete; nothing is writing it."""
    h = Harness(tmp_path)
    h.feed(_event(1, IN_MOVED_TO, "arrived.txt"))

    assert "arrived.txt" in h.all_ready


def test_paths_are_relative_to_the_storage_root(tmp_path):
    """The index speaks in absolute paths; the watcher speaks in names.

    They are joined by the indexer, so what comes out of here has to
    be a plain relative path with no "./" in front of it.
    """
    h = Harness(tmp_path)
    h.watcher._watches[2] = ("sub/deeper", 2)
    h.feed(_event(2, IN_CLOSE_WRITE, "note.txt"))

    assert h.all_ready == {"sub/deeper/note.txt"}


def test_a_new_file_does_not_walk_the_tree(tmp_path):
    """File changes must not trigger a directory sync.

    A full walk per uploaded file would turn a big library into a
    machine that scans itself all day.
    """
    h = Harness(tmp_path)
    called = []
    h.watcher._sync_watches = lambda *a, **k: called.append(a) or set()

    h.feed(_event(1, IN_CREATE, "new.txt"))
    h.feed(_event(1, IN_CLOSE_WRITE, "new.txt"))

    assert called == []


def test_a_new_directory_is_followed(tmp_path):
    """A directory needs a watch, so a directory change does walk."""
    h = Harness(tmp_path)
    called = []
    h.watcher._sync_watches = lambda *a, **k: called.append(a) or set()

    h.feed(_event(1, IN_CREATE | IN_ISDIR, "newdir"))

    assert len(called) == 1


def test_a_directory_is_not_reported_as_a_file(tmp_path):
    """The index holds files. A directory is followed, not indexed."""
    h = Harness(tmp_path)
    h.feed(_event(1, IN_CLOSE_WRITE | IN_ISDIR, "adir"))

    assert "adir" not in h.all_changed
    assert "adir" not in h.all_ready


def test_overflow_asks_for_a_full_rescan(tmp_path):
    """The queue is finite, so the kernel can drop the details.

    When it does, what was missed cannot be known, and the only safe
    answer is to look at everything again.
    """
    h = Harness(tmp_path)
    h.feed(_event(1, IN_Q_OVERFLOW, ""))

    assert h.overflows == 1
    assert h.all_changed == set()


def test_a_root_directory_change_has_no_leading_slash(tmp_path):
    """The root's relative path is empty, so joining is plain."""
    assert StorageWatcher._relative(tmp_path, tmp_path) == ""
    assert StorageWatcher._join("", "a.txt") == "a.txt"
    assert StorageWatcher._join("sub", "a.txt") == "sub/a.txt"


def test_a_filesystem_that_cannot_talk_is_not_watched(monkeypatch, tmp_path):
    """On a network mount inotify is silent, which is worse than absent."""
    monkeypatch.setattr(StorageWatcher, "filesystem_of", staticmethod(lambda p: "nfs"))

    can, fstype = StorageWatcher.can_watch(tmp_path)

    assert can is False
    assert fstype == "nfs"


def test_a_local_filesystem_is_watched(monkeypatch, tmp_path):
    monkeypatch.setattr(StorageWatcher, "filesystem_of", staticmethod(lambda p: "ext4"))

    can, fstype = StorageWatcher.can_watch(tmp_path)

    assert can is True
    assert fstype == "ext4"


def test_the_watch_limit_is_a_positive_number():
    assert _max_user_watches() > 0


# ---- the whole thing, on a filesystem that really does notify ----


def _inotify_works(tmp_path) -> bool:
    can, _ = StorageWatcher.can_watch(tmp_path)
    if not can:
        return False
    w = StorageWatcher(tmp_path, on_change=lambda c, r: None)
    try:
        return w.start()
    except Exception:
        return False
    finally:
        w.stop()


@pytest.mark.skipif(not hasattr(__import__("os"), "O_NONBLOCK"), reason="not POSIX")
def test_a_written_file_is_reported_as_ready(tmp_path):
    """End to end, with the kernel doing the telling."""
    if not _inotify_works(tmp_path):
        pytest.skip("inotify is not available here")

    seen = []
    watcher = StorageWatcher(tmp_path, on_change=lambda c, r: seen.append((c, r)))
    assert watcher.start()
    try:
        (tmp_path / "hello.txt").write_text("hello")
        for _ in range(100):
            if any("hello.txt" in r for _c, r in seen):
                break
            import time

            time.sleep(0.02)
        ready = set().union(*(r for _c, r in seen)) if seen else set()
        assert "hello.txt" in ready
    finally:
        watcher.stop()


def test_files_already_in_a_new_directory_are_reported(tmp_path):
    """A directory copied in whole produces no per-file events.

    The files were written before there was a watch to hear about
    them, so the directory itself has to say they are there.
    """
    if not _inotify_works(tmp_path):
        pytest.skip("inotify is not available here")

    seen = []
    watcher = StorageWatcher(tmp_path, on_change=lambda c, r: seen.append((c, r)))
    assert watcher.start()
    try:
        # The directory and its contents appear together, the way a
        # rename into the tree looks from the outside.
        staging = tmp_path.parent / (tmp_path.name + "-staging")
        staging.mkdir()
        (staging / "one.txt").write_text("one")
        (staging / "two.txt").write_text("two")
        staging.rename(tmp_path / "arrived")

        for _ in range(100):
            ready = set().union(*(r for _c, r in seen)) if seen else set()
            if "arrived/one.txt" in ready and "arrived/two.txt" in ready:
                break
            import time

            time.sleep(0.02)

        ready = set().union(*(r for _c, r in seen)) if seen else set()
        assert "arrived/one.txt" in ready
        assert "arrived/two.txt" in ready
    finally:
        watcher.stop()


class TestTheWatchesThatAreGivenBack:
    """A watch that is not given back takes up a slot for ever.

    The kernel watches the inode, so a directory that is moved out of
    the tree keeps its watch. The kernel only gives it back when the
    directory is deleted -- and a directory that was moved somewhere
    else is not deleted. Each such watch is one of the user's watch
    slots, and there are not many of those.
    """

    def test_a_watch_that_is_dropped_is_also_removed_from_the_kernel(self, tmp_path):
        removed = []

        class FakeLibc:
            @staticmethod
            def inotify_rm_watch(fd, wd):
                removed.append(wd)
                return 0

            @staticmethod
            def inotify_add_watch(fd, path, mask):
                return 0

        h = Harness(tmp_path)
        h.watcher._libc = FakeLibc()
        h.watcher._fd = 7
        h.watcher._watches = {1: ("", 1), 2: ("gone", 2)}

        # Only one of the two directories is still there.
        (tmp_path / "still").mkdir()
        h.watcher._dir_cache = [tmp_path, tmp_path / "still"]
        h.watcher._sync_watches()

        assert 2 in removed, f"the watch was not given back: {removed}"

    def test_the_kernel_watches_do_not_pile_up(self, tmp_path):
        """The real thing: move the directories out and count the watches."""
        import glob
        import os

        h = Harness(tmp_path)
        if not h.watcher.can_watch(tmp_path)[0]:
            pytest.skip("this filesystem does not report changes")

        def watches_held():
            total = 0
            for fd in glob.glob(f"/proc/{os.getpid()}/fdinfo/*"):
                try:
                    with open(fd) as fh:
                        text = fh.read()
                except OSError:
                    continue
                if "inotify wd:" in text:
                    total += text.count("inotify wd:")
            return total

        assert h.watcher.start()
        try:
            before = watches_held()
            moved = tmp_path.parent / f"{tmp_path.name}-moved"
            moved.mkdir()
            for i in range(20):
                (tmp_path / f"dir{i}").mkdir()
            import time

            time.sleep(1.0)
            h.watcher._sync_watches()
            after_creating = watches_held()
            assert after_creating > before

            for i in range(20):
                os.rename(tmp_path / f"dir{i}", moved / f"dir{i}")
            time.sleep(0.5)
            h.watcher._sync_watches()
            after_moving = watches_held()

            assert after_moving < after_creating, (
                f"the watches piled up: {after_creating} -> {after_moving}"
            )
        finally:
            h.watcher.stop()
