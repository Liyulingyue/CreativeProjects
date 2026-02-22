#!/usr/bin/env python
"""routers/terminal.py — Terminal WebSocket routes."""
from fastapi import APIRouter, WebSocket, WebSocketDisconnect
from typing import Optional
from pathlib import Path
import json
import asyncio
from ...core.settings import settings
from ...core.terminal_manager import terminal_manager

router = APIRouter(tags=["terminal"])


@router.websocket("/ws/terminal/{session_id}")
async def terminal_websocket(websocket: WebSocket, session_id: str, workspace: Optional[str] = None):
    """WebSocket endpoint to connect to a PTY session with optional workspace."""
    await websocket.accept()
    
    workspace_path = None
    if workspace:
        if workspace.lower() == "none":
            workspace_path = settings.BASE_DIR  # Fall back to root/home
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
                        pass  # Fall through to raw write if parsing fails
                
                # Default: raw write to PTY
                await session.write(message)
        except WebSocketDisconnect:
            pass
        except Exception as e:
            pass

    # Run both concurrently
    await asyncio.wait(
        [asyncio.create_task(downlink()), asyncio.create_task(uplink())],
        return_when=asyncio.FIRST_COMPLETED
    )
