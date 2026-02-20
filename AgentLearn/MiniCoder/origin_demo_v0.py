#!/usr/bin/env python
"""v0_bash_agent.py - 极简 Claude Code (20行核心) | Bash is All You Need"""
from anthropic import Anthropic
import subprocess, sys, os

client = Anthropic(api_key="your-api-key", base_url="https://api.moonshot.cn/anthropic")
TOOL = [{
    "name": "bash", 
    "description": """Execute shell command. Common patterns:
        - Read: cat/head/tail, grep/find/rg/ls, wc -l
        - Write: echo 'content' > file, sed -i 's/old/new/g' file
        - Subagent: python v0_bash_agent.py 'task description' (spawns isolated agent, returns summary)""",
    "input_schema": {
        "type": "object", 
        "properties": {
            "command": {"type": "string"}
        }, 
        "required": ["command"]
    }
}]
SYSTEM = f"""You are a CLI agent at {os.getcwd()}. Solve problems using bash commands.

Rules:
- Prefer tools over prose. Act first, explain briefly after.
- Read files: cat, grep, find, rg, ls, head, tail
- Write files: echo '...' > file, sed -i, or cat << 'EOF' > file
- Subagent: For complex subtasks, spawn a subagent to keep context clean:
  python v0_bash_agent.py "explore src/ and summarize the architecture"

When to use subagent:
- Task requires reading many files (isolate the exploration)
- Task is independent and self-contained
- You want to avoid polluting current conversation with intermediate details

The subagent runs in isolation and returns only its final summary."""

def chat(prompt, history=[]):
    history.append({"role": "user", "content": prompt})
    while True:
        r = client.messages.create(model="kimi-k2-turbo-preview", system=SYSTEM, messages=history, tools=TOOL, max_tokens=8000)
        content = [{"type": b.type, **({"text": b.text} if hasattr(b, "text") else {"id": b.id, "name": b.name, "input": b.input})} for b in r.content]
        history.append({"role": "assistant", "content": content})
        if r.stop_reason != "tool_use":
            return "".join(b.text for b in r.content if hasattr(b, "text"))
        results = []
        for b in [x for x in r.content if x.type == "tool_use"]:
            print(f"\033[33m$ {b.input['command']}\033[0m")
            try: out = subprocess.run(b.input["command"], shell=True, capture_output=True, text=True, timeout=300, cwd=os.getcwd())
            except subprocess.TimeoutExpired: out = type('', (), {'stdout': '', 'stderr': '(timeout)'})()
            print(out.stdout + out.stderr or "(empty)")
            results.append({"type": "tool_result", "tool_use_id": b.id, "content": (out.stdout + out.stderr)[:50000]})
        history.append({"role": "user", "content": results})

if __name__ == "__main__":
    if len(sys.argv) > 1: 
        print(chat(sys.argv[1]))  # 子代理模式
    else:
        h = []
        while (q := input("\033[36m>> \033[0m")) not in ("q", "exit", ""): 
            print(chat(q, h))  # 交互模式