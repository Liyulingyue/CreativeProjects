from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from typing import List, Dict, Any
import sys
import os
sys.path.append(os.path.dirname(__file__))
from AgentLearn.MiniRAG.mini_rag import chat, chat_stream

app = FastAPI(title="MiniRAG API", description="Simple RAG Chat API")

class ChatRequest(BaseModel):
    prompt: str
    history: List[Dict[str, Any]] = []

class ChatResponse(BaseModel):
    response: str

@app.post("/chat", response_model=ChatResponse)
async def chat_endpoint(request: ChatRequest):
    try:
        # Use streaming version to get thinking process
        response = ""
        for chunk in chat_stream(request.prompt, request.history):
            if chunk.get("type") == "final":
                response = chunk.get("content", "")
        return ChatResponse(response=response)
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)