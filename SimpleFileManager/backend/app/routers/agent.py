"""Native agent loop for file governance.

The agent can only observe the file system via read-only tools. Any mutating
operation must go through `submit_plan`, which persists an approval plan for
the user to review before execution.
"""

import json
import re
import uuid
from pathlib import Path
from typing import Any, Optional

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from ..deps import state, get_storage_root, resolve_under_root, PathOutsideRoot, validate_plan_action
from ..llm_client import chat_completion

agent = APIRouter()

MAX_TOOL_RESULT_CHARS = 4000
MAX_SESSION_MESSAGES = 40


class AgentRequest(BaseModel):
    message: str
    session_id: Optional[str] = None


class AgentResponse(BaseModel):
    response: str
    tool_results: Optional[list[dict]] = None
    plans: list[dict] = []
    steps_used: int = 0
    available_tools: list[str] = []


# ---- tool schemas (OpenAI function calling format) ----

TOOLS = [
    {
        "type": "function",
        "function": {
            "name": "list_directory",
            "description": "List files and directories under a path inside the user's storage. Use this to explore the file structure.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path relative to storage root. Use '' or '.' for the root."}
                },
                "required": ["path"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "read_file",
            "description": "Read a text file's content (truncated). Use to understand file contents before suggesting actions.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path relative to storage root."},
                    "limit": {"type": "integer", "description": "Max characters to read (default 2000)."},
                },
                "required": ["path"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "find_files",
            "description": "Find files whose names match a regular expression, recursively under the storage root.",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Regex matched against file/dir names, e.g. 'screenshot.*png'."}
                },
                "required": ["pattern"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "search_content",
            "description": "Search for a regex inside text files under the storage root. Returns matching file paths and lines.",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Regex to search inside file contents."},
                    "path": {"type": "string", "description": "Optional subdirectory to limit the search."}
                },
                "required": ["pattern"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "submit_plan",
            "description": "Submit a file organization plan for user approval. The plan will NOT be executed until the user approves it. Call this when you have decided what actions to propose.",
            "parameters": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Short title of the plan, e.g. '整理下载目录的截图'."},
                    "summary": {"type": "string", "description": "One-paragraph summary of what the plan does and why."},
                    "actions": {
                        "type": "array",
                        "description": "List of proposed actions in execution order.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "action_type": {"type": "string", "enum": ["move", "rename", "create_folder", "delete"]},
                                "source_path": {"type": "string", "description": "Existing file/dir to act on (relative to storage root). Required for move/rename/delete."},
                                "target_path": {"type": "string", "description": "Destination path. Required for move/rename/create_folder."},
                                "reason": {"type": "string", "description": "Why this action is proposed."}
                            },
                            "required": ["action_type"],
                        },
                    },
                },
                "required": ["title", "actions"],
            },
        },
    },
]

AVAILABLE_TOOLS = [t["function"]["name"] for t in TOOLS]

SYSTEM_PROMPT = """你是 NASMind 的文件治理 Agent，负责帮助用户分析和整理他的私有存储空间。

你可以使用以下只读工具来观察文件系统：
- list_directory: 列出目录内容
- read_file: 读取文本文件内容
- find_files: 按文件名正则搜索文件
- search_content: 按内容正则搜索文件

你唯一可以提交变更的方式是 submit_plan 工具：它会把一份操作计划提交给用户审批，
在你提交计划后用户需要手动批准，计划才会被执行。你无法直接修改、移动或删除任何文件。

工作流程：
1. 先用只读工具充分了解文件系统的现状（目录结构、文件内容）
2. 分析哪些文件需要整理（如截图散落在根目录、命名不规范的文件等）
3. 用 submit_plan 提交一份结构化的整理计划，每个 action 都要给出清晰的 reason

注意：
- 路径都是相对于存储根目录的相对路径
- delete 操作要极其谨慎，只在明确无价值（如临时文件、重复文件）时建议
- 如果用户的请求不涉及文件变更，直接回答即可，无需提交计划
- 用中文与用户交流"""


# ---- tool implementations (return dicts, never raise) ----

def _rel(path: Path) -> str:
    return str(path.relative_to(get_storage_root()))

def tool_list_directory(path: str) -> dict:
    try:
        target = resolve_under_root(path)
    except PathOutsideRoot:
        return {"success": False, "error": f"Path outside storage root: {path}"}
    if not target.exists():
        return {"success": False, "error": f"Directory not found: {path}"}
    if not target.is_dir():
        return {"success": False, "error": f"Not a directory: {path}"}
    items = []
    try:
        for entry in sorted(target.iterdir(), key=lambda x: (not x.is_dir(), x.name.lower())):
            if entry.name.startswith(".") or ".simplefilemanager" in entry.parts:
                continue
            try:
                stat = entry.stat()
                items.append({
                    "name": entry.name,
                    "path": _rel(entry),
                    "is_dir": entry.is_dir(),
                    "size": stat.st_size if entry.is_file() else 0,
                })
            except (PermissionError, OSError):
                continue
    except (PermissionError, OSError) as e:
        return {"success": False, "error": str(e)}
    return {"success": True, "path": _rel(target) if target != get_storage_root() else ".", "items": items, "count": len(items)}


def tool_read_file(path: str, limit: int = 2000) -> dict:
    try:
        target = resolve_under_root(path)
    except PathOutsideRoot:
        return {"success": False, "error": f"Path outside storage root: {path}"}
    if not target.exists() or not target.is_file():
        return {"success": False, "error": f"File not found: {path}"}
    try:
        with open(target, "r", encoding="utf-8", errors="ignore") as f:
            content = f.read(limit)
        return {"success": True, "path": _rel(target), "content": content, "truncated": target.stat().st_size > limit}
    except (PermissionError, OSError) as e:
        return {"success": False, "error": str(e)}


def tool_find_files(pattern: str) -> dict:
    try:
        regex = re.compile(pattern)
    except re.error as e:
        return {"success": False, "error": f"Invalid regex: {e}"}
    root = get_storage_root()
    matches = []
    try:
        for entry in root.rglob("*"):
            if ".simplefilemanager" in entry.parts or entry.name.startswith("."):
                continue
            if regex.search(entry.name):
                try:
                    matches.append({
                        "name": entry.name,
                        "path": _rel(entry),
                        "is_dir": entry.is_dir(),
                    })
                except (PermissionError, OSError):
                    continue
            if len(matches) >= 100:
                break
    except (PermissionError, OSError) as e:
        return {"success": False, "error": str(e)}
    return {"success": True, "pattern": pattern, "matches": matches, "total": len(matches)}


def tool_search_content(pattern: str, path: str = "") -> dict:
    try:
        regex = re.compile(pattern)
    except re.error as e:
        return {"success": False, "error": f"Invalid regex: {e}"}
    try:
        base = resolve_under_root(path)
    except PathOutsideRoot:
        return {"success": False, "error": f"Path outside storage root: {path}"}
    if not base.exists():
        return {"success": False, "error": f"Directory not found: {path}"}
    matches = []
    try:
        for entry in base.rglob("*"):
            if ".simplefilemanager" in entry.parts or entry.name.startswith("."):
                continue
            if not entry.is_file() or entry.stat().st_size > 1024 * 1024:
                continue
            try:
                with open(entry, "r", encoding="utf-8", errors="ignore") as f:
                    for line_no, line in enumerate(f, 1):
                        if regex.search(line):
                            matches.append({
                                "path": _rel(entry),
                                "line": line_no,
                                "content": line.strip()[:200],
                            })
                            if len(matches) >= 50:
                                break
            except (PermissionError, OSError):
                continue
            if len(matches) >= 50:
                break
    except (PermissionError, OSError) as e:
        return {"success": False, "error": str(e)}
    return {"success": True, "pattern": pattern, "matches": matches, "total": len(matches)}


def tool_submit_plan(title: str, summary: str = "", actions: Optional[list[dict]] = None) -> dict:
    actions = actions or []
    if not actions:
        return {"success": False, "error": "Plan must contain at least one action."}
    cleaned = []
    for a in actions:
        error = validate_plan_action(a)
        if error:
            return {"success": False, "error": error}
        cleaned.append({
            "action_type": a.get("action_type", "move"),
            "source_path": a.get("source_path"),
            "target_path": a.get("target_path"),
            "reason": a.get("reason", ""),
        })
    plan = state.get_plan_store().create_plan(
        title=title,
        summary=summary,
        source="agent",
        actions=cleaned,
    )
    return {"success": True, "plan_id": plan.id, "title": plan.title, "action_count": len(plan.actions),
            "message": "Plan submitted. It will be executed only after the user approves it."}


def execute_tool(name: str, arguments: dict) -> dict:
    try:
        if name == "list_directory":
            return tool_list_directory(**arguments)
        if name == "read_file":
            return tool_read_file(**arguments)
        if name == "find_files":
            return tool_find_files(**arguments)
        if name == "search_content":
            return tool_search_content(**arguments)
        if name == "submit_plan":
            return tool_submit_plan(**arguments)
        return {"success": False, "error": f"Unknown tool: {name}"}
    except TypeError as e:
        return {"success": False, "error": f"Invalid arguments for {name}: {e}"}


# ---- LLM plumbing ----

def chat_with_llm(messages: list[dict], tools: list[dict]) -> dict:
    try:
        result = chat_completion(messages, tools=tools)
        choice = {"message": result}
        return {"choices": [choice]}
    except Exception as e:
        return {"error": str(e)}


def _trim(result: dict) -> dict:
    text = json.dumps(result, ensure_ascii=False)
    if len(text) > MAX_TOOL_RESULT_CHARS:
        return {"success": result.get("success", False), "truncated": True,
                "data": text[:MAX_TOOL_RESULT_CHARS]}
    return result


# In-memory conversation history, keyed by session id
sessions: dict[str, list[dict]] = {}


@agent.post("/chat", response_model=AgentResponse)
def agent_chat(req: AgentRequest):
    session_id = req.session_id or "default"
    history = sessions.setdefault(session_id, [])
    history.append({"role": "user", "content": req.message})

    all_messages = [{"role": "system", "content": SYSTEM_PROMPT}] + history[-MAX_SESSION_MESSAGES:]

    tool_results: list[dict] = []
    plan_ids: list[str] = []
    steps_used = 0
    final_text = ""

    for _ in range(state.get_settings().max_agent_steps):
        steps_used += 1
        response = chat_with_llm(all_messages, TOOLS)

        if "error" in response:
            return AgentResponse(
                response=f"模型调用失败: {response['error']}",
                tool_results=tool_results or None,
                plans=[], steps_used=steps_used, available_tools=AVAILABLE_TOOLS,
            )

        choice = (response.get("choices") or [{}])[0]
        message = choice.get("message", {})
        tool_calls = message.get("tool_calls") or []

        if tool_calls:
            # preserve the full assistant tool_calls message so the API
            # can link the following tool results to their call ids
            all_messages.append({
                "role": "assistant",
                "content": message.get("content") or "",
                "tool_calls": tool_calls,
            })
            for tc in tool_calls:
                fn_name = None
                try:
                    fn_name = tc["function"]["name"]
                    arguments = json.loads(tc["function"].get("arguments") or "{}")
                except (KeyError, json.JSONDecodeError) as e:
                    result = {"success": False, "error": f"Malformed tool call: {e}"}
                else:
                    result = execute_tool(fn_name, arguments)
                    if fn_name == "submit_plan" and result.get("success"):
                        plan_ids.append(result.get("plan_id", ""))
                tool_results.append({"tool": fn_name or "unknown", "result": result})
                all_messages.append({
                    "role": "tool",
                    "tool_call_id": tc.get("id", ""),
                    "content": json.dumps(_trim(result), ensure_ascii=False),
                })
            continue

        final_text = message.get("content") or "（模型未返回内容，请重试）"
        all_messages.append({"role": "assistant", "content": final_text})
        break
    else:
        final_text = "已达到单次任务的最大步数限制，请查看已收集的信息或换个更简单的请求。"
        all_messages.append({"role": "assistant", "content": final_text})

    # persist trimmed history (drop intermediate tool noise)
    history.clear()
    history.extend([m for m in all_messages[1:] if m.get("role") in ("user", "assistant")][-MAX_SESSION_MESSAGES:])

    plans = []
    if plan_ids:
        store = state.get_plan_store()
        plans = [p.model_dump() for p in (store.get_plan(pid) for pid in plan_ids) if p]

    return AgentResponse(
        response=final_text,
        tool_results=tool_results or None,
        plans=plans,
        steps_used=steps_used,
        available_tools=AVAILABLE_TOOLS,
    )


@agent.get("/tools")
def get_tools():
    return {"tools": AVAILABLE_TOOLS, "definitions": TOOLS}


@agent.delete("/sessions/{session_id}")
def delete_session(session_id: str):
    sessions.pop(session_id, None)
    return {"success": True}


@agent.delete("/sessions")
def delete_all_sessions():
    sessions.clear()
    return {"success": True}
