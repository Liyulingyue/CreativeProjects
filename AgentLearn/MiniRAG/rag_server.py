from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from typing import List, Dict, Any
import sys
import os
sys.path.append(os.path.dirname(__file__))
from my_rag import chat

app = FastAPI(title="MiniRAG API", description="Simple RAG Chat API")

class ChatRequest(BaseModel):
    prompt: str
    history: List[Dict[str, Any]] = []

class ChatResponse(BaseModel):
    response: str

@app.post("/chat", response_model=ChatResponse)
async def chat_endpoint(request: ChatRequest):
    try:
        response = chat(request.prompt, request.history)
        return ChatResponse(response=response)
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)