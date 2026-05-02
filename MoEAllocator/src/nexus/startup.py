import importlib.metadata
import logging
import os
import platform
import sys
from pathlib import Path
from typing import Any, Iterable

import torch


def package_version() -> str:
    try:
        return importlib.metadata.version("moe-allocator")
    except importlib.metadata.PackageNotFoundError:
        return "0.1.0"


def format_ids(ids: Iterable[int] | str | None) -> str:
    if ids is None:
        return "none"
    if isinstance(ids, str):
        return ids if ids else "none"
    values = list(ids)
    if not values:
        return "none"
    if len(values) <= 16:
        return ",".join(str(x) for x in values)
    return f"{values[0]}..{values[-1]} ({len(values)} ids)"


def path_status(path: str | Path) -> str:
    if not path:
        return "none"
    p = Path(path)
    state = "exists" if p.exists() else "missing"
    return f"{p} ({state})"


def normalize_master_register_url(master_url: str) -> str:
    url = master_url.rstrip("/")
    if not url:
        return ""
    if url.endswith("/workers"):
        return url
    return f"{url}/workers"


def log_kv(logger: logging.Logger, prefix: str, items: Iterable[tuple[str, Any]]) -> None:
    rows = [(key, "none" if value is None or value == "" else value) for key, value in items]
    if not rows:
        return
    width = max(len(key) for key, _ in rows)
    for key, value in rows:
        logger.info("%s: %-*s = %s", prefix, width, key, value)


def log_runtime_info(logger: logging.Logger, role: str) -> None:
    logger.info("moe_allocator_init: version = %s", package_version())
    log_kv(
        logger,
        "system_info",
        [
            ("role", role),
            ("pid", os.getpid()),
            ("cwd", Path.cwd()),
            ("python", sys.version.split()[0]),
            ("platform", f"{platform.system()} {platform.release()} {platform.machine()}"),
            ("cpu_count", os.cpu_count()),
        ],
    )
    log_kv(
        logger,
        "torch_info",
        [
            ("torch", torch.__version__),
            ("threads", torch.get_num_threads()),
            ("interop_threads", torch.get_num_interop_threads()),
            ("cuda", _cuda_info()),
            ("mps", _mps_info()),
        ],
    )


def _cuda_info() -> str:
    if not torch.cuda.is_available():
        return "disabled"
    devices = []
    for idx in range(torch.cuda.device_count()):
        props = torch.cuda.get_device_properties(idx)
        mem_gib = props.total_memory / (1 << 30)
        devices.append(f"{idx}:{props.name} ({mem_gib:.1f} GiB, sm_{props.major}{props.minor})")
    return ", ".join(devices)


def _mps_info() -> str:
    mps = getattr(torch.backends, "mps", None)
    if mps is None:
        return "unavailable"
    return "available" if mps.is_available() else "disabled"
