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
        # 打印便于调试
        # print(f"=== Model Response ===\n{message}\n=====================")
        content = message.content
        tool_calls = message.tool_calls
        if tool_calls:
            history.append({"role": "assistant", "content": content, "tool_calls": tool_calls})
            for tool_call in tool_calls:
                # 打印便于调试
                # print(f"=== Executing tool ===\n{tool_call} \n=====================")
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
    if len(sys.argv) > 1: 
        print(chat(sys.argv[1]))  # 子代理模式
    else:
        h = []
        while (q := input("\033[36m>> \033[0m")) not in ("q", "exit", ""): 
            print(chat(q, h))  # 交互模式
            # 打印对话历史，便于调试
            # print("=== Conversation History ===")
            # print(h) 

"""
examples:
$ python ./MiniClaudeCode/my_coder.py
>> hi
=== Model Response ===
ChatCompletionMessage(content='Hello! How can I assist you today?', refusal=None, role='assistant', annotations=None, audio=None, function_call=None, tool_calls=None, function_calls=None, reasoning_content='')
=====================
Hello! How can I assist you today?
=== Conversation History ===
[{'role': 'user', 'content': 'hi'}, {'role': 'assistant', 'content': 'Hello! How can I assist you today?'}]
>> show the folder
=== Model Response ===
ChatCompletionMessage(content='', refusal=None, role='assistant', annotations=None, audio=None, function_call=None, tool_calls=[ChatCompletionMessageFunctionToolCall(id='call_24d36b8dfb6240fc948ff8', function=Function(arguments='{"command": "ls -la"}', name='bash'), type='function', index=0)], function_calls=None, reasoning_content='')
=====================
=== Executing tool ===
ChatCompletionMessageFunctionToolCall(id='call_24d36b8dfb6240fc948ff8', function=Function(arguments='{"command": "ls -la"}', name='bash'), type='function', index=0) 
=====================
$ ls -la
总计 36
drwxrwxr-x  4 liyulingyue liyulingyue 4096 12月 30 07:41 .
drwxrwxr-x 10 liyulingyue liyulingyue 4096 12月 30 06:08 ..
-rw-rw-r--  1 liyulingyue liyulingyue  311 12月 30 06:46 .env
-rw-rw-r--  1 liyulingyue liyulingyue  126 12月 30 06:31 .env.example
-rw-rw-r--  1 liyulingyue liyulingyue   10 12月 30 07:00 .gitignore
drwxrwxr-x  2 liyulingyue liyulingyue 4096 12月 30 07:40 MiniClaudeCode
-rw-rw-r--  1 liyulingyue liyulingyue  204 12月 30 06:09 README.md
-rw-rw-r--  1 liyulingyue liyulingyue   20 12月 30 06:35 requirements.txt
drwxrwxr-x  5 liyulingyue liyulingyue 4096 12月 30 06:24 .venv

=== Model Response ===
ChatCompletionMessage(content="Here is the current directory structure:\n\n```\n.\n├── .env                  # Environment variables file (311 bytes)\n├── .env.example          # Example environment file (126 bytes)\n├── .gitignore            # Git ignore rules (10 bytes)\n├── MiniClaudeCode/       # Directory (likely project-related)\n├── README.md             # Project documentation (204 bytes)\n├── requirements.txt      # Python dependencies (20 bytes)\n└── .venv/                # Virtual environment directory\n```\n\nLet me know what you'd like to explore or do next!", refusal=None, role='assistant', annotations=None, audio=None, function_call=None, tool_calls=None, function_calls=None, reasoning_content='')
=====================
Here is the current directory structure:

```
.
├── .env                  # Environment variables file (311 bytes)
├── .env.example          # Example environment file (126 bytes)
├── .gitignore            # Git ignore rules (10 bytes)
├── MiniClaudeCode/       # Directory (likely project-related)
├── README.md             # Project documentation (204 bytes)
├── requirements.txt      # Python dependencies (20 bytes)
└── .venv/                # Virtual environment directory
```

Let me know what you'd like to explore or do next!
=== Conversation History ===
[{'role': 'user', 'content': 'hi'}, {'role': 'assistant', 'content': 'Hello! How can I assist you today?'}, {'role': 'user', 'content': 'show the folder'}, {'role': 'assistant', 'content': '', 'tool_calls': [ChatCompletionMessageFunctionToolCall(id='call_24d36b8dfb6240fc948ff8', function=Function(arguments='{"command": "ls -la"}', name='bash'), type='function', index=0)]}, {'role': 'tool', 'tool_call_id': 'call_24d36b8dfb6240fc948ff8', 'content': '总计 36\ndrwxrwxr-x  4 liyulingyue liyulingyue 4096 12月 30 07:41 .\ndrwxrwxr-x 10 liyulingyue liyulingyue 4096 12月 30 06:08 ..\n-rw-rw-r--  1 liyulingyue liyulingyue  311 12月 30 06:46 .env\n-rw-rw-r--  1 liyulingyue liyulingyue  126 12月 30 06:31 .env.example\n-rw-rw-r--  1 liyulingyue liyulingyue   10 12月 30 07:00 .gitignore\ndrwxrwxr-x  2 liyulingyue liyulingyue 4096 12月 30 07:40 MiniClaudeCode\n-rw-rw-r--  1 liyulingyue liyulingyue  204 12月 30 06:09 README.md\n-rw-rw-r--  1 liyulingyue liyulingyue   20 12月 30 06:35 requirements.txt\ndrwxrwxr-x  5 liyulingyue liyulingyue 4096 12月 30 06:24 .venv\n'}, {'role': 'assistant', 'content': "Here is the current directory structure:\n\n```\n.\n├── .env                  # Environment variables file (311 bytes)\n├── .env.example          # Example environment file (126 bytes)\n├── .gitignore            # Git ignore rules (10 bytes)\n├── MiniClaudeCode/       # Directory (likely project-related)\n├── README.md             # Project documentation (204 bytes)\n├── requirements.txt      # Python dependencies (20 bytes)\n└── .venv/                # Virtual environment directory\n```\n\nLet me know what you'd like to explore or do next!"}]

"""