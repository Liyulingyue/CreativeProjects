"""Embedding providers CRUD — the API for the dynamic list in
/settings. Mirrors routers/providers.py for the LLM side, with
two role singletons (is_index, is_search) instead of one.

Endpoints:
  GET    /api/embedding_providers            list all providers
  POST   /api/embedding_providers            create; first row auto-promoted to is_index
  PATCH  /api/embedding_providers/{id}       update fields and/or roles
  DELETE /api/embedding_providers/{id}       remove a provider (refused for the only is_index)
"""
from typing import Optional

from fastapi import APIRouter, HTTPException

from ..deps import state
from ..models import EmbeddingProvider, EmbeddingProviderCreate, EmbeddingProviderUpdate

router = APIRouter(tags=["embedding_providers"])


@router.get("", response_model=list[EmbeddingProvider])
def list_embedding_providers() -> list[EmbeddingProvider]:
    return state.get_chat_history().list_embedding_providers()


@router.post("", response_model=EmbeddingProvider, status_code=201)
def create_embedding_provider(req: EmbeddingProviderCreate) -> EmbeddingProvider:
    if not req.name.strip() or not req.base_url.strip() or not req.model.strip():
        raise HTTPException(status_code=400, detail="name / base_url / model 不能为空")
    return state.get_chat_history().create_embedding_provider(
        req.name.strip(),
        req.base_url.strip(),
        req.model.strip(),
        dim=(req.dim or "AUTO").strip(),
        api_key=req.api_key,
    )


@router.patch("/{provider_id}", response_model=EmbeddingProvider)
def update_embedding_provider(
    provider_id: str, req: EmbeddingProviderUpdate,
) -> EmbeddingProvider:
    chat_svc = state.get_chat_history()
    # Refuse to demote the only is_index. is_search is allowed to be
    # empty, so the unset case is always fine.
    if req.is_index is False:
        target = chat_svc.get_embedding_provider(provider_id)
        if target is None:
            raise HTTPException(status_code=404, detail="provider 不存在")
        if target.is_index:
            with chat_svc._lock:  # type: ignore[attr-defined]
                conn = chat_svc._get_conn()  # type: ignore[attr-defined]
                others = conn.execute(
                    "SELECT COUNT(*) FROM embedding_providers"
                    " WHERE is_index = 1 AND id != ?",
                    (provider_id,),
                ).fetchone()[0]
                conn.close()
            if others == 0:
                raise HTTPException(
                    status_code=400,
                    detail="至少要有一个 index provider；先在另一个上设 index",
                )
    result = chat_svc.update_embedding_provider(
        provider_id,
        name=req.name,
        base_url=req.base_url,
        model=req.model,
        dim=req.dim,
        api_key=req.api_key,
        is_index=req.is_index,
        is_search=req.is_search,
    )
    if result is None:
        raise HTTPException(status_code=404, detail="provider 不存在")
    # Cache invalidation: any role/endpoint change must drop the
    # cached EmbeddingService so the next request picks up the new
    # base_url/model. Field-only changes (name/dim/api_key) also
    # invalidate — dim change is the loud case the operator needs
    # to see land before the next embed() call.
    state.invalidate_embedding_caches()
    return result


@router.delete("/{provider_id}")
def delete_embedding_provider(provider_id: str) -> dict:
    chat_svc = state.get_chat_history()
    target = chat_svc.get_embedding_provider(provider_id)
    if target is None:
        raise HTTPException(status_code=404, detail="provider 不存在")
    if target.is_index:
        with chat_svc._lock:  # type: ignore[attr-defined]
            conn = chat_svc._get_conn()  # type: ignore[attr-defined]
            others = conn.execute(
                "SELECT COUNT(*) FROM embedding_providers WHERE is_index = 1"
            ).fetchone()[0]
            conn.close()
        if others <= 1:
            raise HTTPException(
                status_code=400, detail="至少要有一个 index provider",
            )
    deleted = chat_svc.delete_embedding_provider(provider_id)
    state.invalidate_embedding_caches()
    return {"deleted": deleted}
