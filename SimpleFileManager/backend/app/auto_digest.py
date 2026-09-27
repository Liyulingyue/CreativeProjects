"""Auto digest scheduler.

Two modes:
- scheduled: generate at a fixed hour every day (e.g. 23:00)
- interval: generate every N hours since last generation
"""

import threading
import time
import traceback
from datetime import datetime, timedelta
from typing import Optional

from .deps import state
from .digest import digest_service


class AutoDigestScheduler:
    def __init__(self):
        self._stop_event = threading.Event()
        self._thread: Optional[threading.Thread] = None
        self._last_run_ts: float = 0.0

    def start(self):
        if self._thread and self._thread.is_alive():
            return
        self._stop_event.clear()
        self._thread = threading.Thread(target=self._run, name="auto-digest", daemon=True)
        self._thread.start()

    def stop(self):
        self._stop_event.set()

    def _run(self):
        while not self._stop_event.wait(60):
            try:
                self._tick()
            except Exception:
                traceback.print_exc()

    def _tick(self):
        settings = state.get_settings()
        if not settings.auto_digest_enabled:
            return

        now = datetime.now()
        today = now.strftime("%Y-%m-%d")

        if settings.auto_digest_mode == "interval":
            hours = max(1, settings.auto_digest_interval_hours)
            if self._last_run_ts > 0 and (time.time() - self._last_run_ts) < hours * 3600:
                return
        else:
            if now.hour < settings.auto_digest_hour:
                return
            if self._last_run_ts > 0:
                last_run = datetime.fromtimestamp(self._last_run_ts)
                if last_run.date() == now.date():
                    return

        existing = digest_service.get(today)
        if existing:
            self._last_run_ts = time.time()
            return

        result = digest_service.generate(today)
        if result.get("success"):
            self._last_run_ts = time.time()

    def reset(self):
        self._last_run_ts = 0.0


auto_digest = AutoDigestScheduler()
