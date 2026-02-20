#!/usr/bin/env python
"""minicoder/server/app.py — FastAPI Web Server logic."""
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
from typing import List, Optional, Dict
from ..agents.agent import MiniCoderAgent
from ..core.settings import settings

app = FastAPI(title="MiniCoder Plus API")

# Add CORS Middleware
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],  # For development, allow all. Change this for production.
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

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
