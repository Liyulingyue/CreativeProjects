"""Building the picture index.

A picture costs about ten seconds of the big cores to encode, so this is a
deliberate job rather than something that runs in the background all the
time: the user starts it, or a schedule does, and it goes through the
pictures one at a time.

It shares the big cores with the model, so it stands down whenever the user
is talking -- the same rule the text index follows -- and it can be paused
by hand. A pause is worth having: while a build is running, a picture
search cannot get the cores.
"""

import logging
import threading
import time
from datetime import datetime, timedelta
from pathlib import Path
from typing import Optional

from .cpu_pool import CpuPoolBusy, get_big_pool
from .image_embed import JinaError, get_image_embedder, is_image_file
from .image_store import ImageVectorStore, file_hash, get_image_store
from .indexer import ACTIVE_HOURS_ALL, _in_active_hours, _parse_window
from .interaction import interaction

log = logging.getLogger(__name__)

# Pictures bigger than this are skipped: a panorama can be tens of
# megabytes and the encoder pays for every pixel of it.
MAX_IMAGE_BYTES = 20 * 1024 * 1024


def _is_hidden(entry: Path, root: Optional[Path] = None) -> bool:
    """Whether a path is inside a hidden directory.

    `.trash` and `.uploads` hold deleted and half-written files; indexing
    them would make a deleted picture still findable.

    Only the part below the storage root counts. The root itself may sit
    under a dot-directory (a dotfile-style install path), and then every
    picture would look hidden and the index would quietly stay empty.
    """
    try:
        parts = entry.relative_to(root).parts if root else entry.parts
    except (ValueError, OSError):
        parts = entry.parts
    return any(part.startswith(".") for part in parts)


def _relative(path: Path) -> str:
    from .deps import get_storage_root
    try:
        return str(path.relative_to(get_storage_root()))
    except (ValueError, OSError):
        return path.name


class ImageIndexer:
    """Encodes the pictures in the storage and keeps their vectors."""

    def __init__(self):
        self._lock = threading.Lock()
        self._thread: Optional[threading.Thread] = None
        self._paused = False
        self._stop = False
        # A pause does not end the run: the thread waits here so that
        # "resume" has something to resume. Ending it would leave the
        # remaining pictures for the next day, which is not what the button
        # says.
        self._resume_event = threading.Event()
        self._resume_event.set()
        self._status = {
            "running": False,
            "paused": False,
            "total": 0,
            "done": 0,
            "skipped": 0,
            "encoded": 0,
            "deferred": 0,
            "failed": 0,
            "current": "",
            "last_encode_seconds": 0.0,
            "started_at": 0.0,
            "finished_at": 0.0,
            "last_error": "",
        }

    # -- control -----------------------------------------------------------

    def start(self, limit: int = 0, rebuild: bool = False) -> dict:
        """Start a build in the background.

        `limit` caps how many pictures are processed (0 means all of them).
        `rebuild` re-encodes pictures that already have a vector.
        """
        with self._lock:
            if self._thread and self._thread.is_alive():
                return {**self._status, "started": False,
                        "reason": "图片索引已经在跑了"}
            self._stop = False
            self._paused = False
            self._resume_event.set()
            self._status["running"] = True
            self._thread = threading.Thread(
                target=self._run, args=(limit, rebuild), daemon=True,
                name="image-indexer")
            self._thread.start()
        return {**self.status(), "started": True}

    def pause(self) -> dict:
        with self._lock:
            self._paused = True
            self._status["paused"] = True
            self._resume_event.clear()
        return self.status()

    def resume(self) -> dict:
        with self._lock:
            self._paused = False
            self._status["paused"] = False
            self._resume_event.set()
        return self.status()

    def stop(self) -> dict:
        with self._lock:
            self._stop = True
            self._paused = False
            self._status["paused"] = False
            self._resume_event.set()
        return self.status()

    def status(self) -> dict:
        with self._lock:
            return dict(self._status)

    # -- the work ----------------------------------------------------------

    def _pictures(self, root: Path) -> tuple[list[Path], bool]:
        """The pictures in the storage, and whether the scan was complete.

        The completeness matters: the pictures that are no longer there
        get their vectors dropped at the end of a build, and that is only
        safe when the scan really did see the whole tree. A directory
        that cannot be read -- a permissions problem, a disk that is
        acting up -- would otherwise look like "all the pictures are
        gone", and the index would be emptied.
        """
        found: list[Path] = []
        complete = True
        if not root.exists():
            log.error("the storage root %s does not exist", root)
            return [], False
        try:
            for entry in root.rglob("*"):
                if _is_hidden(entry, root) or not entry.is_file():
                    continue
                if not is_image_file(entry):
                    continue
                try:
                    if entry.stat().st_size == 0 or entry.stat().st_size > MAX_IMAGE_BYTES:
                        continue
                except OSError:
                    continue
                found.append(entry)
        except OSError as e:
            log.warning("cannot scan for pictures: %s", e)
            complete = False
        # rglob swallows a directory it cannot read, so the walk cannot
        # tell us it missed something. Comparing the number of pictures
        # with the number of directories is the cheapest way to notice
        # that something is off.
        if complete and not found:
            try:
                dirs = sum(1 for _ in root.rglob("*") if _.is_dir())
            except OSError:
                dirs = 0
            if dirs:
                log.warning("the storage root has %d directories but no pictures "
                            "were found -- treating the scan as incomplete", dirs)
                complete = False
        return sorted(found), complete

    def _run(self, limit: int, rebuild: bool) -> None:
        """Run the build, and always come back to a sane state.

        Without the finally, an exception that escaped the per-picture
        handler would leave the status saying "running" for ever, and the
        page would offer Pause instead of the button that starts a build.
        """
        try:
            self._build(limit, rebuild)
        except Exception as e:  # noqa: BLE001 - the run must always end cleanly
            log.exception("the picture index build failed")
            with self._lock:
                self._status["last_error"] = f"{type(e).__name__}: {str(e)[:180]}"
        finally:
            with self._lock:
                self._status["running"] = False
                self._status["current"] = ""
                if not self._status["finished_at"]:
                    self._status["finished_at"] = time.time()
            self._resume_event.set()

    def _build(self, limit: int, rebuild: bool) -> None:
        from .deps import DATA_DIR, get_storage_root

        store = get_image_store(str(DATA_DIR / "vectors"))
        embedder = get_image_embedder()
        root = get_storage_root()
        pictures, scan_complete = self._pictures(root)

        with self._lock:
            # The counters start over; the flags do not. A pause issued
            # while the tree was being scanned would otherwise be
            # overwritten, and the build would look like it was running.
            self._status = {
                "running": True,
                "paused": self._paused,
                "total": len(pictures),
                "done": 0,
                "skipped": 0,
                "encoded": 0,
                "deferred": 0,
                "failed": 0,
                "current": "",
                "last_encode_seconds": 0.0,
                "started_at": time.time(),
                "finished_at": 0.0,
                "last_error": "",
            }

        # What is already indexed, read once. Asking the table for this
        # per picture would mean reading the whole table per picture.
        indexed = {} if rebuild else store.indexed()

        # The pictures wait here until there are enough of them to write
        # together. One merge_insert per picture is not a detail: LanceDB
        # keeps a version of the table per write, and a thousand pictures
        # written one at a time leave forty megabytes of version files and
        # take a minute and a half. Written fifty at a time it is a third
        # of a second and thirty kilobytes.
        pending: list[tuple] = []

        # Pictures that could not have the cores. They are tried again at
        # the end of the run rather than dropped -- otherwise a busy
        # machine would leave them for the next day.
        deferred: list[Path] = []

        # How many times writing the batch failed in a row. A disk that
        # is full would otherwise let the build run for hours, encoding
        # pictures that cannot be stored.
        write_failures = 0

        # The pictures are walked by hand rather than with a for-loop:
        # a pause or a busy machine has to *wait* for the same picture,
        # not step over it. A `continue` in a for-loop moves on to the
        # next one, which quietly threw away a picture every time
        # somebody paused.
        position = 0
        while position < len(pictures):
            picture = pictures[position]
            # The limit counts the pictures that were actually dealt
            # with, not the ones that were skipped because they were
            # already indexed.
            if limit and (self._status["encoded"] + self._status["failed"]) >= limit:
                break
            if self._stop:
                break
            # A pause waits here rather than ending the run, so that
            # "resume" continues where the build left off.
            if self._paused:
                self._resume_event.wait(0.5)
                continue
            # The model has priority: if someone is talking, wait.
            if interaction.is_busy():
                time.sleep(2.0)
                continue

            position += 1
            rel = _relative(picture)
            try:
                stat = picture.stat()
            except OSError:
                continue

            digest = ""
            if not rebuild:
                try:
                    digest = file_hash(picture)
                except OSError:
                    digest = ""
                if digest and indexed.get(rel) == digest:
                    with self._lock:
                        self._status["done"] += 1
                        self._status["skipped"] += 1
                    continue

            with self._lock:
                self._status["current"] = rel

            # One picture at a time, and the cores for the duration: the
            # search and the model both wait for this call to finish.
            try:
                # A deadline on the wait: a picture that hangs the encoder
                # must not keep the cores for everybody else.
                with get_big_pool().hold(timeout=POOL_WAIT_TIMEOUT):
                    # Measured from inside the lock: waiting for the cores
                    # is not encoding, and a search that sized its wait
                    # by that would hang for minutes.
                    started = time.monotonic()
                    vector = embedder.embed_image(picture)
                    elapsed = time.monotonic() - started
                with self._lock:
                    self._status["last_encode_seconds"] = round(elapsed, 2)
                if not digest:
                    try:
                        digest = file_hash(picture)
                    except OSError:
                        digest = ""
            except CpuPoolBusy as e:
                # The cores were not free -- the model was generating, or a
                # search had them. The picture is fine; it just has to
                # wait for its turn. Counting it as a failure would put a
                # confusing error on the screen.
                log.info("deferring %s: %s", rel, e)
                with self._lock:
                    self._status["deferred"] += 1
                    self._status["current"] = ""
                deferred.append(picture)
                continue
            except JinaError as e:
                log.warning("cannot encode %s: %s", rel, e)
                with self._lock:
                    self._status["failed"] += 1
                    self._status["last_error"] = str(e)[:200]
                    self._status["current"] = ""
                continue
            except Exception as e:  # noqa: BLE001 - one bad picture must not stop the run
                log.warning("cannot encode %s: %s", rel, e)
                with self._lock:
                    self._status["failed"] += 1
                    self._status["last_error"] = f"{type(e).__name__}: {str(e)[:180]}"
                    self._status["current"] = ""
                continue

            # The picture is encoded. It is not counted as done until the
            # batch it belongs to is actually in the table -- otherwise the
            # status would claim pictures that were never stored.
            pending.append((vector, rel, digest, stat.st_size, stat.st_mtime))
            if len(pending) >= WRITE_BATCH:
                wrote = _flush(store, pending, on_written=_counted(store, indexed, self),
                               on_failed=_failed(self))
                if not wrote:
                    write_failures += 1
                    if write_failures >= MAX_WRITE_FAILURES:
                        log.error("the index cannot be written to (%d tries in a row) "
                                  "-- stopping the build", write_failures)
                        with self._lock:
                            self._status["last_error"] = (
                                "连续写库失败，已中止本轮（磁盘满了？或者向量库被换掉了）")
                        break
                else:
                    write_failures = 0

        if pending:
            _flush(store, pending, on_written=_counted(store, indexed, self),
                   on_failed=_failed(self))

        # One more turn for the pictures that had to wait for the cores.
        if deferred:
            for picture in deferred:
                if self._stop or self._paused:
                    break
                if interaction.is_busy():
                    break
                rel = _relative(picture)
                try:
                    with get_big_pool().hold(timeout=POOL_WAIT_TIMEOUT):
                        vector = embedder.embed_image(picture)
                except Exception as e:  # noqa: BLE001
                    log.info("still cannot encode %s: %s", rel, e)
                    with self._lock:
                        self._status["failed"] += 1
                    continue
                try:
                    digest = file_hash(picture)
                except OSError:
                    digest = ""
                pending.append((vector, rel, digest, 0, 0.0))
            if pending:
                _flush(store, pending, on_written=_counted(store, indexed, self),
                       on_failed=_failed(self))

        # Pictures that are gone from the storage should not stay findable.
        # But only when the scan really saw the whole tree: an unreadable
        # directory looks exactly like "every picture is gone", and acting
        # on that would empty the index.
        if not scan_complete:
            log.warning("the scan was not complete -- leaving the index as it is")
            with self._lock:
                self._status["last_error"] = "扫描没有完成，索引保持原样（可能有目录读不了）"
        else:
            try:
                known = {_relative(p) for p in pictures}
                if not known and store.count() > 0:
                    log.warning("the storage root has no pictures but the index "
                                "has %d -- leaving the index as it is", store.count())
                else:
                    store.delete_missing(known)
            except Exception as e:  # noqa: BLE001
                log.warning("cannot clean the picture index: %s", e)

        # Every write left a version of the table behind. The text indexer
        # cleans those up for the same reason.
        try:
            # The versions this run produced are the ones taking up the
            # room, so they go now rather than in two days.
            store.cleanup_old_versions(older_than_days=0)
        except Exception as e:  # noqa: BLE001
            log.warning("cannot clean up the picture table versions: %s", e)


image_indexer = ImageIndexer()


# ---------------------------------------------------------------------------
# The window
# ---------------------------------------------------------------------------
# A picture costs about three seconds of the big cores, and those are the
# cores the model wants. So the picture index is built in a window -- half
# an hour by default -- and when the window closes the build stops, even
# with pictures left to do. The next window picks up where it left off:
# a picture that already has a vector with the right hash is skipped.
#
# The window is "HH:MM-HH:MM" and may wrap past midnight ("23:00-07:00").
DEFAULT_WINDOW = "09:00-09:30"

# How long to wait before an all-day schedule looks for new pictures. A
# re-scan hashes every picture, so it is not something to do every few
# seconds.
RESTART_INTERVAL = 30 * 60

# How long a picture waits for the big cores before giving up. The encoder
# itself has its own timeout; this one bounds the wait for the lock.
POOL_WAIT_TIMEOUT = 120.0


# How many pictures are written to the table in one go.
WRITE_BATCH = 50

# How many times writing may fail in a row before the build gives up.
MAX_WRITE_FAILURES = 3


def _counted(store, indexed: dict, indexer: "ImageIndexer"):
    """The callback for a batch that made it into the table."""
    def on_written(batch):
        for item in batch:
            indexed[item[1]] = item[2]
        with indexer._lock:
            indexer._status["done"] += len(batch)
            indexer._status["encoded"] += len(batch)
    return on_written


def _failed(indexer: "ImageIndexer"):
    """The callback for a batch that could not be written."""
    def on_failed(batch, error):
        with indexer._lock:
            indexer._status["failed"] += len(batch)
            indexer._status["last_error"] = (
                f"写库失败（{len(batch)} 张未写入）: {type(error).__name__}: {str(error)[:140]}")
    return on_failed


def _flush(store: ImageVectorStore, pending: list[tuple], on_written=None,
           on_failed=None) -> bool:
    """Write the pictures that are waiting, in one go.

    Returns True when they are in the table. A write that fails takes the
    whole batch down with it -- the pictures were encoded, but they are
    not stored, so the run has to say so rather than count them as done.
    """
    if not pending:
        return True
    vectors = [p[0] for p in pending]
    records = [{"file_path": p[1], "content_hash": p[2],
                "file_size": p[3], "modified_at": p[4]} for p in pending]
    batch = list(pending)
    try:
        store.add_many(vectors, records)
    except Exception as e:  # noqa: BLE001
        # The batch is dropped rather than retried picture by picture:
        # keeping it would mean rewriting the whole accumulated batch for
        # every following picture, which is quadratic.
        pending.clear()
        log.warning("cannot write %d pictures to the index: %s", len(batch), e)
        if on_failed:
            on_failed(batch, e)
        return False
    pending.clear()
    if on_written:
        on_written(batch)
    return True


def _sane_window(window: str) -> bool:
    """Whether the window is a real time of day.

    The text indexer accepts "24:00-06:00" as a window that wraps past
    midnight. For the picture index that would mean starting a build at
    hour 24, which is not a time -- so this one insists on real hours and
    falls back to the default when they are not.
    """
    parsed = _parse_window(window)
    if parsed is None:
        return False
    start, end = parsed
    for minutes in (start, end):
        if not (0 <= minutes // 60 < 24 and 0 <= minutes % 60 < 60):
            return False
    return True


def _window_end(started_at: datetime, window: str) -> Optional[datetime]:
    """When the window closes, given when it opened.

    None when there is no closing time: the window is "all", or it does not
    parse. A build in that state runs to the end rather than stopping and
    starting again on the next tick.
    """
    parsed = _parse_window(window)
    if parsed is None or not _sane_window(window):
        return None
    _, end_min = parsed
    end = started_at.replace(hour=end_min // 60, minute=end_min % 60,
                              second=0, microsecond=0)
    if end <= started_at:
        # The window wraps past midnight.
        end += timedelta(days=1)
    return end


class ImageIndexScheduler:
    """Starts the picture index when its window opens, stops it when it closes."""

    def __init__(self):
        self._stop_event = threading.Event()
        self._thread: Optional[threading.Thread] = None
        # True once we have started a build for the current window. It stops
        # the scheduler from starting a second one, and from restarting a
        # build the user paused on purpose.
        self._window_started = False
        self._started_at: Optional[datetime] = None
        self._finished_at: Optional[datetime] = None

    def start(self) -> None:
        if self._thread and self._thread.is_alive():
            return
        self._stop_event.clear()
        self._thread = threading.Thread(target=self._run, name="image-index-schedule",
                                        daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._stop_event.set()

    def _run(self) -> None:
        # Ten seconds, not a minute: a short window would otherwise be
        # missed entirely between two ticks.
        while not self._stop_event.wait(10):
            try:
                self._tick()
            except Exception:
                log.exception("the picture index schedule failed")

    def _tick(self) -> None:
        from .deps import state

        settings = state.get_settings()
        window = (settings.image_index_window or DEFAULT_WINDOW).strip()
        if window != ACTIVE_HOURS_ALL and not _sane_window(window):
            # A window that does not parse would otherwise mean "all day",
            # and the build would run once and then never again -- the
            # schedule would look alive and do nothing. Fall back to the
            # default and say so.
            log.error("图片索引的时间窗口 %r 解析不了，改用默认窗口 %s", window, DEFAULT_WINDOW)
            window = DEFAULT_WINDOW
        now = datetime.now()

        if not settings.image_index_enabled:
            # Turning the schedule off stops the build it started. A build
            # the user started by hand is left alone -- that one is theirs.
            if self._window_started:
                image_indexer.stop()
                self._window_started = False
                self._started_at = None
            return

        if not self._window_started:
            if _in_active_hours(window, now):
                started = image_indexer.start()
                # Only a build we actually started is ours to stop at the
                # end of the window. If the user has one running, it is
                # theirs and it keeps going.
                if started.get("started"):
                    self._window_started = True
                    self._started_at = now
                    log.info("picture index window opened, starting the build")
            return

        if window == ACTIVE_HOURS_ALL:
            # No closing time: the build runs to the end. When it is done,
            # wait a while and look again, so pictures that arrived since
            # get picked up -- otherwise an all-day schedule would index
            # once and then never again.
            if not image_indexer.status().get("running"):
                waited = self._finished_at is None or (now - self._finished_at).total_seconds() >= RESTART_INTERVAL
                if waited:
                    self._finished_at = now
                    started = image_indexer.start()
                    if started.get("started"):
                        self._window_started = True
                        self._started_at = now
            return

        end = _window_end(self._started_at or now, window)
        if end is None:
            # The window does not parse. Treat it as all-day rather than
            # stopping the build and starting it again on the next tick.
            return
        if now >= end:
            log.info("picture index window closed at %s, stopping the build", end)
            image_indexer.stop()
            self._window_started = False
            self._started_at = None


image_index_schedule = ImageIndexScheduler()
