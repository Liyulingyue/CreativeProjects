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
from .image_embed import (JinaError, get_image_embedder, is_image_file,
                          looks_like_an_image)
from .image_store import ImageVectorStore, file_hash, get_image_store
from .indexer import ACTIVE_HOURS_ALL, _in_active_hours, _parse_window
from .interaction import interaction

log = logging.getLogger(__name__)

# Pictures bigger than this are skipped: a panorama can be tens of
# megabytes and the encoder pays for every pixel of it.
MAX_IMAGE_BYTES = 20 * 1024 * 1024


# How long a picture waits for the big cores before giving up. The encoder
# itself has its own timeout; this one bounds the wait for the lock.
POOL_WAIT_TIMEOUT = 120.0

# How many pictures are written to the table in one go.
WRITE_BATCH = 50

# The first write of a run. Small on purpose: if the table is broken, we
# want to hear about it after a handful of pictures, not after a whole
# window of encoding.
CANARY_BATCH = 5

# How many times writing may fail in a row before the build gives up.
MAX_WRITE_FAILURES = 3


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


def _fresh_status(**overrides) -> dict:
    """A status with every field in it.

    There used to be two copies of this dict -- one for the start, one
    for a run that begins -- and a field added to one of them was missing
    from the other. One place to add a field now.
    """
    status = {
        "running": False,
        "paused": False,
        "total": 0,
        "done": 0,
        "skipped": 0,
        "encoded": 0,
        "deferred": 0,
        "failed": 0,
        "not_pictures": 0,
        "current": "",
        "last_encode_seconds": 0.0,
        "started_at": 0.0,
        "finished_at": 0.0,
        "last_error": "",
    }
    status.update(overrides)
    return status


def _follow_the_file(old_path: str, new_path: str) -> None:
    """Carry a picture's vector to the path the file now has.

    Renaming or moving a picture does not change what is in it, so the
    vector is still good. Re-encoding it would cost three seconds of
    the big cores -- the ones the model wants -- for nothing.
    """
    from .image_store import get_image_store

    try:
        store = get_image_store()
        store.move(old_path, new_path)
    except Exception:  # noqa: BLE001
        # The index is a convenience. If it cannot follow the file, the
        # next build will pick the picture up under its new name.
        log.warning("the picture index did not follow %s to %s", old_path,
                    new_path, exc_info=True)


def _forget_the_file(path: str) -> None:
    """Drop the vector of a picture that is gone for good.

    A picture that is merely in the recycle bin keeps its vector for a
    while -- the search skips it anyway, because the path it had is
    gone. A picture that was destroyed for good does not keep it.
    """
    from .image_store import get_image_store

    try:
        get_image_store().delete_by_file(path)
    except Exception:  # noqa: BLE001
        log.warning("the picture index did not forget %s", path, exc_info=True)


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
        # How many files were called pictures but were not.
        self._not_pictures = 0
        self._status = _fresh_status()

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
            # A build that was paused earlier left this behind. Without
            # clearing it, the new build would report itself as paused
            # while it was running, and the page would show "已暂停"
            # over a build that was working.
            self._status["paused"] = False
            self._thread = threading.Thread(
                target=self._run, args=(limit, rebuild), daemon=True,
                name="image-indexer")
            self._thread.start()
        return {**self.status(), "started": True}

    def pause(self) -> dict:
        with self._lock:
            if not (self._thread and self._thread.is_alive()):
                # Nothing is running. Setting the flag anyway would leave
                # a pause behind that the next build would inherit.
                return {**self._status, "paused": False, "started": False,
                        "reason": "没有正在建立的索引"}
            self._paused = True
            self._status["paused"] = True
            self._resume_event.clear()
        return {**self.status(), "started": True}

    def resume(self) -> dict:
        with self._lock:
            if not (self._thread and self._thread.is_alive()):
                return {**self._status, "paused": False, "started": False,
                        "reason": "没有正在建立的索引"}
            self._paused = False
            self._status["paused"] = False
            self._resume_event.set()
        return {**self.status(), "started": True}

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
        # Files that are called .jpg but are not pictures.
        not_pictures = 0
        if not root.exists():
            log.error("the storage root %s does not exist", root)
            return [], False
        try:
            for entry in root.rglob("*"):
                if _is_hidden(entry, root) or not entry.is_file():
                    continue
                if not is_image_file(entry):
                    continue
                # The name says picture, the bytes say otherwise. Encoding
                # it would cost the big cores and fail; skipping it is
                # free and honest.
                if not looks_like_an_image(entry):
                    not_pictures += 1
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
        if not_pictures:
            log.info("%d files are called pictures but are not", not_pictures)
        self._not_pictures = not_pictures
        with self._lock:
            self._status["not_pictures"] = not_pictures
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
            self._status = _fresh_status(
                running=True,
                paused=self._paused,
                total=len(pictures),
                started_at=time.time(),
            )
            # The scan happened before the status was reset, so its count
            # of "files that are called pictures but are not" has to be
            # carried over.
            self._status["not_pictures"] = self._not_pictures

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

        # Whether anything has been written yet. The first batch of a run
        # is deliberately small: it is the canary that finds out whether
        # the table works at all.
        wrote_once = False

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
            # The first write of a run is a small one on purpose. If the
            # table cannot be written to, we want to know after five
            # pictures rather than after fifty -- encoding is the slow
            # part, and the cores are needed for the model.
            batch_size = WRITE_BATCH if wrote_once else CANARY_BATCH
            if len(pending) >= batch_size:
                wrote = _flush(store, pending, on_written=_counted(store, indexed, self),
                               on_failed=_failed(self))
                if not wrote:
                    write_failures += 1
                    if not wrote_once or write_failures >= MAX_WRITE_FAILURES:
                        log.error("the index cannot be written to (%d tries in a row) "
                                  "-- stopping the build", write_failures)
                        with self._lock:
                            self._status["last_error"] = (
                                "连续写库失败，已中止本轮（磁盘满了？或者向量库被换掉了）")
                        break
                else:
                    wrote_once = True
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
