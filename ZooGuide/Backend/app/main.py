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

from . import auth, config, data_loader, db, geo, photo, planner
from .models import (
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
# Checkins (animal打卡)
# ---------------------------------------------------------------------------

class CheckinRequest(BaseModel):
    venue_id: str
    session_id: Optional[str] = None


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
    sid = req.session_id or str(uuid.uuid4())
    user_id = current_user["id"] if current_user else None
    record = db.insert_checkin(
        venue_id=venue.id,
        venue_name=venue.name,
        session_id=sid,
        user_id=user_id,
    )
    # Count user's/session's checkins
    if user_id:
        items = db.list_checkins_by_user(user_id)
    else:
        items = db.list_checkins_by_session(sid)
    total_venues = len(data_loader.get_all_venues())
    return {
        "ok": True,
        "session_id": sid,
        "total_checkins": len(items),
        "completion_rate": round(len(items) / total_venues, 3),
        "venue_name": venue.name,
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
    current_user: Optional[dict] = Depends(auth.get_current_user_optional),
):
    """Upload a photo, get a fun evaluation + auto-checkin."""
    contents = await file.read()
    if len(contents) > 8 * 1024 * 1024:
        raise HTTPException(status_code=413, detail="file too large (max 8MB)")
    suffix = Path(file.filename or "photo.jpg").suffix.lower()
    if suffix not in (".jpg", ".jpeg", ".png", ".webp", ".gif"):
        suffix = ".jpg"
    path = photo.save_photo(contents, suffix)
    result = photo.evaluate_photo(path)
    # Persist
    try:
        db.insert_photo_eval(
            evaluation_id=result["evaluation_id"],
            payload=result,
            image_path=result.get("image_path"),
            user_id=current_user["id"] if current_user else None,
        )
    except Exception as e:
        print(f"[warn] failed to persist photo_eval: {e}")
    return result


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