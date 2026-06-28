import asyncio
import json
from datetime import datetime
from pathlib import Path
from typing import Optional
from dataclasses import dataclass, asdict
import threading

from .config import (
    SEED_CITIES, MATRIX_MAX_OFFSET, MATRIX_MAX_DURATION,
    MATRIX_CONCURRENCY, MATRIX_CACHE_DIR, REFRESH_ENABLED
)
from .matrix import plan_matrix, MatrixCell


@dataclass
class RefreshState:
    is_running: bool = False
    last_run: Optional[str] = None
    cities_completed: int = 0
    cities_total: int = 0


_state = RefreshState()
_state_lock = threading.Lock()


def get_cache_path(city: str) -> Path:
    safe_city = "".join(c for c in city if c.isalnum() or '\u4e00' <= c <= '\u9fff')
    return MATRIX_CACHE_DIR / f"matrix_{safe_city}.json"


def save_matrix_to_cache(city: str, cells: list[MatrixCell]) -> None:
    path = get_cache_path(city)
    data = {
        "city": city,
        "generated_at": datetime.now().isoformat(),
        "cells": [c.to_dict() for c in cells]
    }
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)


def load_matrix_from_cache(city: str) -> Optional[dict]:
    path = get_cache_path(city)
    if not path.exists():
        return None
    try:
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    except Exception:
        return None


async def refresh_city(city: str, progress_callback=None) -> dict:
    cache_path = get_cache_path(city)

    def on_progress(done: int, total: int, cell: MatrixCell):
        if progress_callback:
            progress_callback(city, done, total, cell)

    cells = await plan_matrix(
        city=city,
        max_start_offset=MATRIX_MAX_OFFSET,
        max_duration=MATRIX_MAX_DURATION,
        concurrency=MATRIX_CONCURRENCY,
        lite=True,
        checkpoint_path=cache_path,
        on_progress=on_progress,
    )

    save_matrix_to_cache(city, cells)
    return {
        "city": city,
        "generated_at": datetime.now().isoformat(),
        "cells": [c.to_dict() for c in cells],
        "total": len(cells),
        "success_count": sum(1 for c in cells if c.success)
    }


async def refresh_all_cities(progress_callback=None) -> list[dict]:
    results = []
    with _state_lock:
        _state.is_running = True
        _state.cities_total = len(SEED_CITIES)
        _state.cities_completed = 0

    for i, city in enumerate(SEED_CITIES):
        with _state_lock:
            _state.cities_completed = i
        try:
            result = await refresh_city(city, progress_callback)
            results.append(result)
        except Exception as e:
            results.append({"city": city, "error": str(e)})

    with _state_lock:
        _state.is_running = False
        _state.last_run = datetime.now().isoformat()
        _state.cities_completed = len(SEED_CITIES)

    return results


def get_refresh_state() -> RefreshState:
    with _state_lock:
        return RefreshState(
            is_running=_state.is_running,
            last_run=_state.last_run,
            cities_completed=_state.cities_completed,
            cities_total=_state.cities_total
        )


async def initial_load() -> None:
    """Load cached data for all cities on startup"""
    for city in SEED_CITIES:
        cached = load_matrix_from_cache(city)
        if cached:
            print(f"[refresh] 已加载缓存: {city}")


_background_task: Optional[asyncio.Task] = None


async def start_background_refresh(interval_seconds: int = 3600) -> None:
    global _background_task
    if _background_task is not None and not _background_task.done():
        return

    async def loop():
        while True:
            await refresh_all_cities()
            await asyncio.sleep(interval_seconds)

    _background_task = asyncio.create_task(loop())
