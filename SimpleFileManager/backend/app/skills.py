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

import json
import time
from pathlib import Path
from typing import Any, Optional

from .deps import state

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

BASE_TOOLS = [TOOL_SEARCH, TOOL_LIST_DIR, TOOL_READ_FILE, TOOL_WRITE_FILE, TOOL_SEMANTIC_SEARCH]
ATOMIC_TOOLS = [TOOL_MOVE_FILE, TOOL_DELETE_FILE, TOOL_CREATE_DIR, TOOL_FILE_INFO]
ALL_TOOLS = BASE_TOOLS + ATOMIC_TOOLS

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
}
ALL_TOOL_NAMES = [t["function"]["name"] for t in ALL_TOOLS]

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
    entries = []
    for child in sorted(target.iterdir()):
        entries.append({
            "name": child.name,
            "type": "directory" if child.is_dir() else "file",
            "path": _rel(child),
        })
    return {"path": _rel(target), "entries": entries}


def _handle_read_file(path: str, limit: int = 50000) -> dict:
    target = _abs_path(path)
    if not target.exists():
        return {"error": f"文件不存在: {path}"}
    try:
        content = target.read_text(encoding="utf-8")[:limit]
        return {"path": _rel(target), "content": content, "truncated": len(content) >= limit}
    except Exception as e:
        return {"error": str(e)}


def _handle_write_file(path: str, content: str) -> dict:
    target = _abs_path(path)
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
    dst.parent.mkdir(parents=True, exist_ok=True)
    src.rename(dst)
    return {"source": _rel(src), "destination": _rel(dst), "moved": True}


def _handle_delete_file(path: str) -> dict:
    target = _abs_path(path)
    if not target.exists():
        return {"error": f"路径不存在: {path}"}
    trash_root = Path(state.get_settings().storage_path) / ".trash"
    ts = time.strftime("%Y%m%d-%H%M%S")
    rel = _rel(target)
    dest = trash_root / ts / rel
    dest.parent.mkdir(parents=True, exist_ok=True)
    target.rename(dest)
    return {"path": rel, "trashed_to": str(dest.relative_to(Path(state.get_settings().storage_path)))}


def _handle_create_directory(path: str) -> dict:
    target = _abs_path(path)
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


def execute_tool(name: str, arguments: dict) -> dict:
    handler = TOOL_HANDLERS.get(name)
    if handler is None:
        return {"error": f"未知工具: {name}"}
    try:
        return handler(**arguments)
    except TypeError as e:
        return {"error": f"参数错误: {e}"}
    except Exception as e:
        return {"error": str(e)}

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
            except Exception:
                pass

    def _save(self):
        self._path().write_text(
            json.dumps(
                {"custom": self._custom, "master": self._master,
                 "enabled": self._overrides, "tools": self._tool_overrides},
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
        for sid, s in BUILTIN_SKILLS.items():
            result.append({
                "id": sid,
                "label": s["label"],
                "description": s["description"],
                "enabled": self._overrides.get(sid, s.get("enabled", False)),
                "builtin": True,
                "instruction": s["instruction"],
            })
        for sid, s in self._custom.items():
            result.append({
                "id": sid,
                "label": s["label"],
                "description": s.get("description", ""),
                "enabled": self._overrides.get(sid, s.get("enabled", False)),
                "builtin": False,
                "instruction": s["instruction"],
            })
        return result

    def set_enabled(self, skill_id: str, enabled: bool):
        self._overrides[skill_id] = bool(enabled)
        self._save()

    def add_custom(self, skill_id: str, label: str, instruction: str, description: str = "") -> dict:
        if skill_id in BUILTIN_SKILLS:
            raise ValueError(f"技能 ID '{skill_id}' 与内置技能冲突")
        if not skill_id or not label or not instruction:
            raise ValueError("技能 ID、名称和指令不能为空")
        self._custom[skill_id] = {"label": label, "instruction": instruction, "description": description}
        self._overrides[skill_id] = False
        self._save()
        return {"id": skill_id, "label": label, "description": description,
                "instruction": instruction, "enabled": False, "builtin": False}

    def remove_custom(self, skill_id: str) -> bool:
        if skill_id in self._custom:
            del self._custom[skill_id]
            self._overrides.pop(skill_id, None)
            self._save()
            return True
        return False
    def all_tool_states(self) -> list[dict]:
        result = []
        base_names = {t["function"]["name"] for t in BASE_TOOLS}
        for t in ALL_TOOLS:
            name = t["function"]["name"]
            meta = TOOL_META.get(name, {})
            enabled = self._tool_overrides.get(name, name in base_names)
            result.append({
                "name": name,
                "label": meta.get("label", name),
                "description": meta.get("description", ""),
                "danger": meta.get("danger", "safe"),
                "enabled": enabled,
                "builtin_base": name in base_names,
            })
        return result

    def set_tool_enabled(self, tool_name: str, enabled: bool):
        if tool_name not in ALL_TOOL_NAMES:
            raise ValueError(f"未知工具: {tool_name}")
        self._tool_overrides[tool_name] = bool(enabled)
        self._save()

    def set_all_tools(self, enabled: bool):
        for name in ALL_TOOL_NAMES:
            self._tool_overrides[name] = enabled
        self._save()


    def get_active_instructions(self) -> str:
        if not self._master:
            return ""
        parts = []
        for s in self.all_skills():
            if s["enabled"]:
                parts.append(f"[{s['label']}]\n{s['instruction']}")
        if not parts:
            return ""
        return "可用的技能:\n\n" + "\n\n".join(parts) + "\n\n根据用户的请求选择合适的方法，并用工具完成。"

    def get_active_tools(self) -> list[dict]:
        """Master off → no tools. Master on → individually toggled tools."""
        if not self._master:
            return []
        base_names = {t["function"]["name"] for t in BASE_TOOLS}
        result: list[dict] = []
        for t in ALL_TOOLS:
            name = t["function"]["name"]
            enabled = self._tool_overrides.get(name, name in base_names)
            if enabled:
                result.append(t)
        return result


_registry = SkillRegistry()


def get_skill_registry() -> SkillRegistry:
    return _registry
