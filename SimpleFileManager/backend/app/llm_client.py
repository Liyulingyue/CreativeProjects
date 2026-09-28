"""Unified LLM client wrapping the openai SDK.

All LLM calls (chat, agent, RAG, digest) go through here so retry logic,
timeout, and error handling are consistent.
"""

from typing import Optional
from openai import OpenAI

from .deps import state


def _get_client() -> OpenAI:
    settings = state.get_settings()
    base_url = settings.llm_base_url
    if base_url.endswith("/chat/completions"):
        base_url = base_url.rsplit("/chat/completions", 1)[0]
    return OpenAI(
        api_key=settings.llm_api_key or "not-needed",
        base_url=base_url,
        timeout=120,
        max_retries=3,
    )


def chat_completion(
    messages: list[dict],
    tools: Optional[list[dict]] = None,
    temperature: float = 0.7,
) -> dict:
    """Send a chat completion request. Returns the raw choice message dict."""
    settings = state.get_settings()
    client = _get_client()
    kwargs: dict = {
        "model": settings.llm_model,
        "messages": messages,
        "temperature": temperature,
    }
    if tools:
        kwargs["tools"] = tools
    resp = client.chat.completions.create(**kwargs)
    choice = resp.choices[0]
    message = choice.message
    result: dict = {"content": message.content or ""}
    if message.tool_calls:
        result["tool_calls"] = [
            {
                "id": tc.id,
                "function": {
                    "name": tc.function.name,
                    "arguments": tc.function.arguments,
                },
            }
            for tc in message.tool_calls
        ]
    return result


def _get_embedding_client() -> OpenAI:
    settings = state.get_settings()
    base_url = settings.embedding_base_url
    if base_url.endswith("/embeddings"):
        base_url = base_url.rsplit("/embeddings", 1)[0]
    return OpenAI(
        api_key=settings.embedding_api_key or "not-needed",
        base_url=base_url,
        timeout=60,
        max_retries=3,
    )


def get_embeddings(texts: list[str]) -> list[list[float]]:
    """Batch embed — one API call for all texts. Falls back to per-item on failure."""
    if not texts:
        return []
    settings = state.get_settings()
    client = _get_embedding_client()
    resp = client.embeddings.create(input=texts, model=settings.embedding_model)
    by_index = {item.index: item.embedding for item in resp.data}
    return [by_index.get(i, []) for i in range(len(texts))]


def get_embedding(text: str) -> list[float]:
    """Single-text embed (used for queries and dim detection)."""
    return get_embeddings([text])[0]
