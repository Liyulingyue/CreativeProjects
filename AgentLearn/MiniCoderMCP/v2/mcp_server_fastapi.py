#!/usr/bin/env python
"""mcp_server_fastapi.py

FastAPI-based MCP server using the official MCP Python SDK.
Supports SSE (Server-Sent Events) transport.

Usage:
    uvicorn mcp_server_fastapi:app --host 127.0.0.1 --port 8000
"""
import asyncio
import os
import logging
from pathlib import Path
from typing import List, Optional

from fastapi import FastAPI
from mcp.server import Server
from mcp.server.fastapi import McpFastApi
from mcp.types import Tool, TextContent, ImageContent, EmbeddedResource

# Initialize logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("mcp-fastapi-server")

# 1. Create the MCP Server
mcp_server = Server("MiniCoder-HTTP-Server")

# --- Tool Implementations ---

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

# --- MCP Tool Registration ---

@mcp_server.list_tools()
async def handle_list_tools() -> list[Tool]:
    """List available tools."""
    return [
        Tool(
            name="execute_bash",
            description="Execute a shell command.",
            inputSchema={
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "The command to run"}
                },
                "required": ["command"]
            }
        ),
        Tool(
            name="read_file",
            description="Read file content.",
            inputSchema={
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the file"}
                },
                "required": ["path"]
            }
        ),
        Tool(
            name="write_file",
            description="Write file content.",
            inputSchema={
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the file"},
                    "content": {"type": "string", "description": "Content to write"}
                },
                "required": ["path", "content"]
            }
        ),
        Tool(
            name="list_files",
            description="List files in directory.",
            inputSchema={
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Directory path", "default": "."}
                }
            }
        ),
        Tool(
            name="search_files",
            description="Search string in files.",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "String to search for"},
                    "path": {"type": "string", "description": "Search directory", "default": "."}
                },
                "required": ["query"]
            }
        )
    ]

@mcp_server.call_tool()
async def handle_call_tool(name: str, arguments: dict | None) -> list[TextContent | ImageContent | EmbeddedResource]:
    """Execute a tool call."""
    if not arguments:
        arguments = {}
        
    try:
        if name == "execute_bash":
            res = await _execute_bash(arguments.get("command", ""))
        elif name == "read_file":
            res = await _read_file(arguments.get("path", ""))
        elif name == "write_file":
            res = await _write_file(arguments.get("path", ""), arguments.get("content", ""))
        elif name == "list_files":
            res = await _list_files(arguments.get("path", "."))
        elif name == "search_files":
            res = await _search_files(arguments.get("query", ""), arguments.get("path", "."))
        else:
            raise ValueError(f"Unknown tool: {name}")

        return [TextContent(type="text", text=str(res))]
    except Exception as e:
        logger.error(f"Error calling tool {name}: {e}")
        return [TextContent(type="text", text=f"Error: {str(e)}")]

# 2. Integrate with FastAPI
app = FastAPI(title="MiniCoder MCP (HTTP/SSE)")
mcp_fastapi = McpFastApi(mcp_server, app)

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="127.0.0.1", port=8000)
