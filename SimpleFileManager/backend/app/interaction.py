"""Track whether the user is actively using the assistant.

Background indexing and chat share the four fast cores: measured on this box,
a chat request drops from ~27 tok/s to ~4 tok/s while embedding runs. Since
indexing is resumable and can proceed whenever the user is idle, the cheapest
fix is for indexing to stand down while someone is talking or searching.
"""

import threading
import time

# How long after the last interaction indexing may resume. Long enough that a
# pause between messages does not restart it, short enough that progress picks
# up again promptly once the user stops.
DEFAULT_IDLE_SECONDS = 45.0


class InteractionTracker:
    def __init__(self, idle_seconds: float = DEFAULT_IDLE_SECONDS):
        self._lock = threading.Lock()
        self._last_active = 0.0
        self._idle_seconds = idle_seconds

    def mark_active(self):
        with self._lock:
            self._last_active = time.time()

    def seconds_since_active(self) -> float:
        with self._lock:
            last = self._last_active
        return float("inf") if last == 0.0 else time.time() - last

    def is_busy(self) -> bool:
        """True while the user is mid-session."""
        return self.seconds_since_active() < self._idle_seconds

    def idle_seconds(self) -> float:
        return self._idle_seconds


interaction = InteractionTracker()
