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
from ..core.terminal_manager import terminal_manager
from ..tools import CodeTools
from fastapi import WebSocket, WebSocketDisconnect

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
    session_id: Optional[str] = None

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
        
        # Link session if provided
        if request.session_id:
            session = terminal_manager.get_or_create_session(request.session_id)
            CodeTools.set_active_session(session)
        else:
            CodeTools.set_active_session(None)

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

@app.websocket("/ws/terminal/{session_id}")
async def terminal_websocket(websocket: WebSocket, session_id: str):
    """WebSocket endpoint to connect to a PTY session."""
    await websocket.accept()
    
    # Get or create the terminal session
    session = terminal_manager.get_or_create_session(session_id)
    # Register a dedicated queue for this websocket connection
    output_queue = session.get_output_queue()
    
    # Task to read from PTY and send to WebSocket
    async def downlink():
        try:
            while True:
                data = await output_queue.get()
                if data:
                    await websocket.send_text(data)
        except Exception as e:
            # print(f"Downlink error: {e}")
            pass
        finally:
            session.remove_queue(output_queue)

    # Task to read from WebSocket and write to PTY
    async def uplink():
        try:
            while True:
                message = await websocket.receive_text()
                if not message:
                    continue
                
                # Intercept JSON Control Messages (Resize, etc.)
                if message.startswith('{') and message.endswith('}'):
                    try:
                        cmd = json.loads(message)
                        if cmd.get("type") == "resize":
                            session.resize(cmd["cols"], cmd["rows"])
                            continue
                        elif cmd.get("type") == "input":
                            await session.write(cmd["text"])
                            continue
                    except:
                        pass # Fall through to raw write if parsing fails
                
                # Default: raw write to PTY
                await session.write(message)
        except WebSocketDisconnect:
            pass
        except Exception as e:
            # print(f"Uplink error: {e}")
            pass

    # Run both concurrently
    await asyncio.wait(
        [asyncio.create_task(downlink()), asyncio.create_task(uplink())],
        return_when=asyncio.FIRST_COMPLETED
    )

def run_server(host="0.0.0.0", port=8000):
    import uvicorn
    uvicorn.run(app, host=host, port=port)
