from datetime import datetime, date
from fastapi import APIRouter, HTTPException
from typing import Optional

from src.distance import haversine_km, estimate_travel_time, transport_score, lookup_origin
from src.cities import lookup_city

from .models import (
    CityListResponse, CityMatrixResponse, MatrixCellResponse,
    HealthResponse, RefreshStatusResponse
)
from .search_models import SearchRequest, SearchResponse, SearchResultItem
from .refresh import (
    load_matrix_from_cache, refresh_all_cities,
    get_refresh_state, SEED_CITIES, REFRESH_ENABLED
)


PREFERENCE_KEYWORDS = {
    "自然": ["自然", "风光", "山", "水", "森林", "草原", "湖泊", "海边", "海", "沙滩"],
    "人文": ["历史", "文化", "古迹", "博物馆", "寺庙", "古镇", "古城", "文物"],
    "美食": ["美食", "小吃", "夜市", "特色", "当地"],
    "户外": ["登山", "徒步", "骑行", "露营", "滑雪", "漂流"],
    "亲子": ["孩子", "亲子", "儿童", "乐园", "动物园", "海洋馆"],
    "放松": ["温泉", "SPA", "度假", "休闲", "慢生活"],
}


def calc_preference_score(preference: str, plan_data: dict, city_tags: list[str]) -> float:
    """
    TODO: 当前为简单字符匹配，未来可升级为：
    1. Embedding 向量相似度匹配（使用 sentence-transformers 等本地模型）
    2. LLM 理解用户意图后匹配
    3. 预计算的 city/plan embedding 存储，查询时直接用余弦相似度
    """
    if not preference or not preference.strip():
        return 0.0

    pref_lower = preference.lower()
    score = 0.0
    matched = 0

    for category, keywords in PREFERENCE_KEYWORDS.items():
        for kw in keywords:
            if kw in pref_lower:
                matched += 1
                break

    if matched > 0:
        score = min(matched / 3.0, 1.0)

    return score


def rank_by_preference(results: list[SearchResultItem], preference: str, plan_data_map: dict) -> list[SearchResultItem]:
    """根据偏好重新排序"""
    if not preference or not preference.strip():
        return results

    for item in results:
        plan_data = plan_data_map.get(item.city, {})
        item.preference_score = calc_preference_score(
            preference,
            plan_data,
            plan_data.get("city_tags", [])
        )

    def sort_key(x: SearchResultItem) -> tuple:
        pref_boost = x.preference_score * 20
        return (x.score + pref_boost, x.preference_score)

    return sorted(results, key=sort_key, reverse=True)


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

    results: list[SearchResultItem] = []
    plan_data_map: dict = {}

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

                # 真实距离 + 重新计算 transport 维度分数
                origin_coord = lookup_origin(req.origin) or lookup_origin("北京")
                city_coord = lookup_city(city)
                if origin_coord and city_coord:
                    distance = haversine_km(origin_coord, city_coord)
                    transit_info = estimate_travel_time(distance)
                    new_transport_score = transport_score(distance, cell["duration"])
                    # 替换原有 breakdown.transport
                    breakdown = dict(plan_data.get("score_breakdown") or {})
                    breakdown["transport"] = new_transport_score
                else:
                    distance = 0
                    transit_info = {"recommended_mode": "", "transit_hours": 0}
                    breakdown = plan_data.get("score_breakdown") or {}

                # 重新综合 score（保持 40/25/25/10 权重）
                base_score = plan_data.get("score") or cell.get("score") or 0
                # 如果有完整 breakdown，重算
                if breakdown and all(k in breakdown for k in ["days_match", "weather", "attraction_density", "transport"]):
                    base_score = int(
                        breakdown["days_match"] * 0.40
                        + breakdown["transport"] * 0.25
                        + breakdown["weather"] * 0.25
                        + breakdown["attraction_density"] * 0.10
                    )

                results.append(SearchResultItem(
                    city=city,
                    start_date=cell["start_date"],
                    end_date=cell["end_date"],
                    duration_days=cell["duration"],
                    score=base_score,
                    recommendation=plan_data.get("recommendation") or cell.get("recommendation") or "",
                    weather_summary=cell.get("weather_summary") or "",
                    weather_desc=weather_desc,
                    top_attractions=top_attractions,
                    key_highlights=plan_data.get("key_highlights") or plan_data.get("city_intro", "")[:100],
                    score_breakdown=breakdown,
                    daily_plan=plan_data.get("daily_plan") or [],
                    distance_km=distance,
                    transport_mode=transit_info.get("recommended_mode", ""),
                    transit_hours=transit_info.get("transit_hours", 0),
                ))
                plan_data_map[city] = plan_data

    results = rank_by_preference(results, req.preference or "", plan_data_map)
    results.sort(key=lambda x: x.score + (x.preference_score or 0) * 20, reverse=True)

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
