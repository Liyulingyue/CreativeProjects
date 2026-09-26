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


def get_embedding(text: str) -> list[float]:
    """Get embedding vector for a single text."""
    settings = state.get_settings()
    base_url = settings.embedding_base_url
    if base_url.endswith("/embeddings"):
        base_url = base_url.rsplit("/embeddings", 1)[0]
    client = OpenAI(
        api_key=settings.embedding_api_key or "not-needed",
        base_url=base_url,
        timeout=30,
        max_retries=3,
    )
    resp = client.embeddings.create(input=text, model=settings.embedding_model)
    return resp.data[0].embedding
