import asyncio
import json
from datetime import date, datetime, timedelta
from dataclasses import dataclass, asdict, field
from pathlib import Path
from typing import Optional
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from src.planner import TripPlanner


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

    def to_dict(self) -> dict:
        return asdict(self)


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


_ondemand_lock = asyncio.Lock()


async def generate_single_cell(
    city: str,
    start_date_str: str,
    end_date_str: str,
) -> MatrixCell:
    """为指定城市+日期范围生成单个 matrix cell (on-demand 用在搜索端点)。结果自动写入 SQLite cache。"""
    from src.planner import TripPlanner
    planner = TripPlanner(lite=True)
    loop = asyncio.get_event_loop()

    result = await loop.run_in_executor(None, planner.plan, city, start_date_str, end_date_str)

    import json as _json
    cell = MatrixCell(
        start_offset=0,
        duration=result.duration_days,
        start_date=start_date_str,
        end_date=end_date_str,
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

    # 回写 SQLite cache
    async with _ondemand_lock:
        try:
            from src.db import get_conn
            conn = get_conn()
            full_json = _json.dumps(cell.full_result, ensure_ascii=False) if cell.full_result else None
            generated_at = datetime.now().isoformat()
            conn.execute(
                """INSERT OR REPLACE INTO trip_matrix_cache
                   (city, start_date, duration, end_date,
                    score, recommendation, weather_summary, full_result, input_metadata, generated_at)
                   VALUES (?,?,?,?,?,?,?,?,?,?)""",
                (city, cell.start_date, cell.duration, cell.end_date,
                 cell.score, cell.recommendation, cell.weather_summary,
                 full_json, None, generated_at),
            )
            conn.commit()
        except Exception as e:
            print(f"[matrix] on-demand cache write failed for {city}: {e}")

    return cell


async def plan_matrix(
    city: str,
    max_start_offset: int = 7,
    max_duration: int = 5,
    concurrency: int = 3,
    lite: bool = True,
    checkpoint_path: Optional[Path] = None,
    on_progress: Optional[callable] = None,
) -> list[MatrixCell]:
    planner = TripPlanner(lite=lite)
    today = date.today()

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
            print(f"[matrix] 从断点恢复: 已完成 {len(done_cells)}/{total} 格")

    pending = [(o, d) for o, d in all_combos if (o, d) not in done_keys]

    if concurrency <= 1:
        results = list(done_cells)
        completed = len(done_cells)
        for offset, duration in pending:
            cell = await _plan_cell(planner, city, offset, duration, today)
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
            cell = await _plan_cell(planner, city, offset, duration, today)
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
