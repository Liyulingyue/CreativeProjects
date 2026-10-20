"""Native agent loop for file governance.

The agent can only observe the file system via read-only tools. Any mutating
operation must go through `submit_plan`, which persists an approval plan for
the user to review before execution.
"""

import json
import logging
import math
import re
import time
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
    # Per-turn override from the chat UI's provider dropdown. Wins
    # over the session's default provider. Resolved server-side via
    # _resolve_provider (override > session > main); a stale or
    # unknown id falls through rather than 400-ing the request.
    provider_id: Optional[str] = None
    # Legacy field kept so old clients can still POST without 422.
    # The router ignores it; provider selection lives on the
    # llm_providers table now.
    use_fast: bool = False


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
                    "limit": {"type": "integer", "description": "Max characters to read (default 12000; raise or lower as needed)."},
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
        "summary": "读取文本文件的内容，默认返回 12000 字，超长会截断，可用 offset 分批续读。支持纯文本、Markdown、代码、PDF 与 DOCX。文件很大、想了解整体内容时优先用 semantic_search。",
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
- semantic_search: 按含义检索已索引的段落，理解大文件时优先用它

你唯一可以提交变更的方式是 submit_plan 工具：它会把一份操作计划提交给用户审批，
在你提交计划后用户需要手动批准，计划才会被执行。你无法直接修改、移动或删除任何文件。

工作流程：
1. 先用只读工具充分了解文件系统的现状（目录结构、文件内容）
2. 分析哪些文件需要整理（如截图散落在根目录、命名不规范的文件等）
3. 用 submit_plan 提交一份结构化的整理计划，每个 action 都要给出清晰的 reason

【提交 plan 前必须再 list_directory 确认】
- 不要凭「我记得之前列表里看到过」就直接用某个路径——前几轮的操作可能已经把它移走。
- 每一个 move / rename / delete 之前，必须在 *当前轮次* 调用一次 list_directory（被读到的相对路径）确认文件还在你说的位置。
- 如果上一次 list_directory 距离这次 submit_plan 已经做过别的动作，必须重新 list。
- source_path / target_path 必须用 list_directory 真实返回的相对路径，包括子目录（"<子目录>/<文件名>"），不能省略子目录前缀。
- 找不到的文件，宁可把这个 action 拆成单独的计划或直接报告给用户，也不要用一个错误路径提交——错误路径会被服务端拒绝并留下「Source not found」这种部分失败，比不提交更糟。

注意：
- 路径都是相对于存储根目录的相对路径
- delete 操作要极其谨慎，只在明确无价值（如临时文件、重复文件）时建议
- 如果用户的请求不涉及文件变更，直接回答即可，无需提交计划
- 用中文与用户交流

【本地资料问答必须先检索】
- 用户问「产品/项目介绍」「这个产品是什么」「有哪些功能」「帮我总结某个项目」等，
  都是在问本地资料，不代表你没有上下文。必须先 list_directory('.') 检查文件，
  再对相关文档调用 semantic_search；如果找到明显的介绍文档（如 product-intro、
  README、项目介绍），优先用 read_file 读取该文件。DOCX/PDF 也支持读取。
- 绝不可以在未调用任何只读工具前回答「我没有资料」「没有上下文」或要求用户
  再提供文件。只有工具确实未找到资料或读取失败后，才能说明缺少证据。
- 工具返回了相关内容时，先根据内容作简要回答，并在答案中点明依据文件名；不要把
  上述检索规则或内部推理复述给用户。

【最重要的纪律：只根据取到的证据回答】
用户问的是你的文件，你必须先取证据再作答。绝不允许根据文件名、大小、格式或
训练记忆推测内容——那会编造出不存在的人物和情节。

- read_file 返回 truncated: true 时，说明你只看到了文件的一小部分。此时
  必须二选一：要么用 offset 继续读，要么改用 semantic_search 去找相关段落。
  在这两种情况下直接给出结论，就是编造。
- 想知道一份文件「讲了什么」，用 semantic_search，不要用 read_file 读开头。
- 报文件大小时直接引用 size_human（如「2.5 MB」），不要自己拿 size_bytes
  换算——手算中文单位极易出错，把 2.5 MB 写成「2.5 亿」这类错误很常见。
- 如果某个说法在工具返回的内容里找不到依据，就明确说「工具返回的内容中没有
  依据」，而不是补一个像样的答案。
- 不确定时坦白不确定，比给出一个听起来合理但错误的答案有用得多。"""


# Keywords that strongly suggest the user wants a file-system action.
# Used by the post-processing guard to catch cases where the model
# answered with text instead of calling submit_plan.
_ACTION_PATTERNS = [
    "建立", "创建", "新增", "添加",
    "移动", "转移", "搬移",
    "复制", "拷贝",
    "删除", "移除", "清理",
    "重命名", "改名",
    "放入", "放到", "放到", "放进",
    "整理", "归类",
    "把...移到", "把...放到",
]


def _looks_like_action_request(text: str) -> bool:
    if not text:
        return False
    return any(pat in text for pat in _ACTION_PATTERNS)


# ---- tool implementations (return dicts, never raise) ----

def _rel(path: Path) -> str:
    return str(path.relative_to(get_storage_root()))

def _resolve_provider(chat_svc, override_id: Optional[str], session_provider_id: Optional[str]):
    """Pick the LLMProvider for an agent turn.

    Precedence (highest first):
      1. `override_id` — per-turn override from the chat UI's
         provider dropdown, only honoured if it actually points at an
         existing provider. A stale or bogus id falls through to the
         session default rather than failing the request.
      2. `session_provider_id` — the provider the user picked for
         the session. Stale ids (the provider was deleted) also fall
         through.
      3. The system main — the singleton is_main=1 row.

    Returns None if no provider exists at all (i.e. the operator
    hasn't added one yet); the caller surfaces that as an error
    to the chat UI.
    """
    if override_id:
        p = chat_svc.get_provider(override_id)
        if p is not None:
            return p
    if session_provider_id:
        p = chat_svc.get_provider(session_provider_id)
        if p is not None:
            return p
    return chat_svc.get_main_provider()


def _human_size(num_bytes: int) -> str:
    """Render a byte count the same way the file browser does.

    The tool used to hand the model a raw byte count, which it then converted
    into Chinese units by hand -- and got it wrong, reporting a 2.5 MB file as
    "2.5亿字节". Formatting here leaves no arithmetic for it to fumble.
    """
    if num_bytes <= 0:
        return "0 B"
    k = 1024
    units = ("B", "KB", "MB", "GB", "TB")
    i = min(int(num_bytes.bit_length() ** 0.3333) if num_bytes < 1024 else
            min(int(math.log(num_bytes, k)), len(units) - 1), len(units) - 1)
    # Re-derive exactly rather than trusting the log for values just under a
    # boundary (e.g. 1048575 must read as 1024.0 KB, not 1.0 MB).
    value = num_bytes / (k ** i)
    while i < len(units) - 1 and value >= k:
        value /= k
        i += 1
    while i > 0 and value < 1:
        value *= k
        i -= 1
    return f"{value:.1f} {units[i]}"


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
                size = stat.st_size if entry.is_file() else 0
                items.append({
                    "name": entry.name,
                    "path": _rel(entry),
                    "is_dir": entry.is_dir(),
                    # size_bytes stays for anything that needs the exact value;
                    # size_human is what a reply should quote.
                    "size_human": _human_size(size),
                    "size_bytes": size,
                })
            except (PermissionError, OSError):
                continue
    except (PermissionError, OSError) as e:
        return {"success": False, "error": str(e)}
    return {"success": True, "path": _rel(target) if target != get_storage_root() else ".", "items": items, "count": len(items)}


def tool_read_file(path: str, limit: int = 12000, offset: int = 0) -> dict:
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

def chat_with_llm(messages: list[dict], tools: list[dict], provider) -> dict:
    try:
        result = chat_completion(messages, tools=tools, provider=provider)
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
        # Mirrors chat_sessions.provider_id. Set on restore from db, and
        # updated whenever a per-turn override is applied so the next
        # turn in the same session keeps the user's pick.
        self.provider_id: Optional[str] = None

    def build_messages(self) -> list[dict]:
        # Always use the full agent system prompt. The legacy FAST_SYSTEM_PROMPT
        # was a workaround for the 350M choking on the full instructions;
        # with the new providers design, picking a 350M is a deliberate
        # user choice (it shows in the chat dropdown) and the chat UI
        # surfaces the quality tradeoff in a tooltip, so the operator-
        # facing answer is now: "you picked a 350M, expect a 350M-grade
        # response". The stripped prompt hack is gone.
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
            # Mirror the persisted provider_id into the in-memory
            # session so the first turn after a cold start already
            # sees the right default.
            session.provider_id = chat_session.provider_id
    except Exception:
        pass
    sessions[session_id] = session
    return session


def _require_agent_session(session_id: str) -> None:
    """Reject arbitrary ids instead of silently creating shared contexts."""
    if session_id == "default" or not state.get_chat_history().get_session(session_id):
        raise HTTPException(status_code=404, detail="Agent session not found")


def _get_session(session_id: str) -> Session:
    return _restore_session(session_id)


@agent.post("/chat", response_model=AgentResponse)
def agent_chat(req: AgentRequest):
    interaction.mark_active()
    session_id = req.session_id or "default"
    _require_agent_session(session_id)
    session = _get_session(session_id)
    session.history.append({"role": "user", "content": req.message})

    session.compact()
    # Same provider-resolution precedence as the streaming endpoint.
    chat_svc = state.get_chat_history()
    provider = _resolve_provider(chat_svc, req.provider_id, session.provider_id)
    if provider is None:
        raise HTTPException(status_code=400, detail="没有可用的 LLM provider；请到 /settings 添加")
    if req.provider_id:
        chat_svc.update_session_provider(session_id, provider.id)
        session.provider_id = provider.id
    all_messages = session.build_messages()

    tool_results: list[dict] = []
    plan_ids: list[str] = []
    steps_used = 0
    final_text = ""

    for _ in range(state.get_settings().max_agent_steps):
        steps_used += 1
        response = chat_with_llm(all_messages, TOOLS, provider=provider)

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
            # can link the following tool results to their call ids. The
            # `type: "function"` field is required by the OpenAI tool-call
            # schema; llama-server returns it as None on the wire and rejects
            # it on the way back in ("Missing tool call type"), so inject
            # it here before the assistant message is sent back.
            tool_calls_for_history = [
                {
                    "id": tc.get("id", ""),
                    "type": "function",
                    "function": tc.get("function", {}),
                }
                for tc in tool_calls
            ]
            all_messages.append({
                "role": "assistant",
                "content": message.get("content") or "",
                "tool_calls": tool_calls_for_history,
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

        # ---- post-processing guard ----
        # Small models (350M class) occasionally respond with descriptive text
        # instead of calling submit_plan when the user asks for an action
        # (create folder / move / rename / delete). Detect this and inject a
        # correction nudge so the model gets a second chance to call the tool.
        last_user_msg = ""
        for m in reversed(all_messages[:-1]):
            if m.get("role") == "user":
                last_user_msg = m.get("content", "")
                break
        submitted_plan = any(
            tr.get("tool") == "submit_plan" for tr in tool_results
        )
        if _looks_like_action_request(last_user_msg) and not submitted_plan:
            correction = (
                "注意：你刚才的回复描述了操作结果，但并没有调用 submit_plan 工具。\n"
                "涉及文件变更（创建文件夹/移动/复制/删除/重命名）的操作必须通过 submit_plan "
                "提交计划，等待用户审批后才能执行。你无法直接修改任何文件。\n"
                "请立即调用 submit_plan 工具提交计划，不要在回复中描述操作结果。"
            )
            all_messages.append({"role": "user", "content": correction})
            steps_used -= 1   # correction nudge doesn't count as a step
            continue
        # ---------------------------
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

    session_id = req.session_id or "default"
    _require_agent_session(session_id)

    def sse(event: str, data: dict) -> str:
        return f"event: {event}\ndata: {json.dumps(data, ensure_ascii=False)}\n\n"

    def generate() -> Iterator[str]:
        # Tell the indexer to stand down: chat and embedding share the fast
        # cores, and a conversation measurably slows while indexing runs.
        interaction.mark_active()
        session = _get_session(session_id)
        session.history.append({"role": "user", "content": req.message})
        session.compact()

        # Resolve which provider this turn will use. Precedence:
        #   1. `req.provider_id` — per-turn override from the chat UI's
        #      provider dropdown. Wins over the session default.
        #   2. `session.provider_id` — what the user picked the last
        #      time they sent a message in this session.
        #   3. Main provider — the system's default. Set on the
        #      session at create time, so this branch is just a
        #      backstop in case a legacy session predates the column.
        # The user's pick is *stuck to the session* after this turn:
        # if they picked a non-main provider, the session row gets
        # updated so subsequent messages in the same conversation
        # keep using it. They can switch back via the dropdown.
        chat_svc = state.get_chat_history()
        provider = _resolve_provider(chat_svc, req.provider_id, session.provider_id)
        if provider is None:
            yield sse("error", {"message": "没有可用的 LLM provider；请到 /settings 添加"})
            return
        if req.provider_id:
            # Persist the override so subsequent turns in this session
            # keep using the chosen provider. Done before the loop so
            # the row write is visible to parallel readers (e.g. the
            # sidebar session loader).
            chat_svc.update_session_provider(session_id, provider.id)
            session.provider_id = provider.id
        all_messages = session.build_messages()

        tool_results: list[dict] = []
        plan_ids: list[str] = []
        steps_used = 0
        max_steps = state.get_settings().max_agent_steps
        emitted: list[str] = []  # streamed text, kept for session history
        turn_complete = False
        # Accumulated across steps: the model reasons again after each tool
        # call. Lives in the generator, not the enclosing function.
        all_reasoning = ""
        # Per-turn performance. t0 is request start (before any work);
        # first_token_at is stamped on the FIRST text byte the LLM produces
        # (reasoning chunk or answer chunk, whichever arrives first) so TTFT
        # reflects when the user perceives the model as "started" — the
        # first scratchpad delta lands in the UI even if it is collapsed.
        # first_answer_token_at and last_answer_token_at bracket the answer
        # decode phase only, so tps stays the model's actual generation rate
        # and is not dragged down by tool-call latency.
        t0 = time.monotonic()
        first_token_at: float | None = None
        first_answer_token_at: float | None = None
        last_answer_token_at: float | None = None
        token_count = 0

        yield sse("meta", {
            "steps_max": max_steps,
            "provider_id": provider.id,
            "provider_name": provider.name,
        })

        try:
            for _ in range(max_steps):
                steps_used += 1
                yield sse("step", {"step": steps_used, "of": max_steps})

                turn_text = ""
                tool_calls: list[dict] = []
                # The chosen provider drives the entire stream, including
                # any tool-call follow-up. Pick a 24B-class provider if
                # the turn calls tools; the 350M picks them unreliably.
                # The chat UI warns the user about this in the provider
                # dropdown's tooltip.
                for kind, payload in chat_completion_stream(all_messages, tools=TOOLS, provider=provider):
                    # First LLM text byte — reasoning chunk or answer chunk,
                    # whichever arrives first. Tool calls are structured, not
                    # text, so they don't qualify as "first token" in the
                    # LLM-serving sense. The user perceives the model as
                    # "started" the moment the first text delta lands,
                    # regardless of whether it's a scratchpad chunk or the
                    # final answer — so TTFT should reflect that.
                    if first_token_at is None and kind in ("reasoning", "token"):
                        first_token_at = time.monotonic()
                    if kind == "reasoning":
                        all_reasoning += payload
                        yield sse("reasoning", {"text": payload})
                    elif kind == "token":
                        # Bracket the answer-text decode phase. Across
                        # multiple agent steps each "final" step emits its
                        # own answer chunk; tps is computed over the union
                        # of those brackets minus the tool-call gaps.
                        if first_answer_token_at is None:
                            first_answer_token_at = time.monotonic()
                        last_answer_token_at = time.monotonic()
                        turn_text += payload
                        emitted.append(payload)
                        token_count += 1
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
            turn_complete = True

            plans = []
            if plan_ids:
                store = state.get_plan_store()
                plans = [p.model_dump() for p in (store.get_plan(pid) for pid in plan_ids) if p]

            # Per-turn perf for the UI / persistence.
            #   ttft_ms:    request start -> first LLM text byte
            #               (reasoning chunk or answer chunk — whichever
            #               lands first). Captures the "I started
            #               thinking" moment the user perceives.
            #   total_ms:   request start -> done (wall clock).
            #   tps:        answer decode rate only. Denominator is
            #               last_answer_token_at - first_answer_token_at,
            #               which excludes prefill, reasoning, and
            #               tool-call gaps. So tps reflects the model's
            #               pure generation speed, not turn efficiency.
            now = time.monotonic()
            total_ms = int((now - t0) * 1000)
            ttft_ms = int((first_token_at - t0) * 1000) if first_token_at else 0
            if first_answer_token_at and last_answer_token_at and last_answer_token_at > first_answer_token_at:
                decode_s = last_answer_token_at - first_answer_token_at
            elif first_token_at:
                # No answer tokens were emitted (reasoning-only or
                # all-tooling turn). Fall back to wall-clock so tps is
                # 0 rather than NaN.
                decode_s = now - first_token_at
            else:
                decode_s = 0.0
            tps = (token_count / decode_s) if decode_s > 0 else 0.0

            yield sse("done", {
                "response": final_text,
                "reasoning": all_reasoning,
                "tool_results": tool_results or None,
                "plans": plans,
                "steps_used": steps_used,
                "available_tools": AVAILABLE_TOOLS,
                "ttft_ms": ttft_ms,
                "total_ms": total_ms,
                "tokens": token_count,
                "tps": round(tps, 2),
                # Echo the resolved provider so the chat UI can render the
                # "via <name>" badge on the persisted bubble. The same id
                # is written to chat_messages.provider_id, so this is
                # the canonical answer the client should use.
                "provider_id": provider.id,
                "provider_name": provider.name,
            })
        except GeneratorExit:
            # The client already displayed these deltas. Keep the server-side
            # context and permanent transcript aligned with what it saw; do not
            # yield from a GeneratorExit handler (the connection is gone).
            partial = "".join(emitted).strip()
            if not turn_complete:
                if partial:
                    all_messages.append({"role": "assistant", "content": partial})
                session.history = [
                    m for m in all_messages if m.get("role") in ("user", "assistant")
                ][-MAX_SESSION_MESSAGES:]
            log.info("agent stream for session %s closed by client", session_id)
            raise
        except Exception as e:
            log.exception("agent stream failed")
            if not turn_complete:
                partial = "".join(emitted).strip()
                if partial:
                    all_messages.append({"role": "assistant", "content": partial})
                session.history = [
                    m for m in all_messages if m.get("role") in ("user", "assistant")
                ][-MAX_SESSION_MESSAGES:]
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
