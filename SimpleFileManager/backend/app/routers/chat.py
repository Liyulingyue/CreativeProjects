from __future__ import annotations

import json
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from ..deps import state
from ..llm_client import chat_completion
from ..skills import get_skill_registry, execute_tool

chat = APIRouter()


class ChatRequest(BaseModel):
    message: str
    system_prompt: str = "你是一个有用的AI助手。"
    provider_id: str | None = None


class ToolUse(BaseModel):
    name: str
    arguments: dict
    result: dict


class ChatResponse(BaseModel):
    response: str
    reasoning: str = ""
    provider_id: str = ""
    provider_name: str = ""
    tools_used: list[ToolUse] = []


_MAX_TOOL_ROUNDS = 8
_TOOL_CALLS_KEY = "tool_calls"


def _build_system_prompt(base_prompt: str) -> str:
    """Append active skill instructions to the system prompt."""
    reg = get_skill_registry()
    instruction = reg.get_active_instructions()
    if not instruction:
        return base_prompt
    return f"{base_prompt}\n\n{instruction}"


def _run_tool_loop(messages: list[dict], tools: list[dict], provider,
                   reasoning_budget: int | None) -> tuple[str, list[ToolUse]]:
    """Call the model, execute any tool calls, loop until it stops."""
    tools_used: list[ToolUse] = []
    reg = get_skill_registry()
    for _round in range(_MAX_TOOL_ROUNDS):
        result = chat_completion(
            messages=messages,
            tools=tools,
            provider=provider,
            reasoning_budget=reasoning_budget,
        )
        tool_calls = result.get(_TOOL_CALLS_KEY)
        if not tool_calls:
            return result.get("content", "") or "", tools_used

        # Append the assistant message that requested these tool calls
        messages.append({
            "role": "assistant",
            "content": result.get("content") or "",
            "tool_calls": [
                {
                    "id": tc["id"],
                    "type": "function",
                    "function": {
                        "name": tc["function"]["name"],
                        "arguments": tc["function"]["arguments"],
                    },
                }
                for tc in tool_calls
            ],
        })

        for tc in tool_calls:
            name = tc["function"]["name"]
            try:
                args = json.loads(tc["function"]["arguments"])
            except (json.JSONDecodeError, TypeError):
                args = {}
            tool_result = execute_tool(name, args)
            tools_used.append(ToolUse(name=name, arguments=args, result=tool_result))
            messages.append({
                "role": "tool",
                "tool_call_id": tc["id"],
                "content": json.dumps(tool_result, ensure_ascii=False),
            })

    # Budget of rounds exhausted: ask for a final answer
    result = chat_completion(
        messages=messages,
        tools=tools,
        provider=provider,
        reasoning_budget=reasoning_budget,
    )
    return result.get("content", "") or "", tools_used


@chat.post("/query")
def chat_query(req: ChatRequest) -> ChatResponse:
    chat_svc = state.get_chat_history()
    provider = None
    if req.provider_id:
        provider = chat_svc.get_provider(req.provider_id)
    if provider is None:
        provider = chat_svc.get_main_provider()
    if provider is None:
        raise HTTPException(
            status_code=400,
            detail="没有可用的 LLM provider；请到 /settings 添加"
        )

    reg = get_skill_registry()
    tools = reg.get_active_tools()
    system_prompt = _build_system_prompt(req.system_prompt)

    messages = [
        {"role": "system", "content": system_prompt},
        {"role": "user", "content": req.message},
    ]

    if tools:
        response_text, tools_used = _run_tool_loop(
            messages, tools, provider, provider.reasoning_budget
        )
    else:
        result = chat_completion(
            messages=messages,
            provider=provider,
            reasoning_budget=provider.reasoning_budget,
        )
        response_text = result.get("content", "")
        tools_used = []

    return ChatResponse(
        response=response_text,
        provider_id=provider.id,
        provider_name=provider.name,
        tools_used=tools_used,
    )
