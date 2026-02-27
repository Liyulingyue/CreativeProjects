#!/usr/bin/env python
"""mcp_server_fastapi.py

Minimal FastAPI-based MCP server (HTTP) to run independently from the agent.

Endpoints:
- GET  /list_tools -> list available tools and their parameter metadata
- POST /call_tool -> call a tool by name with JSON arguments

Run: `uvicorn mcp_server_fastapi:app --host 127.0.0.1 --port 8000`
"""
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
import asyncio
import os
from pathlib import Path
from typing import Any, Dict, List

app = FastAPI(title="MiniCoder MCP (HTTP)")


class CallRequest(BaseModel):
    name: str
    arguments: Dict[str, Any] = {}


def _tools_metadata():
    return [
        {"name": "execute_bash", "description": "Execute a shell command.", "parameters": {"command": "str"}},
        {"name": "read_file", "description": "Read file content.", "parameters": {"path": "str"}},
        {"name": "write_file", "description": "Write file content.", "parameters": {"path": "str", "content": "str"}},
        {"name": "list_files", "description": "List files in directory.", "parameters": {"path": "str"}},
        {"name": "search_files", "description": "Search string in files.", "parameters": {"query": "str", "path": "str"}},
    ]


@app.get("/list_tools")
async def list_tools():
    return _tools_metadata()


@app.post("/call_tool")
async def call_tool(req: CallRequest):
    name = req.name
    args = req.arguments or {}
    try:
        if name == "execute_bash":
            return {"result": await _execute_bash(args.get("command", ""))}
        if name == "read_file":
            return {"result": await _read_file(args.get("path", ""))}
        if name == "write_file":
            return {"result": await _write_file(args.get("path", ""), args.get("content", ""))}
        if name == "list_files":
            return {"result": await _list_files(args.get("path", "."))}
        if name == "search_files":
            return {"result": await _search_files(args.get("query", ""), args.get("path", "."))}
        raise HTTPException(status_code=404, detail=f"tool '{name}' not found")
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


async def _execute_bash(command: str) -> str:
    try:
        proc = await asyncio.create_subprocess_shell(
            command,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd=os.getcwd(),
        )
        stdout, stderr = await proc.communicate()
        return (stdout.decode() + stderr.decode()) or "(empty output)"
    except Exception as e:
        return f"Error executing command: {e}"


async def _read_file(path: str) -> str:
    try:
        p = Path(path)
        if not p.exists():
            return f"Error: File {path} does not exist."
        return p.read_text(encoding="utf-8")
    except Exception as e:
        return f"Error reading file: {e}"


async def _write_file(path: str, content: str) -> str:
    try:
        p = Path(path)
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(content, encoding="utf-8")
        return f"Successfully wrote to {path}"
    except Exception as e:
        return f"Error writing file: {e}"


async def _list_files(path: str = ".") -> str:
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


async def _search_files(query: str, path: str = ".") -> str:
    try:
        # Try Python-based search first for portability
        if not query:
            return "No query provided."
        matches = []
        base = Path(path)
        if not base.exists():
            return f"Error: Path {path} does not exist."
        for p in base.rglob("*"):
            if p.is_file():
                try:
                    text = p.read_text(encoding="utf-8", errors="ignore")
                    if query in text:
                        matches.append(str(p))
                except Exception:
                    continue
        return "\n".join(matches) if matches else "No matches found."
    except Exception as e:
        return f"Error searching files: {e}"


if __name__ == "__main__":
    import uvicorn

    uvicorn.run("mcp_server_fastapi:app", host="127.0.0.1", port=8000, reload=False)
