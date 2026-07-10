"""FastAPI entry + routes."""

from __future__ import annotations

import uuid
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Optional

from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel

from . import config
from . import data_loader
from . import planner
from .models import (
    PlanRequest,
    ReplanRequest,
    Route,
    VenueBrief,
)


# In-memory checkin store (process-local; resets on restart)
_checkins: dict[str, list[dict]] = {}


@asynccontextmanager
async def lifespan(app: FastAPI):
    n = len(data_loader.get_all_venues())
    print(f"[startup] ZooGuide ready: {n} venues loaded; USE_LLM={config.USE_LLM}")
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
def plan(req: PlanRequest):
    try:
        route, used_llm = planner.plan_route(req)
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
def checkin(req: CheckinRequest):
    venue = data_loader.get_venue_by_id(req.venue_id)
    if not venue:
        raise HTTPException(status_code=404, detail="venue not found")
    sid = req.session_id or str(uuid.uuid4())
    from datetime import datetime
    _checkins.setdefault(sid, []).append(
        {"venue_id": venue.id, "venue_name": venue.name, "ts": datetime.now().isoformat(timespec="seconds")}
    )
    total = len(data_loader.get_all_venues())
    return {
        "ok": True,
        "session_id": sid,
        "total_checkins": len(_checkins[sid]),
        "completion_rate": round(len(_checkins[sid]) / total, 3),
        "venue_name": venue.name,
    }


@app.get("/api/checkin/{session_id}")
def get_checkins(session_id: str):
    items = _checkins.get(session_id, [])
    total = len(data_loader.get_all_venues())
    return {
        "session_id": session_id,
        "checkins": items,
        "completion_rate": round(len(items) / total, 3) if total else 0,
    }


# ---------------------------------------------------------------------------
# Static frontend (production: after `npm run build`)
# ---------------------------------------------------------------------------

web_dist = Path(__file__).resolve().parent.parent / "Web" / "PWA" / "dist"
if web_dist.exists():
    app.mount("/", StaticFiles(directory=str(web_dist), html=True))