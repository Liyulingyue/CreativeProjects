#!/usr/bin/env python
"""agent.py — MiniCoder Agent with Tool Use loop."""
import json
import sys
import os
import asyncio
import threading
import logging
import traceback
from typing import List, Dict, Optional, Any

from llm_client import LLMClient

# MCP client abstraction. Focus on a stable, dependency-free implementation
# that uses JSON-over-stdio to communicate with `mcp_server.py`.
class MCPClient:
    def __init__(self, proc: asyncio.subprocess.Process):
        self.proc = proc
        self._lock = asyncio.Lock()

        # small helper logger for debugging MCP interactions
        self._log = logging.getLogger("MCPClient")
        # allow runtime control of logging verbosity via env var
        level_name = os.environ.get("MINICODER_LOG_LEVEL", "INFO").upper()
        level = getattr(logging, level_name, logging.INFO)
        if not logging.getLogger().handlers:
            logging.basicConfig(level=level)

    @classmethod
    async def connect(cls, cmd: Optional[List[str]] = None) -> "MCPClient":
        """Start the server subprocess and return a client wrapper."""
        if cmd is None:
            # assume server script is next to this file
            path = os.path.join(os.path.dirname(__file__), "mcp_server.py")
            cmd = [sys.executable, "-u", path]
        
        # We redirect stderr to DEVNULL to prevents any non-JSON logs from 
        # the server from being interpreted as responses by our JSON parser.
        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.DEVNULL
        )
        return cls(proc)

    async def _read_json(self) -> Any:
        """Read lines until a valid JSON object is parsed."""
        assert self.proc.stdout is not None
        while True:
            line = await self.proc.stdout.readline()
            if not line:
                raise RuntimeError("MCP server process terminated")
            text = line.decode().strip()
            # debug: show raw line received from server
            self._log.debug("_read_json raw: %r", text)
            if not text:
                continue
            try:
                return json.loads(text)
            except json.JSONDecodeError:
                # log the failure so the user can see non-json output
                self._log.debug("_read_json JSON decode failed for text: %r", text)
                continue

    async def _send(self, message: dict) -> Any:
        async with self._lock:
            if self.proc.returncode is not None:
                 raise RuntimeError(f"Server exited with code {self.proc.returncode}")
            
            # detailed checks for debugging
            if self.proc.stdin is None:
                tb = "".join(traceback.format_stack())
                self._log.error("stdin is None on subprocess; cannot send message. stack:\n%s", tb)
                raise RuntimeError("MCP subprocess stdin is not available")

            out_text = json.dumps(message)
            try:
                self._log.debug("_send -> %s", out_text)
                self.proc.stdin.write((out_text + "\n").encode())
                await self.proc.stdin.drain()
            except Exception as e:
                self._log.exception("Exception writing to MCP stdin: %s", e)
                raise

            resp = await self._read_json()
            self._log.debug("_send <- %s", resp)
            return resp

    async def list_tools(self) -> List[Dict]:
        return await self._send({"type": "list_tools"})

    async def call_tool(self, name: str, arguments: dict) -> Any:
        return await self._send({"type": "call_tool", "name": name, "arguments": arguments})

__all__ = ["MCPClient"]


class MiniCoderAgent:
    """An agent that can use tools to solve coding tasks via an MCP server."""

    def __init__(self, llm_client: Optional[LLMClient] = None):
        self.llm = llm_client or LLMClient()
        self.system_prompt = (
            "You are MiniCoder, a world-class autonomous coding agent.\n"
            "Your goal is to solve the user's request by taking action in the local environment.\n\n"
            "RULES:\n"
            "1. ALWAYS use tools to accomplish tasks. If the user asks for code, do not JUST print it; use `write_file` to create the script.\n"
            "2. START by exploring the environment if needed (use `list_files` or `execute_bash`).\n"
            "3. After writing code, VERIFY it. Run it or check the file content.\n"
            "4. If a task is complex, break it down: Plan -> Action -> Verify.\n"
            "5. Be concise and professional. If you have a thought process, keep it brief.\n"
            f"Current working directory: {__import__('os').getcwd()}"
        )

        # placeholder for the MCP client and discovered tools
        self.mcp: Optional[MCPClient] = None
        self.tools: Optional[List[Dict]] = None
        self._log = logging.getLogger("MiniCoderAgent")

        # Start a dedicated asyncio event loop in a background thread so
        # subprocess pipes remain attached to a persistent loop. This avoids
        # creating/closing event loops on each synchronous `run()` call which
        # can leave subprocess transports with a closed proactor.
        self._loop = asyncio.new_event_loop()
        self._loop_thread = threading.Thread(target=self._start_loop, daemon=True)
        self._loop_thread.start()

    async def _ensure_mcp(self):
        """Establish connection to the MCP server and fetch tool metadata.

        The MCP server returns a minimal description (name, description,
        parameters as annotation mapping). OpenAI's function-calling API
        requires each tool to be wrapped in a `type:function` envelope with a
        JSON Schema describing its arguments.  We also drop any "return"
        annotation since it is not part of the input parameters.
        """
        if self.mcp is None:
            # debug: create MCP client and log process cmd
            self._log.debug("Connecting to MCP server...")
            self.mcp = await MCPClient.connect()
            try:
                proc = self.mcp.proc
                self._log.debug("MCP subprocess: pid=%s, returncode=%s, stdin=%s, stdout=%s",
                                getattr(proc, 'pid', None), getattr(proc, 'returncode', None),
                                getattr(proc, 'stdin', None) is not None, getattr(proc, 'stdout', None) is not None)
            except Exception:
                self._log.exception("Failed to log MCP subprocess details")
        if self.tools is None:
            raw = await self.mcp.list_tools()
            self._log.debug("Raw tools metadata: %r", raw)
            normalized: List[Dict] = []

            for t in raw:
                # ensure we have a plain dict form
                if not isinstance(t, dict):
                    t = {
                        "name": getattr(t, "name", None) or getattr(t, "tool_name", None),
                        "description": getattr(t, "description", ""),
                        "parameters": getattr(t, "parameters", {}),
                    }

                name = t.get("name")
                desc = t.get("description", "")
                params_ann: Dict = t.get("parameters", {}) or {}
                # remove return annotation if present
                params_ann.pop("return", None)

                # build a simple JSON schema where every argument is treated as
                # a string (MCP annotation values are already simple names).
                props: Dict[str, Dict] = {}
                for key, val in params_ann.items():
                    props[key] = {"type": "string", "description": str(val)}

                schema: Dict = {"type": "object", "properties": props}

                normalized.append({
                    "type": "function",
                    "function": {
                        "name": name,
                        "description": desc,
                        "parameters": schema,
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

                    result_obj = await self.mcp.call_tool(name, args)
                    # client stub returns {"result": ...} or raw value
                    if isinstance(result_obj, dict) and "result" in result_obj:
                        result = result_obj["result"]
                    else:
                        result = result_obj

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
