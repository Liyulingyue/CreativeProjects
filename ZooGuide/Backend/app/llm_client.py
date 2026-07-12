"""LLM client wrapper. All LLM calls go through OpenAIJsonWrapper."""

from __future__ import annotations

import json
import threading
from typing import Optional

from openai import OpenAI
from openaijsonwrapper import OpenAIJsonWrapper

from . import config


_client: Optional[OpenAI] = None


def _get_client() -> OpenAI:
    global _client
    if _client is None:
        _client = OpenAI(
            api_key=config.API_KEY,
            base_url=config.BASE_URL,
            timeout=180.0,
        )
    return _client


def get_client() -> OpenAI:
    return _get_client()


def get_wrapper(
    target_structure: dict,
    background: str,
    requirements: list[str],
    model: Optional[str] = None,
) -> OpenAIJsonWrapper:
    return OpenAIJsonWrapper(
        _get_client(),
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
    """Returns dict with keys: error, data, reasoning, raw_content.

    Bounded by overall_timeout (seconds) so the route generation never hangs the HTTP request.
    """
    wrapper = get_wrapper(target_structure, background, requirements, model)

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


def dump_prompt_for_debug(
    prefs_dict: dict,
    candidates: list[dict],
    walking_matrix: dict,
) -> str:
    """Build a human-readable summary of what we're sending to the LLM."""
    return json.dumps(
        {
            "preferences": prefs_dict,
            "candidates_count": len(candidates),
            "candidate_ids": [c["id"] for c in candidates],
            "walking_matrix_size": len(walking_matrix),
        },
        ensure_ascii=False,
        indent=2,
    )