#!/usr/bin/env python
"""tools.py - MiniCoder 工具函数"""
import os
import json
import subprocess
from pathlib import Path
from typing import List, Dict, Optional

class CodeTools:
    """代码相关工具类"""
    
    @staticmethod
    def execute_bash(command: str) -> str:
        """执行 Bash 命令"""
        try:
            result = subprocess.run(
                command, 
                shell=True, 
                capture_output=True, 
                text=True, 
                timeout=30,
                cwd=os.getcwd()
            )
            output = result.stdout + result.stderr
            return output if output else "(empty output)"
        except subprocess.TimeoutExpired:
            return "Error: Command timed out after 30 seconds."
        except Exception as e:
            return f"Error executing command: {str(e)}"

    @staticmethod
    def read_file(path: str) -> str:
        """读取文件内容"""
        try:
            p = Path(path)
            if not p.exists():
                return f"Error: File {path} does not exist."
            return p.read_text(encoding='utf-8')
        except Exception as e:
            return f"Error reading file: {str(e)}"

    @staticmethod
    def write_file(path: str, content: str) -> str:
        """写入文件内容"""
        try:
            p = Path(path)
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(content, encoding='utf-8')
            return f"Successfully wrote to {path}"
        except Exception as e:
            return f"Error writing file: {str(e)}"

    @staticmethod
    def list_files(path: str = ".") -> str:
        """列出目录下的文件和文件夹 (带类型标记)"""
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
            return f"Error listing files: {str(e)}"

    @staticmethod
    def search_files(query: str, path: str = ".") -> str:
        """在路径下搜索包含特定文本的文件 (grep)"""
        try:
            # 使用 grep 递归搜索
            result = subprocess.run(
                f"grep -ril \"{query}\" {path}",
                shell=True,
                capture_output=True,
                text=True,
                timeout=30
            )
            output = result.stdout.strip()
            return output if output else "No matches found."
        except Exception as e:
            return f"Error searching files: {str(e)}"

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
