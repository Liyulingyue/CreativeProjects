"""LLM client wrapper. All LLM calls go through OpenAIJsonWrapper."""

from __future__ import annotations

import json
from typing import Optional

from openai import OpenAI
from openaijsonwrapper import OpenAIJsonWrapper

from . import config


_client: Optional[OpenAI] = None


def _get_client() -> OpenAI:
    global _client
    if _client is None:
        _client = OpenAI(api_key=config.API_KEY, base_url=config.BASE_URL)
    return _client


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
    max_retries: int = 2,
) -> dict:
    """Returns dict with keys: error, data, reasoning, raw_content."""
    wrapper = get_wrapper(target_structure, background, requirements, model)
    last_err: Optional[str] = None
    for attempt in range(max_retries):
        try:
            result = wrapper.chat(messages=messages)
            if not result.get("error"):
                return result
            last_err = result.get("error")
        except Exception as e:
            last_err = str(e)
    return {"error": last_err or "unknown", "data": None, "reasoning": None, "raw_content": None}


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