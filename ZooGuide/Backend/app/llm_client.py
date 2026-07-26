"""LLM client wrapper. Supports both sync and async calls."""

from __future__ import annotations

import asyncio
import threading
from typing import Optional

from openai import AsyncOpenAI, OpenAI
from openaijsonwrapper import OpenAIJsonWrapper

from . import config

_sync_client: Optional[OpenAI] = None
_async_client: Optional[AsyncOpenAI] = None
_wrapper: Optional[OpenAIJsonWrapper] = None


def _get_sync_client() -> OpenAI:
    global _sync_client
    if _sync_client is None:
        _sync_client = OpenAI(
            api_key=config.API_KEY,
            base_url=config.BASE_URL,
            timeout=180.0,
        )
    return _sync_client


def _get_async_client() -> AsyncOpenAI:
    global _async_client
    if _async_client is None:
        _async_client = AsyncOpenAI(
            api_key=config.API_KEY,
            base_url=config.BASE_URL,
            timeout=180.0,
        )
    return _async_client


def get_client() -> OpenAI:
    return _get_sync_client()


def get_async_client() -> AsyncOpenAI:
    return _get_async_client()


def _get_wrapper(
    target_structure: dict,
    background: str,
    requirements: list[str],
    model: Optional[str] = None,
) -> OpenAIJsonWrapper:
    return OpenAIJsonWrapper(
        _get_sync_client(),
        model=model or config.MODEL_NAME,
        target_structure=target_structure,
        background=background,
        requirements=requirements,
    )


def is_llm_enabled() -> bool:
    return config.has_valid_llm_config()


def chat_json(
    messages: list,
    target_structure: dict,
    background: str,
    requirements: list[str],
    model: Optional[str] = None,
    max_retries: int = 1,
    overall_timeout: float = 75.0,
) -> dict:
    """Synchronous LLM call. Returns dict with keys: error, data, reasoning, raw_content."""
    wrapper = _get_wrapper(target_structure, background, requirements, model)

    result_holder: dict = {}

    def _call():
        try:
            result_holder["result"] = wrapper.chat(messages=messages)
        except Exception as e:
            result_holder["error"] = str(e)

    th = threading.Thread(target=_call, daemon=True)
    th.start()
    th.join(timeout=overall_timeout)

    if "result" not in result_holder and "error" not in result_holder:
        return {
            "error": f"LLM timeout after {overall_timeout}s",
            "data": None,
            "reasoning": None,
            "raw_content": None,
        }

    if "error" in result_holder:
        return {
            "error": result_holder["error"],
            "data": None,
            "reasoning": None,
            "raw_content": None,
        }

    return result_holder["result"]


async def async_chat_json(
    messages: list,
    target_structure: dict,
    background: str,
    requirements: list[str],
    model: Optional[str] = None,
    overall_timeout: float = 120.0,
) -> dict:
    """Async LLM call. Reuses OpenAIJsonWrapper's prompt building and parsing."""
    wrapper = _get_wrapper(target_structure, background, requirements, model)

    prompt = wrapper._build_system_prompt(target_structure, requirements=requirements, background=background)
    new_messages = []
    has_system = False
    for m in messages:
        if m.get("role") == "system":
            new_messages.append({"role": "system", "content": f"{prompt}\n\n{m['content']}"})
            has_system = True
        else:
            new_messages.append(wrapper._normalize_message(m))
    if not has_system:
        new_messages.insert(0, {"role": "system", "content": prompt})

    client = _get_async_client()
    try:
        response = await asyncio.wait_for(
            client.chat.completions.create(
                model=model or config.MODEL_NAME,
                messages=new_messages,
            ),
            timeout=overall_timeout,
        )
    except asyncio.TimeoutError:
        print(f"[llm] ⏱️ Timeout after {overall_timeout}s (model={model or config.MODEL_NAME})")
        return {
            "error": f"LLM timeout after {overall_timeout}s",
            "data": None,
            "reasoning": None,
            "raw_content": None,
        }
    except Exception as e:
        print(f"[llm] ❌ Exception: {type(e).__name__}: {e}")
        return {
            "error": str(e),
            "data": None,
            "reasoning": None,
            "raw_content": None,
        }

    content = response.choices[0].message.content or ""
    print(f"[llm] 📥 Response: {len(content)} chars, model={model or config.MODEL_NAME}")
    reasoning, data, error = wrapper._parse_content(content)
    if error:
        print(f"[llm] ⚠️ Parse error: {error} | content preview: {content[:200]}")
    else:
        print(f"[llm] ✅ Parsed OK, data keys: {list(data.keys()) if isinstance(data, dict) else type(data).__name__}")

    return {
        "reasoning": reasoning,
        "data": data,
        "error": error,
        "raw_content": content,
    }
