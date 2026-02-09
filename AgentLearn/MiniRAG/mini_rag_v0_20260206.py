#!/usr/bin/env python
"""my_rag.py - 极简 RAG Agent | Retrieval-Augmented Generation with file search"""
from openai import OpenAI
from dotenv import load_dotenv
import os, json, sys

from pathlib import Path
from search_utils import search_files, search_filenames, semantic_search
from tools import TOOLS

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
SYSTEM = f"""You are an intelligent RAG agent at {os.getcwd()}. Your role is to answer questions using retrieval-augmented generation.

## 核心检索策略 (Core Retrieval Strategy):
1. **双轨并行**: 
   - **向量化检索 (semantic_search)**: 擅长捕捉“含义”，跨越词义障碍。用于快速定位相关概念和原理。
   - **关键字检索 (search_files)**: 擅长“精准打击”和“完整获取”。用于查找特定代码、私有名词、报错信息或需要获取包含某词的所有精确上下文。
2. **互补与交叉验证**: 
   - 同一个事情可能有多种描述方式，如果一种搜索方式结果不佳，**必须**切换另一种或尝试同义词。
   - 对于复杂问题，应同时结合两种方式：用向量搜索确定“在哪聊过”，用关键字搜索确保“没漏掉细节”。

## 思考逻辑 (Thinking Process):
1. **意图拆解**: 将问题拆解为“语义概念”和“特征关键词”。
2. **灵活检索路径**:
   - **路径 A (扫描结构)**: 如果文件名可能包含关键词，先用 `search_filenames` 定位相关文件。
   - **路径 B (内容溯源/反推)**: 如果文件名不直观或搜索无果，直接进行全局 `semantic_search` 或 `search_files`。**从结果中观察 `file_path` 字段**，通过内容命中反推并锁定目标文件，后续再利用路径参数进行子区域搜索。
3. **多阶段探索**:
   - **第一阶段 (广度探索)**: 全局搜索锁定相关文件路径。
   - **第二阶段 (针对性深入)**: 使用锁定文件的 `file_path` 参数限制范围，进行高频次的关键字/向量搜索以获取完整、准确信息。
4. **自愈与重试**: 如果结果为 "(no matches found)"，自动尝试同义词、扩大搜索范围或暂时放弃路径限制。

## 工具使用规范:
- **search_filenames**: 了解目录、定位文件。如果名称无法判断，请果断通过内容搜索来“反推”文件位置。
- **semantic_search**: 搜原理、搜场景、搜模糊描述。支持 `file_path` 缩小范围。
- **search_files**: 搜代码、搜配置、搜术语。支持 `file_path` 缩小范围。
- **注意**: 搜索结果中的 `file_path`（如 `knowledge/coffee.txt`）是极佳的定位线索，请学会根据内容反推出文件名。

## 规则:
- 在执行搜索前，简要说明你的思考过程和搜索策略。
- 禁止在没有尝试搜索的情况下直接回答“不知道”。
- 每次任务最多尝试 3 次不同的搜索组合。
"""

def chat(prompt, history=[]):
    history.append({"role": "user", "content": prompt})
    while True:
        messages = [{"role": "system", "content": SYSTEM}] + history
        r = client.chat.completions.create(model=model_name, messages=messages, tools=TOOLS, max_tokens=8000)
        message = r.choices[0].message
        content = message.content
        tool_calls = message.tool_calls
        
        if content:
            print(f"\033[32mAgent: {content}\033[0m")

        if tool_calls:
            history.append({"role": "assistant", "content": content, "tool_calls": tool_calls})
            for tool_call in tool_calls:
                fn_name = tool_call.function.name
                args = json.loads(tool_call.function.arguments)
                
                print(f"\033[33mCalling tool: {fn_name}({args})\033[0m")
                
                if fn_name == "search_files":
                    output = search_files(args.get('keyword'), args.get('file_path'))
                elif fn_name == "search_filenames":
                    output = search_filenames(args.get('keyword'))
                elif fn_name == "semantic_search":
                    output = semantic_search(args.get('query'), file_path=args.get('file_path'))
                else:
                    output = f"Error: Unknown tool {fn_name}"
                
                print(f"\033[34mResult: {output[:500]}...\033[0m")
                history.append({"role": "tool", "tool_call_id": tool_call.id, "content": output[:50000]})
        else:
            history.append({"role": "assistant", "content": content})
            return content

if __name__ == "__main__":
    if len(sys.argv) > 1: 
        print(chat(sys.argv[1]))  # 子代理模式
    else:
        h = []
        while (q := input("\033[36m>> \033[0m")) not in ("q", "exit", "bye", ""): 
            print(chat(q, h))  # 交互模式