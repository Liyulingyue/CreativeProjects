import time
from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel
from ..deps import state
from .auth import require_admin
from ..cpu_pool import CpuPoolBusy
from ..image_index_scheduler import _sane_window
from ..indexer import ACTIVE_HOURS_ALL, _parse_window, indexer
from ..models import AppSettings, IndexStats, IndexStatus, AutoIndexStatus

settings = APIRouter()


@settings.get("")
def get_settings() -> AppSettings:
    # Both LLM and embedding credentials now live in their own
    # providers tables (llm_providers / embedding_providers). Nothing
    # sensitive in AppSettings needs the "***" mask any more.
    return state.get_settings()


@settings.post("")
def update_settings(updates: dict, _admin=Depends(require_admin)) -> AppSettings:
    # A window that does not parse is treated as all-day, which for the
    # picture index means it never stops. Better to say so than to accept
    # a setting that quietly behaves the opposite of what was asked.
    window = updates.get("image_index_window")
    if window is not None and window != ACTIVE_HOURS_ALL:
        # The same rule the scheduler applies: a window the scheduler
        # would refuse must not be accepted here either, or the setting
        # would be taken and then quietly replaced.
        if _parse_window(str(window)) is None or not _sane_window(str(window)):
            raise HTTPException(
                status_code=400,
                detail="时间窗口格式应为 HH:MM-HH:MM，例如 09:00-09:30（或 all 表示不限时）")
    return state.update_settings(updates)


@settings.get("/index_stats")
def get_index_stats() -> IndexStats:
    return state.get_index_stats()


@settings.get("/index_skipped")
def get_index_skipped() -> dict:
    """Per-file detail for what the indexer could not index, and why."""
    return indexer.skipped_files()


@settings.get("/index_status")
def get_index_status() -> IndexStatus:
    return state.get_index_status()


@settings.get("/auto_index_status")
def get_auto_index_status() -> AutoIndexStatus:
    status = indexer.status()
    status.watcher = indexer.watch_status()
    return status


class PauseRequest(BaseModel):
    seconds: int = 0


@settings.post("/auto_index/pause")
def pause_auto_index(req: PauseRequest, _admin=Depends(require_admin)) -> dict:
    """Pause indexing. seconds=0 pauses until resumed manually."""
    remaining = indexer.pause(req.seconds)
    return {"paused": True, "seconds": remaining, "indefinite": remaining < 0}


@settings.post("/auto_index/resume")
def resume_auto_index(_admin=Depends(require_admin)) -> dict:
    indexer.resume()
    return {"paused": False, "seconds": 0, "indefinite": False}


@settings.post("/auto_index/run")
def run_auto_index_now(_admin=Depends(require_admin)) -> dict:
    """Kick off a scan without waiting for it.

    scan() is synchronous and a cold corpus can take minutes; running it inline
    saturated the event loop and made the whole API unresponsive (requests
    timing out) until it finished.
    """
    if indexer.is_scanning:
        return {"started": False, "reason": "already scanning"}
    indexer.scan_async()
    return {"started": True}


@settings.get("/service_health")
def service_health() -> dict:
    """Lightweight reachability check for LLM and Embedding endpoints."""
    return _probe_services()


@settings.post("/test_connection")
def test_connection() -> dict:
    """Actively call all reachable services and report latency. The fast
    LLM is best-effort — when the operator has not configured a base
    URL (or the 350M llama-server isn't running) the probe returns
    ok=False with a clear "not configured" message rather than 500-ing
    the whole request."""
    result = {}
    for key, probe in (("llm", _probe_llm), ("embedding", _probe_embedding), ("embedding_search", _probe_search_embedding)):
        t0 = time.time()
        ok, detail = probe()
        result[key] = {
            "ok": ok,
            "detail": detail,
            "latency_ms": int((time.time() - t0) * 1000),
        }
    return result


def _probe_services() -> dict:
    from ..health import check_memory

    return {
        "llm": {"ok": _probe_llm()[0]},
        "embedding": {"ok": _probe_embedding()[0]},
        # How much room the machine has left. The models take most of
        # it, and when it runs out the kernel starts killing processes --
        # this one included.
        "memory": check_memory(),
    }


def _probe_llm() -> tuple[bool, str]:
    """Probe the main provider. Returns ok=True with a short content
    excerpt on success; the agent's chat endpoint will report latency
    on the badge. With the providers refactor, the "main" is the
    is_main=1 row in llm_providers — when none is configured yet
    the probe returns ok=False with a clear "add a provider" hint.
    """
    chat_svc = state.get_chat_history()
    provider = chat_svc.get_main_provider()
    if provider is None:
        return False, "未配置（请到 /settings 添加 provider）"
    try:
        from ..llm_client import chat_completion
        result = chat_completion(
            messages=[{"role": "user", "content": "ping"}],
            temperature=0,
            # only the first token is needed to prove the server answers
            max_tokens=8,
            provider=provider,
            reasoning_budget=provider.reasoning_budget,
            # The sidebar asks for this every minute. Waiting a minute for
            # the cores would hold the whole server up, and a probe that
            # times out would show the model as broken when it is merely
            # busy.
            pool_timeout=2.0,
            request_timeout=8.0,
        )
        content = (result.get("content") or "").strip()
        return True, content[:60] or "ok"
    except CpuPoolBusy:
        # Busy is not broken. The model is answering somebody else.
        return True, "模型正忙（有人在用）"
    except Exception as e:
        from ..llm_client import _friendly_error
        return False, _friendly_error(e)[:200]


def _probe_embedding() -> tuple[bool, str]:
    """Probe the is_index embedding provider. When the row declares
    'AUTO' we just report the detected dim; when it's pinned the
    probe verifies the actual response dim matches what the row
    declared, so a misconfigured row is loud rather than silent."""
    chat_svc = state.get_chat_history()
    idx = chat_svc.get_index_embedding_provider()
    if idx is None:
        return False, "未配置（请到 /settings 添加 provider）"
    try:
        from ..llm_client import get_embedding
        vec = get_embedding(
            "ping",
            base_url=idx.base_url,
            api_key=idx.api_key,
            model=idx.model,
        )
        dim = len(vec)
        actual = _dim_matches(dim, idx.dim)
        if not actual:
            return False, f"维度不匹配：服务返回 {dim}，设置为 {idx.dim}，请修改 dim"
        return True, f"维度 {dim}"
    except Exception as e:
        return False, str(e)[:200]


def _probe_search_embedding() -> tuple[bool, str]:
    """Probe the search-time embedding path. When the search role
    is empty the system falls back to the index row, so we report
    that explicitly; when the dedicated search row exists we
    verify it answers. The dim must match the index row because
    both share the same vector space — a mismatch surfaces as
    probe failure rather than a runtime crash."""
    chat_svc = state.get_chat_history()
    idx = chat_svc.get_index_embedding_provider()
    search = chat_svc.get_search_embedding_provider()
    if search is None:
        return True, "未配置（共用 index）"
    if idx is not None and search.id == idx.id:
        return True, "未配置（共用 index）"
    try:
        from ..llm_client import get_embedding
        vec = get_embedding(
            "ping",
            base_url=search.base_url,
            api_key=search.api_key,
            model=search.model,
        )
        dim = len(vec)
        if idx is not None:
            try:
                declared = int(idx.dim)
            except (TypeError, ValueError):
                declared = 0
            if declared and dim != declared:
                return False, (
                    f"维度不匹配：search 服务返回 {dim}，index 设置为 {idx.dim}，两者必须一致"
                )
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
