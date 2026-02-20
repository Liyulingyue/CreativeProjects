#!/usr/bin/env python
"""agent.py — MiniCoder Agent with Tool Use loop."""
import json
from typing import List, Dict, Optional

from .llm_client import LLMClient
from .tools import TOOL_SCHEMAS, handle_tool_call


class MiniCoderAgent:
    """An agent that can use tools to solve coding tasks."""

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

    def run(self, prompt: str, history: List[Dict] = None) -> str:
        """Run the agent loop for a given prompt."""
        if history is None:
            history = []
        
        history.append({"role": "user", "content": prompt})
        
        while True:
            messages = [{"role": "system", "content": self.system_prompt}] + history
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
            else:
                history.append({"role": "assistant", "content": content})
                return content or "(No response from agent)"


__all__ = ["MiniCoderAgent"]
