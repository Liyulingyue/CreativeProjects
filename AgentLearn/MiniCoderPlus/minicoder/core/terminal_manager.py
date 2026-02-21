#!/usr/bin/env python
"""terminal_manager.py — Persistent PTY management for MiniCoder Plus."""
import os
import asyncio
import threading
import json
import subprocess
from pathlib import Path
from .settings import settings

class TerminalSession:
    """A persistent PTY session using Node.js PTY Bridge for maximum stability."""
    
    def __init__(self, session_id: str, working_dir: Path = None):
        self.session_id = session_id
        self.working_dir = working_dir or settings.WORKSPACE_DIR
        self.proc = None
        self.output_queue = asyncio.Queue()
        self.is_running = False
        self._loop = None

    def start(self):
        """Start the Node.js PTY Bridge process."""
        if self.is_running:
            return

        bridge_path = Path(__file__).parent / "pty_bridge" / "index.js"
        
        # Build environment for the bridge
        env = os.environ.copy()
        env["PTY_CWD"] = str(self.working_dir)
        env["PTY_COLS"] = "120"
        env["PTY_ROWS"] = "30"
        
        print(f"Spawning PTY Bridge: node {bridge_path}")
        
        try:
            self.proc = subprocess.Popen(
                ["node", str(bridge_path)],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=False, # Use raw bytes for terminal data
                env=env,
                bufsize=0   # Unbuffered for real-time
            )
            
            self.is_running = True
            try:
                self._loop = asyncio.get_running_loop()
            except RuntimeError:
                self._loop = asyncio.new_event_loop()
            
            # Start background reader threads
            threading.Thread(target=self._read_loop, daemon=True).start()
            threading.Thread(target=self._error_loop, daemon=True).start()
            
            print(f"PTY Bridge spawned successfully for session {self.session_id}")
            
        except Exception as e:
            print(f"Failed to spawn PTY Bridge: {e}")
            self.is_running = False
            raise

    def _read_loop(self):
        """Read output from bridge stdout (which is the PTY's actual output)."""
        while self.is_running and self.proc and self.proc.stdout:
            try:
                # Read chunks of data
                data = self.proc.stdout.read(1024)
                if not data:
                    break
                
                # Terminal data is often UTF-8 with ANSI codes
                text = data.decode('utf-8', errors='replace')
                self._loop.call_soon_threadsafe(self.output_queue.put_nowait, text)
            except Exception as e:
                print(f"Bridge Read error: {e}")
                break
        
        self.is_running = False
        print(f"PTY Bridge stdout closed for session {self.session_id}")

    def _error_loop(self):
        """Monitor stderr for bridge logs."""
        while self.is_running and self.proc and self.proc.stderr:
            line = self.proc.stderr.readline()
            if not line:
                break
            print(f"[Bridge Log] {line.decode().strip()}")

    async def write(self, data: str):
        """Send input to PTY via Bridge stdin (JSON format)."""
        if self.is_running and self.proc and self.proc.stdin:
            msg = json.dumps({"type": "input", "data": data}) + "\n"
            self.proc.stdin.write(msg.encode())
            self.proc.stdin.flush()

    def resize(self, cols: int, rows: int):
        """Send resize command to Bridge stdin (JSON format)."""
        if self.is_running and self.proc and self.proc.stdin:
            msg = json.dumps({"type": "resize", "cols": cols, "rows": rows}) + "\n"
            self.proc.stdin.write(msg.encode())
            self.proc.stdin.flush()
            print(f"Sent resize to bridge: {cols}x{rows}")

    async def get_output(self):
        """Yield output from the queue."""
        while True:
            data = await self.output_queue.get()
            yield data

    def stop(self):
        """Shutdown the bridge and PTY."""
        self.is_running = False
        if self.proc:
            self.proc.terminate()
            self.proc = None

class SessionManager:
    """Manages multiple active terminal sessions."""
    
    def __init__(self):
        self.sessions = {}

    def get_or_create_session(self, session_id: str) -> TerminalSession:
        if session_id not in self.sessions:
            session = TerminalSession(session_id)
            session.start()
            self.sessions[session_id] = session
        return self.sessions[session_id]

    def close_session(self, session_id: str):
        if session_id in self.sessions:
            self.sessions[session_id].stop()
            del self.sessions[session_id]

# Singleton instance
terminal_manager = SessionManager()
