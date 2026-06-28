from datetime import datetime, date
from fastapi import APIRouter, HTTPException, Query
from typing import Optional

from .models import (
    CityListResponse, CityMatrixResponse, MatrixCellResponse,
    HealthResponse, RefreshStatusResponse
)
from .search_models import SearchRequest, SearchResponse, SearchResultItem
from .refresh import (
    load_matrix_from_cache, refresh_all_cities,
    get_refresh_state, SEED_CITIES, REFRESH_ENABLED
)


router = APIRouter()


@router.get("/health", response_model=HealthResponse)
async def health():
    return HealthResponse(
        status="ok",
        refresh_enabled=REFRESH_ENABLED,
        seed_cities=SEED_CITIES
    )


@router.get("/cities", response_model=CityListResponse)
async def list_cities():
    return CityListResponse(cities=SEED_CITIES, count=len(SEED_CITIES))


@router.post("/search", response_model=SearchResponse)
async def search_travel_plans(req: SearchRequest):
    try:
        start = datetime.strptime(req.start_date, "%Y-%m-%d")
        end = datetime.strptime(req.end_date, "%Y-%m-%d")
    except ValueError:
        raise HTTPException(status_code=400, detail="日期格式错误，请使用 YYYY-MM-DD")

    if end < start:
        raise HTTPException(status_code=400, detail="返回日期不能早于出发日期")

    duration = (end - start).days + 1
    today = date.today()
    start_offset = (start.date() - today).days
    end_offset = (end.date() - today).days

    results: list[SearchResultItem] = []

    for city in SEED_CITIES:
        cached = load_matrix_from_cache(city)
        if not cached:
            continue

        for cell in cached.get("cells", []):
            if not cell.get("success"):
                continue

            cell_start = datetime.strptime(cell["start_date"], "%Y-%m-%d").date()
            cell_end = datetime.strptime(cell["end_date"], "%Y-%m-%d").date()

            if cell_start == start.date() and cell_end == end.date():
                full = cell.get("full_result") or {}
                plan_data = full.get("data") or {}
                weather_parts = (cell.get("weather_summary") or "").split("|")
                weather_desc = weather_parts[0].strip() if weather_parts else ""

                raw_attractions = plan_data.get("top_attractions") or plan_data.get("attractions") or []
                if raw_attractions and isinstance(raw_attractions[0], dict):
                    top_attractions = [a.get("name", "") for a in raw_attractions[:5]]
                else:
                    top_attractions = raw_attractions[:5]

                results.append(SearchResultItem(
                    city=city,
                    start_date=cell["start_date"],
                    end_date=cell["end_date"],
                    duration_days=cell["duration"],
                    score=plan_data.get("score") or cell.get("score") or 0,
                    recommendation=plan_data.get("recommendation") or cell.get("recommendation") or "",
                    weather_summary=cell.get("weather_summary") or "",
                    weather_desc=weather_desc,
                    top_attractions=top_attractions,
                    key_highlights=plan_data.get("key_highlights") or plan_data.get("city_intro", "")[:100],
                    score_breakdown=plan_data.get("score_breakdown") or {},
                    daily_plan=plan_data.get("daily_plan") or [],
                ))

    results.sort(key=lambda x: x.score, reverse=True)

    return SearchResponse(
        items=results,
        total=len(results),
        generated_at=datetime.now().isoformat(),
    )


@router.get("/cities/{city}", response_model=CityMatrixResponse)
async def get_city_matrix(city: str):
    if city not in SEED_CITIES:
        raise HTTPException(status_code=404, detail=f"城市 {city} 不在种子列表中")

    cached = load_matrix_from_cache(city)
    if cached:
        return CityMatrixResponse(
            city=cached["city"],
            generated_at=cached["generated_at"],
            cells=[MatrixCellResponse(**c) for c in cached["cells"]],
            total=len(cached["cells"]),
            success_count=sum(1 for c in cached["cells"] if c.get("success", False))
        )

    raise HTTPException(status_code=404, detail=f"城市 {city} 暂无数据，请稍后刷新")


@router.post("/refresh")
async def trigger_refresh():
    state = get_refresh_state()
    if state.is_running:
        raise HTTPException(status_code=409, detail="刷新任务正在进行中")

    import asyncio
    asyncio.create_task(refresh_all_cities())
    return {"message": "刷新任务已启动", "cities": SEED_CITIES}


@router.get("/refresh/status", response_model=RefreshStatusResponse)
async def get_refresh_status():
    state = get_refresh_state()
    return RefreshStatusResponse(
        is_running=state.is_running,
        last_run=state.last_run,
        cities_completed=state.cities_completed,
        cities_total=state.cities_total
    )
