"""Auto digest scheduler.

Checks every minute if auto_digest_enabled is on and current hour
matches auto_digest_hour. Generates a digest for today if not already done.
"""

import threading
import time
import traceback
from datetime import datetime
from typing import Optional

from .deps import state
from .digest import digest_service


class AutoDigestScheduler:
    def __init__(self):
        self._stop_event = threading.Event()
        self._thread: Optional[threading.Thread] = None
        self._last_run_date: Optional[str] = None

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
        if self._last_run_date == today:
            return
        if now.hour < settings.auto_digest_hour:
            return
        existing = digest_service.get(today)
        if existing:
            self._last_run_date = today
            return
        result = digest_service.generate(today)
        if result.get("success"):
            self._last_run_date = today

    def reset(self):
        self._last_run_date = None


auto_digest = AutoDigestScheduler()
