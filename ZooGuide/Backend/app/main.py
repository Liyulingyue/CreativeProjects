"""FastAPI entry + routes."""

from __future__ import annotations

import uuid
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Optional

from fastapi import Depends, FastAPI, File, Form, Header, HTTPException, UploadFile
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel

from . import auth, chat as chat_mod, config, data_loader, db, geo, photo, planner
from .models import (
    ChatRequest,
    ChatResponse,
    Facility,
    PlanRequest,
    ReplanRequest,
    Route,
    VenueBrief,
)


@asynccontextmanager
async def lifespan(app: FastAPI):
    db.init_db()
    n = len(data_loader.get_all_venues())
    print(f"[startup] ZooGuide ready: {n} venues loaded; USE_LLM={config.USE_LLM}; DB initialized")
    yield


try:
    _app_desc = f"{data_loader.get_meta().get('name', '动物园')}省力 Agent"
except Exception:
    _app_desc = "ZooGuide省力 Agent"

app = FastAPI(
    title="ZooGuide API",
    description=_app_desc,
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
    quiz = data_loader.get_quiz_config()
    return {
        "party_types": quiz.get("party_types", []),
        "interests": quiz.get("interests", []),
        "gates": [
            {"value": k, "label": v.get("label", k), "desc": v.get("desc", "")}
            for k, v in data_loader.get_meta().get("gates", {}).items()
        ],
        "sliders": quiz.get("sliders", {}),
        "hike_options": quiz.get("hike_options", {}),
        "hike_terrain_hint": quiz.get("hike_terrain_hint", ""),
        "conditional_fields": quiz.get("conditional_fields", []),
        "required_fields": quiz.get("required_fields", []),
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
                area=v.area,
                description=v.description,
                open_time=v.open_time,
                close_time=v.close_time,
                narration=v.narration,
                seasonal_tips=v.seasonal_tips,
                keeper_talk=v.keeper_talk,
                near_gate=v.near_gate,
                lat=v.lat,
                lon=v.lon,
                neighbors=v.neighbors,
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
# Facilities
# ---------------------------------------------------------------------------

@app.get("/api/facilities")
def list_facilities(category: Optional[str] = None):
    facilities = data_loader.get_all_facilities()
    if category:
        facilities = [f for f in facilities if f.category == category]
    categories = data_loader.get_facility_categories()
    return {"facilities": [f.model_dump() for f in facilities], "categories": categories}


@app.get("/api/facilities/{facility_id}")
def get_facility(facility_id: str):
    f = data_loader.get_facility_by_id(facility_id)
    if not f:
        raise HTTPException(status_code=404, detail="facility not found")
    result = f.model_dump()
    if f.near_venue_id:
        v = data_loader.get_venue_dict_by_id(f.near_venue_id)
        if v:
            result["near_venue_name"] = v["name"]
    return result


# ---------------------------------------------------------------------------
# Plan (core)
# ---------------------------------------------------------------------------

@app.post("/api/plan")
async def plan(
    req: PlanRequest,
    current_user: Optional[dict] = Depends(auth.get_current_user_optional),
):
    try:
        print(f"[plan] 📋 Request: {req.model_dump_json()[:300]}")
        route, used_llm = await planner.plan_route(req, force_fast=req.fast)
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
        resp["_exit_gate"] = req.exit_gate
        resp["_start_time"] = req.start_time
        resp["_available_hours"] = req.available_hours
        resp["llm_used"] = used_llm
        return resp
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@app.post("/api/plan-variants")
async def plan_variants(req: PlanRequest):
    """Generate 2-3 alternative routes for comparison (always fast / rule-based)."""
    try:
        variants = await planner.plan_route_variants(req)
        return {
            "variants": variants,
            "prefs": req.model_dump(),
        }
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@app.post("/api/replan")
async def replan(req: ReplanRequest):
    try:
        route, used_llm = await planner.replan_route(req.original_route, req)
        resp = route.model_dump()
        resp["llm_used"] = used_llm
        for key in ("_party_type", "_with_kids", "_kids_age", "_stamina",
                     "_sun_tolerance", "_willing_to_hike", "_animal_interests",
                     "_entry_gate", "_exit_gate", "_start_time", "_available_hours"):
            if key in req.original_route:
                resp[key] = req.original_route[key]
        return resp
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


# ---------------------------------------------------------------------------
# Chat (natural-language replan)
# ---------------------------------------------------------------------------

@app.post("/api/chat")
async def chat_endpoint(req: ChatRequest):
    reply = await chat_mod.chat(req)
    return reply


# ---------------------------------------------------------------------------
# Checkins (animal打卡)
# ---------------------------------------------------------------------------

class CheckinRequest(BaseModel):
    venue_id: str
    session_id: Optional[str] = None
    source: Optional[str] = None


class CheckinRecord(BaseModel):
    venue_id: str
    venue_name: str
    ts: str


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
    note = f"source={req.source}" if req.source else None
    record = db.insert_checkin(
        venue_id=venue.id,
        venue_name=venue.name,
        session_id=sid,
        user_id=user_id,
        note=note,
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

@app.post("/api/photo-checkin")
async def photo_checkin(
    file: UploadFile = File(...),
    expected_venue_id: str = Form(...),
    session_id: Optional[str] = Form(None),
    thumbnail: Optional[str] = Form(None),
    preview: Optional[str] = Form(None),
    current_user: Optional[dict] = Depends(auth.get_current_user_optional),
):
    """Verify a photo matches the expected venue, auto-checkin on success."""
    contents = await file.read()
    if len(contents) > 8 * 1024 * 1024:
        raise HTTPException(status_code=413, detail="file too large (max 8MB)")
    suffix = Path(file.filename or "photo.jpg").suffix.lower()
    if suffix not in (".jpg", ".jpeg", ".png", ".webp", ".gif"):
        suffix = ".jpg"
    user_id = current_user["id"] if current_user else None
    sid = session_id or (f"u{user_id}" if user_id else "anon")

    expected_venue = data_loader.get_venue_dict_by_id(expected_venue_id)
    if not expected_venue:
        raise HTTPException(status_code=404, detail=f"venue not found: {expected_venue_id}")

    result = await photo.evaluate_photo_with_expected(
        contents, suffix,
        user_id=user_id,
        session_id=sid,
        auto_checkin=False,
        expected_venue=expected_venue,
    )

    matched = bool(result.get("is_match", False))
    result["success"] = matched
    result["expected_venue_id"] = expected_venue_id
    result["source"] = "checkin"
    result["thumbnail"] = thumbnail or ""
    result["preview"] = preview or ""

    if matched:
        result["auto_checkin"] = db.insert_checkin(
            venue_id=expected_venue["id"],
            venue_name=expected_venue["name"],
            session_id=sid,
            user_id=user_id,
            note=f"photo checkin {result['evaluation_id']}",
        )
    else:
        result["failure_reason"] = f"照片里没有 {expected_venue['name']}"

    if user_id and matched:
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


@app.post("/api/photo-evaluate")
async def photo_evaluate(
    file: UploadFile = File(...),
    session_id: Optional[str] = Form(None),
    thumbnail: Optional[str] = Form(None),
    preview: Optional[str] = Form(None),
    current_user: Optional[dict] = Depends(auth.get_current_user_optional),
):
    """Fun evaluation for photo wall — no venue verification."""
    contents = await file.read()
    if len(contents) > 8 * 1024 * 1024:
        raise HTTPException(status_code=413, detail="file too large (max 8MB)")
    suffix = Path(file.filename or "photo.jpg").suffix.lower()
    if suffix not in (".jpg", ".jpeg", ".png", ".webp", ".gif"):
        suffix = ".jpg"
    user_id = current_user["id"] if current_user else None
    sid = session_id or (f"u{user_id}" if user_id else "anon")

    result = await photo.evaluate_photo_for_wall(contents, suffix)
    result["source"] = "wall"
    result["thumbnail"] = thumbnail or ""
    result["preview"] = preview or ""

    if user_id:
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


class ClaimSessionRequest(BaseModel):
    session_id: str


@app.post("/api/auth/claim-session")
def claim_session(
    req: ClaimSessionRequest,
    current_user: dict = Depends(auth.get_current_user),
):
    result = db.claim_session_data(req.session_id, current_user["id"])
    return {"ok": True, **result}


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


@app.get("/api/me/visited-venue-ids")
def me_visited_venue_ids(current_user: dict = Depends(auth.get_current_user)):
    items = db.list_checkins_by_user(current_user["id"])
    venue_ids = list({c["venue_id"] for c in items if c.get("venue_id")})
    return {"user_id": current_user["id"], "venue_ids": venue_ids}


@app.get("/api/me/photo-evals")
def me_photo_evals(current_user: dict = Depends(auth.get_current_user)):
    items = db.list_photo_evals_by_user(current_user["id"])
    return {"user_id": current_user["id"], "evals": items}


@app.get("/api/session/photo-evals")
def session_photo_evals(session_id: str, current_user: Optional[dict] = Depends(auth.get_current_user_optional)):
    uid = current_user["id"] if current_user else None
    items = db.list_photo_evals_by_session(session_id, uid)
    return {"session_id": session_id, "evals": items}


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


@app.get("/api/me/summary")
def me_summary(current_user: dict = Depends(auth.get_current_user)):
    checkins = db.list_checkins_by_user(current_user["id"])
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
            "photos_evaluated": len(photo_evals),
        },
        "recent_checkins": checkins[:5],
        "recent_photos": [
            {
                "evaluation_id": e["eval_id"],
                "ts": e["created_at"],
                "is_match": bool(e.get("is_match", False)),
                "desc": e.get("desc", "") or "",
                "matched_venue_name": e.get("matched_venue_name", ""),
                "score": e.get("score", 0),
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

dl_dir = Path(__file__).resolve().parent.parent / "data" / "downloads"
if dl_dir.exists():
    from starlette.routing import Mount
    from starlette.staticfiles import StaticFiles as StarletteStaticFiles
    app.routes.insert(0, Mount("/downloads", app=StarletteStaticFiles(directory=str(dl_dir), html=False)))