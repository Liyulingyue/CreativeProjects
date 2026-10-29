from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from ..deps import state, resolve_reasoning_budget
from ..llm_client import chat_completion

chat = APIRouter()


class ChatRequest(BaseModel):
    message: str
    system_prompt: str = "你是一个有用的AI助手。"
    # Optional override; falls back to the main provider.
    provider_id: str | None = None


class ChatResponse(BaseModel):
    response: str
    # The model emits a separate reasoning channel before its answer. Dropping
    # it made the model look like it answered without thinking, which is exactly
    # the case where seeing the reasoning matters most.
    reasoning: str = ""
    provider_id: str = ""
    provider_name: str = ""


@chat.post("/query")
def chat_query(req: ChatRequest) -> ChatResponse:
    # Resolve the provider: request override wins, else main. Same
    # precedence as the agent flow so callers can predict which model
    # they're talking to.
    chat_svc = state.get_chat_history()
    provider = None
    if req.provider_id:
        provider = chat_svc.get_provider(req.provider_id)
    if provider is None:
        provider = chat_svc.get_main_provider()
    if provider is None:
        raise HTTPException(
            status_code=400,
            detail="没有可用的 LLM provider；请到 /settings 添加"
        )
    try:
        result = chat_completion(
            messages=[
                {"role": "system", "content": req.system_prompt},
                {"role": "user", "content": req.message},
            ],
            temperature=0.7,
            provider=provider,
            reasoning_budget=resolve_reasoning_budget(provider, state.get_settings()),
        )
        return ChatResponse(
            response=result.get("content", ""),
            reasoning=result.get("reasoning_content") or "",
            provider_id=provider.id,
            provider_name=provider.name,
        )
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))
