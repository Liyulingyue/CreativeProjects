#!/usr/bin/env python
"""v0_bash_agent.py - 极简 Claude Code (20行核心) | Bash is All You Need"""
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
base_url = os.getenv("MODEL_URL")
model_name = os.getenv("MODEL_NAME", "gpt-4")

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

def chat(prompt, history=[]):
    history.append({"role": "user", "content": prompt})
    while True:
        messages = [{"role": "system", "content": SYSTEM}] + history
        r = client.chat.completions.create(model=model_name, messages=messages, tools=TOOL, max_tokens=8000)
        message = r.choices[0].message
        content = message.content
        tool_calls = message.tool_calls
        if tool_calls:
            history.append({"role": "assistant", "content": content, "tool_calls": tool_calls})
            for tool_call in tool_calls:
                # print(f"=== Executing tool: {tool_call} ===")
                args = json.loads(tool_call.function.arguments)
                command = args['command']
                print(f"\033[33m$ {command}\033[0m")
                try: 
                    out = subprocess.run(command, shell=True, capture_output=True, text=True, timeout=300, cwd=os.getcwd())
                except subprocess.TimeoutExpired: 
                    out = type('', (), {'stdout': '', 'stderr': '(timeout)'})()
                output = out.stdout + out.stderr or "(empty)"
                print(output)
                history.append({"role": "tool", "tool_call_id": tool_call.id, "content": output[:50000]})
        else:
            history.append({"role": "assistant", "content": content})
            return content

if __name__ == "__main__":
    if len(sys.argv) > 1: print(chat(sys.argv[1]))  # 子代理模式
    else:
        h = []
        while (q := input("\033[36m>> \033[0m")) not in ("q", "exit", ""): 
            print(chat(q, h))  # 交互模式
            # 打印对话历史，便于调试
            # print("=== Conversation History ===")
            # print(h) 