import asyncio
import json
import time
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


def _get_seed_cities() -> list[str]:
    """从 DB 读最新 seed cities，fallback 到 config。"""
    try:
        from src.db import get_seed_cities as db_get
        cities = db_get()
        if cities:
            return cities
    except Exception:
        pass
    return SEED_CITIES


@dataclass
class RefreshState:
    is_running: bool = False
    last_run: Optional[str] = None
    cities_completed: int = 0
    cities_total: int = 0


_state = RefreshState()
_state_lock = threading.Lock()


def get_cache_path(city: str) -> Path:
    """JSON cache path（兼容旧版，新版写入 SQLite）。"""
    safe_city = "".join(c for c in city if c.isalnum() or '\u4e00' <= c <= '\u9fff')
    return MATRIX_CACHE_DIR / f"matrix_{safe_city}.json"


def save_matrix_to_cache(city: str, cells: list[MatrixCell]) -> None:
    """保存 matrix 到 SQLite（同时备份 JSON 防止迁移期数据丢失）。"""
    import sqlite3

    generated_at = datetime.now().isoformat()
    rows = []
    for c in cells:
        full_json = json.dumps(c.full_result, ensure_ascii=False) if c.full_result else None
        rows.append((
            city,
            c.start_offset,
            c.duration,
            c.start_date,
            c.end_date,
            c.score,
            c.recommendation,
            c.weather_summary,
            full_json,
            generated_at,
        ))

    # 写入 SQLite（唯一数据源）
    try:
        from src.db import get_conn
        conn = get_conn()
        cur = conn.cursor()
        cur.execute(
            "DELETE FROM trip_matrix_cache WHERE city=?",
            (city,),
        )
        cur.executemany(
            """INSERT OR REPLACE INTO trip_matrix_cache
               (city, start_offset, duration, start_date, end_date,
                score, recommendation, weather_summary, full_result, generated_at)
               VALUES (?,?,?,?,?,?,?,?,?,?)""",
            rows,
        )
        conn.commit()
    except Exception as e:
        print(f"[refresh] DB write failed for {city}: {e}")


def load_matrix_from_cache(city: str) -> Optional[dict]:
    """从 SQLite 读取 matrix cache（唯一数据源）。"""
    import sqlite3
    from src.db import get_conn
    conn = get_conn()
    conn.row_factory = sqlite3.Row
    rows = conn.execute(
        """SELECT start_offset, duration, start_date, end_date, score,
                  recommendation, weather_summary, full_result, generated_at
           FROM trip_matrix_cache WHERE city=? ORDER BY start_offset, duration""",
        (city,),
    ).fetchall()

    if not rows:
        return None

    cells = []
    for r in rows:
        cells.append({
            "start_offset": r["start_offset"],
            "duration": r["duration"],
            "start_date": r["start_date"],
            "end_date": r["end_date"],
            "score": r["score"],
            "recommendation": r["recommendation"],
            "weather_summary": r["weather_summary"],
            "success": True,
            "full_result": json.loads(r["full_result"]) if r["full_result"] else None,
        })
    return {
        "city": city,
        "generated_at": rows[0]["generated_at"],
        "cells": cells,
    }


async def refresh_city(city: str, progress_callback=None) -> dict:
    """重新生成指定城市的 matrix。返回带统计的字典（含耗时、成功数）。"""
    cache_path = get_cache_path(city)
    started_at = datetime.now().isoformat()
    t0 = time.time()

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

    finished_at = datetime.now().isoformat()
    duration = round(time.time() - t0, 1)

    try:
        from src.db import log_generation
        log_generation(
            city=city,
            started_at=started_at,
            finished_at=finished_at,
            cells_total=len(cells),
            cells_success=sum(1 for c in cells if c.success),
            duration=duration,
            source="manual",
        )
    except Exception:
        pass

    return {
        "city": city,
        "generated_at": finished_at,
        "cells": [c.to_dict() for c in cells],
        "total": len(cells),
        "success_count": sum(1 for c in cells if c.success),
        "duration_seconds": duration,
    }


async def refresh_all_cities(progress_callback=None) -> list[dict]:
    results = []
    cities = _get_seed_cities()
    with _state_lock:
        _state.is_running = True
        _state.cities_total = len(cities)
        _state.cities_completed = 0

    for i, city in enumerate(cities):
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
