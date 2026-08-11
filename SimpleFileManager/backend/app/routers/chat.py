from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from ..deps import state

chat = APIRouter()


class ChatRequest(BaseModel):
    message: str
    system_prompt: str = "你是一个有用的AI助手。"


class ChatResponse(BaseModel):
    response: str


@chat.post("/query")
def chat_query(req: ChatRequest) -> ChatResponse:
    try:
        settings = state.get_settings()
        import httpx
        headers = {"Authorization": f"Bearer {settings.llm_api_key}"} if settings.llm_api_key else {}
        resp = httpx.post(
            settings.llm_base_url,
            headers=headers,
            json={
                "model": settings.llm_model,
                "messages": [
                    {"role": "system", "content": req.system_prompt},
                    {"role": "user", "content": req.message}
                ],
                "temperature": 0.7,
            },
            timeout=60,
        )
        resp.raise_for_status()
        data = resp.json()
        return ChatResponse(response=data["choices"][0]["message"]["content"])
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))
