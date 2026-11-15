"""When the picture index is built.

The builder lives in image_indexer.py; this is the part that decides
*when* it runs. The two are apart because they change for different
reasons: the builder changes when the model or the handling of a
picture changes, and the schedule changes when the hours the machine
may be busy change.
"""

import logging
import threading
from datetime import datetime, timedelta
from typing import Optional

from .image_indexer import image_indexer
from .indexer import ACTIVE_HOURS_ALL, _in_active_hours, _parse_window

log = logging.getLogger(__name__)

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
