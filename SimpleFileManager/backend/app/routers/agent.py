import os
import json
import subprocess
import httpx
from pathlib import Path
from typing import Any, Optional
from pydantic import BaseModel
from fastapi import APIRouter, HTTPException
from ..deps import state

agent = APIRouter(prefix="/api/agent", tags=["agent"])


class ToolCall(BaseModel):
    name: str
    arguments: dict[str, Any]


class AgentRequest(BaseModel):
    message: str
    tool_calls: Optional[list[ToolCall]] = None
    session_id: Optional[str] = None


class AgentResponse(BaseModel):
    response: str
    tool_results: Optional[list[dict]] = None
    needs_tool_calls: bool = False
    available_tools: list[str] = []


TOOLS = {
    "bash": {
        "name": "bash",
        "description": "Execute a bash command. Returns stdout and stderr. Use for file operations, running scripts, etc.",
        "parameters": {
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                },
                "timeout": {
                    "type": "number",
                    "description": "Timeout in seconds (default: 30)"
                }
            },
            "required": ["command"]
        }
    },
    "read_file": {
        "name": "read_file",
        "description": "Read the contents of a file. Returns the file content or error message.",
        "parameters": {
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to read"
                },
                "limit": {
                    "type": "number",
                    "description": "Maximum number of lines to read (optional)"
                },
                "offset": {
                    "type": "number",
                    "description": "Line offset to start reading from (optional)"
                }
            },
            "required": ["path"]
        }
    },
    "write_file": {
        "name": "write_file",
        "description": "Write content to a file. Creates the file if it doesn't exist, overwrites if it does.",
        "parameters": {
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        }
    },
    "list_directory": {
        "name": "list_directory",
        "description": "List files and directories in a path.",
        "parameters": {
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the directory to list"
                }
            },
            "required": ["path"]
        }
    },
    "search_files": {
        "name": "search_files",
        "description": "Search for files matching a pattern using grep.",
        "parameters": {
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path to search in"
                },
                "pattern": {
                    "type": "string",
                    "description": "Search pattern (regex or simple string)"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Search recursively (default: true)"
                }
            },
            "required": ["path", "pattern"]
        }
    }
}


def execute_bash(command: str, timeout: int = 30) -> dict:
    try:
        result = subprocess.run(
            command,
            shell=True,
            capture_output=True,
            text=True,
            timeout=timeout
        )
        return {
            "success": True,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "returncode": result.returncode
        }
    except subprocess.TimeoutExpired:
        return {
            "success": False,
            "error": f"Command timed out after {timeout} seconds",
            "stdout": "",
            "stderr": "",
            "returncode": -1
        }
    except Exception as e:
        return {
            "success": False,
            "error": str(e),
            "stdout": "",
            "stderr": "",
            "returncode": -1
        }


def read_file_tool(path: str, limit: Optional[int] = None, offset: Optional[int] = None) -> dict:
    try:
        file_path = Path(path)
        if not file_path.exists():
            return {"success": False, "error": f"File not found: {path}"}
        if not file_path.is_file():
            return {"success": False, "error": f"Not a file: {path}"}

        with open(file_path, "r", encoding="utf-8") as f:
            lines = f.readlines()

        start = offset or 0
        end = start + limit if limit else len(lines)
        content = "".join(lines[start:end])

        return {
            "success": True,
            "content": content,
            "total_lines": len(lines),
            "read_lines": end - start
        }
    except Exception as e:
        return {"success": False, "error": str(e)}


def write_file_tool(path: str, content: str) -> dict:
    try:
        file_path = Path(path)
        file_path.parent.mkdir(parents=True, exist_ok=True)
        with open(file_path, "w", encoding="utf-8") as f:
            f.write(content)
        return {"success": True, "path": path, "bytes_written": len(content.encode("utf-8"))}
    except Exception as e:
        return {"success": False, "error": str(e)}


def list_directory_tool(path: str) -> dict:
    try:
        dir_path = Path(path)
        if not dir_path.exists():
            return {"success": False, "error": f"Directory not found: {path}"}
        if not dir_path.is_dir():
            return {"success": False, "error": f"Not a directory: {path}"}

        items = []
        for item in sorted(dir_path.iterdir(), key=lambda x: (not x.is_dir(), x.name.lower())):
            items.append({
                "name": item.name,
                "is_dir": item.is_dir(),
                "size": item.stat().st_size if item.is_file() else 0
            })

        return {"success": True, "path": path, "items": items, "count": len(items)}
    except Exception as e:
        return {"success": False, "error": str(e)}


def search_files_tool(path: str, pattern: str, recursive: bool = True) -> dict:
    try:
        import re
        dir_path = Path(path)
        if not dir_path.exists():
            return {"success": False, "error": f"Directory not found: {path}"}

        matches = []
        regex = re.compile(pattern)

        if recursive:
            for item in dir_path.rglob("*"):
                if item.is_file():
                    try:
                        with open(item, "r", encoding="utf-8") as f:
                            for i, line in enumerate(f, 1):
                                if regex.search(line):
                                    matches.append({
                                        "file": str(item),
                                        "line": i,
                                        "content": line.strip()
                                    })
                    except:
                        pass
        else:
            for item in dir_path.iterdir():
                if item.is_file():
                    try:
                        with open(item, "r", encoding="utf-8") as f:
                            for i, line in enumerate(f, 1):
                                if regex.search(line):
                                    matches.append({
                                        "file": str(item),
                                        "line": i,
                                        "content": line.strip()
                                    })
                    except:
                        pass

        return {"success": True, "pattern": pattern, "matches": matches[:100], "total": len(matches)}
    except Exception as e:
        return {"success": False, "error": str(e)}


def execute_tool(tool_name: str, arguments: dict) -> dict:
    if tool_name == "bash":
        return execute_bash(**arguments)
    elif tool_name == "read_file":
        return read_file_tool(**arguments)
    elif tool_name == "write_file":
        return write_file_tool(**arguments)
    elif tool_name == "list_directory":
        return list_directory_tool(**arguments)
    elif tool_name == "search_files":
        return search_files_tool(**arguments)
    else:
        return {"success": False, "error": f"Unknown tool: {tool_name}"}


SYSTEM_PROMPT = """You are a helpful AI assistant with access to tools to help users accomplish tasks.

Available tools:
- bash: Execute bash commands. Use for file operations, running scripts, etc.
- read_file: Read file contents. Specify path, optional limit (lines) and offset.
- write_file: Write content to a file. Creates or overwrites.
- list_directory: List files and directories at a path.
- search_files: Search for text patterns in files using regex.

When a user asks you to do something:
1. First, think about what tools you need
2. Call the tools with appropriate arguments
3. Review the results
4. Continue calling tools if needed
5. Provide a final response to the user

Be careful with destructive operations (rm, del, etc.). Always confirm before executing."""


def chat_with_llm(messages: list[dict], tools: list[dict]) -> dict:
    settings = state.get_settings()

    headers = {"Authorization": f"Bearer {settings.llm_api_key}"} if settings.llm_api_key else {}

    payload = {
        "model": settings.llm_model,
        "messages": messages,
        "tools": tools if tools else None,
        "temperature": 0.7,
    }

    try:
        resp = httpx.post(
            settings.llm_base_url,
            headers=headers,
            json=payload,
            timeout=60
        )
        resp.raise_for_status()
        data = resp.json()
        return data
    except Exception as e:
        return {"error": str(e)}


# Simple in-memory session store for conversation history
sessions: dict[str, list[dict]] = {}


@agent.post("/chat", response_model=AgentResponse)
def agent_chat(req: AgentRequest):
    try:
        settings = state.get_settings()

        # Get or create session
        session_id = req.session_id or "default"
        if session_id not in sessions:
            sessions[session_id] = []

        # Add user message
        sessions[session_id].append({"role": "user", "content": req.message})

        # Build messages with system prompt
        all_messages = [{"role": "system", "content": SYSTEM_PROMPT}] + sessions[session_id]

        # Convert tools to OpenAI format
        openai_tools = [
            {
                "type": "function",
                "function": {
                    "name": tool["name"],
                    "description": tool["description"],
                    "parameters": tool["parameters"]
                }
            }
            for tool in TOOLS.values()
        ]

        # First call to LLM
        response = chat_with_llm(all_messages, openai_tools)

        if "error" in response:
            return AgentResponse(
                response=f"Error: {response['error']}",
                available_tools=list(TOOLS.keys())
            )

        # Check if LLM wants to call tools
        choices = response.get("choices", [{}])
        choice = choices[0] if choices else {}

        if choice.get("finish_reason") == "tool_calls" or "tool_calls" in choice:
            tool_calls = choice.get("message", {}).get("tool_calls", [])

            if tool_calls:
                tool_results = []
                for tc in tool_calls:
                    tool_name = tc["function"]["name"]
                    arguments = json.loads(tc["function"]["arguments"])
                    result = execute_tool(tool_name, arguments)
                    tool_results.append({
                        "tool": tool_name,
                        "arguments": arguments,
                        "result": result
                    })

                    # Add tool result to messages
                    sessions[session_id].append({
                        "role": "assistant",
                        "content": ""
                    })
                    sessions[session_id].append({
                        "role": "tool",
                        "tool_call_id": tc["id"],
                        "content": json.dumps(result)
                    })

                # Second call to LLM with tool results
                response2 = chat_with_llm(all_messages, openai_tools)

                if "error" in response2:
                    return AgentResponse(
                        response=f"Tool executed but LLM error: {response2['error']}",
                        tool_results=tool_results,
                        available_tools=list(TOOLS.keys())
                    )

                final_response = response2.get("choices", [{}])[0].get("message", {}).get("content", "")

                # Add assistant response to history
                sessions[session_id].append({"role": "assistant", "content": final_response})

                return AgentResponse(
                    response=final_response,
                    tool_results=tool_results,
                    available_tools=list(TOOLS.keys())
                )

        # No tool calls, return direct response
        response_text = choice.get("message", {}).get("content", "")

        if not response_text and choices:
            response_text = "I'm thinking... please try again."

        # Add assistant response to history
        sessions[session_id].append({"role": "assistant", "content": response_text})

        return AgentResponse(
            response=response_text,
            available_tools=list(TOOLS.keys())
        )

    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@agent.get("/tools")
def get_tools():
    return {"tools": list(TOOLS.keys()), "definitions": TOOLS}


@agent.delete("/sessions/{session_id}")
def delete_session(session_id: str):
    if session_id in sessions:
        del sessions[session_id]
    return {"success": True}


@agent.delete("/sessions")
def delete_all_sessions():
    sessions.clear()
    return {"success": True}
