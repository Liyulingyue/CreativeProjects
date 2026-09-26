from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from ..llm_client import chat_completion

chat = APIRouter()


class ChatRequest(BaseModel):
    message: str
    system_prompt: str = "你是一个有用的AI助手。"


class ChatResponse(BaseModel):
    response: str


@chat.post("/query")
def chat_query(req: ChatRequest) -> ChatResponse:
    try:
        result = chat_completion(
            messages=[
                {"role": "system", "content": req.system_prompt},
                {"role": "user", "content": req.message},
            ],
            temperature=0.7,
        )
        return ChatResponse(response=result.get("content", ""))
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))
