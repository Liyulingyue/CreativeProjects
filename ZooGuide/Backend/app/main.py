"""FastAPI entry + routes."""

from __future__ import annotations

import uuid
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Optional

from fastapi import Depends, FastAPI, Header, HTTPException, UploadFile, File
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel

from . import auth, chat as chat_mod, config, data_loader, db, geo, photo, planner
from .models import (
    ChatRequest,
    ChatResponse,
    PlanRequest,
    ReplanRequest,
    Route,
    VenueBrief,
)


# In-memory checkin store (process-local; resets on restart) — DEPRECATED, using DB
_checkins: dict[str, list[dict]] = {}


@asynccontextmanager
async def lifespan(app: FastAPI):
    db.init_db()
    n = len(data_loader.get_all_venues())
    print(f"[startup] ZooGuide ready: {n} venues loaded; USE_LLM={config.USE_LLM}; DB initialized")
    yield


app = FastAPI(
    title="ZooGuide API",
    description="南京红山森林动物园省力 Agent",
    version="1.0.0",
    lifespan=lifespan,
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=config.CORS_ORIGINS + ["*"],  # permissive for local dev
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


# ---------------------------------------------------------------------------
# Health & meta
# ---------------------------------------------------------------------------

@app.get("/api/health")
def health():
    return {
        "status": "ok",
        "use_llm": config.has_valid_llm_config(),
        "model": config.MODEL_NAME if config.has_valid_llm_config() else None,
        "venue_count": len(data_loader.get_all_venues()),
    }


@app.get("/api/meta")
def meta():
    return data_loader.get_meta()


@app.get("/api/quiz-options")
def quiz_options():
    return {
        "party_types": [
            {"value": "solo", "label": "一个人", "icon": "🧍", "desc": "专注观察动物，按自己节奏走"},
            {"value": "couple", "label": "情侣/朋友", "icon": "👥", "desc": "兼顾出片与轻松"},
            {"value": "family_young", "label": "带学龄前娃", "icon": "👨‍👩‍👧", "desc": "节奏要慢，多亲子互动"},
            {"value": "family_teen", "label": "带青少年", "icon": "🧑‍🎓", "desc": "可步行更多，可加科普"},
            {"value": "seniors", "label": "带老人", "icon": "👵", "desc": "少爬坡，多座椅与厕所"},
        ],
        "interests": [
            {"value": "panda", "label": "国宝大熊猫"},
            {"value": "ape", "label": "灵长类"},
            {"value": "cat", "label": "猫科动物"},
            {"value": "bird", "label": "鸟类"},
            {"value": "australian", "label": "澳洲动物"},
            {"value": "african", "label": "非洲动物"},
            {"value": "local", "label": "中国本土物种"},
            {"value": "exotic", "label": "异域奇观"},
            {"value": "kids_favorite", "label": "孩子最爱"},
        ],
        "gates": [
            {"value": "north", "label": "北门", "desc": "地铁1号线，最近大熊猫馆"},
            {"value": "south", "label": "南门", "desc": "2025新馆区，非洲/唐家河/大猩猩"},
            {"value": "east", "label": "东门", "desc": "高黎贡、冈瓦纳"},
        ],
        "stamina_descriptions": {
            "1": "体力一般，不想走太多",
            "2": "偏休闲，可走 1-2 公里",
            "3": "一般，可走 3-4 公里",
            "4": "较好，可走 5-6 公里",
            "5": "精力充沛，可暴走全园",
        },
        "sun_descriptions": {
            "1": "非常怕晒，必须阴凉/室内",
            "2": "怕晒，倾向遮阴路线",
            "3": "无所谓",
            "4": "能晒",
            "5": "喜欢阳光户外",
        },
    }


# ---------------------------------------------------------------------------
# Venues
# ---------------------------------------------------------------------------

@app.get("/api/venues")
def list_venues():
    venues = data_loader.get_all_venues()
    return {
        "venues": [
            VenueBrief(
                id=v.id,
                name=v.name,
                animals=v.animals,
                tags=v.tags,
                themes=v.themes,
                recommended_visit_minutes=v.recommended_visit_minutes,
                kid_friendly=v.kid_friendly,
                photo_op=v.photo_op,
                must_see=v.must_see,
                shaded=v.shaded,
                rest_spots=v.rest_spots,
            ).model_dump()
            for v in venues
        ]
    }


@app.get("/api/venues/{venue_id}")
def get_venue(venue_id: str):
    v = data_loader.get_venue_by_id(venue_id)
    if not v:
        raise HTTPException(status_code=404, detail="venue not found")
    return v.model_dump()


# ---------------------------------------------------------------------------
# Plan (core)
# ---------------------------------------------------------------------------

@app.post("/api/plan")
def plan(
    req: PlanRequest,
    current_user: Optional[dict] = Depends(auth.get_current_user_optional),
):
    try:
        route, used_llm = planner.plan_route(req, force_fast=req.fast)
        # Echo prefs back so client can /replan later
        resp = route.model_dump()
        resp["_party_type"] = req.party_type
        resp["_with_kids"] = req.with_kids
        resp["_kids_age"] = req.kids_age
        resp["_stamina"] = req.stamina
        resp["_sun_tolerance"] = req.sun_tolerance
        resp["_willing_to_hike"] = req.willing_to_hike
        resp["_animal_interests"] = req.animal_interests
        resp["_entry_gate"] = req.entry_gate
        resp["_start_time"] = req.start_time
        resp["_available_hours"] = req.available_hours
        resp["llm_used"] = used_llm
        # Persist to DB (if logged in)
        if current_user:
            try:
                prefs_dict = req.model_dump()
                db.insert_route(
                    route_id=route.id,
                    prefs=prefs_dict,
                    summary=route.summary,
                    total_minutes=route.total_minutes,
                    stops_count=len(route.stops),
                    llm_used=used_llm,
                    fallback=route.fallback,
                    user_id=current_user["id"],
                )
            except Exception as e:
                print(f"[warn] failed to persist route: {e}")
        return resp
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@app.post("/api/plan-variants")
def plan_variants(req: PlanRequest):
    """Generate 2-3 alternative routes for comparison (always fast / rule-based)."""
    try:
        variants = planner.plan_route_variants(req)
        return {
            "variants": variants,
            "prefs": req.model_dump(),
        }
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


# ---------------------------------------------------------------------------
# Streaming plan (SSE)
# ---------------------------------------------------------------------------

@app.post("/api/plan-stream")
async def plan_stream(req: PlanRequest):
    """Server-Sent Events stream of the planning process.

    Events:
      - thinking: {"text": "..."} chunks
      - done: {"route": {...}}
      - error: {"message": "..."}
    Falls back to non-streaming rule engine if model doesn't support streaming.
    """
    from fastapi.responses import StreamingResponse
    from . import config
    import json as _json

    async def event_gen():
        # 1. Emit progress preamble
        yield f"event: thinking\ndata: {_json.dumps({'text': '正在分析你的偏好…'})}\n\n"
        yield f"event: thinking\ndata: {_json.dumps({'text': '从 23 个场馆中筛选候选…'})}\n\n"

        if req.fast or not llm_client.is_llm_enabled():
            # Fast path: no streaming, just rule-based route
            yield f"event: thinking\ndata: {_json.dumps({'text': '使用规则引擎（极速模式）'})}\n\n"
            route, _ = planner.plan_route(req, force_fast=True)
            yield f"event: done\ndata: {_json.dumps({'route': route.model_dump()})}\n\n"
            return

        # 2. Slow path: try LLM with streaming
        try:
            from openai import OpenAI
            client = OpenAI(api_key=config.API_KEY, base_url=config.BASE_URL, timeout=180.0)
            from .prompts import PLAN_TARGET_STRUCTURE, SYSTEM_BACKGROUND, PLAN_REQUIREMENTS
            from .rule_engine import filter_and_rank, max_stops_by_time
            from .walking import build_walking_matrix
            import json as _json2
            from .planner import _build_user_prompt_plan, _route_from_llm_data

            yield f"event: thinking\ndata: {_json.dumps({'text': '请 LLM 编排路线（30-90 秒）…'})}\n\n"

            candidates = filter_and_rank(req)
            walking_matrix = build_walking_matrix([c["id"] for c in candidates])
            user_prompt = _build_user_prompt_plan(req, candidates, walking_matrix)

            system = (
                SYSTEM_BACKGROUND
                + "\n\n请输出严格的 JSON，不要任何额外文字。"
                + "\n\nJSON 结构:\n"
                + _json2.dumps(PLAN_TARGET_STRUCTURE, ensure_ascii=False)
                + "\n\nRequirements:\n"
                + "\n".join(PLAN_REQUIREMENTS)
            )

            stream = client.chat.completions.create(
                model=config.MODEL_NAME,
                messages=[
                    {"role": "system", "content": system},
                    {"role": "user", "content": user_prompt},
                ],
                stream=True,
                max_tokens=4000,
                timeout=180.0,
            )

            collected = ""
            for chunk in stream:
                delta = chunk.choices[0].delta.content or ""
                if delta:
                    collected += delta
                    yield f"event: token\ndata: {_json.dumps({'text': delta})}\n\n"

            # Try to parse collected as JSON
            from .planner import _route_from_llm_data
            try:
                if "```" in collected:
                    for fence in ("```json", "```"):
                        if fence in collected:
                            collected = collected.split(fence)[1].split("```")[0]
                            break
                data = _json2.loads(collected)
                route = _route_from_llm_data(data, candidates, req.entry_gate, req.start_time)
                yield f"event: done\ndata: {_json.dumps({'route': route.model_dump()})}\n\n"
            except Exception as e:
                # Parse failed, fall back to rule engine
                yield f"event: thinking\ndata: {_json.dumps({'text': '解析失败，回退到规则引擎'})}\n\n"
                route, _ = planner.plan_route(req, force_fast=True)
                yield f"event: done\ndata: {_json.dumps({'route': route.model_dump()})}\n\n"
        except Exception as e:
            yield f"event: error\ndata: {_json.dumps({'message': str(e)})}\n\n"
            # Always provide a fallback route
            try:
                route, _ = planner.plan_route(req, force_fast=True)
                yield f"event: done\ndata: {_json.dumps({'route': route.model_dump()})}\n\n"
            except Exception:
                pass

    return StreamingResponse(event_gen(), media_type="text/event-stream")


@app.post("/api/replan")
def replan(req: ReplanRequest):
    try:
        route, used_llm = planner.replan_route(req.original_route, req)
        resp = route.model_dump()
        resp["llm_used"] = used_llm
        return resp
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


# ---------------------------------------------------------------------------
# Chat (natural-language replan)
# ---------------------------------------------------------------------------

@app.post("/api/chat")
async def chat_endpoint(req: ChatRequest):
    """Conversational replan. Returns reply + optional new route."""
    import copy
    # Deep copy current_route to avoid mutation
    current_route_copy = copy.deepcopy(req.current_route) if req.current_route else None
    reply = await chat_mod.chat(req)
    # chat() may have already called apply_chat_constraint internally (rule path)
    # Only call it again if no new_route yet
    if "new_route" not in reply and current_route_copy and reply.get("suggested_replan") and reply.get("extracted_constraint"):
        try:
            new_route = chat_mod.apply_chat_constraint(
                current_route_copy,
                reply["extracted_constraint"],
                req.prefs or {},
            )
            if new_route:
                reply["new_route"] = new_route.model_dump()
        except Exception as e:
            print(f"[warn] chat apply_constraint failed: {e}", flush=True)
    return reply


# ---------------------------------------------------------------------------
# Checkins (animal打卡)
# ---------------------------------------------------------------------------

class CheckinRequest(BaseModel):
    venue_id: str
    session_id: Optional[str] = None


class CheckinRecord(BaseModel):
    venue_id: str
    venue_name: str
    ts: str


class ChatRequest(BaseModel):
    message: str
    current_route: Optional[dict] = None
    prefs: Optional[dict] = None
    history: list = []


class ChatResponse(BaseModel):
    reply: str
    suggested_replan: bool = False
    extracted_constraint: Optional[dict] = None  # e.g. {"type":"skip","venue_id":"..."}


@app.post("/api/checkin")
def checkin(
    req: CheckinRequest,
    current_user: Optional[dict] = Depends(auth.get_current_user_optional),
):
    venue = data_loader.get_venue_by_id(req.venue_id)
    if not venue:
        raise HTTPException(status_code=404, detail="venue not found")
    sid = req.session_id or (f"u{current_user['id']}" if current_user else "anon")
    user_id = current_user["id"] if current_user else None
    record = db.insert_checkin(
        venue_id=venue.id,
        venue_name=venue.name,
        session_id=sid,
        user_id=user_id,
    )
    if user_id:
        items = db.list_checkins_by_user(user_id)
    else:
        items = db.list_checkins_by_session(sid)
    total_venues = len(data_loader.get_all_venues())
    # Evaluate achievements (only for logged-in users)
    new_achievements = []
    if user_id:
        try:
            new_achievements = db.evaluate_achievements(user_id)
        except Exception as e:
            print(f"[warn] achievement eval failed: {e}")
    return {
        "ok": True,
        "session_id": sid,
        "total_checkins": len(items),
        "completion_rate": round(len(items) / total_venues, 3),
        "venue_name": venue.name,
        "new_achievements": new_achievements,
    }


@app.get("/api/checkin/{session_id}")
def get_checkins(session_id: str):
    items = db.list_checkins_by_session(session_id)
    total = len(data_loader.get_all_venues())
    return {
        "session_id": session_id,
        "checkins": items,
        "completion_rate": round(len(items) / total, 3) if total else 0,
    }


# ---------------------------------------------------------------------------
# Geo: nearest venue
# ---------------------------------------------------------------------------

@app.get("/api/nearest")
def nearest(lat: float, lon: float, top_k: int = 3):
    """Find top-k nearest venues to (lat, lon)."""
    if not (-90 <= lat <= 90 and -180 <= lon <= 180):
        raise HTTPException(status_code=400, detail="invalid lat/lon")
    results = geo.find_nearest_venues(lat, lon, top_k=top_k)
    in_park = geo.is_within_park(lat, lon)
    return {
        "lat": lat,
        "lon": lon,
        "in_park_estimate": in_park,
        "bbox": geo.bbox(),
        "results": results,
    }


# ---------------------------------------------------------------------------
# Photo evaluation (合照彩蛋)
# ---------------------------------------------------------------------------

@app.post("/api/photo-evaluate")
async def photo_evaluate(
    file: UploadFile = File(...),
    session_id: Optional[str] = None,
    auto_checkin: bool = True,
    expected_venue_id: Optional[str] = None,
    current_user: Optional[dict] = Depends(auth.get_current_user_optional),
):
    """Upload a photo, get a fun evaluation + auto-checkin.

    If expected_venue_id is provided, verifies the photo matches that venue
    and returns a 'success' field. If photo doesn't match, still evaluates
    but doesn't auto-checkin (user must select correct venue).
    """
    contents = await file.read()
    if len(contents) > 8 * 1024 * 1024:
        raise HTTPException(status_code=413, detail="file too large (max 8MB)")
    suffix = Path(file.filename or "photo.jpg").suffix.lower()
    if suffix not in (".jpg", ".jpeg", ".png", ".webp", ".gif"):
        suffix = ".jpg"
    path = photo.save_photo(contents, suffix)
    user_id = current_user["id"] if current_user else None
    sid = session_id or (f"u{user_id}" if user_id else "anon")

    # Build expected venue context for LLM
    expected_venue = None
    if expected_venue_id:
        expected_venue = data_loader.get_venue_dict_by_id(expected_venue_id)
        if expected_venue:
            result = photo.evaluate_photo_with_expected(
                path,
                user_id=user_id,
                session_id=sid,
                auto_checkin=False,  # we'll handle checkin based on success
                expected_venue=expected_venue,
            )
        else:
            result = photo.evaluate_photo(
                path,
                user_id=user_id,
                session_id=sid,
                auto_checkin=auto_checkin,
            )
    else:
        result = photo.evaluate_photo(
            path,
            user_id=user_id,
            session_id=sid,
            auto_checkin=auto_checkin,
        )

# Determine success: matched_venue_id == expected_venue_id
    if expected_venue_id:
        matched = result.get("matched_venue_id") == expected_venue_id
        result["success"] = matched
        result["expected_venue_id"] = expected_venue_id
        if matched:
            # Only auto-checkin on success
            venue = data_loader.get_venue_dict_by_id(expected_venue_id)
            if venue:
                result["auto_checkin"] = db.insert_checkin(
                    venue_id=expected_venue["id"],
                    venue_name=expected_venue["name"],
                    session_id=sid,
                    user_id=user_id,
                    note=f"photo eval {result['evaluation_id']}",
                )
        else:
            actual_name = result.get("matched_venue_name") or "未识别"
            expected_name = expected_venue["name"]
            result["failure_reason"] = (
                f"照片里没有 {expected_name}（识别为：{actual_name}）"
            )
    else:
        # No expected venue - just use normal auto-checkin logic
        if auto_checkin and result.get("matched_venue_id"):
            venue = data_loader.get_venue_dict_by_id(result["matched_venue_id"])
            if venue:
                sid = session_id or (str(user_id) if user_id else "anon")
                try:
                    checkin = db.insert_checkin(
                        venue_id=venue["id"],
                        venue_name=venue["name"],
                        session_id=sid,
                        user_id=user_id,
                        note=f"photo eval {result['evaluation_id']}",
                    )
                    result["auto_checkin"] = checkin
                except Exception:
                    pass

    # Evaluate achievements (only for logged-in users, on success)
    if user_id and (not expected_venue_id or result.get("success")):
        try:
            newly_earned = db.evaluate_achievements(user_id)
            if newly_earned:
                catalog = {a["id"]: a for a in db.list_all_achievements()}
                result["new_achievements"] = [
                    {**catalog[aid], "earned_at": "just now"}
                    for aid in newly_earned
                    if aid in catalog
                ]
        except Exception as e:
            print(f"[warn] achievement eval failed: {e}")

    return result


class GpsCheckinRequest(BaseModel):
    lat: float
    lon: float
    in_park: bool = False
    nearest_venue_id: Optional[str] = None
    nearest_venue_name: Optional[str] = None


@app.post("/api/gps-checkin")
def gps_checkin(
    req: GpsCheckinRequest,
    current_user: Optional[dict] = Depends(auth.get_current_user_optional),
):
    """Record a GPS-based check-in (for achievement tracking)."""
    user_id = current_user["id"] if current_user else None
    sid = f"u{user_id}" if user_id else "anon"
    try:
        db.insert_gps_checkin(
            lat=req.lat,
            lon=req.lon,
            user_id=user_id,
            session_id=sid,
            nearest_venue_id=req.nearest_venue_id,
            nearest_venue_name=req.nearest_venue_name,
            in_park=req.in_park,
        )
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))

    newly_earned = []
    if user_id:
        try:
            newly_earned = db.evaluate_achievements(user_id)
        except Exception as e:
            print(f"[warn] achievement eval failed: {e}")
    return {"ok": True, "new_achievements": newly_earned}


@app.get("/api/achievements")
def list_achievements():
    """Public list of all available achievements."""
    return {"achievements": db.list_all_achievements()}


@app.get("/api/photo-evaluate/{eval_id}")
def get_photo_eval(eval_id: str):
    e = photo.get_evaluation(eval_id)
    if not e:
        raise HTTPException(status_code=404, detail="evaluation not found")
    return e


# ---------------------------------------------------------------------------
# Auth
# ---------------------------------------------------------------------------

class RegisterRequest(BaseModel):
    username: str
    password: str
    display_name: Optional[str] = None


class LoginRequest(BaseModel):
    username: str
    password: str


@app.post("/api/auth/register")
def register(req: RegisterRequest):
    if len(req.username) < 2 or len(req.username) > 32:
        raise HTTPException(status_code=400, detail="用户名长度 2-32")
    if len(req.password) < 4:
        raise HTTPException(status_code=400, detail="密码至少 4 位")
    if db.find_user_by_username(req.username):
        raise HTTPException(status_code=409, detail="用户名已被占用")
    uid = db.create_user(req.username, auth.hash_password(req.password), req.display_name)
    token = db.create_token(uid)
    return {
        "ok": True,
        "token": token,
        "user": {"id": uid, "username": req.username, "display_name": req.display_name or req.username},
    }


@app.post("/api/auth/login")
def login(req: LoginRequest):
    user = db.find_user_by_username(req.username)
    if not user or not auth.verify_password(req.password, user["password_hash"]):
        raise HTTPException(status_code=401, detail="用户名或密码错误")
    token = db.create_token(user["id"])
    return {
        "ok": True,
        "token": token,
        "user": {"id": user["id"], "username": user["username"], "display_name": user["display_name"]},
    }


@app.post("/api/auth/logout")
def logout(authorization: Optional[str] = Header(default=None)):
    if authorization and authorization.lower().startswith("bearer "):
        token = authorization[7:].strip()
        if token:
            db.delete_token(token)
    return {"ok": True}


@app.get("/api/auth/me")
def auth_me(current_user: dict = Depends(auth.get_current_user)):
    return {
        "id": current_user["id"],
        "username": current_user["username"],
        "display_name": current_user["display_name"],
        "created_at": current_user["created_at"],
    }


# ---------------------------------------------------------------------------
# User history ("/me/*")
# ---------------------------------------------------------------------------

@app.get("/api/me/checkins")
def me_checkins(current_user: dict = Depends(auth.get_current_user)):
    items = db.list_checkins_by_user(current_user["id"])
    return {"user_id": current_user["id"], "checkins": items}


@app.get("/api/me/photo-evals")
def me_photo_evals(current_user: dict = Depends(auth.get_current_user)):
    items = db.list_photo_evals_by_user(current_user["id"])
    return {"user_id": current_user["id"], "evals": items}


@app.get("/api/me/achievements")
def me_achievements(current_user: dict = Depends(auth.get_current_user)):
    """All achievements + which ones the user has earned."""
    catalog = db.list_all_achievements()
    earned = db.get_user_earned(current_user["id"])
    earned_ids = {a["id"] for a in earned}
    # Augment catalog with progress per achievement
    stats = db.get_user_stats_for_achievements(current_user["id"])
    items = []
    for a in catalog:
        is_earned = a["id"] in earned_ids
        current = stats.get(a["criteria_type"], 0)
        progress_pct = (
            min(100, int(100 * current / a["criteria_threshold"])) if a["criteria_threshold"] > 0 else 0
        )
        earned_record = next((e for e in earned if e["id"] == a["id"]), None)
        items.append(
            {
                **a,
                "earned": is_earned,
                "progress": progress_pct,
                "current_value": current,
                "earned_at": earned_record["earned_at"] if earned_record else None,
            }
        )
    return {
        "user_id": current_user["id"],
        "stats": stats,
        "achievements": items,
        "earned_count": sum(1 for i in items if i["earned"]),
    }


@app.get("/api/me/routes")
def me_routes(current_user: dict = Depends(auth.get_current_user)):
    items = db.list_routes_by_user(current_user["id"])
    return {"user_id": current_user["id"], "routes": items}


@app.get("/api/me/summary")
def me_summary(current_user: dict = Depends(auth.get_current_user)):
    checkins = db.list_checkins_by_user(current_user["id"])
    routes = db.list_routes_by_user(current_user["id"])
    photo_evals = db.list_photo_evals_by_user(current_user["id"])
    venue_ids = {c["venue_id"] for c in checkins}
    return {
        "user": {
            "id": current_user["id"],
            "username": current_user["username"],
            "display_name": current_user["display_name"],
        },
        "stats": {
            "checkins_count": len(checkins),
            "venues_visited": len(venue_ids),
            "routes_planned": len(routes),
            "photos_evaluated": len(photo_evals),
        },
        "recent_checkins": checkins[:5],
        "recent_routes": routes[:5],
        "recent_photos": [
            {
                "evaluation_id": e["evaluation_id"],
                "ts": e["ts"],
                "badge": e["payload"].get("badge", ""),
                "animal_guess": e["payload"].get("animal_guess", ""),
                "matched_venue_name": e["payload"].get("matched_venue_name", ""),
                "vibe_score": e["payload"].get("vibe_score", 0),
            }
            for e in photo_evals[:5]
        ],
    }


# ---------------------------------------------------------------------------
# Static frontend (production: after `npm run build`)
# ---------------------------------------------------------------------------

web_dist = Path(__file__).resolve().parent.parent / "Web" / "PWA" / "dist"
if web_dist.exists():
    app.mount("/", StaticFiles(directory=str(web_dist), html=True))