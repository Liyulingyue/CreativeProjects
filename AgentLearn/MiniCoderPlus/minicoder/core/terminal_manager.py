#!/usr/bin/env python
"""terminal_manager.py — Persistent PTY management for MiniCoder Plus."""
import os
import asyncio
import threading
import json
import subprocess
import time
from pathlib import Path
from .settings import settings

class TerminalSession:
    """A persistent PTY session using Node.js PTY Bridge."""
    
    def __init__(self, session_id: str, working_dir: Path = None):
        self.session_id = session_id
        self.working_dir = working_dir or settings.WORKSPACE_DIR
        self.proc = None
        self.is_running = False
        
        # We use a set of async queues for broadcasting to multiple listeners (WS and Agent)
        self._queues = set()
        self._lock = threading.Lock()
        self._loop = None
        self.last_activity = time.time()

    def start(self):
        """Start the Node.js PTY Bridge process."""
        if self.is_running:
            return

        bridge_path = Path(__file__).parent / "pty_bridge" / "index.js"
        
        # Ensure we have the main loop reference for thread-safe operations
        try:
            self._loop = asyncio.get_running_loop()
        except RuntimeError:
            self._loop = asyncio.new_event_loop()
            asyncio.set_event_loop(self._loop)

        env = os.environ.copy()
        env["PTY_CWD"] = str(self.working_dir)
        env["PTY_COLS"] = "120"
        env["PTY_ROWS"] = "30"
        
        try:
            self.proc = subprocess.Popen(
                ["node", str(bridge_path)],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=False,
                env=env,
                cwd=str(bridge_path.parent)
            )
            
            self.is_running = True
            # Start background reader threads
            threading.Thread(target=self._read_loop, daemon=True).start()
            threading.Thread(target=self._error_loop, daemon=True).start()
            
            print(f"DEBUG: PTY Bridge spawned for {self.session_id}")
            
        except Exception as e:
            print(f"ERROR: Failed to spawn PTY Bridge: {e}")
            self.is_running = False
            raise

    def _read_loop(self):
        """Read output from bridge stdout and broadcast to all current queues."""
        while self.is_running and self.proc and self.proc.stdout:
            try:
                data = self.proc.stdout.read1(16384) # Multi-KB chunks for high volume
                if not data:
                    break
                
                text = data.decode('utf-8', errors='replace')
                self.last_activity = time.time()

                # Broadcast to all registered async queues
                with self._lock:
                    for q in self._queues:
                        self._loop.call_soon_threadsafe(q.put_nowait, text)
            except Exception as e:
                break
        
        self.is_running = False
        print(f"DEBUG: PTY Session {self.session_id} ended.")

    def _error_loop(self):
        """Monitor stderr for bridge logs."""
        while self.is_running and self.proc and self.proc.stderr:
            line = self.proc.stderr.readline()
            if not line: break
            print(f" [NODE-BRIDGE-{self.session_id}] {line.decode().strip()}")

    async def write(self, data: str):
        """Send input to PTY via Bridge stdin (JSON format)."""
        self.last_activity = time.time()
        if self.is_running and self.proc and self.proc.stdin:
            msg = json.dumps({"type": "input", "data": data}) + "\n"
            self.proc.stdin.write(msg.encode())
            self.proc.stdin.flush()

    def resize(self, cols: int, rows: int):
        """Send resize command to Bridge stdin."""
        if self.is_running and self.proc and self.proc.stdin:
            msg = json.dumps({"type": "resize", "cols": cols, "rows": rows}) + "\n"
            self.proc.stdin.write(msg.encode())
            self.proc.stdin.flush()

    def stop(self):
        """Shutdown the bridge."""
        self.is_running = False
        if self.proc:
            self.proc.terminate()
            self.proc = None

    def get_output_queue(self) -> asyncio.Queue:
        """Create and register a new output queue for a listener."""
        q = asyncio.Queue()
        with self._lock:
            self._queues.add(q)
        return q

    def remove_queue(self, q: asyncio.Queue):
        """Unregister an output queue."""
        with self._lock:
            if q in self._queues:
                self._queues.remove(q)

class SessionManager:
    """Manages multiple active terminal sessions."""
    
    def __init__(self):
        self.sessions = {}
        self._cleanup_thread = threading.Thread(target=self._cleanup_loop, daemon=True)
        self._cleanup_thread.start()

    def get_or_create_session(self, session_id: str) -> TerminalSession:
        if session_id not in self.sessions:
            session = TerminalSession(session_id)
            session.start()
            self.sessions[session_id] = session
        return self.sessions[session_id]

    def _cleanup_loop(self):
        """Evict stale sessions after 1 hour of inactivity."""
        while True:
            time.sleep(300) # Check every 5 mins
            now = time.time()
            stale_ids = [
                sid for sid, s in self.sessions.items() 
                if now - s.last_activity > 3600
            ]
            for sid in stale_ids:
                print(f"DEBUG: Cleaning up stale session {sid}")
                self.sessions[sid].stop()
                del self.sessions[sid]

# Singleton instance
terminal_manager = SessionManager()
