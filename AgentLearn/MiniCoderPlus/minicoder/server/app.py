#!/usr/bin/env python
"""minicoder/server/app.py — FastAPI Web Server logic."""
from fastapi import FastAPI, HTTPException, APIRouter
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import StreamingResponse
from pydantic import BaseModel
from typing import List, Optional, Dict
import asyncio
import json
from ..agents.agent import MiniCoderAgent
from ..core.settings import settings
from ..core.terminal_manager import terminal_manager
from ..tools import CodeTools, set_current_workspace
from fastapi import WebSocket, WebSocketDisconnect
from pathlib import Path

app = FastAPI(title="MiniCoder Plus API")
api_router = APIRouter(prefix="/api/v1")

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
    workspace: Optional[str] = None # Optional custom workspace path

class ChatResponse(BaseModel):
    response: str
    history: List[Dict]

@app.get("/")
async def root():
    return {"status": "ok", "message": "MiniCoder Plus API is running"}


@api_router.get("/workspace/resolve")
async def resolve_workspace(path: Optional[str] = None):
    """Resolve a workspace identifier to an absolute path."""
    # default to configured workspace_dir
    if not path or path.lower() == "none":
        return {"absolute": str(settings.WORKSPACE_DIR)}

    candidate = Path(path)
    if not candidate.is_absolute():
        candidate = settings.BASE_DIR / candidate
    return {"absolute": str(candidate)}

@api_router.get("/files/list")
async def list_files(path: Optional[str] = None):
    """List files in the workspace or at a specific path."""
    # Resolve root
    root_path = settings.WORKSPACE_DIR
    if path:
        root_path = Path(path)
        if not root_path.is_absolute():
            root_path = settings.BASE_DIR / root_path

    if not root_path.exists():
        return {"error": "Path does not exist", "files": []}

    try:
        items = []
        for item in sorted(root_path.iterdir(), key=lambda x: (not x.is_dir(), x.name.lower())):
            # Ignore hidden files/folders (except common ones like .env)
            if item.name.startswith('.') and item.name not in ['.env', '.gitignore', '.vscode']:
                 continue
            
            is_dir = item.is_dir()
            items.append({
                "name": item.name,
                "path": str(item.relative_to(root_path)).replace('\\', '/'),
                "abs_path": str(item.absolute()),
                "type": "directory" if is_dir else "file",
            })
        return {"files": items, "root": str(root_path)}
    except PermissionError:
        return {"error": "Permission denied", "files": []}
    except Exception as e:
        return {"error": str(e), "files": []}

@api_router.get("/files/read")
async def read_file_content(path: str):
    """Read content of a file."""
    p = Path(path)
    # Security: Ensure it's not trying to read something sensitive outside (optional, but good)
    # For now, allow absolute paths as requested
    
    if not p.exists() or not p.is_file():
        raise HTTPException(status_code=404, detail="File not found")
    
    try:
        content = p.read_text(encoding='utf-8', errors='replace')
        return {"content": content, "path": str(p)}
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Error reading file: {str(e)}")

@api_router.post("/chat")
async def chat(request: ChatRequest, stream: bool = False):
    try:
        history = request.history or []
        
        # Determine preferred workspace (None = default or current dir)
        workspace_path = None
        if request.workspace:
            # Special case for "root" or similar could be handled here
            if request.workspace.lower() == "none":
                workspace_path = None  # Will fall back to whatever default or cwd
            else:
                workspace_path = Path(request.workspace)
                if not workspace_path.is_absolute():
                    workspace_path = settings.BASE_DIR / workspace_path

        # Link session if provided
        # Keep track of active session
        if request.session_id:
            session = terminal_manager.get_or_create_session(request.session_id, working_dir=workspace_path)
            CodeTools.set_active_session(session)
        else:
            CodeTools.set_active_session(None)

        # Helper to call agent with proper workspace context
        def agent_call_wrapper(*args, **kwargs):
            with set_current_workspace(workspace_path):
                # Clean path string for agent's system prompt
                wp_str = None
                if workspace_path:
                    try:
                        wp_str = str(workspace_path.absolute().relative_to(settings.BASE_DIR.absolute()))
                    except:
                        wp_str = str(workspace_path)
                return agent.run(*args, **kwargs, workspace_path=wp_str)

        # Non-streaming
        if not stream:
            response_text = agent_call_wrapper(request.prompt, history)
            return ChatResponse(response=response_text, history=history)

        # Streaming mode
        queue: asyncio.Queue = asyncio.Queue()
        loop = asyncio.get_running_loop()

        def _on_update(item: Dict):
            loop.call_soon_threadsafe(queue.put_nowait, item)

        async def event_generator():
            # Use the wrapper to ensure context is passed into the thread
            run_task = asyncio.to_thread(agent_call_wrapper, request.prompt, history, _on_update)
            task = asyncio.create_task(run_task)

            while True:
                try:
                    item = await asyncio.wait_for(queue.get(), timeout=0.1)
                    yield f"data: {json.dumps({'type': 'update', 'item': item})}\n\n"
                except asyncio.TimeoutError:
                    if task.done() and queue.empty():
                        break
                except asyncio.CancelledError:
                    task.cancel()
                    break

            await task
            yield f"data: {json.dumps({'type': 'done', 'history': history})}\n\n"

        return StreamingResponse(event_generator(), media_type="text/event-stream")

    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))

# Include API router
app.include_router(api_router)

@app.websocket("/ws/terminal/{session_id}")
async def terminal_websocket(websocket: WebSocket, session_id: str, workspace: Optional[str] = None):
    """WebSocket endpoint to connect to a PTY session with optional workspace."""
    await websocket.accept()
    
    workspace_path = None
    if workspace:
        if workspace.lower() == "none":
            workspace_path = settings.BASE_DIR # Fall back to root/home
        else:
            workspace_path = Path(workspace)
            if not workspace_path.is_absolute():
                workspace_path = settings.BASE_DIR / workspace_path

    # Get or create the terminal session
    session = terminal_manager.get_or_create_session(session_id, working_dir=workspace_path)
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
