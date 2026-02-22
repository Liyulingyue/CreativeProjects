#!/usr/bin/env python
"""services/chat_service.py — Chat handling and history management."""
import asyncio
import json
from typing import List, Optional, Dict
from pathlib import Path
from ...agents.agent import MiniCoderAgent
from ...core.settings import settings
from ...core.terminal_manager import terminal_manager
from ...core.db import ChatDatabase
from ...tools import CodeTools, set_current_workspace
from ..models import ChatRequest, ChatResponse


class ChatService:
    """Service for handling chat requests and managing chat history."""
    
    def __init__(self):
        self.agent = MiniCoderAgent()
        self.db = ChatDatabase()
    
    async def handle_chat(self, request: ChatRequest, stream: bool = False):
        """Process a chat request and optionally save to history."""
        history = request.history or []
        
        # Determine workspace path
        workspace_path = self._resolve_workspace(request.workspace)
        
        # Set up terminal session if needed
        # Home route doesn't need terminal (pure LLM chat)
        # Workbench route needs terminal (can execute code/tools)
        if request.session_id and request.needs_terminal:
            session = terminal_manager.get_or_create_session(
                request.session_id, 
                working_dir=workspace_path
            )
            CodeTools.set_active_session(session)
        else:
            CodeTools.set_active_session(None)
            print(f"[DEBUG] Skipping PTY creation: session_id={request.session_id}, needs_terminal={request.needs_terminal}")
        
        # Create wrapper for agent call with proper context
        def agent_call_wrapper(*args, **kwargs):
            with set_current_workspace(workspace_path):
                wp_str = self._format_workspace_string(workspace_path)
                return self.agent.run(*args, **kwargs, workspace_path=wp_str)
        
        # Non-streaming mode
        if not stream:
            # Capture history length before run to identify new messages
            start_index = len(history)
            print(f"[DEBUG] Non-streaming mode: start_index={start_index}, history len={len(history)}")
            print(f"[DEBUG] Request prompt: {request.prompt}")
            print(f"[DEBUG] Current history: {history}")
            
            response_text = agent_call_wrapper(request.prompt, history)
            
            # Save to database if session_id provided and add IDs to history
            if request.session_id:
                # Save all new messages (User prompt + Assistant responses/tools)
                new_messages = history[start_index:]
                print(f"[DEBUG] New messages to save (count={len(new_messages)}): {new_messages}")
                saved_with_ids = self.db.save_messages(
                    request.session_id,
                    new_messages,
                    request.workspace
                )
                print(f"[DEBUG] Saved {len(saved_with_ids)} messages to session {request.session_id}")
                
                # Update history with IDs
                for i, saved in enumerate(saved_with_ids):
                    history[start_index + i]["id"] = saved["id"]
            else:
                print(f"[DEBUG] No session_id provided, skipping save")
            
            return ChatResponse(response=response_text, history=history)
        
        # Streaming mode - return async generator
        async def event_generator():
            queue: asyncio.Queue = asyncio.Queue()
            loop = asyncio.get_running_loop()
            
            # Keep track of history state for saving later
            start_index = len(history)
            print(f"[DEBUG] Streaming mode: start_index={start_index}, history len={len(history)}")
            
            def _on_update(item: Dict):
                loop.call_soon_threadsafe(queue.put_nowait, item)
            
            # Run agent in thread with proper context
            run_task = asyncio.to_thread(
                agent_call_wrapper,
                request.prompt,
                history,
                _on_update
            )
            task = asyncio.create_task(run_task)
            
            # Stream updates
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
            
            # Save all new messages from this turn and update history with IDs
            if request.session_id:
                new_messages = history[start_index:]
                print(f"[DEBUG] Streaming saved {len(new_messages)} messages to session {request.session_id}")
                print(f"[DEBUG] New messages: {new_messages}")
                saved_with_ids = self.db.save_messages(
                    request.session_id,
                    new_messages,
                    request.workspace
                )
                
                # Update history with IDs
                for i, saved in enumerate(saved_with_ids):
                    history[start_index + i]["id"] = saved["id"]
            else:
                print(f"[DEBUG] Streaming: No session_id, skipping save")
            
            yield f"data: {json.dumps({'type': 'done', 'history': history})}\n\n"
        
        return event_generator()
    
    def get_history(
        self,
        session_id: str,
        workspace: Optional[str] = None,
        limit: int = 100,
        offset: int = 0
    ) -> List[Dict]:
        """Retrieve chat history."""
        return self.db.get_messages(session_id, workspace, limit, offset)
    
    def get_history_count(
        self,
        session_id: str,
        workspace: Optional[str] = None
    ) -> int:
        """Get total count of messages in history."""
        return self.db.get_message_count(session_id, workspace)
    
    def delete_history(
        self,
        session_id: str,
        workspace: Optional[str] = None
    ) -> int:
        """Delete chat history, returns count of deleted messages."""
        return self.db.delete_messages(session_id, workspace)
    
    def get_sessions(self) -> List[Dict]:
        """Get list of unique sessions with metadata."""
        return self.db.get_sessions()
    
    def save_feedback(self, message_id: int, session_id: str, feedback: str, comment: Optional[str] = None) -> int:
        """Save user feedback for a message (training data collection).
        
        This stores user evaluations (thumbs up/down) and optional comments for model improvement analysis.
        """
        return self.db.save_feedback(message_id, session_id, feedback, comment)
    
    def _resolve_workspace(self, workspace: Optional[str]) -> Optional[Path]:
        """Convert workspace string to Path object."""
        if not workspace or workspace.lower() == "none":
            return None
        
        candidate = Path(workspace)
        if not candidate.is_absolute():
            candidate = settings.BASE_DIR / candidate
        
        return candidate
    
    def _format_workspace_string(self, workspace_path: Optional[Path]) -> Optional[str]:
        """Format workspace path for agent's system prompt."""
        if not workspace_path:
            return None
        
        try:
            return str(workspace_path.absolute().relative_to(settings.BASE_DIR.absolute()))
        except:
            return str(workspace_path)
