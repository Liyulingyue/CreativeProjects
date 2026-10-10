"""Native agent loop for file governance.

The agent can only observe the file system via read-only tools. Any mutating
operation must go through `submit_plan`, which persists an approval plan for
the user to review before execution.
"""

import json
import logging
import re
import uuid
from pathlib import Path
from typing import Any, Iterator, Optional

from fastapi import APIRouter, HTTPException
from fastapi.responses import StreamingResponse
from pydantic import BaseModel

from ..deps import state, get_storage_root, resolve_under_root, PathOutsideRoot, validate_plan_action
from ..doc_extract import ExtractionError, extract_text
from ..interaction import interaction
from ..llm_client import chat_completion, chat_completion_stream, _friendly_error

log = logging.getLogger(__name__)

agent = APIRouter()

MAX_TOOL_RESULT_CHARS = 4000
# Backstop only. Conversation length is normally bounded by the token budget in
# compact(); this exists so a runaway session (millions of tiny messages) cannot
# grow without limit. Kept high deliberately: the window is applied as a slice,
# so whenever it does engage it invalidates the prompt cache prefix.
MAX_SESSION_MESSAGES = 400


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
            "description": "Read a file's text content. Plain text, Markdown, code, PDF and DOCX are supported; PDF text is extracted and scanned/image-only PDFs will fail. For large files prefer semantic_search, which finds the relevant passages instead of returning only the first couple of thousand characters.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path relative to storage root."},
                    "limit": {"type": "integer", "description": "Max characters to read (default 2000; raise it for long files)."},
                    "offset": {"type": "integer", "description": "Character offset to start from, for reading a long file in parts (default 0)."},
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
            "name": "semantic_search",
            "description": "Search by meaning rather than keywords. Use this to understand what a large document is about, or to find passages that discuss a topic whose exact wording you do not know. Returns matching passages with their file and location. Much better than read_file for large files, which can only return a small excerpt. IMPORTANT: if the question is about one specific file, pass that file's name as 'path' — otherwise passages from other files come back too and you may attribute them to the wrong document.",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "A natural-language question or description of what to find."},
                    "path": {"type": "string", "description": "Optional file name or subdirectory to restrict the search to."},
                    "top_k": {"type": "integer", "description": "Number of passages to return (default 5, max 20)."}
                },
                "required": ["query"],
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

# User-facing copy for the tool reference page. It lives here rather than in the
# frontend so adding a tool touches one file: the English `description` above is
# prompt text aimed at the model and must stay English, while this is the text a
# human reads.
TOOL_DISPLAY: dict[str, dict] = {
    "list_directory": {
        "title": "列出目录",
        "summary": "列出存储目录下某个路径的文件与子目录，用来摸清目录结构。",
        "risk": "read",
        "params": {"path": "相对存储根目录的路径，留空或填 . 表示根目录"},
    },
    "semantic_search": {
        "title": "语义检索",
        "summary": "按含义而非关键词检索已索引的内容。适合了解一份大文档讲了什么，或寻找措辞未知的相关段落。",
        "risk": "read",
        "params": {
            "query": "用自然语言描述你想找什么",
            "path": "可选，限定到某个文件或子目录",
            "top_k": "返回的段落数，默认 5，最大 20",
        },
    },
    "read_file": {
        "title": "读取文件",
        "summary": "读取文本文件的内容，超长会截断，可分批续读。支持纯文本、Markdown、代码、PDF 与 DOCX。文件很大时优先用 semantic_search。",
        "risk": "read",
        "params": {
            "path": "相对存储根目录的文件路径",
            "limit": "最多读取的字符数，默认 2000",
        },
    },
    "find_files": {
        "title": "查找文件",
        "summary": "按正则表达式递归匹配文件名，用于定位特定命名的文件。",
        "risk": "read",
        "params": {"pattern": "针对文件名的正则表达式，例如 screenshot.*png"},
    },
    "search_content": {
        "title": "搜索内容",
        "summary": "在文本文件内容中搜索，返回命中的文件路径与所在行。",
        "risk": "read",
        "params": {
            "pattern": "在文件内容中搜索的正则表达式",
            "path": "可选，限定搜索的子目录",
        },
    },
    "submit_plan": {
        "title": "提交整理计划",
        "summary": "提交一份文件整理计划等待你审批。批准前不会执行任何操作。",
        "risk": "approval",
        "params": {
            "title": "计划标题",
            "summary": "一句话说明这份计划要做什么",
            "actions": "操作数组，每项描述一个移动、重命名或删除动作",
        },
    },
}

RISK_LABELS = {"read": "只读", "approval": "需审批"}

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


def tool_read_file(path: str, limit: int = 2000, offset: int = 0) -> dict:
    try:
        target = resolve_under_root(path)
    except PathOutsideRoot:
        return {"success": False, "error": f"Path outside storage root: {path}"}
    if not target.exists() or not target.is_file():
        return {"success": False, "error": f"File not found: {path}"}

    # PDFs and DOCX carry their text in a container, so they are parsed rather
    # than decoded as UTF-8 (which would return mojibake).
    try:
        extracted = extract_text(target)
    except ExtractionError as e:
        return {"success": False, "error": str(e)}

    if extracted is not None:
        offset = max(0, offset)
        content = extracted[offset: offset + limit]
        total = len(extracted)
        out = {
            "success": True,
            "path": _rel(target),
            "content": content,
            "truncated": total > limit,
        }
        if total > limit:
            out["chars_returned"] = len(content)
            out["total_chars"] = total
            next_offset = offset + len(content)
            out["next_offset"] = next_offset if next_offset < total else None
            out["hint"] = (
                f"This file has {total} characters; you read {len(content)}. "
                f"Call read_file again with offset={next_offset} to continue, "
                f"or use semantic_search to find relevant passages without "
                f"reading the whole file."
            )
        return out

    try:
        offset = max(0, offset)
        with open(target, "r", encoding="utf-8", errors="ignore") as f:
            if offset:
                f.seek(offset)
            content = f.read(limit)
        out = {
            "success": True,
            "path": _rel(target),
            "content": content,
            "truncated": target.stat().st_size > limit,
        }
        if out["truncated"]:
            out["chars_returned"] = len(content)
            out["hint"] = (
                "File is longer than the limit. Pass a larger limit, or use "
                "semantic_search to find the relevant part."
            )
        return out
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


def tool_semantic_search(query: str, path: str = "", top_k: int = 5) -> dict:
    """Meaning-based retrieval over the indexed corpus.

    read_file can only hand back a couple of thousand characters, which is
    useless against a 2.5MB novel — 0.2% of it. This reads from the chunk index
    instead, so the model can actually learn what a document contains.
    """
    from ..search_engine import search_engine

    top_k = max(1, min(top_k, 20))
    needle = (path or "").strip()

    # A smaller model does not reliably pass `path` even when told to, and an
    # unfiltered search mixes books: "林黛玉" also appears in other works and
    # vector neighbours ignore book boundaries. When the question names exactly
    # one indexed file, honour that intent automatically instead of relying on
    # the model to cooperate.
    inferred = ""
    if not needle:
        # Match on the stem: the question says "红楼梦" but the indexed name
        # is "红楼梦.txt", so a plain substring test never fires.
        candidates = [
            name for name in search_engine.indexed_file_names()
            if name and (name in query or Path(name).stem in query)
        ]
        if len(candidates) == 1:
            inferred = candidates[0]
            needle = inferred

    try:
        # The filter is applied inside hybrid_search, before the per-file cap;
        # filtering afterwards returned whatever other books happened to rank.
        result = search_engine.hybrid_search(query, top_k=top_k, path_filter=needle)
    except Exception as e:
        return {"success": False, "error": f"semantic_search failed: {e}"}

    items = result.get("results", [])[:top_k]

    if not items:
        return {
            "success": True,
            "matches": [],
            "hint": "Nothing indexed matched. The corpus may not be indexed yet, or the path filter matched nothing — try search_content or read_file instead.",
        }

    matches = [
        {
            "file": r["name"],
            "location": r["path"].split("/")[-1],
            "chunk": r.get("chunk_idx"),
            "matched_by": r.get("sources", []),
            "text": (r.get("preview") or "")[:600],
        }
        for r in items
    ]

    out: dict = {"success": True, "query": query, "matches": matches}
    if inferred:
        out["auto_filtered_to"] = inferred

    # Semantic neighbours are not book-aware, and a keyword like a character
    # name appears in several files, so an unfiltered search can hand back
    # passages from works the caller never asked about. Say so explicitly
    # rather than letting the model quietly attribute them.
    if not needle:
        others = sorted({m["file"] for m in matches} - {needle})
        distinct = sorted({m["file"] for m in matches})
        if len(distinct) > 1:
            out["files_matched"] = distinct
            out["warning"] = (
                f"No path filter was given and passages came from "
                f"{len(distinct)} different files ({', '.join(distinct)}). "
                f"Only use passages from the file the question is about; "
                f"re-run with path=<file name> to restrict the search."
            )
    return out


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
        if name == "semantic_search":
            return tool_semantic_search(**arguments)
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
        return {"error": _friendly_error(e)}


def _trim(result: dict) -> dict:
    text = json.dumps(result, ensure_ascii=False)
    if len(text) > MAX_TOOL_RESULT_CHARS:
        return {"success": result.get("success", False), "truncated": True,
                "data": text[:MAX_TOOL_RESULT_CHARS]}
    return result


# ---- context window management ----

CONTEXT_RESERVE_TOKENS = 4096  # reserved for model response
MAX_MEMORY_CHARS = 4000


def estimate_tokens(text: str) -> int:
    """Heuristic token estimate. CJK chars ~1 token each, others ~4 chars/token."""
    if not text:
        return 0
    cjk = 0
    for ch in text:
        o = ord(ch)
        if (0x4E00 <= o <= 0x9FFF or 0x3400 <= o <= 0x4DBF
                or 0x3000 <= o <= 0x303F or 0xFF00 <= o <= 0xFFEF):
            cjk += 1
    other = len(text) - cjk
    return cjk + max(1, other // 4)


def estimate_messages_tokens(messages: list[dict]) -> int:
    total = 0
    for m in messages:
        content = m.get("content") or ""
        if isinstance(content, str):
            total += estimate_tokens(content)
        for tc in m.get("tool_calls") or []:
            fn = tc.get("function") or {}
            total += estimate_tokens(fn.get("name") or "") + estimate_tokens(fn.get("arguments") or "")
        total += 4  # per-message framing overhead
    return total


def context_budget() -> int:
    return max(2000, state.get_settings().max_context_tokens - CONTEXT_RESERVE_TOKENS)


def _compress_to_memory(msgs: list[dict]) -> str:
    """Summarize old conversation turns into a compact working memory."""
    transcript = "\n".join(
        f"[{m.get('role', '?')}] {(m.get('content') or '')[:800]}" for m in msgs
    )[:16000]
    prompt = f"""请把以下对话历史压缩成一份「工作记忆」，供后续对话衔接使用。要求：
1. 记录：用户的核心意图、已探索过的目录/文件、得到的关键结论、已提交的计划标题
2. 去掉冗余细节、重复内容和寒暄
3. 不超过 500 字，中文，分点列出

对话历史：
{transcript}"""
    try:
        result = chat_completion(
            messages=[
                {"role": "system", "content": "你是对话历史压缩助手，负责提炼关键信息为简短工作记忆。"},
                {"role": "user", "content": prompt},
            ],
            temperature=0.3,
            # A summary that runs long has defeated the point of compacting;
            # it must stay far smaller than the conversation it replaces.
            max_tokens=1024,
        )
        return (result.get("content") or "").strip()
    except Exception:
        return ""


class Session:
    """Conversation state: clean user/assistant history + rolling memory summary."""

    def __init__(self):
        self.history: list[dict] = []
        self.memory: str = ""

    def build_messages(self) -> list[dict]:
        msgs = [{"role": "system", "content": SYSTEM_PROMPT}]
        if self.memory:
            msgs.append({"role": "system", "content": f"【历史工作记忆（早期对话摘要）】\n{self.memory}"})
        # No sliding window here. A window that drops the oldest message each
        # turn shifts the whole prefix, which throws away the server's prompt
        # cache on every request. Size is bounded by compact() against the
        # token budget; MAX_SESSION_MESSAGES is only a pathological backstop.
        msgs.extend(self.history[-MAX_SESSION_MESSAGES:])
        return msgs

    def compact(self) -> bool:
        """Compress history until it fits the context budget.

        Returns True if anything changed. The final trim is a safety net: a
        single oversized message (a pasted document, say) cannot be split, and
        sending an over-budget prompt would be rejected by the server.
        """
        budget = context_budget()
        changed = False
        for _ in range(3):
            if estimate_messages_tokens(self.build_messages()) <= budget:
                return changed
            user_idx = [i for i, m in enumerate(self.history) if m.get("role") == "user"]
            if len(user_idx) < 2:
                # can't split safely — hard trim the oldest message
                if self.history:
                    self.history.pop(0)
                    changed = True
                continue
            half = max(1, len(user_idx) // 2)
            cut = user_idx[half]
            old = self.history[:cut]
            summary = _compress_to_memory(old)
            self.history = self.history[cut:]
            changed = True
            if summary:
                self.memory = (self.memory + "\n" + summary).strip()[-MAX_MEMORY_CHARS:]

        # Still over budget: drop from the front until it fits. Without this a
        # session whose single message exceeds the budget would send an
        # over-length prompt and fail at the model.
        while self.history and estimate_messages_tokens(self.build_messages()) > budget:
            self.history.pop(0)
            changed = True
        return changed


sessions: dict[str, Session] = {}


def _restore_session(session_id: str) -> Session:
    """Load session from in-memory cache, or rebuild from chat history db.

    The chat history table holds the full permanent record; on cold start we
    rebuild the agent's LLM context from the most recent turns there.
    """
    if session_id in sessions:
        return sessions[session_id]
    session = Session()
    try:
        chat_session = state.get_chat_history().get_session(session_id)
        if chat_session:
            session.history = [
                {"role": m.role, "content": m.content}
                for m in chat_session.messages
                if m.role in ("user", "assistant") and m.content
            ][-MAX_SESSION_MESSAGES:]
    except Exception:
        pass
    sessions[session_id] = session
    return session


def _get_session(session_id: str) -> Session:
    return _restore_session(session_id)


@agent.post("/chat", response_model=AgentResponse)
def agent_chat(req: AgentRequest):
    interaction.mark_active()
    session_id = req.session_id or "default"
    session = _get_session(session_id)
    session.history.append({"role": "user", "content": req.message})

    session.compact()
    all_messages = session.build_messages()

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
    session.history = [m for m in all_messages if m.get("role") in ("user", "assistant")][-MAX_SESSION_MESSAGES:]

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


@agent.post("/chat/stream")
def agent_chat_stream(req: AgentRequest):
    """Server-sent events variant of /chat.

    Event types: meta, step, token, tool, done, error. Tool-call turns are
    streamed too, so the model can emit a short "let me look" preamble before
    calling a tool instead of the UI sitting blank through the whole turn.
    """

    def sse(event: str, data: dict) -> str:
        return f"event: {event}\ndata: {json.dumps(data, ensure_ascii=False)}\n\n"

    def generate() -> Iterator[str]:
        # Tell the indexer to stand down: chat and embedding share the fast
        # cores, and a conversation measurably slows while indexing runs.
        interaction.mark_active()
        session_id = req.session_id or "default"
        session = _get_session(session_id)
        session.history.append({"role": "user", "content": req.message})
        session.compact()
        all_messages = session.build_messages()

        tool_results: list[dict] = []
        plan_ids: list[str] = []
        steps_used = 0
        max_steps = state.get_settings().max_agent_steps
        emitted: list[str] = []  # streamed text, kept for session history

        yield sse("meta", {"steps_max": max_steps})

        try:
            for _ in range(max_steps):
                steps_used += 1
                yield sse("step", {"step": steps_used, "of": max_steps})

                turn_text = ""
                tool_calls: list[dict] = []
                for kind, payload in chat_completion_stream(all_messages, tools=TOOLS):
                    if kind == "token":
                        turn_text += payload
                        emitted.append(payload)
                        yield sse("token", {"text": payload})
                    elif kind == "tool_call":
                        tool_calls.append(payload)

                if tool_calls:
                    all_messages.append({
                        "role": "assistant",
                        "content": turn_text,
                        "tool_calls": [
                            {
                                "id": tc["id"],
                                "type": "function",
                                "function": {"name": tc["name"], "arguments": tc["arguments"]},
                            }
                            for tc in tool_calls
                        ],
                    })
                    for tc in tool_calls:
                        name = tc["name"]
                        args_for_display: dict = {}
                        try:
                            arguments = json.loads(tc["arguments"] or "{}")
                            args_for_display = arguments
                        except json.JSONDecodeError as e:
                            result = {"success": False, "error": f"Malformed tool call: {e}"}
                        else:
                            # announce before running so the UI can show real
                            # progress instead of guessing when work finishes
                            yield sse("tool_start", {"tool": name, "args": args_for_display})
                            result = execute_tool(name, arguments)
                            if name == "submit_plan" and result.get("success"):
                                plan_ids.append(result.get("plan_id", ""))
                        tool_results.append({"tool": name, "result": result})
                        yield sse("tool", {"tool": name, "args": args_for_display, "result": result})
                        all_messages.append({
                            "role": "tool",
                            "tool_call_id": tc["id"],
                            "content": json.dumps(_trim(result), ensure_ascii=False),
                        })
                    continue

                final_text = turn_text or "（模型未返回内容，请重试）"
                yield sse("token", {"text": ""})
                all_messages.append({"role": "assistant", "content": final_text})
                break
            else:
                final_text = "已达到单次任务的最大步数限制，请查看已收集的信息或换个更简单的请求。"
                yield sse("token", {"text": final_text})
                all_messages.append({"role": "assistant", "content": final_text})

            session.history = [
                m for m in all_messages if m.get("role") in ("user", "assistant")
            ][-MAX_SESSION_MESSAGES:]

            plans = []
            if plan_ids:
                store = state.get_plan_store()
                plans = [p.model_dump() for p in (store.get_plan(pid) for pid in plan_ids) if p]

            yield sse("done", {
                "response": final_text,
                "tool_results": tool_results or None,
                "plans": plans,
                "steps_used": steps_used,
                "available_tools": AVAILABLE_TOOLS,
            })
        except GeneratorExit:
            # client hit stop / navigated away; keep what we already have
            log.info("agent stream for session %s closed by client", session_id)
            raise
        except Exception as e:
            log.exception("agent stream failed")
            yield sse("error", {"message": _friendly_error(e)})

    return StreamingResponse(
        generate(),
        media_type="text/event-stream",
        headers={
            "Cache-Control": "no-cache, no-transform",
            "Connection": "keep-alive",
            "X-Accel-Buffering": "no",
        },
    )


@agent.get("/tools")
def get_tools():
    """Tool reference for the UI.

    `definitions` keeps the raw JSON schema the model consumes; `display`
    carries the Chinese copy so the frontend holds no tool knowledge of its
    own and cannot drift from this list.
    """
    display = []
    for t in TOOLS:
        name = t["function"]["name"]
        display.append({
            "name": name,
            "display": TOOL_DISPLAY.get(name, {}),
            "risk_label": RISK_LABELS.get(TOOL_DISPLAY.get(name, {}).get("risk"), "未知"),
        })
    return {"tools": AVAILABLE_TOOLS, "definitions": TOOLS, "display": display}


@agent.delete("/sessions/{session_id}")
def delete_session(session_id: str):
    sessions.pop(session_id, None)
    return {"success": True}


@agent.delete("/sessions")
def delete_all_sessions():
    sessions.clear()
    return {"success": True}
