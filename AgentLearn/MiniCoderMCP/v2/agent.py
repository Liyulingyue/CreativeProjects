#!/usr/bin/env python
"""agent.py — MiniCoder Agent with Tool Use loop."""
import json
import sys
import os
import asyncio
import threading
import logging
import os
from typing import List, Dict, Optional, Any

from mcp import ClientSession
from mcp.client.sse import sse_client

# --- LLM Client Integration ---

MODEL_KEY = os.getenv("MODEL_KEY", "")
BASE_URL = os.getenv("MODEL_URL", "https://api.openai.com/v1")
MODEL_NAME = os.getenv("MODEL_NAME", "gpt-4")

class LLMClient:
    """Thin wrapper around the LLM API."""
    def __init__(self, api_key: str = None, base_url: str = None, model_name: str = None):
        self.api_key = api_key or MODEL_KEY
        self.base_url = base_url or BASE_URL
        self.model_name = model_name or MODEL_NAME

    def chat(self, messages: List[Dict], tools: List[Dict] = None, temperature: float = 0.7):
        if not self.api_key:
            return "⚠️  请先设置 MODEL_KEY 环境变量"

        try:
            from openai import OpenAI
            client = OpenAI(api_key=self.api_key, base_url=self.base_url)
            
            params = {
                "model": self.model_name,
                "messages": messages,
                "temperature": temperature,
            }
            if tools:
                params["tools"] = tools

            response = client.chat.completions.create(**params)
            return response.choices[0].message
        except Exception as e:
            return f"⚠️  API调用失败: {str(e)}"

# --- MCP HUB abstraction ---
class MCPHub:
    """Manages multiple MCP connections and aggregates their tools."""
    def __init__(self, urls: List[str]):
        self.urls = urls
        self._sessions: Dict[str, ClientSession] = {}
        self._exit_stacks: List[asyncio.ExitStack] = []
        self._tool_to_session: Dict[str, str] = {} # tool_name -> server_url
        self._log = logging.getLogger("MCPHub")

    async def __aenter__(self):
        for url in self.urls:
            try:
                stack = asyncio.ExitStack()
                self._exit_stacks.append(stack)
                # 使用高层封装 sse_client
                read, write = await stack.enter_async_context(sse_client(url))
                session = await stack.enter_async_context(ClientSession(read, write))
                await session.initialize()
                self._sessions[url] = session
                self._log.info(f"Connected to MCP Server: {url}")
            except Exception as e:
                self._log.error(f"Failed to connect to {url}: {e}")
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        for stack in self._exit_stacks:
            await stack.aclose()

    async def list_all_tools(self) -> List[Any]:
        all_tools = []
        for url, session in self._sessions.items():
            result = await session.list_tools()
            for tool in result.tools:
                # 记录工具属于哪个 server，以便后续路由
                self._tool_to_session[tool.name] = url
                all_tools.append(tool)
        return all_tools

    async def call_tool(self, name: str, arguments: dict) -> Any:
        url = self._tool_to_session.get(name)
        if not url:
            raise ValueError(f"Unknown tool: {name}")
        session = self._sessions[url]
        return await session.call_tool(name, arguments)

__all__ = ["MCPHub"]


class MiniCoderAgent:
    """An agent that can use tools from multiple MCP servers to solve tasks."""

    def __init__(self, 
                 llm_client: Optional[LLMClient] = None,
                 mcp_urls: Optional[List[str]] = None):
        self.llm = llm_client or LLMClient()
        # 默认地址列表
        self.mcp_urls = mcp_urls or [os.environ.get("MINICODER_MCP_URL", "http://127.0.0.1:8000/sse")]
        self.system_prompt = (
            "You are MiniCoder, a world-class autonomous coding agent.\n"
            "Your goal is to solve the user's request by taking action in the local or remote environment.\n\n"
            "RULES:\n"
            "1. ALWAYS use tools to accomplish tasks.\n"
            "2. You have access to multiple toolsets via MCP; use them as needed.\n"
            "3. After taking action, VERIFY the results.\n"
            f"Current working directory: {os.getcwd()}"
        )

        self.hub: Optional[MCPHub] = None
        self.tools: Optional[List[Dict]] = None
        self._log = logging.getLogger("MiniCoderAgent")

        self._loop = asyncio.new_event_loop()
        self._loop_thread = threading.Thread(target=self._start_loop, daemon=True)
        self._loop_thread.start()

    async def _ensure_mcp(self):
        if self.hub is None:
            self.hub = MCPHub(self.mcp_urls)
            await self.hub.__aenter__()
            
        if self.tools is None:
            mcp_tools = await self.hub.list_all_tools()
            normalized: List[Dict] = []

            for t in mcp_tools:
                normalized.append({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.inputSchema,
                    },
                })
            self.tools = normalized

    async def run_async(self, prompt: str, history: List[Dict] = None) -> str:
        """Async version of the main loop.  This is wrapped by :meth:`run`."""
        if history is None:
            history = []
        await self._ensure_mcp()

        history.append({"role": "user", "content": prompt})

        while True:
            messages = [{"role": "system", "content": self.system_prompt}] + history
            response = self.llm.chat(messages, tools=self.tools)

            if isinstance(response, str):
                return response

            content = response.content
            tool_calls = response.tool_calls

            if tool_calls:
                if content:
                    print(f"\033[34m[Thought] {content}\033[0m")

                history.append({
                    "role": "assistant",
                    "content": content,
                    "tool_calls": [
                        {
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.function.name,
                                "arguments": tc.function.arguments,
                            },
                        }
                        for tc in tool_calls
                    ],
                })

                for tool_call in tool_calls:
                    name = tool_call.function.name
                    args = json.loads(tool_call.function.arguments)
                    print(f"\033[32m[Tool Call] {name}({args})\033[0m")

                    result_obj = await self.hub.call_tool(name, args)
                    result = str(result_obj.content)

                    history.append({
                        "role": "tool",
                        "tool_call_id": tool_call.id,
                        "name": name,
                        "content": result,
                    })
            else:
                history.append({"role": "assistant", "content": content})
                return content or "(No response from agent)"

    def run(self, prompt: str, history: List[Dict] = None) -> str:
        """Synchronous wrapper; users call this from normal code.

        We submit the coroutine to the background event loop and wait for
        the result using `asyncio.run_coroutine_threadsafe`, ensuring the
        MCP subprocess stays attached to the persistent loop.
        """
        if history is None:
            history = []
        future = asyncio.run_coroutine_threadsafe(self.run_async(prompt, history), self._loop)
        return future.result()

    def _start_loop(self):
        asyncio.set_event_loop(self._loop)
        self._loop.run_forever()


__all__ = ["MiniCoderAgent"]
