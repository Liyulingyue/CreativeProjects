#!/usr/bin/env python
"""minicoder/server/app.py — FastAPI Web Server logic."""
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import StreamingResponse
from pydantic import BaseModel
from typing import List, Optional, Dict
import asyncio
import json
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

@app.post("/chat")
async def chat(request: ChatRequest, stream: bool = False):
    try:
        history = request.history or []

        # Non-streaming (existing behavior)
        if not stream:
            response_text = agent.run(request.prompt, history)
            return ChatResponse(response=response_text, history=history)

        # Streaming mode: push incremental history updates as SSE events
        queue: asyncio.Queue = asyncio.Queue()
        loop = asyncio.get_running_loop()

        def _on_update(item: Dict):
            # Safe way to push to queue from a worker thread
            loop.call_soon_threadsafe(queue.put_nowait, item)

        async def event_generator():
            # Run the blocking agent.run in a thread
            run_task = asyncio.to_thread(agent.run, request.prompt, history, _on_update)
            # Wrap as a task so we can check discovery
            task = asyncio.create_task(run_task)

            while True:
                try:
                    # Wait for items with a timeout to allow checking if task is done
                    item = await asyncio.wait_for(queue.get(), timeout=0.1)
                    yield f"data: {json.dumps({'type': 'update', 'item': item})}\n\n"
                except asyncio.TimeoutError:
                    # If task is done and queue is empty, we are finished
                    if task.done() and queue.empty():
                        break
                except asyncio.CancelledError:
                    task.cancel()
                    break

            # Ensure task result is fetched (and exceptions raised)
            await task
            
            # Final payload with full history consistency
            yield f"data: {json.dumps({'type': 'done', 'history': history})}\n\n"

        return StreamingResponse(event_generator(), media_type="text/event-stream")

    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))

def run_server(host="0.0.0.0", port=8000):
    import uvicorn
    uvicorn.run(app, host=host, port=port)
