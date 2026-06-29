import asyncio
import json
import hashlib
from datetime import date, timedelta
from dataclasses import dataclass, asdict, field
from pathlib import Path
from typing import Optional

from .planner import TripPlanner
from .weather import fetch_weather, compute_weather_hash
from .cities import lookup_city


@dataclass
class MatrixCell:
    start_offset: int
    duration: int
    start_date: str
    end_date: str
    score: Optional[int] = None
    recommendation: Optional[str] = None
    weather_summary: Optional[str] = None
    success: bool = False
    error: Optional[str] = None
    full_result: Optional[dict] = None
    input_metadata: Optional[dict] = None
    skipped: bool = False  # 命中 metadata 缓存

    def to_dict(self) -> dict:
        return asdict(self)


def _build_input_metadata(
    weather_list,
    start_str: str,
    duration: int,
    model: str,
    lite: bool,
) -> dict:
    """构建影响本次生成结果的输入快照。

    任何字段变化都会导致 cache miss，需要重新调 LLM。
    """
    return {
        "start_date": start_str,
        "duration": duration,
        "model": model,
        "lite": lite,
        "weather_hash": compute_weather_hash(weather_list),
    }


def _metadata_fingerprint(meta: dict) -> str:
    """基于 metadata dict 计算稳定 fingerprint（用于快速比对）。"""
    payload = json.dumps(meta, sort_keys=True, ensure_ascii=False)
    return hashlib.sha1(payload.encode("utf-8")).hexdigest()[:16]


def _load_cached_metadata(city: str) -> dict[tuple[str, int], dict]:
    """从 DB 读取该城市所有 cell 的 input_metadata，key=(start_date, duration)。"""
    try:
        from .db import get_conn
        conn = get_conn()
        rows = conn.execute(
            "SELECT start_date, duration, input_metadata FROM trip_matrix_cache WHERE city=?",
            (city,),
        ).fetchall()
        result = {}
        for sd, dur, raw in rows:
            if not raw:
                continue
            try:
                result[(sd, dur)] = json.loads(raw)
            except Exception:
                continue
        return result
    except Exception:
        return {}


async def _plan_cell(
    planner: TripPlanner,
    city: str,
    start_offset: int,
    duration: int,
    today: date,
    cached_meta: dict[tuple[str, int], dict],
    weather_list: Optional[list] = None,
) -> MatrixCell:
    start = today + timedelta(days=start_offset)
    end = start + timedelta(days=duration - 1)
    start_str = start.strftime("%Y-%m-%d")
    end_str = end.strftime("%Y-%m-%d")
    key = (start_str, duration)

    coords = lookup_city(city)
    if coords is None:
        cell = MatrixCell(
            start_offset=start_offset, duration=duration,
            start_date=start_str, end_date=end_str,
            success=False, error=f"未找到城市坐标: {city}",
        )
        return cell

    if weather_list is None:
        try:
            weather_list = fetch_weather(coords[0], coords[1], start_str, end_str)
        except Exception as e:
            cell = MatrixCell(
                start_offset=start_offset, duration=duration,
                start_date=start_str, end_date=end_str,
                success=False, error=f"天气查询失败: {e}",
            )
            return cell

    metadata = _build_input_metadata(
        weather_list, start_str, duration, planner.model, planner.lite,
    )
    cached = cached_meta.get(key)
    if cached == metadata:
        return MatrixCell(
            start_offset=start_offset, duration=duration,
            start_date=start_str, end_date=end_str,
            success=True, skipped=True, input_metadata=metadata,
        )

    loop = asyncio.get_event_loop()
    result = await loop.run_in_executor(
        None, lambda: planner.plan(city, start_str, end_str, preset_weather=weather_list)
    )

    cell = MatrixCell(
        start_offset=start_offset,
        duration=duration,
        start_date=start_str,
        end_date=end_str,
        success=result.success,
        error=result.error,
        input_metadata=metadata,
    )

    if result.success and result.data:
        cell.score = result.data.get("score")
        cell.recommendation = result.data.get("recommendation")
        if result.weather_forecast:
            cell.weather_summary = " | ".join(
                f"{w['date']} {w['weather_desc']}" for w in result.weather_forecast
            )
        cell.full_result = result.to_dict()

    return cell


async def _plan_cell(
    planner: TripPlanner,
    city: str,
    start_offset: int,
    duration: int,
    today: date,
) -> MatrixCell:
    start = today + timedelta(days=start_offset)
    end = start + timedelta(days=duration - 1)
    start_str = start.strftime("%Y-%m-%d")
    end_str = end.strftime("%Y-%m-%d")

    loop = asyncio.get_event_loop()
    result = await loop.run_in_executor(None, planner.plan, city, start_str, end_str)

    cell = MatrixCell(
        start_offset=start_offset,
        duration=duration,
        start_date=start_str,
        end_date=end_str,
        success=result.success,
        error=result.error,
    )

    if result.success and result.data:
        cell.score = result.data.get("score")
        cell.recommendation = result.data.get("recommendation")
        if result.weather_forecast:
            cell.weather_summary = " | ".join(
                f"{w['date']} {w['weather_desc']}" for w in result.weather_forecast
            )
        cell.full_result = result.to_dict()

    return cell


def _save_checkpoint(cells: list[MatrixCell], path: Path) -> None:
    data = [c.to_dict() for c in cells]
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)


def _load_checkpoint(path: Path) -> list[MatrixCell]:
    if not path.exists():
        return []
    try:
        with open(path, "r", encoding="utf-8") as f:
            data = json.load(f)
        return [MatrixCell(**d) for d in data]
    except Exception:
        return []


async def plan_matrix(
    city: str,
    max_start_offset: int = 7,
    max_duration: int = 5,
    concurrency: int = 1,
    lite: bool = True,
    checkpoint_path: Optional[Path] = None,
    on_progress: Optional[callable] = None,
    use_cache: bool = True,
) -> list[MatrixCell]:
    planner = TripPlanner(lite=lite)
    today = date.today()

    cached_meta: dict[tuple[str, int], dict] = {}
    if use_cache:
        cached_meta = _load_cached_metadata(city)

    all_combos = [
        (offset, duration)
        for offset in range(1, max_start_offset + 1)
        for duration in range(1, max_duration + 1)
    ]
    total = len(all_combos)

    done_cells: list[MatrixCell] = []
    done_keys: set[tuple[int, int]] = set()
    if checkpoint_path:
        done_cells = _load_checkpoint(checkpoint_path)
        done_keys = {(c.start_offset, c.duration) for c in done_cells}
        if done_cells:
            print(f"从断点恢复: 已完成 {len(done_cells)}/{total} 格")

    pending = [(o, d) for o, d in all_combos if (o, d) not in done_keys]

    if concurrency <= 1:
        results = list(done_cells)
        completed = len(done_cells)
        for offset, duration in pending:
            sd = (today + timedelta(days=offset)).strftime("%Y-%m-%d")
            tag = "skip" if sd in {k[0] for k in cached_meta if k[1] == duration} else "plan"
            print(f"  -> +{offset}d {duration}d [{tag}]", flush=True)
            cell = await _plan_cell(planner, city, offset, duration, today, cached_meta)
            completed += 1
            if on_progress:
                on_progress(completed, total, cell)
            results.append(cell)
            if checkpoint_path:
                _save_checkpoint(results, checkpoint_path)
        return results

    semaphore = asyncio.Semaphore(concurrency)
    counter = [len(done_cells)]

    async def bounded(offset: int, duration: int) -> MatrixCell:
        async with semaphore:
            cell = await _plan_cell(planner, city, offset, duration, today, cached_meta)
            counter[0] += 1
            if on_progress:
                on_progress(counter[0], total, cell)
            return cell

    tasks = [bounded(o, d) for o, d in pending]
    new_results = await asyncio.gather(*tasks)
    results = done_cells + new_results
    if checkpoint_path:
        _save_checkpoint(results, checkpoint_path)
    return results
