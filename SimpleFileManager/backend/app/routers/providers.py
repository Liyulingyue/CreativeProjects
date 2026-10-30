"""LLM providers CRUD — the API for the dynamic provider list in
/settings. Each row in `llm_providers` corresponds to one LLM endpoint
the operator has configured; the agent flow routes through whichever
provider the chat session (or the per-turn override) picks.

Endpoints:
  GET    /api/providers            list all providers
  POST   /api/providers            create a new provider
  PATCH  /api/providers/{id}      update fields; is_main is a singleton
  DELETE /api/providers/{id}      remove a provider (refused for main)

The router delegates persistence to ChatHistoryService.* on the
AppState singleton so the methods share the same lock as chat_history
operations. SQLite serializes writers, so the read-modify-write for
the is_main singleton is safe.
"""
import uuid
from typing import Optional

from fastapi import APIRouter, Depends, HTTPException

from ..deps import state
from .auth import require_admin
from ..models import LLMProvider, LLMProviderCreate, LLMProviderUpdate

router = APIRouter(tags=["providers"])


@router.get("", response_model=list[LLMProvider])
def list_providers(_admin=Depends(require_admin)) -> list[LLMProvider]:
    return state.get_chat_history().list_providers()


@router.post("", response_model=LLMProvider, status_code=201)
def create_provider(req: LLMProviderCreate, _admin=Depends(require_admin)) -> LLMProvider:
    if not req.name.strip() or not req.base_url.strip() or not req.model.strip():
        raise HTTPException(status_code=400, detail="name / base_url / model 不能为空")
    return state.get_chat_history().create_provider(
        req.name.strip(), req.base_url.strip(), req.model.strip(), req.api_key,
        max_tokens=req.max_tokens,
        reasoning_budget=req.reasoning_budget,
    )


@router.patch("/{provider_id}", response_model=LLMProvider)
def update_provider(provider_id: str, req: LLMProviderUpdate, _admin=Depends(require_admin)) -> LLMProvider:
    # Reject demoting the only main — leaves the system with no default
    # for new sessions. The router layer is the right place to catch
    # this because the service layer doesn't know "is the last main?".
    if req.is_main is False:
        svc = state.get_chat_history()
        with svc._lock:  # type: ignore[attr-defined]
            conn = svc._get_conn()  # type: ignore[attr-defined]
            others_main = conn.execute(
                "SELECT COUNT(*) FROM llm_providers WHERE is_main = 1 AND id != ?",
                (provider_id,),
            ).fetchone()[0]
            conn.close()
        if others_main == 0:
            raise HTTPException(status_code=400, detail="至少要有一个 main provider；先在另一个上设 main")
    # A provider that names no thinking budget follows the system
    # default, and the operator can put it back on the default by
    # clearing the field -- which arrives as an explicit null. Leaving
    # the field out of the request means "leave it as it is", so the
    # two are told apart by which fields the client actually sent.
    extra = {}
    if "reasoning_budget" in req.model_fields_set:
        extra["reasoning_budget"] = req.reasoning_budget
    result = state.get_chat_history().update_provider(
        provider_id,
        name=req.name,
        base_url=req.base_url,
        model=req.model,
        api_key=req.api_key,
        is_main=req.is_main,
        max_tokens=req.max_tokens,
        **extra,
    )
    if result is None:
        raise HTTPException(status_code=404, detail="provider 不存在")
    return result


@router.delete("/{provider_id}")
def delete_provider(provider_id: str, _admin=Depends(require_admin)) -> dict:
    svc = state.get_chat_history()
    # Reject deleting the only main; same reason as the PATCH demote
    # above (no default would be left).
    target = svc.get_provider(provider_id)
    if target is None:
        raise HTTPException(status_code=404, detail="provider 不存在")
    if target.is_main:
        with svc._lock:  # type: ignore[attr-defined]
            conn = svc._get_conn()  # type: ignore[attr-defined]
            others_main = conn.execute(
                "SELECT COUNT(*) FROM llm_providers WHERE is_main = 1"
            ).fetchone()[0]
            conn.close()
        if others_main <= 1:
            raise HTTPException(status_code=400, detail="至少要有一个 main provider")
    deleted = svc.delete_provider(provider_id)
    return {"deleted": deleted}


@router.post("/{provider_id}/set_main", response_model=LLMProvider)
def set_main(provider_id: str, _admin=Depends(require_admin)) -> LLMProvider:
    """Convenience endpoint for the \"Set as main\" button in the UI.

    Equivalent to PATCH {is_main: true} but reads better as a verb and
    skips the body. Goes through the same singleton-enforcement path
    in the service.
    """
    result = state.get_chat_history().update_provider(provider_id, is_main=True)
    if result is None:
        raise HTTPException(status_code=404, detail="provider 不存在")
    return result
