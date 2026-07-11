from fastapi import APIRouter
from ..deps import state
from ..models import AppSettings, Stats

router = APIRouter(prefix="/api", tags=["settings"])


@router.get("/settings", response_model=AppSettings)
def get_settings():
    return state.get_settings()


@router.put("/settings", response_model=AppSettings)
def update_settings(settings: AppSettings):
    return state.update_settings(settings.model_dump())


@router.get("/stats", response_model=Stats)
def get_stats():
    return Stats(**state.get_stats())
