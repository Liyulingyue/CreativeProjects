import time
from fastapi import APIRouter
from ..deps import state
from ..indexer import indexer
from ..models import AppSettings, IndexStats, IndexStatus, AutoIndexStatus

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


@settings.get("/auto_index_status")
def get_auto_index_status() -> AutoIndexStatus:
    return indexer.status()


@settings.post("/auto_index/run")
def run_auto_index_now() -> dict:
    return indexer.scan()


@settings.get("/service_health")
def service_health() -> dict:
    """Lightweight reachability check for LLM and Embedding endpoints."""
    return _probe_services()


@settings.post("/test_connection")
def test_connection() -> dict:
    """Actively call both services and report latency + embedding dimension."""
    result = {}
    for key, probe in (("llm", _probe_llm), ("embedding", _probe_embedding)):
        t0 = time.time()
        ok, detail = probe()
        result[key] = {
            "ok": ok,
            "detail": detail,
            "latency_ms": int((time.time() - t0) * 1000),
        }
    return result


def _probe_services() -> dict:
    return {
        "llm": {"ok": _probe_llm()[0]},
        "embedding": {"ok": _probe_embedding()[0]},
    }


def _probe_llm() -> tuple[bool, str]:
    try:
        from ..llm_client import chat_completion
        result = chat_completion(
            messages=[{"role": "user", "content": "ping"}],
            temperature=0,
        )
        content = (result.get("content") or "").strip()
        return True, content[:60] or "ok"
    except Exception as e:
        return False, str(e)[:200]


def _probe_embedding() -> tuple[bool, str]:
    settings_data = state.get_settings()
    try:
        from ..llm_client import get_embedding
        vec = get_embedding("ping")
        dim = len(vec)
        actual = _dim_matches(dim, settings_data.embedding_dim)
        if not actual:
            return False, f"维度不匹配：服务返回 {dim}，设置为 {settings_data.embedding_dim}，请修改 EMBEDDING_DIM"
        return True, f"维度 {dim}"
    except Exception as e:
        return False, str(e)[:200]


def _dim_matches(actual: int, configured: str) -> bool:
    if configured in ("AUTO", ""):
        return True
    try:
        return int(configured) == actual
    except ValueError:
        return True
