from datetime import datetime, date, timedelta
import json
from fastapi import APIRouter, HTTPException, Query, Depends, Header, Request
from typing import Optional

from src.distance import calc_distance, transport_score, lookup_city_coord, haversine_km
from src.holidays import get_holiday_impact, list_holidays_in_range, calculate_holiday_insights
from src.weather import fetch_weather, is_extreme_weather, is_rainy
from src.cities import lookup_city
from src.auth import (
    create_user, authenticate, create_session, verify_session,
    delete_session, get_user_by_id,
)

from .models import (
    CityListResponse, CityMatrixResponse, MatrixCellResponse,
    HealthResponse, RefreshStatusResponse
)
from .search_models import SearchRequest, SearchResponse, SearchResultItem, PlanRecommendRequest, PlanRecommendResponse
from .refresh import (
    load_matrix_from_cache, refresh_all_cities,
    get_refresh_state, REFRESH_ENABLED
)
from src.db import get_all_cities, get_all_cities_with_pois
from src.db import get_attraction_by_name, get_conn
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


def _calc_pref_match(preference: str, tags: list[str], blurb: str) -> float:
    """简易偏好匹配（guide 搜索用）。keyword match against tags + blurb。"""
    if not preference or not preference.strip():
        return 0.0
    text = preference.lower()
    matched = 0.0
    for t in tags:
        if t.lower() in text:
            matched += 0.5
    # blurb 匹配
    for word in text.split():
        if len(word) > 1 and word in blurb.lower():
            matched += 0.1
    return min(matched, 1.0)


def _weather_score(weather_list: list) -> float:
    """根据天气预报列表计算 0~1 的天气评分。雨/雷暴越少分越高。"""
    if not weather_list:
        return 0.5
    score = 0.0
    for w in weather_list:
        code = w.get("weather_code", 0)
        if is_extreme_weather(code):
            score += 0.0
        elif is_rainy(code):
            score += 0.4
        else:
            score += 1.0
    return score / len(weather_list)


def _calc_attraction_score(highlights: list, tags: list[str], blurb: str) -> float:
    """
    景点/吸引力评分 (0-100)。
    基于高亮景点数量、标签匹配度、简介内容质量。
    """
    score = 50.0
    if highlights:
        score += min(len(highlights) * 8, 25)
    if tags:
        score += min(len(tags) * 5, 15)
    if blurb and len(blurb) > 50:
        score += 10
    return min(score, 100)


def calc_dynamic_score(
    duration_days: int,
    distance_km: float,
    weather_list: list,
    highlights: list,
    tags: list[str],
    blurb: str,
) -> dict:
    """
    多维度动态评分系统。

    权重分配:
      - 天数合理性 (days): 40%
      - 交通/距离 (transport): 25%
      - 天气 (weather): 25%
      - 景点吸引力 (attraction): 10%

    Returns:
        {
            "total": int,           # 综合评分 0-100
            "breakdown": {
                "days_score": int,    # 天数得分 (0-100)
                "days_weight": 0.4,
                "transport_score": int,  # 交通得分 (0-100)
                "transport_weight": 0.25,
                "weather_score": int,    # 天气得分 (0-100)
                "weather_weight": 0.25,
                "attraction_score": int, # 吸引力得分 (0-100)
                "attraction_weight": 0.1,
                "distance_km": float,
                "transport_hours": float,
                "transport_mode": str,
            }
        }
    """
    days_score = 50
    if 2 <= duration_days <= 4:
        days_score = 100
    elif duration_days == 1 or duration_days == 5:
        days_score = 75
    elif duration_days > 5:
        days_score = max(40, 80 - (duration_days - 5) * 10)

    if distance_km > 0:
        t_score = transport_score(distance_km, duration_days)
    else:
        t_score = 50

    w_score = _weather_score(weather_list) if weather_list else 0.5
    weather_score_val = int(w_score * 100)

    attraction_score_val = int(_calc_attraction_score(highlights, tags, blurb or ""))

    total = int(
        days_score * 0.40 +
        t_score * 0.25 +
        weather_score_val * 0.25 +
        attraction_score_val * 0.10
    )

    transit_info = {"transport_mode": "", "transit_hours": 0.0, "distance_km": distance_km}
    if distance_km > 0:
        from src.distance import estimate_travel_time
        transit_info = estimate_travel_time(distance_km)

    return {
        "total": max(0, min(100, total)),
        "breakdown": {
            "days_score": days_score,
            "days_weight": 0.4,
            "transport_score": t_score,
            "transport_weight": 0.25,
            "weather_score": weather_score_val,
            "weather_weight": 0.25,
            "attraction_score": attraction_score_val,
            "attraction_weight": 0.1,
            "distance_km": distance_km,
            "transport_hours": transit_info.get("transit_hours", 0),
            "transport_mode": transit_info.get("recommended_mode", ""),
        }
    }


router = APIRouter()


@router.get("/health", response_model=HealthResponse)
async def health():
    """健康检查 + 系统概览。"""
    from src.db import get_overview_stats
    from src.db import get_conn
    stats = get_overview_stats()

    conn = get_conn()
    row = conn.execute(
        "SELECT MIN(start_date), MAX(start_date) FROM trip_matrix_cache"
    ).fetchone()
    date_range = (row[0], row[1]) if row[0] else None

    return HealthResponse(
        status="ok",
        refresh_enabled=REFRESH_ENABLED,
        total_cities=len(get_all_cities()),
        cached_cities=stats["cached_cities"],
        cells_total=stats["cells_total"],
        cache_hit_rate=stats["cache_hit_rate"],
        date_range=date_range,
    )


@router.get("/cities", response_model=CityListResponse)
async def list_cities():
    cities = get_all_cities()
    return CityListResponse(cities=cities, count=len(cities))


@router.get("/geo/cities")
async def list_all_geo_cities():
    from src.db import get_all_cities
    return {"cities": get_all_cities()}


@router.post("/search", response_model=SearchResponse)
async def search_travel_plans(req: SearchRequest, request: Request):
    from .rate_limit import search_limiter

    client_ip = request.client.host if request.client else "unknown"
    allowed, retry_after = search_limiter.check(client_ip)
    if not allowed:
        raise HTTPException(
            status_code=429,
            detail=f"请求过于频繁，请 {retry_after} 秒后重试",
            headers={"Retry-After": str(retry_after)}
        )

    generated_at = datetime.now().isoformat()

    duration = req.duration or 3
    style = req.style or "standard"

    calculated_duration = duration
    if req.start_date and req.end_date:
        try:
            start_dt = datetime.strptime(req.start_date, "%Y-%m-%d")
            end_dt = datetime.strptime(req.end_date, "%Y-%m-%d")
            if end_dt >= start_dt:
                calculated_duration = (end_dt - start_dt).days + 1
        except Exception:
            pass

    conn = get_conn()
    rows = conn.execute("""
        SELECT g.city, g.duration, g.style, g.guide_json,
               f.blurb, f.tags, f.trip_capacity
        FROM city_guides g
        LEFT JOIN city_features f ON g.city = f.city
        WHERE g.duration = ? AND g.style = ?
        ORDER BY g.city
    """, (calculated_duration, style)).fetchall()
    conn.commit()

    items: list[SearchResultItem] = []
    weather_map: dict[str, list] = {}
    source = "guide"
    origin_coord = lookup_city_coord(req.origin_city or "北京市")

    if req.start_date and req.end_date:
        try:
            start_dt = datetime.strptime(req.start_date, "%Y-%m-%d")
            end_dt = datetime.strptime(req.end_date, "%Y-%m-%d")
            if end_dt >= start_dt:
                matrix_rows = conn.execute("""
                    SELECT city, start_date, end_date, duration, score, recommendation,
                           weather_summary, full_result
                    FROM trip_matrix_cache
                    WHERE start_date = ? AND end_date = ?
                """, (req.start_date, req.end_date)).fetchall()
                conn.commit()

                if matrix_rows:
                    source = "matrix"
                    for mr in matrix_rows:
                        try:
                            full_result = json.loads(mr[6]) if isinstance(mr[6], str) else mr[6]
                        except Exception:
                            full_result = {}
                        highlights = full_result.get("highlights", [])
                        tags = full_result.get("tags", [])
                        blurb = full_result.get("city_intro", "") or ""

                        distance_km = 0.0
                        if origin_coord:
                            dest_coord = lookup_city_coord(mr[0])
                            if dest_coord:
                                distance_km = round(haversine_km(origin_coord, dest_coord), 1)

                        coords = lookup_city(mr[0])
                        w_list = []
                        if coords:
                            try:
                                w_list = fetch_weather(coords[0], coords[1], req.start_date, req.end_date)
                                weather_map[mr[0]] = [
                                    {
                                        "date": w.date,
                                        "weather_code": w.weather_code,
                                        "weather_desc": w.weather_desc,
                                        "temp_max": w.temp_max,
                                        "temp_min": w.temp_min,
                                        "precipitation_mm": w.precipitation_mm,
                                        "precipitation_probability": w.precipitation_probability,
                                    }
                                    for w in w_list
                                ]
                            except Exception:
                                pass

                        dynamic = calc_dynamic_score(
                            duration_days=mr[3],
                            distance_km=distance_km,
                            weather_list=weather_map.get(mr[0], []),
                            highlights=highlights,
                            tags=tags,
                            blurb=blurb,
                        )

                        weather_summary = ""
                        weather_desc = ""
                        if weather_map.get(mr[0]):
                            weather_summary = " | ".join(
                                f"{w['date']} {w['weather_desc']}" for w in weather_map[mr[0]]
                            )
                            weather_desc = weather_map[mr[0]][0].get("weather_desc", "")

                        recommendation = "推荐"
                        if dynamic["total"] >= 80:
                            recommendation = "强烈推荐"
                        elif dynamic["total"] < 50:
                            recommendation = "不建议"

                        items.append(SearchResultItem(
                            source="matrix",
                            city=mr[0],
                            start_date=mr[1],
                            end_date=mr[2],
                            duration_days=mr[3],
                            score=dynamic["total"],
                            recommendation=recommendation,
                            key_highlights=blurb[:100] if blurb else "",
                            top_attractions=(highlights or [])[:5],
                            blurb=blurb[:200],
                            tags=tags,
                            weather_summary=weather_summary,
                            weather_desc=weather_desc,
                            daily_plan=full_result.get("daily_plan", []),
                            score_breakdown=dynamic["breakdown"],
                            distance_km=distance_km,
                            transport_mode=dynamic["breakdown"]["transport_mode"],
                            transit_hours=dynamic["breakdown"]["transport_hours"],
                        ))
        except Exception:
            pass

    if not items:
        for r in rows:
            try:
                import json as _json
                data = _json.loads(r[3]) if isinstance(r[3], str) else r[3]
            except Exception:
                data = {}
            tags = []
            try:
                tags = _json.loads(r[5]) if r[5] and isinstance(r[5], str) else (r[5] or [])
            except Exception:
                tags = []

            highlights = data.get("highlights", [])
            blurb = r[4] or ""

            distance_km = 0.0
            if origin_coord:
                dest_coord = lookup_city_coord(r[0])
                if dest_coord:
                    distance_km = round(haversine_km(origin_coord, dest_coord), 1)

            w_list = weather_map.get(r[0], [])
            dynamic = calc_dynamic_score(
                duration_days=r[1],
                distance_km=distance_km,
                weather_list=w_list,
                highlights=highlights,
                tags=tags,
                blurb=blurb,
            )

            weather_summary = ""
            weather_desc = ""
            if w_list:
                weather_summary = " | ".join(
                    f"{w['date']} {w['weather_desc']}" for w in w_list
                )
                weather_desc = w_list[0].get("weather_desc", "")

            pref_score = 0.0
            if req.preference:
                pref_score = _calc_pref_match(req.preference, tags, blurb or "")

            recommendation = "推荐"
            if dynamic["total"] >= 80:
                recommendation = "强烈推荐"
            elif dynamic["total"] < 50:
                recommendation = "不建议"

            item = SearchResultItem(
                source="guide",
                city=r[0],
                start_date=req.start_date or "",
                end_date=req.end_date or "",
                duration_days=r[1],
                score=dynamic["total"],
                recommendation=recommendation,
                key_highlights=blurb[:100] if blurb else "",
                top_attractions=(highlights or [])[:5],
                blurb=blurb[:200],
                tags=tags,
                weather_summary=weather_summary,
                weather_desc=weather_desc,
                daily_plan=data.get("daily_plan", []),
                score_breakdown=dynamic["breakdown"],
                preference_score=pref_score,
                distance_km=dynamic["breakdown"]["distance_km"],
                transport_mode=dynamic["breakdown"]["transport_mode"],
                transit_hours=dynamic["breakdown"]["transport_hours"],
            )
            items.append(item)

    sort_by = req.sort_by or "score"
    if sort_by == "weather":
        items.sort(key=lambda x: (
            -len(weather_map.get(x.city, [])),
            -x.distance_km if x.distance_km else 0
        ))
    elif sort_by == "preference":
        items.sort(key=lambda x: -((x.preference_score or 0) * 20 + x.score))
    elif sort_by == "distance":
        items.sort(key=lambda x: x.distance_km if x.distance_km else float("inf"))
    else:
        items.sort(key=lambda x: -x.score)

    return SearchResponse(
        source=source,
        items=items,
        total=len(items),
        generated_at=generated_at,
    )


@router.post("/plan/recommend")
async def plan_recommend(req: PlanRecommendRequest):
    """为指定城市生成专项旅行方案,支持偏好标签和自由文本。"""
    from src.planner import TripPlanner

    if not req.city:
        raise HTTPException(status_code=400, detail="城市名不能为空")

    start_date = req.start_date or ""
    end_date = req.end_date or ""

    # 至少要有日期才能生成
    if not start_date or not end_date:
        raise HTTPException(status_code=400, detail="生成专项方案需要提供出发和返回日期")

    try:
        datetime.strptime(start_date, "%Y-%m-%d")
        datetime.strptime(end_date, "%Y-%m-%d")
    except ValueError:
        raise HTTPException(status_code=400, detail="日期格式错误，请使用 YYYY-MM-DD")

    planner = TripPlanner(lite=True)
    result = planner.plan(
        city=req.city,
        start_date=start_date,
        end_date=end_date,
        preference_tags=req.preference_tags or None,
        preference_text=req.preference_text or None,
    )

    weather_parts = (result.weather_forecast or [{}])[:1]
    weather_desc = weather_parts[0].get("weather_desc", "") if weather_parts else ""

    if result.success and result.data:
        data = result.data
        return PlanRecommendResponse(
            city=req.city,
            start_date=start_date,
            end_date=end_date,
            duration_days=result.duration_days,
            score=data.get("score", 0),
            recommendation=data.get("recommendation", ""),
            weather_summary=" | ".join(
                f"{w['date']} {w['weather_desc']}" for w in (result.weather_forecast or [])
            ),
            weather_desc=weather_desc,
            top_attractions=data.get("top_attractions", []),
            key_highlights=data.get("city_intro", "")[:100],
            score_breakdown=data.get("score_breakdown", {}),
            daily_plan=data.get("daily_plan", []),
            success=True,
            error=None,
            generated_at=datetime.now().isoformat(),
        )
    else:
        return PlanRecommendResponse(
            city=req.city,
            start_date=start_date,
            end_date=end_date,
            duration_days=result.duration_days,
            score=0,
            recommendation="生成失败",
            weather_summary="",
            weather_desc="",
            top_attractions=[],
            key_highlights="",
            score_breakdown={},
            daily_plan=[],
            success=False,
            error=result.error or "未知错误",
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
    if city not in get_all_cities():
        raise HTTPException(status_code=404, detail=f"城市 {city} 不在数据库中")

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
    return {"message": "刷新任务已启动", "cities": get_all_cities()}


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
from src.db import get_overview_stats, get_recent_generations


@router.get("/admin/overview")
async def admin_overview(_: dict = Depends(require_admin)):
    """系统总览。"""
    return get_overview_stats()


@router.get("/admin/cities")
async def admin_list_cities(_: dict = Depends(require_admin)):
    """所有有坐标的城市列表。"""
    return {"cities": get_all_cities()}


@router.post("/admin/cities/{city}/refresh")
async def admin_refresh_city(city: str, _: dict = Depends(require_admin)):
    """手动触发某城市重新生成。"""
    if city not in get_all_cities():
        raise HTTPException(404, f"城市 {city} 不在数据库中")

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


@router.get("/admin/config")
async def admin_get_config(_: dict = Depends(require_admin)):
    """获取运行时配置（敏感字段已掩码）。"""
    from .config import get_runtime_config
    return get_runtime_config()


@router.put("/admin/config")
async def admin_update_config(payload: dict, _: dict = Depends(require_admin)):
    """更新运行时配置。"""
    from .config import update_runtime_config
    from .refresh import restart_background_refresh

    allowed = {
        "refresh_enabled", "refresh_interval_seconds",
        "matrix_max_offset", "matrix_max_duration", "matrix_concurrency",
        "api_key", "base_url", "model_name",
    }
    updates = {k: v for k, v in payload.items() if k in allowed}
    if not updates:
        raise HTTPException(400, "无可更新的字段")

    updated = update_runtime_config(updates)

    if "refresh_enabled" in updates or "refresh_interval_seconds" in updates:
        try:
            restart_background_refresh()
        except Exception as e:
            print(f"[config] 重启刷新任务失败: {e}")

    return {"message": "配置已更新", "config": updated}
