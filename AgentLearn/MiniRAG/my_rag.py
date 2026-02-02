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
SYSTEM = f"""You are an intelligent RAG agent at {os.getcwd()}. Your role is to answer questions using retrieval-augmented generation from the knowledge base.

## Core Strategy:
1. **Assess Information Need**: First determine if the question requires external knowledge from the knowledge base, or if you can answer from general knowledge.

2. **Smart Retrieval**:
   - Use semantic_search for conceptual/meaning-based queries (e.g., "how to train a model", "explain algorithms")
   - Use search_files for specific technical terms or code-related queries
   - Use search_filenames when looking for specific documents or topics by name

3. **Iterative Search Strategy**:
   - If first search returns no results, IMMEDIATELY try different keywords or synonyms
   - Maximum 3 search attempts per question - DO NOT GIVE UP AFTER ONE TRY
   - Only after 3 failed attempts, conclude the knowledge base doesn't contain relevant information
   - When retrying, explain your new search strategy

4. **Search Keyword Selection**:
   - Break down complex queries into key technical terms
   - Try both specific and general terms
   - Consider synonyms and related concepts
   - If searching for "origin of coffee", try: "coffee history", "coffee origins", "where coffee comes from"

## Rules:
- ALWAYS use tools for questions that might have information in the knowledge base
- NEVER give up after one failed search - try at least 2-3 different approaches
- If you get "(no matches found)", immediately try a different search term
- Explain your search strategy briefly before each search attempt
- Only use general knowledge as fallback AFTER exhausting search attempts
- For coding questions, prioritize search_files and semantic_search
- Subagent: For complex multi-step tasks, spawn subagents to keep context clean.

## Example Search Patterns:
- Question: "How does gradient descent work?"
  → semantic_search("gradient descent algorithm")
  → If no results: search_files("gradient descent")
- Question: "Find the API documentation"
  → search_filenames("api") + search_files("documentation")
- Question: "What are the hyperparameters for model X?"
  → search_files("hyperparameters") + semantic_search("model configuration")

## Example Search Patterns:
- Question: "How does gradient descent work?"
  → semantic_search("gradient descent algorithm")
- Question: "Find the API documentation"
  → search_filenames("api") + search_files("documentation")
- Question: "What are the hyperparameters for model X?"
  → search_files("hyperparameters") + semantic_search("model configuration")"""

def chat(prompt, history=[]):
    history.append({"role": "user", "content": prompt})
    while True:
        messages = [{"role": "system", "content": SYSTEM}] + history
        r = client.chat.completions.create(model=model_name, messages=messages, tools=TOOLS, max_tokens=8000)
        message = r.choices[0].message
        content = message.content
        tool_calls = message.tool_calls
        if tool_calls:
            history.append({"role": "assistant", "content": content, "tool_calls": tool_calls})
            for tool_call in tool_calls:
                args = json.loads(tool_call.function.arguments)
                if tool_call.function.name == "search_files":
                    keyword = args['keyword']
                    print(f"\033[33mSearching file contents for '{keyword}'\033[0m")
                    output = search_files(keyword)
                    print(output[:2000])  # Print truncated
                    history.append({"role": "tool", "tool_call_id": tool_call.id, "content": output[:50000]})
                elif tool_call.function.name == "search_filenames":
                    keyword = args['keyword']
                    print(f"\033[33mSearching filenames for '{keyword}'\033[0m")
                    output = search_filenames(keyword)
                    print(output[:2000])  # Print truncated
                    history.append({"role": "tool", "tool_call_id": tool_call.id, "content": output[:50000]})
                elif tool_call.function.name == "semantic_search":
                    query = args['query']
                    print(f"\033[33mSemantic searching for '{query}'\033[0m")
                    output = semantic_search(query)
                    print(output[:2000])  # Print truncated
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