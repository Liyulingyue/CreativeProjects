"""Skills: methodology on top of tools.

A skill is NOT a fixed pipeline. It is a chunk of instruction that,
when active, is injected into the model's system prompt so the model
knows HOW to approach certain tasks. The model still freely picks
which tools to call — the skill merely guides its strategy.

Two levels of tools:
  - base:  always available (search, list, read, write)
  - atomic: only exposed when skills are on (move, delete, mkdir, info)

The two-level tool exposure is deliberate: a 20B model with access
to move/delete but no methodology is dangerous. With a skill's
instruction guiding it, the same model knows when and how to use
those powers.
"""

from __future__ import annotations

import io
import json
import re
import shutil
import threading
import time
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any, Optional

from .deps import state, can_access, visible_children
from .trash import TrashService

# ---------------------------------------------------------------------------
# Tool definitions (OpenAI function-calling format)
# ---------------------------------------------------------------------------


def _tool(name: str, description: str, properties: dict, required: list[str] | None = None) -> dict:
    return {
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": {
                "type": "object",
                "properties": properties,
                "required": required or list(properties.keys()),
            },
        },
    }


TOOL_SEARCH = _tool(
    "search_files",
    "按文件名或内容搜索文件，返回匹配的文件路径列表。",
    {
        "query": {"type": "string", "description": "搜索关键词"},
        "limit": {"type": "integer", "description": "最多返回条数", "default": 20},
    },
    ["query"],
)

TOOL_LIST_DIR = _tool(
    "list_directory",
    "列出一个目录下的文件和子目录。",
    {"path": {"type": "string", "description": "目录路径，空字符串表示根目录"}},
    ["path"],
)

TOOL_READ_FILE = _tool(
    "read_file",
    "读取一个文本文件的内容。",
    {
        "path": {"type": "string", "description": "要读取的文件路径"},
        "limit": {"type": "integer", "description": "最多读取字符数", "default": 50000},
    },
    ["path"],
)

TOOL_SEMANTIC_SEARCH = _tool(
    "semantic_search",
    "按含义而非关键词搜索已索引的文件内容。适合了解一份大文档讲了什么，或寻找措辞未知的相关段落。",
    {
        "query": {"type": "string", "description": "自然语言描述你想找什么"},
        "top_k": {"type": "integer", "description": "返回段落数，默认 5，最大 20"},
    },
    ["query"],
)

TOOL_WRITE_FILE = _tool(
    "write_file",
    "创建或覆盖一个文件。",
    {
        "path": {"type": "string", "description": "目标文件路径"},
        "content": {"type": "string", "description": "文件内容"},
    },
    ["path", "content"],
)

# Atomic — only available when skills are enabled
TOOL_MOVE_FILE = _tool(
    "move_file",
    "移动或重命名文件。",
    {
        "source": {"type": "string", "description": "当前路径"},
        "destination": {"type": "string", "description": "目标路径"},
    },
    ["source", "destination"],
)

TOOL_DELETE_FILE = _tool(
    "delete_file",
    "把文件或目录移入回收站。",
    {"path": {"type": "string", "description": "要删除的路径"}},
    ["path"],
)

TOOL_CREATE_DIR = _tool(
    "create_directory",
    "新建目录。",
    {"path": {"type": "string", "description": "要创建的目录路径"}},
    ["path"],
)

TOOL_FILE_INFO = _tool(
    "file_info",
    "获取文件元信息（大小、修改时间、类型）。",
    {"path": {"type": "string", "description": "文件路径"}},
    ["path"],
)

TOOL_READ_SKILL = _tool(
    "read_skill",
    "读取一个技能的完整说明（SKILL.md）或它附带的资源文件。技能说明通常只在系统提示里给出摘要，需要细节时用这个工具按需读取。",
    {
        "skill_id": {"type": "string", "description": "技能 ID"},
        "file": {"type": "string", "description": "资源文件的相对路径；留空读取 SKILL.md"},
    },
    ["skill_id"],
)

TOOL_LIST_SKILL_FILES = _tool(
    "list_skill_files",
    "列出某个技能附带的文件（SKILL.md 与资源），用于知道有哪些资料可读。",
    {"skill_id": {"type": "string", "description": "技能 ID"}},
    ["skill_id"],
)

BASE_TOOLS = [TOOL_SEARCH, TOOL_LIST_DIR, TOOL_READ_FILE, TOOL_WRITE_FILE, TOOL_SEMANTIC_SEARCH]
ATOMIC_TOOLS = [TOOL_MOVE_FILE, TOOL_DELETE_FILE, TOOL_CREATE_DIR, TOOL_FILE_INFO]
META_TOOLS = [TOOL_READ_SKILL, TOOL_LIST_SKILL_FILES]
ALL_TOOLS = BASE_TOOLS + ATOMIC_TOOLS + META_TOOLS

# Human-facing metadata for the settings UI and safety labeling.
TOOL_META: dict[str, dict] = {
    "semantic_search": {"label": "语义检索", "description": "按含义搜索文档内容", "danger": "safe"},
    "search_files":  {"label": "搜索文件",    "description": "按关键词搜索文件名或内容", "danger": "safe"},
    "list_directory": {"label": "列出目录",   "description": "查看目录结构",           "danger": "safe"},
    "read_file":     {"label": "读取文件",    "description": "读取文本文件内容",        "danger": "safe"},
    "write_file":    {"label": "写入文件",    "description": "创建或覆盖文件",          "danger": "low"},
    "move_file":     {"label": "移动/重命名", "description": "移动或重命名文件",        "danger": "medium"},
    "delete_file":   {"label": "删除文件",    "description": "移入回收站",             "danger": "high"},
    "create_directory": {"label": "创建目录", "description": "新建文件夹",             "danger": "low"},
    "file_info":     {"label": "文件信息",    "description": "查看大小、修改时间等",    "danger": "safe"},
    "read_skill":    {"label": "读取技能",    "description": "读取 SKILL.md 或资源文件", "danger": "safe"},
    "list_skill_files": {"label": "技能文件", "description": "列出技能附带文件",         "danger": "safe"},
}
ALL_TOOL_NAMES = [t["function"]["name"] for t in ALL_TOOLS]

# The agent endpoint ships its own richer implementations of these tools
# (with per-user access checks and better argument handling), so they are
# always available there. The skills registry's copies only matter to the
# legacy /api/chat endpoint; the settings UI hides their toggles because
# flipping them on the agent path would do nothing.
AGENT_BUILTIN_TOOL_NAMES = {"list_directory", "read_file", "semantic_search", "search_files"}

# Read-only/info tools run directly; move/delete default to plan-only so a
# mistaken direct call cannot mutate files without the user approving a plan.
DEFAULT_DIRECT_EXECUTABLE = {"safe": True, "low": True, "medium": False, "high": False}

# Meta tools that must be present whenever skills are on: without them the
# model cannot read a file-based skill's SKILL.md or resources, so they are
# not user-toggleable.
ALWAYS_ON_TOOL_NAMES = {"read_skill", "list_skill_files"}

# ---------------------------------------------------------------------------
# Built-in skills
# ---------------------------------------------------------------------------

BUILTIN_SKILLS: dict[str, dict] = {
    "organize": {
        "label": "整理文件",
        "description": "按类型、日期整理杂乱的目录",
        "instruction": (
            "整理文件的方法：\n"
            "1. 先用 list_directory 看清目录结构\n"
            "2. 按文件类型或修改日期分组（图片、文档、视频等）\n"
            "3. 必要时 create_directory 创建子目录\n"
            "4. 用 move_file 把文件移入对应目录\n"
            "5. 每一步都告诉用户你做了什么\n"
            "永远不要未经用户确认就删除任何文件。"
        ),
        "enabled": True,
    },
    "deduplicate": {
        "label": "去重清理",
        "description": "找出重复文件并清理多余副本",
        "instruction": (
            "去重的方法：\n"
            "1. 用 list_directory 或 search_files 扫描目录\n"
            "2. 用 file_info 对比大小和修改时间找出重复\n"
            "3. 列出重复文件让用户确认\n"
            "4. 确认后 delete_file 删除多余副本\n"
            "删除前必须得到用户明确同意。"
        ),
        "enabled": True,
    },
    "summarize": {
        "label": "内容摘要",
        "description": "总结一个文件或目录的内容",
        "instruction": (
            "总结内容的方法：\n"
            "1. 先 list_directory 了解目录下有哪些文件\n"
            "2. read_file 读取关键文件\n"
            "3. 用简洁的中文给用户一个总结"
        ),
        "enabled": True,
    },
    "find_old": {
        "label": "旧文件搜寻",
        "description": "按时间、名称等条件搜寻老旧文件",
        "instruction": (
            "找旧文件的方法：\n"
            "1. 用 list_directory 或 search_files 缩小范围\n"
            "2. 用 file_info 查看修改时间\n"
            "3. 帮用户定位很久没动过的文件或特定时间段的文件\n"
            "按修改时间排序展示结果。"
        ),
        "enabled": False,
    },
}

# ---------------------------------------------------------------------------
# Tool handlers
# ---------------------------------------------------------------------------

def _abs_path(path: str) -> Path:
    """Resolve a user-supplied path under the storage root.

    Raises ValueError if the path resolves outside the root — without
    this, a crafted path like "../../etc/passwd" would read or write
    anywhere on the host.
    """
    root = Path(state.get_settings().storage_path).resolve()
    p = Path(path)
    target = (root / p) if not p.is_absolute() else p
    target = target.resolve()
    if not target.is_relative_to(root):
        raise ValueError(f"路径越界: {path}")
    return target


def _rel(p: Path) -> str:
    root = Path(state.get_settings().storage_path).resolve()
    try:
        return str(p.relative_to(root))
    except ValueError:
        return str(p)


def _handle_search_files(query: str, limit: int = 20) -> dict:
    from .routers.search import search_files
    from .deps import interaction
    interaction.mark_active()
    result = search_files(q=query, limit=limit)
    files = [_rel(Path(i.path)) for i in result.items]
    return {"files": files, "total": len(files)}


def _handle_list_directory(path: str) -> dict:
    target = _abs_path(path) if path.strip() else Path(state.get_settings().storage_path)
    if not target.exists():
        return {"error": f"目录不存在: {path}"}
    all_children = list(target.iterdir())
    allowed = visible_children(target, all_children)
    entries = []
    for child in sorted(allowed):
        entries.append({
            "name": child.name,
            "type": "directory" if child.is_dir() else "file",
            "path": _rel(child),
        })
    return {"path": _rel(target), "entries": entries}


def _handle_read_file(path: str, limit: int | None = None) -> dict:
    if limit is None or limit < 1 or limit > 200000:
        limit = 50000
    target = _abs_path(path)
    if not target.exists():
        return {"error": f"文件不存在: {path}"}
    if not can_access(target, "read"):
        return {"error": f"无权读取: {path}"}
    try:
        content = target.read_text(encoding="utf-8")[:limit]
        return {"path": _rel(target), "content": content, "truncated": len(content) >= limit}
    except Exception as e:
        return {"error": str(e)}


def _handle_write_file(path: str, content: str) -> dict:
    target = _abs_path(path)
    if target.exists() and not can_access(target, "write"):
        return {"error": f"无权写入: {path}"}
    if target.exists():
        # Keep what is there now, so overwriting can be undone.
        try:
            state.get_version_store().snapshot(target, note="被 Agent 写入覆盖")
        except Exception:
            pass
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(content, encoding="utf-8")
    return {"path": _rel(target), "size": target.stat().st_size, "written": True}


def _handle_move_file(source: str, destination: str) -> dict:
    src = _abs_path(source)
    dst = _abs_path(destination)
    if not src.exists():
        return {"error": f"源文件不存在: {source}"}
    if not can_access(src, "write") or not can_access(dst.parent, "write"):
        return {"error": f"无权移动: {source}"}
    dst.parent.mkdir(parents=True, exist_ok=True)
    src.rename(dst)
    return {"source": _rel(src), "destination": _rel(dst), "moved": True}


def _handle_delete_file(path: str) -> dict:
    target = _abs_path(path)
    if not target.exists():
        return {"error": f"路径不存在: {path}"}
    if not can_access(target, "write"):
        return {"error": f"无权删除: {path}"}
    svc = TrashService(Path(state.get_settings().storage_path))
    try:
        entry = svc.trash(target)
    except Exception as e:
        return {"error": str(e)}
    return {"path": _rel(target), "trashed_to": entry.get("id", "")}


def _handle_create_directory(path: str) -> dict:
    target = _abs_path(path)
    if not can_access(target.parent, "write"):
        return {"error": f"无权创建目录: {path}"}
    target.mkdir(parents=True, exist_ok=True)
    return {"path": _rel(target), "created": True}


def _handle_semantic_search(query: str, top_k: int = 5) -> dict:
    from .deps import interaction
    from .search_engine import search_engine
    interaction.mark_active()
    payload = search_engine.hybrid_search(query, min(top_k, 20), "", "semantic")
    results = payload.get("results", [])
    passages = []
    for r in results:
        passages.append({
            "path": _rel(Path(r.get("path", ""))),
            "score": r.get("score", 0),
            "text": r.get("preview", "")[:200],
        })
    return {"query": query, "passages": passages, "total": len(passages)}


def _handle_file_info(path: str) -> dict:
    target = _abs_path(path)
    if not target.exists():
        return {"error": f"路径不存在: {path}"}
    if not can_access(target, "read"):
        return {"error": f"无权访问: {path}"}
    st = target.stat()
    return {
        "path": _rel(target),
        "size": st.st_size,
        "modified": time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(st.st_mtime)),
        "type": "directory" if target.is_dir() else "file",
    }


TOOL_HANDLERS: dict[str, Any] = {
    "search_files": _handle_search_files,
    "list_directory": _handle_list_directory,
    "read_file": _handle_read_file,
    "semantic_search": _handle_semantic_search,
    "write_file": _handle_write_file,
    "move_file": _handle_move_file,
    "delete_file": _handle_delete_file,
    "create_directory": _handle_create_directory,
    "file_info": _handle_file_info,
}


MAX_TOOL_RESULT_CHARS = 4000


def _trim_tool_result(result: dict) -> dict:
    """Keep tool results bounded so a large result does not blow the
    model's context window."""
    text = json.dumps(result, ensure_ascii=False)
    if len(text) > MAX_TOOL_RESULT_CHARS:
        return {"success": result.get("success", False), "truncated": True,
                "data": text[:MAX_TOOL_RESULT_CHARS]}
    return result


def execute_tool(name: str, arguments: dict) -> dict:
    handler = TOOL_HANDLERS.get(name)
    if handler is None:
        return {"success": False, "error": f"未知工具: {name}"}
    try:
        result = _trim_tool_result(handler(**arguments))
        if "success" not in result:
            result["success"] = True
        return result
    except TypeError as e:
        return {"success": False, "error": f"参数错误: {e}"}
    except Exception as e:
        return {"success": False, "error": str(e)}

# ---------------------------------------------------------------------------
# File-based skills (SKILL.md + resources)
# ---------------------------------------------------------------------------

SKILL_FILE_NAME = "SKILL.md"
SKILL_ID_RE = re.compile(r"^[a-z][a-z0-9_]{1,63}$")
MAX_SKILL_FILES = 200
MAX_SKILL_BYTES = 50 * 1024 * 1024
MAX_SKILL_READ_CHARS = 200_000


def skills_root() -> Path:
    from .deps import DATA_DIR
    root = DATA_DIR / "skills"
    root.mkdir(parents=True, exist_ok=True)
    return root


def normalize_skill_id(raw: str) -> str:
    """Turn a frontmatter name or folder name into a safe skill id."""
    text = (raw or "").strip().lower()
    text = re.sub(r"[^a-z0-9_]+", "_", text).strip("_")
    if not text:
        raise ValueError("技能 ID 不能为空")
    if not text[0].isalpha():
        text = "skill_" + text
    if not SKILL_ID_RE.match(text):
        raise ValueError("技能 ID 须以小写字母开头，仅含小写字母、数字、下划线，2-64 个字符")
    return text


def _split_frontmatter(text: str) -> tuple[dict, str]:
    """Parse a leading YAML frontmatter block if present."""
    if not text.startswith("---"):
        return {}, text
    nl = text.find("\n")
    if nl == -1:
        return {}, text
    end = text.find("\n---", nl)
    if end == -1:
        return {}, text
    fm_text = text[nl + 1:end]
    body = text[end + 4:]
    if body.startswith("\n"):
        body = body[1:]
    try:
        import yaml
        fm = yaml.safe_load(fm_text) or {}
        if isinstance(fm, dict):
            return fm, body
    except Exception:
        pass
    return {}, text


def _safe_rel(rel: str) -> str:
    rel = (rel or "").replace("\\", "/")
    path = PurePosixPath(rel)
    if path.is_absolute():
        raise ValueError("非法路径")
    parts = [part for part in path.parts if part not in ("", ".")]
    if not parts or any(part == ".." for part in parts):
        raise ValueError("非法路径")
    return "/".join(parts)


def _read_dir_skill(skill_dir: Path) -> Optional[dict]:
    md = skill_dir / SKILL_FILE_NAME
    if not md.is_file():
        return None
    try:
        raw = md.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None
    fm, body = _split_frontmatter(raw)
    files: list[str] = []
    for path in sorted(skill_dir.rglob("*")):
        if path.is_file():
            files.append(path.relative_to(skill_dir).as_posix())
    return {
        "id": skill_dir.name,
        "label": str(fm.get("name") or skill_dir.name),
        "description": str(fm.get("description") or ""),
        "instruction": body,
        "files": files,
        "source": "file",
    }


def _write_skill_md(skill_dir: Path, label: str, description: str, body: str) -> None:
    """Rewrite SKILL.md, preserving any extra frontmatter keys."""
    md = skill_dir / SKILL_FILE_NAME
    meta: dict = {}
    if md.is_file():
        try:
            existing, _ = _split_frontmatter(md.read_text(encoding="utf-8", errors="replace"))
            if isinstance(existing, dict):
                meta.update(existing)
        except OSError:
            pass
    meta["name"] = label
    meta["description"] = description
    try:
        import yaml
        header = yaml.safe_dump(meta, allow_unicode=True, sort_keys=False).strip()
    except Exception:
        header = f"name: {label}\ndescription: {description}"
    md.write_text(f"---\n{header}\n---\n\n{body}", encoding="utf-8")


def discover_dir_skills() -> dict[str, dict]:
    root = skills_root()
    found: dict[str, dict] = {}
    try:
        children = sorted(root.iterdir())
    except OSError:
        return found
    for child in children:
        if not child.is_dir() or not SKILL_ID_RE.match(child.name):
            continue
        info = _read_dir_skill(child)
        if info:
            found[child.name] = info
    return found


def _skill_dir(skill_id: str) -> Path:
    if not SKILL_ID_RE.match(skill_id or ""):
        raise ValueError("非法技能 ID")
    root = skills_root().resolve()
    target = (root / skill_id).resolve()
    if target.parent != root:
        raise ValueError("非法技能 ID")
    return target


def _handle_list_skill_files(skill_id: str) -> dict:
    skill = discover_dir_skills().get(skill_id)
    if not skill:
        return {"error": f"技能不存在或不是文件技能: {skill_id}"}
    return {"skill_id": skill_id, "files": skill["files"], "total": len(skill["files"])}


def _handle_read_skill(skill_id: str, file: str = "") -> dict:
    skill = discover_dir_skills().get(skill_id)
    if not skill:
        builtin = BUILTIN_SKILLS.get(skill_id)
        if builtin:
            return {"skill_id": skill_id, "file": SKILL_FILE_NAME,
                    "content": builtin["instruction"], "builtin": True}
        return {"error": f"技能不存在: {skill_id}"}
    base = _skill_dir(skill_id).resolve()
    rel = _safe_rel(file) if file else SKILL_FILE_NAME
    target = (base / rel).resolve()
    try:
        target.relative_to(base)
    except ValueError:
        return {"error": "资源路径越界"}
    if not target.is_file():
        return {"error": f"文件不存在: {rel}"}
    try:
        if target.stat().st_size > 4 * 1024 * 1024:
            return {"error": "文件过大，无法读取"}
        content = target.read_text(encoding="utf-8", errors="replace")
    except OSError as e:
        return {"error": str(e)}
    return {
        "skill_id": skill_id,
        "file": rel,
        "content": content[:MAX_SKILL_READ_CHARS],
        "truncated": len(content) > MAX_SKILL_READ_CHARS,
    }


TOOL_HANDLERS.update({
    "read_skill": _handle_read_skill,
    "list_skill_files": _handle_list_skill_files,
})


MAX_RESOURCE_BYTES = 4 * 1024 * 1024


def _require_dir_skill(skill_id: str) -> Path:
    if skill_id not in discover_dir_skills():
        raise ValueError("技能不存在或不是文件技能")
    return _skill_dir(skill_id)


def _resource_target(skill_id: str, rel: str) -> tuple[Path, str]:
    safe = _safe_rel(rel)
    if safe == SKILL_FILE_NAME:
        raise ValueError("请通过「编辑技能」修改 SKILL.md")
    base = _require_dir_skill(skill_id).resolve()
    target = (base / safe).resolve()
    try:
        target.relative_to(base)
    except ValueError:
        raise ValueError("资源路径越界")
    return target, safe


def read_skill_resource(skill_id: str, rel: str) -> dict:
    target, safe = _resource_target(skill_id, rel)
    if not target.is_file():
        raise ValueError(f"文件不存在: {safe}")
    if target.stat().st_size > MAX_RESOURCE_BYTES:
        raise ValueError("文件过大（上限 4MB）")
    data = target.read_bytes()
    if b"\x00" in data[:2048]:
        raise ValueError("二进制文件不支持在线编辑")
    return {"path": safe, "content": data.decode("utf-8", "replace"),
            "size": len(data)}


def write_skill_resource(skill_id: str, rel: str, content: str) -> dict:
    target, safe = _resource_target(skill_id, rel)
    data = content.encode("utf-8")
    if len(data) > MAX_RESOURCE_BYTES:
        raise ValueError("文件过大（上限 4MB）")
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(data)
    return {"path": safe, "size": len(data)}


def add_skill_resources(skill_id: str, files: list[tuple[str, bytes]]) -> dict:
    base = _require_dir_skill(skill_id).resolve()
    if not files:
        raise ValueError("没有收到文件")
    total = sum(len(data) for _, data in files)
    if total > MAX_SKILL_BYTES:
        raise ValueError("资源总大小超限（上限 50MB）")
    added: list[str] = []
    for filename, data in files:
        safe = _safe_rel(filename)
        if safe == SKILL_FILE_NAME:
            continue
        target = (base / safe).resolve()
        try:
            target.relative_to(base)
        except ValueError:
            raise ValueError("资源路径越界")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        added.append(safe)
    return {"files": added, "count": len(added)}


def delete_skill_resource(skill_id: str, rel: str) -> dict:
    target, safe = _resource_target(skill_id, rel)
    if not target.is_file():
        raise ValueError(f"文件不存在: {safe}")
    target.unlink()
    return {"deleted": safe}


def _write_skill_tree(skill_id: str, entries: dict[str, bytes]) -> dict:
    if SKILL_FILE_NAME not in entries:
        raise ValueError("技能包必须包含 SKILL.md")
    if len(entries) > MAX_SKILL_FILES:
        raise ValueError(f"文件数过多（上限 {MAX_SKILL_FILES}）")
    if sum(len(v) for v in entries.values()) > MAX_SKILL_BYTES:
        raise ValueError("技能包过大（上限 50MB）")
    root = skills_root()
    target = root / skill_id
    if target.exists():
        raise ValueError(f"技能已存在: {skill_id}")
    target.mkdir(parents=True)
    try:
        for rel, data in entries.items():
            dest = target / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(data)
        info = _read_dir_skill(target)
        if not info:
            raise ValueError("SKILL.md 无法解析")
    except Exception:
        shutil.rmtree(target, ignore_errors=True)
        raise
    return info


def _derive_skill_id(entries: dict[str, bytes], prefix: str, explicit: Optional[str]) -> str:
    if explicit:
        return normalize_skill_id(explicit)
    fm, _ = _split_frontmatter(entries.get(SKILL_FILE_NAME, b"").decode("utf-8", "replace"))
    candidate = str(fm.get("name") or "")
    if not candidate and prefix not in ("", "."):
        candidate = prefix.split("/")[-1]
    if not candidate:
        candidate = "skill"
    return normalize_skill_id(candidate)


def _collect_entries(raw: dict[str, bytes], prefix: str) -> dict[str, bytes]:
    entries: dict[str, bytes] = {}
    for rel, data in raw.items():
        rel = rel.replace("\\", "/")
        if prefix not in ("", "."):
            if not rel.startswith(prefix + "/"):
                continue
            rel = rel[len(prefix) + 1:]
        if not rel or rel.startswith("__MACOSX/"):
            continue
        safe = _safe_rel(rel)
        entries[safe] = data
    if SKILL_FILE_NAME not in entries:
        raise ValueError("技能包必须包含 SKILL.md")
    return entries


def import_zip_skill(data: bytes, skill_id: Optional[str] = None) -> dict:
    try:
        zf = zipfile.ZipFile(io.BytesIO(data))
    except zipfile.BadZipFile:
        raise ValueError("不是有效的 ZIP 文件")
    with zf:
        infos = [i for i in zf.infolist() if not i.is_dir() and not i.filename.endswith("/")]
        if not infos:
            raise ValueError("ZIP 是空的")
        if len(infos) > MAX_SKILL_FILES:
            raise ValueError(f"文件数过多（上限 {MAX_SKILL_FILES}）")
        if sum(i.file_size for i in infos) > MAX_SKILL_BYTES:
            raise ValueError("技能包过大（上限 50MB）")
        names = [i.filename.replace("\\", "/") for i in infos]
        md_name = next((n for n in names if Path(n).name.lower() == SKILL_FILE_NAME.lower()), None)
        if not md_name:
            raise ValueError("ZIP 中缺少 SKILL.md")
        prefix = str(PurePosixPath(md_name).parent)
        raw: dict[str, bytes] = {}
        for info, name in zip(infos, names):
            raw[name] = zf.read(info)
        entries = _collect_entries(raw, prefix)
        sid = _derive_skill_id(entries, prefix, skill_id)
        return _write_skill_tree(sid, entries)


def import_files_skill(files: list[tuple[str, bytes]], skill_id: Optional[str] = None) -> dict:
    if not files:
        raise ValueError("没有收到文件")
    if len(files) > MAX_SKILL_FILES:
        raise ValueError(f"文件数过多（上限 {MAX_SKILL_FILES}）")
    if sum(len(d) for _, d in files) > MAX_SKILL_BYTES:
        raise ValueError("技能包过大（上限 50MB）")
    raw: dict[str, bytes] = {}
    md_name = None
    for filename, data in files:
        name = (filename or "").replace("\\", "/")
        if not name:
            continue
        if Path(name).name.lower() == SKILL_FILE_NAME.lower() and md_name is None:
            md_name = name
        raw[name] = data
    if not md_name:
        raise ValueError("请选择包含 SKILL.md 的文件")
    prefix = str(PurePosixPath(md_name).parent)
    entries = _collect_entries(raw, prefix)
    sid = _derive_skill_id(entries, prefix, skill_id)
    return _write_skill_tree(sid, entries)


# ---------------------------------------------------------------------------
# Skill registry
# ---------------------------------------------------------------------------


class SkillRegistry:
    """Built-in skills plus user-uploaded custom skills."""

    def __init__(self):
        self._custom: dict[str, dict] = {}
        self._master: bool = False
        self._overrides: dict[str, bool] = {}
        self._tool_overrides: dict[str, bool] = {}
        # Per-tool "may be called directly" flag. A tool can be visible
        # to the model (advertised so it can plan with it) yet not
        # directly executable — it may only run through an approved plan.
        self._exec_overrides: dict[str, bool] = {}
        self._lock = threading.Lock()
        self._load()

    def _path(self) -> Path:
        from .deps import DATA_DIR
        return DATA_DIR / "skills.json"

    def _load(self):
        p = self._path()
        if p.exists():
            try:
                data = json.loads(p.read_text(encoding="utf-8"))
                self._custom = data.get("custom", {})
                self._master = data.get("master", False)
                self._overrides = data.get("enabled", {})
                self._tool_overrides = data.get("tools", {})
                self._exec_overrides = data.get("exec", {})
            except Exception:
                pass

    def _save(self):
        with self._lock:
            self._path().write_text(
                json.dumps(
                    {"custom": self._custom, "master": self._master,
                     "enabled": self._overrides, "tools": self._tool_overrides,
                     "exec": self._exec_overrides},
                    ensure_ascii=False,
                    indent=2,
                ),
                encoding="utf-8",
            )

    @property
    def master_enabled(self) -> bool:
        return self._master

    def set_master(self, v: bool):
        self._master = bool(v)
        self._save()

    def all_skills(self) -> list[dict]:
        result = []
        for sid, skill in BUILTIN_SKILLS.items():
            result.append({
                "id": sid,
                "label": skill["label"],
                "description": skill["description"],
                "enabled": self._overrides.get(sid, skill.get("enabled", False)),
                "builtin": True,
                "source": "builtin",
                "instruction": skill["instruction"],
            })
        dir_skills = discover_dir_skills()
        for sid, skill in dir_skills.items():
            result.append({
                "id": sid,
                "label": skill["label"],
                "description": skill["description"],
                "enabled": self._overrides.get(sid, False),
                "builtin": False,
                "source": "file",
                "files": skill["files"],
                "instruction": skill["instruction"],
            })
        for sid, skill in self._custom.items():
            if sid in dir_skills:
                continue
            result.append({
                "id": sid,
                "label": skill["label"],
                "description": skill.get("description", ""),
                "enabled": self._overrides.get(sid, skill.get("enabled", False)),
                "builtin": False,
                "source": "inline",
                "instruction": skill["instruction"],
            })
        return result

    def set_enabled(self, skill_id: str, enabled: bool):
        self._overrides[skill_id] = bool(enabled)
        self._save()

    def add_custom(self, skill_id: str, label: str, instruction: str, description: str = "") -> dict:
        if skill_id in BUILTIN_SKILLS:
            raise ValueError(f"技能 ID '{skill_id}' 与内置技能冲突")
        if skill_id in discover_dir_skills():
            raise ValueError(f"技能 ID '{skill_id}' 已存在")
        if not skill_id or not label or not instruction:
            raise ValueError("技能 ID、名称和指令不能为空")
        self._custom[skill_id] = {"label": label, "instruction": instruction, "description": description}
        self._overrides[skill_id] = False
        self._save()
        return {"id": skill_id, "label": label, "description": description,
                "instruction": instruction, "enabled": False, "builtin": False}

    def update_custom(self, skill_id: str, label: Optional[str] = None,
                      description: Optional[str] = None,
                      instruction: Optional[str] = None) -> dict:
        if skill_id in BUILTIN_SKILLS:
            raise ValueError("内置技能不可修改")
        dir_skills = discover_dir_skills()
        if skill_id in dir_skills:
            info = dir_skills[skill_id]
            new_label = (label if label is not None else info["label"]).strip()
            new_desc = description if description is not None else info["description"]
            new_body = instruction if instruction is not None else info["instruction"]
            if not new_label:
                raise ValueError("名称不能为空")
            if instruction is not None and not instruction.strip():
                raise ValueError("指令不能为空")
            _write_skill_md(_skill_dir(skill_id), new_label, new_desc, new_body)
            updated = discover_dir_skills()[skill_id]
            return {
                "id": skill_id, "label": updated["label"],
                "description": updated["description"],
                "instruction": updated["instruction"], "files": updated["files"],
                "enabled": self._overrides.get(skill_id, False),
                "builtin": False, "source": "file",
            }
        if skill_id in self._custom:
            entry = self._custom[skill_id]
            if label is not None:
                if not label.strip():
                    raise ValueError("名称不能为空")
                entry["label"] = label.strip()
            if description is not None:
                entry["description"] = description
            if instruction is not None:
                if not instruction.strip():
                    raise ValueError("指令不能为空")
                entry["instruction"] = instruction
            self._save()
            return {
                "id": skill_id, "label": entry["label"],
                "description": entry.get("description", ""),
                "instruction": entry["instruction"],
                "enabled": self._overrides.get(skill_id, False),
                "builtin": False, "source": "inline",
            }
        raise ValueError("技能不存在")

    def remove_custom(self, skill_id: str) -> bool:
        removed = False
        if skill_id in self._custom:
            del self._custom[skill_id]
            removed = True
        try:
            target = _skill_dir(skill_id)
        except ValueError:
            target = None
        if target is not None and target.is_dir():
            shutil.rmtree(target, ignore_errors=True)
            removed = True
        if removed:
            self._overrides.pop(skill_id, None)
            self._save()
            return True
        return False
    def _is_visible(self, name: str, meta: dict | None = None) -> bool:
        """Whether the tool schema is advertised to the model (master on)."""
        if not self._master:
            return False
        if name in ALWAYS_ON_TOOL_NAMES:
            return True
        base_names = {t["function"]["name"] for t in BASE_TOOLS}
        return self._tool_overrides.get(name, name in base_names)

    def _is_directly_executable(self, name: str, meta: dict | None = None) -> bool:
        """Whether the model may invoke the tool itself.

        Agent-builtin tools are always direct (the agent ships them).
        Skill tools must be visible and not pinned to plan-only.
        """
        if name in AGENT_BUILTIN_TOOL_NAMES or name in ALWAYS_ON_TOOL_NAMES:
            return True
        if not self._is_visible(name, meta):
            return False
        meta = meta or TOOL_META.get(name, {})
        danger = meta.get("danger", "safe")
        return self._exec_overrides.get(name, DEFAULT_DIRECT_EXECUTABLE.get(danger, True))

    def all_tool_states(self) -> list[dict]:
        result = []
        base_names = {t["function"]["name"] for t in BASE_TOOLS}
        for t in ALL_TOOLS:
            name = t["function"]["name"]
            meta = TOOL_META.get(name, {})
            always_on = name in ALWAYS_ON_TOOL_NAMES
            enabled = always_on or self._tool_overrides.get(name, name in base_names)
            result.append({
                "name": name,
                "label": meta.get("label", name),
                "description": meta.get("description", ""),
                "danger": meta.get("danger", "safe"),
                "enabled": enabled,
                "always_on": always_on,
                "builtin_base": name in base_names,
                "agent_builtin": name in AGENT_BUILTIN_TOOL_NAMES,
                "direct_executable": self._is_directly_executable(name, meta),
                "parameters": t["function"].get("parameters", {}),
            })
        return result

    def set_tool_enabled(self, tool_name: str, enabled: bool):
        if tool_name not in ALL_TOOL_NAMES:
            raise ValueError(f"未知工具: {tool_name}")
        self._tool_overrides[tool_name] = bool(enabled)
        self._save()

    def set_tool_executable(self, tool_name: str, executable: bool):
        if tool_name not in ALL_TOOL_NAMES:
            raise ValueError(f"未知工具: {tool_name}")
        self._exec_overrides[tool_name] = bool(executable)
        self._save()

    def set_all_tools(self, enabled: bool):
        for name in ALL_TOOL_NAMES:
            self._tool_overrides[name] = enabled
        self._save()

    def tool_is_visible(self, name: str) -> bool:
        return self._is_visible(name)

    def tool_is_directly_executable(self, name: str) -> bool:
        return self._is_directly_executable(name)

    def plan_only_tool_names(self) -> list[str]:
        """Visible skill tools the model may reference but not call directly."""
        out = []
        for t in ALL_TOOLS:
            name = t["function"]["name"]
            if name in AGENT_BUILTIN_TOOL_NAMES:
                continue
            if self._is_visible(name) and not self._is_directly_executable(name):
                out.append(name)
        return out


    def get_active_instructions(self) -> str:
        if not self._master:
            return ""
        parts = []
        for skill in self.all_skills():
            if not skill["enabled"]:
                continue
            if skill.get("source") == "file":
                summary = skill.get("description") or "（无描述）"
                parts.append(
                    f"[{skill['label']}] {summary}\n"
                    f"完整说明和附带资源请按需读取：read_skill(skill_id=\"{skill['id']}\")，"
                    f"查看资源列表用 list_skill_files(skill_id=\"{skill['id']}\")。"
                )
            else:
                parts.append(f"[{skill['label']}]\n{skill['instruction']}")
        if not parts:
            return ""
        return ("可用的技能:\n\n" + "\n\n".join(parts)
                + "\n\n根据用户的请求选择合适的方法，并用工具完成。")

    def get_active_tools(self) -> list[dict]:
        """Master off → no tools. Master on → individually toggled tools."""
        if not self._master:
            return []
        base_names = {t["function"]["name"] for t in BASE_TOOLS}
        result: list[dict] = []
        for t in ALL_TOOLS:
            name = t["function"]["name"]
            enabled = name in ALWAYS_ON_TOOL_NAMES or self._tool_overrides.get(name, name in base_names)
            if enabled:
                result.append(t)
        return result


_registry = SkillRegistry()


def get_skill_registry() -> SkillRegistry:
    return _registry
