from datetime import datetime, date, timedelta
from fastapi import APIRouter, HTTPException, Query, Depends, Header, Request
from typing import Optional

from src.distance import calc_distance, transport_score
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
        seed_cities=db_get_seed_cities(),
        cached_cities=stats["cached_cities"],
        cells_total=stats["cells_total"],
        cache_hit_rate=stats["cache_hit_rate"],
        date_range=date_range,
    )


@router.get("/cities", response_model=CityListResponse)
async def list_cities():
    return CityListResponse(cities=db_get_seed_cities(), count=len(db_get_seed_cities()))


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

    # ── 无日期模式：走 city_guides 静态攻略 ──
    if not req.start_date or not req.end_date:
        duration = req.duration or 3
        style = req.style or "standard"

        conn = get_conn()
        rows = conn.execute("""
            SELECT g.city, g.duration, g.style, g.guide_json,
                   f.blurb, f.tags, f.trip_capacity
            FROM city_guides g
            LEFT JOIN city_features f ON g.city = f.city
            WHERE g.duration = ? AND g.style = ?
            ORDER BY g.city
        """, (duration, style)).fetchall()
        conn.close()

        items: list[SearchResultItem] = []
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
            budget = data.get("budget", {})
            tips = data.get("tips", [])

            items.append(SearchResultItem(
                source="guide",
                city=r[0],
                duration_days=r[1],
                score=85,
                recommendation="推荐",
                key_highlights=(
                    (r[4] or "")[:100]
                ),
                top_attractions=(highlights or [])[:5],
                blurb=(r[4] or "")[:200],
                tags=tags,
            ))

        # 排序（按 preferences 匹配）
        if req.preference:
            for item in items:
                pref_score = _calc_pref_match(req.preference, item.tags, item.blurb or "")
                item.preference_score = pref_score
            items.sort(key=lambda x: -(x.preference_score or 0))

        return SearchResponse(
            source="guide",
            items=items,
            total=len(items),
            generated_at=generated_at,
        )

    # ── 有日期模式：走 matrix_cache 实时方案 ──
    try:
        start = datetime.strptime(req.start_date, "%Y-%m-%d")
        end = datetime.strptime(req.end_date, "%Y-%m-%d")
    except ValueError:
        raise HTTPException(status_code=400, detail="日期格式错误，请使用 YYYY-MM-DD")

    if end < start:
        raise HTTPException(status_code=400, detail="返回日期不能早于出发日期")

    start_str = req.start_date
    end_str = req.end_date
    duration_days = (end - start).days + 1

    from app.matrix import generate_single_cell, MatrixCell

    def _cell_to_result_item(city: str, cell: dict | MatrixCell, is_on_demand: bool = False) -> tuple[SearchResultItem, dict] | None:
        if isinstance(cell, MatrixCell):
            if not cell.success:
                return None
            cell_d = cell.to_dict()
        else:
            cell_d = cell
            if not cell_d.get("success"):
                return None

        full = (cell_d.get("full_result") or {}) if isinstance(cell_d.get("full_result"), dict) else {}
        plan_data = full.get("data") or {}

        weather_parts = (cell_d.get("weather_summary") or "").split("|")
        weather_desc = weather_parts[0].strip() if weather_parts else ""

        raw_attractions = plan_data.get("top_attractions") or plan_data.get("attractions") or []
        if raw_attractions and isinstance(raw_attractions[0], dict):
            top_attractions = [a.get("name", "") for a in raw_attractions[:5]]
        else:
            top_attractions = raw_attractions[:5]

        if top_attractions:
            verified = []
            for name in top_attractions:
                att = get_attraction_by_name(name, city)
                if att:
                    verified.append(att["name"])
            if not verified:
                from src.db import get_city_attractions
                db_atts = get_city_attractions(city, limit=5)
                verified = [a["name"] for a in db_atts]
            top_attractions = verified

        distance, transit_info = calc_distance(req.origin_city or "", city)
        if distance > 0 and transit_info:
            new_transport_score = transport_score(distance, cell_d.get("duration", duration_days))
            breakdown = dict(plan_data.get("score_breakdown") or {})
            breakdown["transport"] = new_transport_score
        else:
            distance = 0
            transit_info = {"recommended_mode": "", "transit_hours": 0}
            breakdown = plan_data.get("score_breakdown") or {}

        base_score = plan_data.get("score") or cell_d.get("score") or 0
        if breakdown and all(k in breakdown for k in ["days_match", "weather", "attraction_density", "transport"]):
            base_score = int(
                breakdown["days_match"] * 0.40
                + breakdown["transport"] * 0.25
                + breakdown["weather"] * 0.25
                + breakdown["attraction_density"] * 0.10
            )
        else:
            base_score = plan_data.get("score") or cell_d.get("score") or 0

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

        item = SearchResultItem(
            source="matrix",
            city=city,
            start_date=cell_d.get("start_date", start_str),
            end_date=cell_d.get("end_date", end_str),
            duration_days=cell_d.get("duration", duration_days),
            score=base_score,
            recommendation=plan_data.get("recommendation") or cell_d.get("recommendation") or "",
            weather_summary=cell_d.get("weather_summary") or "",
            weather_desc=weather_desc,
            top_attractions=top_attractions,
            key_highlights=plan_data.get("key_highlights") or plan_data.get("city_intro", "")[:100],
            score_breakdown=breakdown,
            daily_plan=daily_plan,
            distance_km=distance,
            transport_mode=transit_info.get("recommended_mode", ""),
            transit_hours=transit_info.get("transit_hours", 0),
        )
        return item, plan_data

    # ── 获取候选城市 ──
    from src.db import get_seed_cities as db_get_seed
    candidate_cities = db_get_seed()

    # ── 第一步：查缓存 ──
    results: list[SearchResultItem] = []
    plan_data_map: dict = {}
    cache_miss_cities: list[str] = []

    for city in candidate_cities:
        cached = load_matrix_from_cache(city)
        if not cached:
            cache_miss_cities.append(city)
            continue

        matched = None
        for cell in cached.get("cells", []):
            if cell.get("start_date") == start_str and cell.get("end_date") == end_str:
                matched = cell
                break

        if matched:
            r = _cell_to_result_item(city, matched)
            if r:
                item, pd = r
                results.append(item)
                plan_data_map[city] = pd
        else:
            cache_miss_cities.append(city)

    # ── 第二步：对缓存缺失的城市 on-demand 生成（最多 3 个）──
    if cache_miss_cities:
        import asyncio as _asyncio
        ondemand_limit = min(len(cache_miss_cities), 3)
        ondemand_tasks = []
        for city in cache_miss_cities[:ondemand_limit]:
            task = _asyncio.ensure_future(
                generate_single_cell(city, start_str, end_str)
            )
            ondemand_tasks.append((city, task))

        for city, task in ondemand_tasks:
            try:
                cell = await task
                r = _cell_to_result_item(city, cell, is_on_demand=True)
                if r:
                    item, pd = r
                    results.append(item)
                    plan_data_map[city] = pd
                    print(f"[search] on-demand generated: {city} {start_str}~{end_str}")
            except Exception as e:
                print(f"[search] on-demand failed for {city}: {e}")

    results = rank_by_preference(results, req.preference or "", plan_data_map)
    results.sort(key=lambda x: x.score + (x.preference_score or 0) * 20, reverse=True)

    return SearchResponse(
        source="matrix",
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
