#!/usr/bin/env python
"""minicoder/server/app.py — FastAPI Web Server logic."""
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from typing import List, Optional, Dict
from ..agent import MiniCoderAgent

app = FastAPI(title="MiniCoder Plus API")
agent = MiniCoderAgent()

class ChatRequest(BaseModel):
    prompt: str
    history: Optional[List[Dict]] = None

class ChatResponse(BaseModel):
    response: str
    history: List[Dict]

@app.get("/")
async def root():
    return {"status": "ok", "message": "MiniCoder Plus API is running"}

@app.post("/chat", response_model=ChatResponse)
async def chat(request: ChatRequest):
    try:
        history = request.history or []
        response_text = agent.run(request.prompt, history)
        return ChatResponse(response=response_text, history=history)
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))

def run_server(host="0.0.0.0", port=8000):
    import uvicorn
    uvicorn.run(app, host=host, port=port)
