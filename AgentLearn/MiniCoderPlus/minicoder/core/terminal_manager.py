#!/usr/bin/env python
"""terminal_manager.py — Persistent PTY management for MiniCoder Plus."""
import os
import asyncio
import threading
import queue
from pathlib import Path
from winpty import PTY
from .settings import settings

class TerminalSession:
    """A persistent PTY session (PowerShell/CMD on Windows)."""
    
    def __init__(self, session_id: str, working_dir: Path = None):
        self.session_id = session_id
        self.working_dir = working_dir or settings.WORKSPACE_DIR
        self.pty = None
        self.output_queue = asyncio.Queue()
        self.is_running = False
        self._thread = None
        self._loop = None

    def start(self):
        """Start the PTY and worker thread."""
        if self.is_running:
            return

        # Start PowerShell in the workspace
        # On Windows, winpty uses full paths or executable names in PATH
        # We find powershell.exe
        pwsh_path = r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"
        if not os.path.exists(pwsh_path):
            pwsh_path = "powershell.exe"

        print(f"Spawning PTY with {pwsh_path} in {self.working_dir}")
        # Try to use ConPTY if possible (backend=1), which is much more stable on modern Windows
        try:
            from winpty import PTY
            # Initialize with standard size
            self.pty = PTY(120, 30, backend=1) 
            print("Using ConPTY backend")
        except Exception as e:
            print(f"Failed to load ConPTY: {e}. Falling back to legacy.")
            self.pty = PTY(120, 30)
            print("Using WinPTY legacy backend")
            
        # Spawn the process
        # Use full path for reliability
        if not self.pty.spawn(pwsh_path, cwd=str(self.working_dir)):
            print("ERROR: winpty failed to spawn process")
            raise RuntimeError("Failed to spawn PowerShell PTY")
        
        # Give ConPTY a moment to initialize before we start reading
        print(f"PTY spawned successfully for session {self.session_id}")
        self.is_running = True
        try:
            self._loop = asyncio.get_running_loop()
        except RuntimeError:
            self._loop = asyncio.new_event_loop()
        
        # Start background reader thread
        self._thread = threading.Thread(target=self._read_loop, daemon=True)
        self._thread.start()

    def _read_loop(self):
        """Background thread to read from PTY."""
        while self.is_running and self.pty:
            try:
                # Use blocking=True for maximum responsiveness in the background thread.
                # This ensures we push data to WS the instant it appears.
                data = self.pty.read(True) 
                if data:
                    self._loop.call_soon_threadsafe(self.output_queue.put_nowait, data)
                else:
                    # If we get empty but no exception, the PTY might be closing
                    break
            except Exception as e:
                # Handle cases where PTY is closed during read
                if self.is_running:
                    print(f"PTY Read error: {e}")
                break
        
        self.is_running = False
        print(f"PTY Session {self.session_id} terminated.")

    async def write(self, data: str):
        """Write string to PTY."""
        if self.is_running and self.pty:
            self.pty.write(data)

    def resize(self, cols: int, rows: int):
        """Resize the PTY dimensions."""
        if self.is_running and self.pty:
            try:
                self.pty.set_size(cols, rows)
                print(f"PTY Session {self.session_id} resized to {cols}x{rows}")
            except Exception as e:
                print(f"Resize error: {e}")

    async def get_output(self):
        """Yield output from the queue."""
        while True:
            data = await self.output_queue.get()
            yield data

    def stop(self):
        """Shutdown the PTY."""
        self.is_running = False
        if self.pty:
            self.pty.close()
            self.pty = None

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
