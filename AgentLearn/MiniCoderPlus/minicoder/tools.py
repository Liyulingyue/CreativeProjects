#!/usr/bin/env python
"""tools.py - MiniCoder 工具函数"""
import os
import json
import subprocess
import asyncio
import time
from pathlib import Path
from typing import List, Dict, Optional
from contextvars import ContextVar
from contextlib import contextmanager
from .core.settings import settings

# Concurrency-safe context variables
_active_session_ctx: ContextVar[Optional[object]] = ContextVar("active_session_ctx", default=None)
_workspace_override_ctx: ContextVar[Optional[Path]] = ContextVar("workspace_override_ctx", default=None)

@contextmanager
def set_current_workspace(workspace_path: Optional[Path]):
    """Context manager to temp set the workspace for current task thread."""
    token = _workspace_override_ctx.set(workspace_path)
    try:
        yield
    finally:
        _workspace_override_ctx.reset(token)

class CodeTools:
    """代码相关工具类"""
    
    @classmethod
    def set_active_session(cls, session):
        """Set the active terminal session in the current context."""
        _active_session_ctx.set(session)

    @classmethod
    def get_active_session(cls):
        """Get the active terminal session from current context."""
        return _active_session_ctx.get()

    @classmethod
    def get_current_workspace(cls) -> Path:
        """Determines the current active workspace directory."""
        # 1. Use override from context if present
        ctx_path = _workspace_override_ctx.get()
        if ctx_path:
            return ctx_path
            
        # 2. Use session's working dir if present
        session = cls.get_active_session()
        if session and hasattr(session, 'working_dir') and session.working_dir:
            return session.working_dir
            
        return settings.WORKSPACE_DIR

    @staticmethod
    def _resolve_path(path: str) -> Path:
        """解析并确保路径在当前工作区内"""
        root = CodeTools.get_current_workspace()
        p = Path(path)
        if p.is_absolute():
            # 如果是绝对路径，尝试检查它是否在根目录内
            try:
                p.relative_to(root)
                return p
            except ValueError:
                # 如果不在，则将其视为相对路径并拼接到根目录
                return root / p.name
        return root / p

    @staticmethod
    def execute_bash(command: str) -> str:
        """执行 Bash 命令 (在当前工作区目录下)"""
        # If we have an active interactive session, use it
        session = CodeTools.get_active_session()
        if session:
            return CodeTools._execute_interactive(command)
        
        root = CodeTools.get_current_workspace()
        try:
            result = subprocess.run(
                command, 
                shell=True, 
                capture_output=True, 
                text=True, 
                timeout=30,
                cwd=str(root)
            )
            output = result.stdout + result.stderr
            return output if output else "(empty output)"
        except subprocess.TimeoutExpired:
            return "Error: Command timed out after 30 seconds."
        except Exception as e:
            return f"Error executing command: {str(e)}"

    @staticmethod
    def _execute_interactive(command: str) -> str:
        """Execute command in the persistent PTY session."""
        session = CodeTools.get_active_session()
        if not session:
            return "Error: No active interactive session found."
        
        try:
            # We use the session's loop to create and manage the queue
            loop = session._loop
            output_queue = session.get_output_queue()

            async def _run():
                try:
                    # Clear any existing backlog in OUR queue
                    while not output_queue.empty():
                        output_queue.get_nowait()
                    
                    # Send command
                    await session.write(command + "\r\n")
                    
                    output = ""
                    idle_count = 0
                    max_wait = 100 
                    
                    while idle_count < 2 and max_wait > 0:
                        try:
                            # Wait for output
                            chunk = await asyncio.wait_for(output_queue.get(), timeout=0.1)
                            output += chunk
                            idle_count = 0 
                        except asyncio.TimeoutError:
                            if output:
                                idle_count += 1
                        max_wait -= 1
                    
                    return output if output else "(executed in PTY)"
                finally:
                    session.remove_queue(output_queue)

            # This code is running in a worker thread (from agent.py's to_thread)
            # We MUST run the coroutine in the session's home loop
            future = asyncio.run_coroutine_threadsafe(_run(), loop)
            return future.result(timeout=15) # Wait for result safely

        except Exception as e:
            return f"Error in interactive PTY: {str(e)}"

    @staticmethod
    def read_file(path: str) -> str:
        """读取文件内容 (从 WorkSpace 内)"""
        try:
            p = CodeTools._resolve_path(path)
            if not p.exists():
                return f"Error: File {path} does not exist in WorkSpace."
            return p.read_text(encoding='utf-8')
        except Exception as e:
            return f"Error reading file: {str(e)}"

    @staticmethod
    def write_file(path: str, content: str) -> str:
        """写入文件内容 (在 WorkSpace 内)"""
        try:
            p = CodeTools._resolve_path(path)
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(content, encoding='utf-8')
            return f"Successfully wrote to {path} (in WorkSpace)"
        except Exception as e:
            return f"Error writing file: {str(e)}"

    @staticmethod
    def list_files(path: str = ".") -> str:
        """列出 WorkSpace 目录下的文件和文件夹"""
        try:
            p = CodeTools._resolve_path(path)
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
            return f"Error listing files: {str(e)}"

    @staticmethod
    def search_files(query: str, path: str = ".") -> str:
        """在 WorkSpace 下搜索包含特定文本的文件 (grep)"""
        try:
            p = CodeTools._resolve_path(path)
            # 使用 grep 递归搜索
            result = subprocess.run(
                f"grep -ril \"{query}\" .", # 始终在当前 resolved 路径搜索
                shell=True,
                capture_output=True,
                text=True,
                timeout=30,
                cwd=str(p)
            )
            output = result.stdout.strip()
            return output if output else "No matches found."
        except Exception as e:
            return f"Error searching files: {str(e)}"

class EmbeddedSSHTools:
    """Special tools for constrained SSH/Buildroot environments."""
    
    @staticmethod
    def pipe_command(command: str) -> str:
        """Execute a command through a base64-encoded pipe to avoid character escaping issues."""
        import base64
        # This is a conceptual tool for use in the PROPOSAL_EMBEDDED_SSH.md
        encoded_cmd = base64.b64encode(command.encode()).decode()
        # In a real scenario, we'd send: echo "encoded_cmd" | base64 -d | sh
        return f"Executed pipelined: {command} (Simulated)"

# 工具定义（OpenAI 格式）
TOOL_SCHEMAS = [
    {
        "type": "function",
        "function": {
            "name": "execute_bash",
            "description": "Execute a bash command in the local environment. Use this for running tests, checking status, or searching.",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "The command to run."}
                },
                "required": ["command"]
            }
        }
    },
    {
        "type": "function",
        "function": {
            "name": "read_file",
            "description": "Read the content of a file from the disk.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the file."}
                },
                "required": ["path"]
            }
        }
    },
    {
        "type": "function",
        "function": {
            "name": "write_file",
            "description": "Write or overwrite a file with the provided content.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the file."},
                    "content": {"type": "string", "description": "The content to write."}
                },
                "required": ["path", "content"]
            }
        }
    },
    {
        "type": "function",
        "function": {
            "name": "list_files",
            "description": "List files and directories in a given path.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "The directory path (defaults to current)."}
                }
            }
        }
    },
    {
        "type": "function",
        "function": {
            "name": "search_files",
            "description": "Search for a specific string in files within a directory.",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "The text to search for."},
                    "path": {"type": "string", "description": "The directory path to search in (defaults to current)."}
                },
                "required": ["query"]
            }
        }
    }
]

def handle_tool_call(tool_name: str, args: dict) -> str:
    """根据函数名称路由调用"""
    if tool_name == "execute_bash":
        return CodeTools.execute_bash(args.get("command", ""))
    elif tool_name == "read_file":
        return CodeTools.read_file(args.get("path", ""))
    elif tool_name == "write_file":
        return CodeTools.write_file(args.get("path", ""), args.get("content", ""))
    elif tool_name == "list_files":
        return CodeTools.list_files(args.get("path", "."))
    elif tool_name == "search_files":
        return CodeTools.search_files(args.get("query", ""), args.get("path", "."))
    return f"Error: Tool {tool_name} not found."

# 便捷函数
def print_success(message: str):
    """打印成功消息"""
    print(f"✅ {message}")

def print_error(message: str):
    """打印错误消息"""
    print(f"❌ {message}")

def print_info(message: str):
    """打印信息消息"""
    print(f"!{message}")

if __name__ == "__main__":
    print("MiniCoder tools loaded.")
