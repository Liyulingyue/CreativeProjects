"""Application logging.

Nothing in this project configured logging before, so every ``log.info()``
call went nowhere -- the only thing that ever reached a log file was a
traceback printed to stderr. A service meant to run unattended needs the
rest of that story as well: what it was doing, how long it took, and
which request it was doing it for.

Everything lands in one file that is rotated on its own: the app's own
records, uvicorn's access log, and anything printed to stdout. A copy
also goes to stdout, so ``docker logs`` shows the same thing.
"""

from __future__ import annotations

import logging
import os
import sys
import uuid
from contextvars import ContextVar
from logging.handlers import RotatingFileHandler
from pathlib import Path

# The id of the request being served, so a line in the log can be traced
# back to the request that produced it. "-" outside a request.
_request_id: ContextVar[str] = ContextVar("request_id", default="-")


def request_id() -> str:
    """The current request's id, or "-" outside a request."""
    return _request_id.get()


def start_request_id(rid: str | None = None) -> str:
    """Begin a request scope and return its id."""
    rid = rid or uuid.uuid4().hex[:12]
    _request_id.set(rid)
    return rid


class _RequestIdFilter(logging.Filter):
    def filter(self, record: logging.LogRecord) -> bool:
        record.request_id = _request_id.get()
        return True


class _Formatter(logging.Formatter):
    def format(self, record: logging.LogRecord) -> str:
        rid = getattr(record, "request_id", "-")
        base = super().format(record)
        return f"{base} [{rid}]"


_configured = False


def log_file() -> Path | None:
    """Where the log file lives, or None when only stdout is wanted.

    ``LOG_FILE`` wins. Otherwise the development layout's ``logs/``
    directory is used, since that is where the start script used to
    dump stdout -- but only when it can actually be written to. A
    container that mounts its data elsewhere sets ``LOG_FILE`` itself.
    """
    env = os.getenv("LOG_FILE", "").strip()
    if env:
        return Path(env)
    # backend/app/logging_setup.py -> backend/app -> backend -> <root>
    root = Path(__file__).resolve().parent.parent.parent
    candidate = root / "logs" / "backend.log"
    try:
        candidate.parent.mkdir(parents=True, exist_ok=True)
        # A directory that exists but cannot be written to is worse than
        # no file handler at all, so check before committing to it.
        probe = candidate.parent / ".write-probe"
        probe.write_text("")
        probe.unlink()
        return candidate
    except OSError:
        return None


def setup_logging() -> None:
    """Configure the root logger once. Safe to call more than once."""
    global _configured
    if _configured:
        return
    _configured = True

    level = os.getenv("LOG_LEVEL", "INFO").upper()
    formatter = _Formatter(
        "%(asctime)s %(levelname)-7s %(name)-24s %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )
    rid_filter = _RequestIdFilter()

    handlers: list[logging.Handler] = []

    path = log_file()
    if path is not None:
        try:
            rotating = RotatingFileHandler(
                path,
                maxBytes=int(os.getenv("LOG_MAX_BYTES", str(10 * 1024 * 1024))),
                backupCount=int(os.getenv("LOG_BACKUP_COUNT", "5")),
                encoding="utf-8",
            )
            rotating.setFormatter(formatter)
            rotating.addFilter(rid_filter)
            handlers.append(rotating)
        except OSError as e:
            print(f"[logging] cannot open {path}: {e}", file=sys.stderr)

    stream = logging.StreamHandler(sys.stdout)
    stream.setFormatter(formatter)
    stream.addFilter(rid_filter)
    handlers.append(stream)

    root = logging.getLogger()
    root.setLevel(getattr(logging, level, logging.INFO))
    for h in handlers:
        root.addHandler(h)

    # uvicorn configures its own loggers with propagate off, so its
    # access log would never reach the file handler above. Point them at
    # the same handler instead of writing a second, unrotated copy.
    for name in ("uvicorn", "uvicorn.error", "uvicorn.access"):
        lg = logging.getLogger(name)
        lg.handlers = []
        lg.propagate = False
        for h in handlers:
            lg.addHandler(h)
        if name == "uvicorn.access":
            # The request middleware already logs every request with its
            # id and duration; uvicorn's copy would say the same thing
            # a second time.
            lg.setLevel(
                getattr(logging, os.getenv("ACCESS_LOG_LEVEL", "WARNING").upper(), logging.WARNING)
            )

    # Third-party libraries are chatty at DEBUG and drown out the app's
    # own records. Everything above INFO is left alone.
    for name in ("httpx", "httpcore", "urllib3", "openai", "lancedb", "pyarrow"):
        logging.getLogger(name).setLevel(logging.WARNING)
