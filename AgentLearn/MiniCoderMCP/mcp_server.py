#!/usr/bin/env python
"""mcp_server.py - standalone MCP server for MiniCoderMCP tools.

This module exposes the same helpers that used to live in tools.py, but
wrapped as MCP tools. It listens on stdio (JSON messages) and can be
started with `python mcp_server.py`.

The real "mcp" SDK is ideal; if it isn't installed we provide a
lightweight fallback implementation so the code still runs for
exploration and testing.
"""

import os
import sys
import json
import asyncio
import subprocess
from pathlib import Path
from typing import Any, Dict, List

# Minimalist fallback implementation when the official SDK is not used.
# This avoids confusion and provides a simple, direct way to learn MCP.
class MCP:
    def __init__(self):
        self._tools: Dict[str, Any] = {}

    def tool(self, *args, **kwargs):
        def decorator(func):
            self._tools[func.__name__] = func
            return func
        return decorator

    async def list_tools(self):
        """Returns the lists of names/descriptions/parameters for each tool."""
        out = []
        for name, func in self._tools.items():
            ann = getattr(func, "__annotations__", {})
            params = {k: v.__name__ if isinstance(v, type) else str(v) 
                     for k, v in ann.items() if k != "return"}
            out.append({
                "name": name,
                "description": func.__doc__ or "",
                "parameters": params,
            })
        return out

    async def call_tool(self, name: str, arguments: dict):
        func = self._tools.get(name)
        if func is None:
            raise ValueError(f"tool '{name}' not found")
        if asyncio.iscoroutinefunction(func):
            return await func(**arguments)
        else:
            return func(**arguments)

    async def serve(self):
        """Robust JSON-over-stdio loop. Logs are directed to stderr."""
        sys.stderr.write("Server listening on stdin...\n")
        while True:
            # We use executor to not block the event loop while waiting for 
            # stdin, though in a single-threaded subprocess this is just 
            # for hygiene.
            line = await asyncio.get_event_loop().run_in_executor(None, sys.stdin.readline)
            if not line:
                break
            try:
                msg = json.loads(line)
            except:
                continue

            resp: Any = None
            try:
                if msg.get("type") == "list_tools":
                    resp = await self.list_tools()
                elif msg.get("type") == "call_tool":
                    res = await self.call_tool(msg["name"], msg.get("arguments", {}))
                    resp = {"result": res}
                else:
                    resp = {"error": "unknown request type"}
            except Exception as e:
                resp = {"error": str(e)}

            # MUST write only JSON to stdout followed by exactly one newline.
            sys.stdout.write(json.dumps(resp, default=str) + "\n")
            sys.stdout.flush()

def tool(*args, **kwargs):
    def decorator(func):
        return func
    return decorator

# create global server instance; real SDK may provide its own helper
server = MCP()

# --- tool implementations --------------------------------------------------
@server.tool()
async def execute_bash(command: str) -> str:
    """Execute a bash command in the local environment."""
    try:
        proc = await asyncio.create_subprocess_shell(
            command,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd=os.getcwd(),
        )
        stdout, stderr = await proc.communicate()
        output = stdout.decode() + stderr.decode()
        return output or "(empty output)"
    except asyncio.TimeoutError:
        return "Error: Command timed out."
    except Exception as e:
        return f"Error executing command: {e}"


@server.tool()
async def read_file(path: str) -> str:
    """Read the content of a file from disk."""
    try:
        p = Path(path)
        if not p.exists():
            return f"Error: File {path} does not exist."
        return p.read_text(encoding="utf-8")
    except Exception as e:
        return f"Error reading file: {e}"


@server.tool()
async def write_file(path: str, content: str) -> str:
    """Write or overwrite a file with the provided content."""
    try:
        p = Path(path)
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(content, encoding="utf-8")
        return f"Successfully wrote to {path}"
    except Exception as e:
        return f"Error writing file: {e}"


@server.tool()
async def list_files(path: str = ".") -> str:
    """List files and directories in a given path."""
    try:
        p = Path(path)
        if not p.exists():
            return f"Error: Directory {path} does not exist."
        if not p.is_dir():
            return f"Error: {path} is not a directory."
        items = []
        for item in p.iterdir():
            marker = "[DIR] " if item.is_dir() else "[FILE] "
            items.append(f"{marker}{item.name}")
        return "\n".join(sorted(items)) if items else "(empty directory)"
    except Exception as e:
        return f"Error listing files: {e}"


@server.tool()
async def search_files(query: str, path: str = ".") -> str:
    """Search for a specific string in files within a directory."""
    try:
        proc = await asyncio.create_subprocess_shell(
            f'grep -ril "{query}" {path}',
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, stderr = await proc.communicate()
        output = stdout.decode().strip()
        return output or "No matches found."
    except Exception as e:
        return f"Error searching files: {e}"


# --- server entry point ----------------------------------------------------
async def main():
    sys.stderr.write("Starting MCP server (JSON over stdio)...\n")
    sys.stderr.flush()
    await server.serve()


if __name__ == "__main__":
    asyncio.run(main())
