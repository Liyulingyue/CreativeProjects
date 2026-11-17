"""Health checks that look at the things this service actually needs.

The old ``/api/health`` returned a fixed ``{"status": "ok"}`` no matter
what state the machine was in. A monitor pointed at it would report a
healthy service while the database was locked and the disk was full.

Each check is cheap and bounded. A check that hangs is worse than no
check, so the ones that touch the network get a short timeout and
report "degraded" rather than blocking the response.
"""

from __future__ import annotations

import logging
import os
import shutil
import sqlite3
import time
from pathlib import Path

log = logging.getLogger(__name__)

# A database that cannot be opened for reading within this many seconds
# is treated as locked. The default SQLite timeout is 5s, which is long
# enough to make a health check itself time out.
DB_PROBE_TIMEOUT_S = 2.0

# Below this many free bytes the service is still running but cannot
# take on new work safely.
DISK_LOW_BYTES = int(os.getenv("HEALTH_DISK_LOW_BYTES", str(1024 * 1024 * 1024)))

# Below this many *available* megabytes the machine is close to the
# point where the kernel's OOM killer starts choosing victims -- and
# the victim is usually this service, taking the chat history and the
# index with it. Saying so while there is still time is the whole
# point of a health check.
MEMORY_LOW_MB = int(os.getenv("HEALTH_MEMORY_LOW_MB", "2048"))


def _check_db(path: Path) -> dict:
    """Open a SQLite file read-only and read a row from it."""
    if not path.exists():
        return {"ok": False, "detail": "missing"}
    started = time.monotonic()
    try:
        uri = f"file:{path}?mode=ro"
        conn = sqlite3.connect(uri, uri=True, timeout=DB_PROBE_TIMEOUT_S)
        try:
            conn.execute("PRAGMA busy_timeout = %d" % int(DB_PROBE_TIMEOUT_S * 1000))
            conn.execute("SELECT count(*) FROM sqlite_master").fetchone()
        finally:
            conn.close()
        return {"ok": True, "ms": int((time.monotonic() - started) * 1000)}
    except sqlite3.OperationalError as e:
        # "database is locked" is the failure this check exists for.
        return {"ok": False, "detail": str(e)[:120]}
    except Exception as e:
        return {"ok": False, "detail": f"{type(e).__name__}: {str(e)[:100]}"}


def _check_disk(path: Path) -> dict:
    try:
        usage = shutil.disk_usage(path)
    except OSError as e:
        return {"ok": False, "detail": str(e)[:120]}
    return {
        "ok": usage.free > DISK_LOW_BYTES,
        "total": usage.total,
        "free": usage.free,
        "used_pct": round(usage.used / usage.total * 100, 1) if usage.total else 0,
        "detail": None if usage.free > DISK_LOW_BYTES else "free space below threshold",
    }


def check_memory() -> dict:
    """How much memory the machine has left for anybody to use.

    "Available" is the number that matters: it counts what could be
    handed out without the kernel having to swap or kill something.
    """
    try:
        with open("/proc/meminfo", encoding="utf-8") as fh:
            info = {}
            for line in fh:
                key, _, value = line.partition(":")
                info[key] = int(value.strip().split()[0]) if value.strip() else 0
    except (OSError, ValueError, IndexError) as e:
        return {"ok": True, "detail": f"读不到内存信息: {e}"}

    total_mb = info.get("MemTotal", 0) // 1024
    available_mb = info.get("MemAvailable", 0) // 1024
    swap_free_mb = info.get("SwapFree", 0) // 1024
    ok = available_mb >= MEMORY_LOW_MB
    return {
        "ok": ok,
        "total_mb": total_mb,
        "available_mb": available_mb,
        "swap_free_mb": swap_free_mb,
        "detail": None if ok else
        f"可用内存只剩 {available_mb} MB（机器快满了，内核会开始杀进程）",
    }


def _check_http(url: str, timeout: float = 2.0) -> dict:
    """Reach an endpoint and see whether it answers.

    The httpx2 package is the one the OpenAI SDK already pulls in, so
    using it here means the service depends on one HTTP client rather
    than two.
    """
    import httpx2 as httpx

    started = time.monotonic()
    try:
        r = httpx.get(url, timeout=timeout)
        return {
            "ok": r.status_code < 500,
            "status": r.status_code,
            "ms": int((time.monotonic() - started) * 1000),
        }
    except Exception as e:
        return {"ok": False, "detail": f"{type(e).__name__}: {str(e)[:80]}"}


def check_health() -> dict:
    """Check every dependency and summarise.

    ``status`` is "ok" when nothing is broken, "degraded" when the
    service still works but something is wrong, and "error" when it
    cannot.
    """
    from .deps import state

    checks: dict[str, dict] = {}

    data_dir = state.get_chat_history().data_dir if hasattr(state.get_chat_history(), "data_dir") else None
    if data_dir is None:
        from .deps import DATA_DIR

        data_dir = DATA_DIR

    for name in ("file_index", "search", "chat_history", "plans"):
        path = Path(data_dir) / f"{name}.db"
        checks[f"db.{name}"] = _check_db(path)

    from .deps import get_storage_root

    checks["disk"] = _check_disk(Path(get_storage_root()))
    checks["memory"] = check_memory()

    # The vector store is the expensive one: opening it reads the
    # manifest, so it is only touched when the file is there.
    vec_dir = Path(data_dir) / "vector_db"
    checks["vector_store"] = {"ok": vec_dir.exists()}

    # The model endpoints are optional: the service still serves files
    # and search without them, so a failure degrades rather than fails.
    try:
        providers = state.get_chat_history().list_providers()
        main = state.get_chat_history().get_main_provider()
        if main:
            base = main.base_url.rstrip("/")
            if base.endswith("/chat/completions"):
                base = base[: -len("/chat/completions")]
            checks["llm"] = _check_http(base + "/health", timeout=3.0)
        checks["providers"] = {"ok": True, "count": len(providers)}
    except Exception as e:
        checks["llm"] = {"ok": False, "detail": f"{type(e).__name__}: {str(e)[:100]}"}

    try:
        emb = state.get_chat_history().get_index_embedding_provider()
        if emb:
            base = emb.base_url.rstrip("/")
            if base.endswith("/embeddings"):
                base = base[: -len("/embeddings")]
            checks["embedding"] = _check_http(base + "/health", timeout=3.0)
    except Exception as e:
        checks["embedding"] = {"ok": False, "detail": f"{type(e).__name__}: {str(e)[:100]}"}


    broken = [k for k, v in checks.items() if not v.get("ok")]
    degraded = [
        k for k in broken if k.startswith(("llm", "embedding", "vector_store", "memory"))
    ]
    hard = [k for k in broken if k not in degraded]

    if hard:
        status = "error"
    elif broken:
        status = "degraded"
    else:
        status = "ok"

    return {
        "status": status,
        "app": "SimpleFileManager",
        "version": "0.4.0",
        "checks": checks,
        "broken": broken,
    }
