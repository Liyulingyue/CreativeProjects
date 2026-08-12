from fastapi import APIRouter
from ..deps import state
from ..models import AppSettings, IndexStats, IndexStatus

settings = APIRouter()


@settings.get("")
def get_settings() -> AppSettings:
    s = state.get_settings()
    s.llm_api_key = "***" if s.llm_api_key else ""
    s.embedding_api_key = "***" if s.embedding_api_key else ""
    return s


@settings.post("")
def update_settings(updates: dict) -> AppSettings:
    if "llm_api_key" in updates and updates["llm_api_key"] == "***":
        updates.pop("llm_api_key")
    if "embedding_api_key" in updates and updates["embedding_api_key"] == "***":
        updates.pop("embedding_api_key")
    return state.update_settings(updates)


@settings.get("/index_stats")
def get_index_stats() -> IndexStats:
    return state.get_index_stats()


@settings.get("/index_status")
def get_index_status() -> IndexStatus:
    return state.get_index_status()
