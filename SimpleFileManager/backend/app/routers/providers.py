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

from ..deps import state, LastMainError
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
    # Demoting the last main provider is refused: a new chat session
    # needs a main provider to land on. The check and the change are
    # done together, under one lock, so two administrators acting at
    # same time cannot each demote theirs and leave none behind.
    #
    # A provider that names no thinking budget follows the system
    # default, and the operator can put it back on the default by
    # clearing the field -- which arrives as an explicit null. Leaving
    # the field out of the request means "leave it as it is", so the
    # two are told apart by which fields the client actually sent.
    extra = {}
    if "reasoning_budget" in req.model_fields_set:
        extra["reasoning_budget"] = req.reasoning_budget
    try:
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
    except LastMainError as e:
        raise HTTPException(status_code=400, detail=str(e))
    if result is None:
        raise HTTPException(status_code=404, detail="provider 不存在")
    return result


@router.delete("/{provider_id}")
def delete_provider(provider_id: str, _admin=Depends(require_admin)) -> dict:
    # Deleting the last main provider is refused, same reason as the
    # demote above -- and the check and the deletion are done together
    # so two administrators cannot each delete theirs and leave none.
    try:
        deleted = state.get_chat_history().delete_provider(provider_id)
    except LastMainError as e:
        raise HTTPException(status_code=400, detail=str(e))
    if not deleted:
        raise HTTPException(status_code=404, detail="provider 不存在")
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
