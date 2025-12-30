#!/usr/bin/env python
"""v0_bash_agent.py - 极简 Claude Code (20行核心) | Bash is All You Need"""
# based on https://mp.weixin.qq.com/s/WPkCONFnBc84Q3V5Qjynrg
from openai import OpenAI
from dotenv import load_dotenv
import subprocess, sys, os, json

from pathlib import Path

# 尝试加载当前目录的.env
load_dotenv()
# 如果没有MODEL_KEY，尝试上一级目录的.env
if not os.getenv("MODEL_KEY"):
    parent_env = Path("..") / ".env"
    if parent_env.exists():
        load_dotenv(dotenv_path=parent_env)

api_key = os.getenv("MODEL_KEY")
# base_url = os.getenv("MODEL_URL")
base_url = "http://localhost:9000/v1"
model_name = os.getenv("MODEL_NAME", "user_model")

client = OpenAI(api_key=api_key, base_url=base_url)
TOOL = [{
    "type": "function",
    "function": {
        "name": "bash", 
        "description": """Execute shell command. Common patterns:
        - Read: cat/head/tail, grep/find/rg/ls, wc -l
        - Write: echo 'content' > file, sed -i 's/old/new/g' file
        - Subagent: python v0_bash_agent.py 'task description' (spawns isolated agent, returns summary)""",
        "parameters": {
            "type": "object", 
            "properties": {
                "command": {"type": "string"}
            }, 
            "required": ["command"]
        }
    }
}]
SYSTEM = f"""You are a CLI agent at {os.getcwd()}. Solve problems using bash commands.

Rules:
- Prefer tools over prose. Act first, explain briefly after.
- Read files: cat, grep, find, rg, ls, head, tail
- Write files: echo '...' > file, sed -i, or cat << 'EOF' > file
- Subagent: For complex subtasks, spawn a subagent to keep context clean:
  python my_coder.py "explore src/ and summarize the architecture"

When to use subagent:
- Task requires reading many files (isolate the exploration)
- Task is independent and self-contained
- You want to avoid polluting current conversation with intermediate details

The subagent runs in isolation and returns only its final summary."""

history = []
prompt = "hi"
history.append({"role": "user", "content": prompt})
r = client.chat.completions.create(model=model_name, messages=history, tools=TOOL, max_tokens=8000)
message = r.choices[0].message
print(f"=== Model Response ===\n{message}\n=====================")

