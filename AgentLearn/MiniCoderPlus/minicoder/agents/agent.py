#!/usr/bin/env python
"""agent.py — MiniCoder Agent with Tool Use loop."""
import json
from typing import List, Dict, Optional, Callable

from ..llm_client import LLMClient
from ..tools import TOOL_SCHEMAS, handle_tool_call
from ..core.settings import settings


class MiniCoderAgent:
    """An agent that can use tools to solve coding tasks."""

    def __init__(self, llm_client: Optional[LLMClient] = None):
        self.llm = llm_client or LLMClient()
        self.system_prompt = (
            "You are MiniCoder, a world-class autonomous coding agent.\n"
            "Your goal is to solve the user's request by taking action in the local environment.\n\n"
            "ISOLATION RULE:\n"
            "All your file operations and commands are isolated inside a dedicated 'WorkSpace' folder.\n"
            "Treat the 'WorkSpace' as your root directory. Do not try to access files outside of it.\n\n"
            "CORE PROTOCOL:\n"
            "1. THOUGHT: Before every action (tool call or final answer), think about what you need to do and why.\n"
            "2. TOOL USE: Use tools to discover, create, edit, or test code. Don't just show code; perform the work.\n"
            "3. HIERARCHY: Explore (list_files) -> Read (read_file) -> Act (write_file/execute_bash) -> Verify (execute_bash).\n"
            "4. CONSTRAINTS: Be concise. Don't output extremely large file contents unless asked.\n"
            f"Current isolated WorkSpace: {settings.WORKSPACE_DIR}"
        )

    def run(self, prompt: str, history: List[Dict] = None, on_update: Optional[Callable[[Dict], None]] = None, workspace_path: Optional[str] = None) -> str:
        """Run the agent loop for a given prompt.

        If `on_update` is provided it will be called with each new history entry
        so callers (e.g. an HTTP streaming endpoint) can push incremental updates.
        """
        if history is None:
            history = []
        
        # Build system prompt with current workspace context
        current_workspace = workspace_path or str(settings.WORKSPACE_DIR.relative_to(settings.BASE_DIR))
        dynamic_system_prompt = (
            "You are MiniCoder, a world-class autonomous coding agent.\n"
            "Your goal is to solve the user's request by taking action in the local environment.\n\n"
            "ISOLATION RULE:\n"
            f"All your file operations and commands are isolated inside a dedicated '{current_workspace}' folder.\n"
            f"Treat the '{current_workspace}' as your root directory. Do not try to access files outside of it.\n\n"
            "CORE PROTOCOL:\n"
            "1. THOUGHT: Before every action (tool call or final answer), think about what you need to do and why.\n"
            "2. TOOL USE: Use tools to discover, create, edit, or test code. Don't just show code; perform the work.\n"
            "3. HIERARCHY: Explore (list_files) -> Read (read_file) -> Act (write_file/execute_bash) -> Verify (execute_bash).\n"
            "4. CONSTRAINTS: Be concise. Don't output extremely large file contents unless asked.\n"
            f"Current isolated WorkSpace: {current_workspace}"
        )

        history.append({"role": "user", "content": prompt})
        if on_update:
            on_update(history[-1])
        
        while True:
            messages = [{"role": "system", "content": dynamic_system_prompt}] + history
            response = self.llm.chat(messages, tools=TOOL_SCHEMAS)
            
            # If response is a string (error or warning), return it
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
                                "arguments": tc.function.arguments
                            }
                        } for tc in tool_calls
                    ]
                })
                if on_update:
                    on_update(history[-1])
                
                for tool_call in tool_calls:
                    name = tool_call.function.name
                    args = json.loads(tool_call.function.arguments)
                    
                    # Log for the user
                    print(f"\033[32m[Tool Call] {name}({args})\033[0m")
                    
                    result = handle_tool_call(name, args)
                    
                    # Log result for user
                    # print(f"\033[90m{result}\033[0m")
                    
                    history.append({
                        "role": "tool",
                        "tool_call_id": tool_call.id,
                        "name": name,
                        "content": result
                    })
                    if on_update:
                        on_update(history[-1])
            else:
                history.append({"role": "assistant", "content": content})
                if on_update:
                    on_update(history[-1])
                return content or "(No response from agent)"


__all__ = ["MiniCoderAgent"]
