from datetime import datetime, date, timedelta
from fastapi import APIRouter, HTTPException, Query, Depends, Header, Request
from typing import Optional

from src.distance import haversine_km, estimate_travel_time, transport_score, lookup_origin, lookup_origin_from_json
from src.cities import lookup_city
from src.holidays import get_holiday_impact, list_holidays_in_range, calculate_holiday_insights
from src.auth import (
    create_user, authenticate, create_session, verify_session,
    delete_session, get_user_by_id,
)

from .models import (
    CityListResponse, CityMatrixResponse, MatrixCellResponse,
    HealthResponse, RefreshStatusResponse
)
from .search_models import SearchRequest, SearchResponse, SearchResultItem
from .refresh import (
    load_matrix_from_cache, refresh_all_cities,
    get_refresh_state, SEED_CITIES, REFRESH_ENABLED
)
from src.db import get_seed_cities as db_get_seed_cities
from src.db import get_attraction_by_name
from .deps import get_current_user, require_user, require_admin


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
    """健康检查 + 系统概览。"""
    from src.db import get_overview_stats
    stats = get_overview_stats()
    return HealthResponse(
        status="ok",
        refresh_enabled=REFRESH_ENABLED,
        seed_cities=db_get_seed_cities(),
        cached_cities=stats["cached_cities"],
        cells_total=stats["cells_total"],
        cache_hit_rate=stats["cache_hit_rate"],
    )


@router.get("/cities", response_model=CityListResponse)
async def list_cities():
    return CityListResponse(cities=db_get_seed_cities(), count=len(db_get_seed_cities()))


@router.post("/search", response_model=SearchResponse)
async def search_travel_plans(req: SearchRequest, request: Request):
    from .rate_limit import search_limiter

    # 限流：按 IP 限制
    client_ip = request.client.host if request.client else "unknown"
    allowed, retry_after = search_limiter.check(client_ip)
    if not allowed:
        raise HTTPException(
            status_code=429,
            detail=f"请求过于频繁，请 {retry_after} 秒后重试",
            headers={"Retry-After": str(retry_after)}
        )

    try:
        start = datetime.strptime(req.start_date, "%Y-%m-%d")
        end = datetime.strptime(req.end_date, "%Y-%m-%d")
    except ValueError:
        raise HTTPException(status_code=400, detail="日期格式错误，请使用 YYYY-MM-DD")

    if end < start:
        raise HTTPException(status_code=400, detail="返回日期不能早于出发日期")

    results: list[SearchResultItem] = []
    plan_data_map: dict = {}

    for city in db_get_seed_cities():
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

                # 优先用 DB 景点库覆盖 LLM 输出（如果 LLM 的景点在 DB 里有真实记录）
                if top_attractions:
                    from src.db import get_attraction_by_name
                    verified = []
                    for name in top_attractions:
                        att = get_attraction_by_name(name, city)
                        if att:
                            verified.append(att["name"])
                    # 如果 LLM 给的景点全部不在 DB 里，fallback 用 DB 全部景点
                    if not verified:
                        from src.db import get_city_attractions
                        db_atts = get_city_attractions(city, limit=5)
                        verified = [a["name"] for a in db_atts]
                    top_attractions = verified

                # 真实距离 + 重新计算 transport 维度分数
                origin_coord = lookup_origin_from_json(
                    req.origin_province or "",
                    req.origin_city or "",
                    req.origin_county or "",
                ) or lookup_origin(req.origin_city or "") or lookup_origin("北京")
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
                else:
                    base_score = plan_data.get("score") or cell.get("score") or 0

                # 应用节假日双向影响（仅作提示，不调整基础评分）
                # 用户已选日期，只提供人流/活动洞察，不影响推荐分数
                _holiday_insights = calculate_holiday_insights(  # noqa: F841
                    cell["start_date"], cell["end_date"]
                )

                # 验证 daily_plan 里的活动景点，标注是否在 DB 中
                daily_plan = plan_data.get("daily_plan") or []
                for day in daily_plan:
                    for route in day.get("routes", []):
                        for act in route.get("activities", []):
                            att_name = act.get("attraction", "")
                            if att_name:
                                att = get_attraction_by_name(att_name, city)
                                act["verified"] = att is not None
                                if att:
                                    act["attraction_id"] = att["id"]

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


@router.get("/holidays")
async def get_holidays(
    start_date: str = Query(..., description="起始日期 YYYY-MM-DD"),
    end_date: str = Query(..., description="结束日期 YYYY-MM-DD"),
):
    """查询日期范围内的节假日洞察（人流/活动/价格/提示）。"""
    try:
        datetime.strptime(start_date, "%Y-%m-%d")
        datetime.strptime(end_date, "%Y-%m-%d")
    except ValueError:
        raise HTTPException(status_code=400, detail="日期格式错误")

    if end_date < start_date:
        raise HTTPException(status_code=400, detail="结束日期不能早于起始日期")

    insights = calculate_holiday_insights(start_date, end_date)
    return {
        "start_date": start_date,
        "end_date": end_date,
        **insights,
    }


@router.get("/cities/{city}", response_model=CityMatrixResponse)
async def get_city_matrix(city: str):
    if city not in db_get_seed_cities():
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
    return {"message": "刷新任务已启动", "cities": db_get_seed_cities()}


@router.get("/refresh/status", response_model=RefreshStatusResponse)
async def get_refresh_status():
    state = get_refresh_state()
    return RefreshStatusResponse(
        is_running=state.is_running,
        last_run=state.last_run,
        cities_completed=state.cities_completed,
        cities_total=state.cities_total
    )


# =================== Regions API ===================

@router.get("/regions")
async def get_regions():
    """返回所有省/市/县数据。供前端省市县选择器使用。"""
    import sqlite3
    from pathlib import Path
    db_path = Path(__file__).resolve().parent.parent / "data" / "travelsites.db"
    conn = sqlite3.connect(str(db_path))
    conn.row_factory = sqlite3.Row
    provinces = conn.execute("SELECT name FROM geo_provinces ORDER BY name").fetchall()
    regions = []
    for p in provinces:
        prov_name = p["name"]
        cities = conn.execute(
            "SELECT name FROM geo_cities WHERE province_name=? ORDER BY name",
            (prov_name,),
        ).fetchall()
        city_list = []
        for c in cities:
            city_name = c["name"]
            counties = conn.execute(
                "SELECT name FROM geo_counties WHERE province_name=? AND city_name=? ORDER BY name",
                (prov_name, city_name),
            ).fetchall()
            city_list.append({
                "name": city_name,
                "counties": [c["name"] for c in counties]
            })
        regions.append({"province": prov_name, "cities": city_list})
    conn.close()
    return {"regions": regions}


# =================== Attractions API ===================

@router.get("/attractions")
async def list_attractions(city: str = Query(..., description="城市名"), limit: int = 20):
    """获取某城市所有景点（按评分排序）。"""
    from src.db import get_city_attractions
    items = get_city_attractions(city, limit)
    return {"city": city, "total": len(items), "items": items}


@router.get("/attractions/search")
async def search_attractions_api(
    q: str = Query(..., description="搜索关键词"),
    city: Optional[str] = Query(None, description="可选城市过滤"),
    limit: int = 20,
):
    """按名字模糊搜索景点。"""
    from src.db import search_attractions
    return {"query": q, "results": search_attractions(q, city, limit)}


@router.get("/attractions/cities")
async def list_cities_with_attractions():
    """返回有景点数据的城市列表。"""
    from src.db import get_cities_with_attractions
    return {"cities": get_cities_with_attractions()}


# =================== Auth API ===================

@router.post("/auth/register")
async def register(payload: dict):
    """注册新用户。body: {username, password, email?, display_name?}"""
    username = payload.get("username", "")
    password = payload.get("password", "")
    email = payload.get("email")
    display_name = payload.get("display_name")

    result = create_user(
        username=username,
        password=password,
        email=email,
        display_name=display_name,
        role="user",
    )
    if "error" in result:
        raise HTTPException(status_code=400, detail=result["error"])

    session = create_session(result["user_id"])
    return {
        "user": result,
        "token": session["token"],
        "expires_at": session["expires_at"],
    }


@router.post("/auth/login")
async def login(payload: dict):
    """登录。body: {username, password}"""
    username = payload.get("username", "")
    password = payload.get("password", "")

    user = authenticate(username, password)
    if not user:
        raise HTTPException(status_code=401, detail="用户名或密码错误")

    session = create_session(user["user_id"])
    return {
        "user": user,
        "token": session["token"],
        "expires_at": session["expires_at"],
    }


@router.post("/auth/logout")
async def logout(user: dict = Depends(require_user), authorization: Optional[str] = Header(None)):
    """登出：销毁当前 session。"""
    if authorization:
        token = authorization.replace("Bearer ", "").strip()
        delete_session(token)
    return {"message": "已登出"}


@router.get("/auth/me")
async def me(user: dict = Depends(require_user)):
    """获取当前登录用户信息。"""
    full = get_user_by_id(user["user_id"])
    if not full:
        raise HTTPException(404, "用户不存在")
    return full


# =================== Admin API ===================
import os as _os
from src.db import set_seed_cities, get_overview_stats, get_recent_generations


@router.get("/admin/overview")
async def admin_overview(_: dict = Depends(require_admin)):
    """系统总览。"""
    return get_overview_stats()


@router.get("/admin/cities")
async def admin_list_cities(_: dict = Depends(require_admin)):
    """当前生效的 seed cities 列表。"""
    from src.db import get_seed_cities
    return {"cities": get_seed_cities()}


@router.put("/admin/cities")
async def admin_update_cities(payload: dict, _: dict = Depends(require_admin)):
    """替换 seed cities 列表。body: {"cities": ["北京", "上海", ...]}"""
    cities = payload.get("cities", [])
    if not isinstance(cities, list) or not all(isinstance(c, str) for c in cities):
        raise HTTPException(400, "cities 必须是非空字符串数组")
    set_seed_cities(cities)
    return {"message": "已更新", "count": len(cities), "cities": cities}


@router.post("/admin/cities/{city}/refresh")
async def admin_refresh_city(city: str, _: dict = Depends(require_admin)):
    """手动触发某城市重新生成。"""
    from src.db import get_seed_cities
    if city not in get_seed_cities():
        raise HTTPException(404, f"城市 {city} 不在 seed 列表中")

    from .refresh import refresh_city
    result = await refresh_city(city)
    return {
        "message": f"{city} 重新生成完成",
        "city": result["city"],
        "success_count": result["success_count"],
        "total": result["total"],
        "duration_seconds": result["duration_seconds"],
    }


@router.get("/admin/logs")
async def admin_get_logs(limit: int = 20, _: dict = Depends(require_admin)):
    """最近的生成日志。"""
    return {"logs": get_recent_generations(limit)}


@router.get("/admin/poi/status")
async def admin_poi_status(_: dict = Depends(require_admin)):
    """POI 数据源状态。"""
    from src.amap_poi import is_enabled
    return {
        "amap_enabled": is_enabled(),
        "message": "高德 POI 已启用" if is_enabled() else "未启用：需在 .env 设置 AMAP_API_KEY=true 且 AMAP_API_KEY=xxx"
    }


@router.post("/admin/poi/import")
async def admin_poi_import(payload: dict, _: dict = Depends(require_admin)):
    """从高德拉景点到 DB。body: {city, type}"""
    from src.amap_poi import is_enabled, import_poi_to_db
    if not is_enabled():
        raise HTTPException(400, "POI 源未启用：需在 .env 设置 AMAP_API_KEY")

    city = payload.get("city", "")
    poi_type = payload.get("type", "风景名胜")
    if not city:
        raise HTTPException(400, "city 必填")

    inserted = await import_poi_to_db(city, poi_type)
    return {
        "message": f"{city} ({poi_type}) 新增 {inserted} 个景点",
        "city": city,
        "type": poi_type,
        "inserted": inserted,
    }
