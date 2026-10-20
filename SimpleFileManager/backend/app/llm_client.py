"""Unified LLM client wrapping the openai SDK.

All LLM calls (chat, agent, RAG, digest) go through here so retry logic,
timeout, and error handling are consistent.
"""

import logging
import time
from typing import Iterator, Optional

from openai import APIConnectionError, APIStatusError, OpenAI

from .deps import state

log = logging.getLogger(__name__)

# llama-server runs a small number of slots, so bursts (indexer digest + chat +
# several browser tabs) get answered with 429/503 while a slot frees up. The
# openai SDK retries too, but on its own schedule and with no logging, so the
# user just sees a raw "429 Too Many Requests" surface error.
RETRY_STATUS = {408, 409, 425, 429, 500, 502, 503, 504}
RETRY_BASE_DELAY = 1.5
RETRY_MAX_DELAY = 20.0


def _friendly_error(exc: Exception) -> str:
    """Turn transport/SDK errors into something worth showing in the UI."""
    if isinstance(exc, APIStatusError):
        code = exc.status_code
        if code == 429:
            return "模型服务繁忙（429），已自动重试多次仍被拒绝。请稍后再试，或在设置里确认 llama-server 已用 --parallel 启动。"
        if code in (500, 502, 503, 504):
            return f"模型服务暂时不可用（{code}），请检查 LLMServices/serve.sh 是否在运行。"
        if code == 404:
            return "模型服务返回 404，请检查设置里的 Base URL 与模型名称是否匹配。"
        detail = getattr(exc, "response", None)
        return f"模型服务返回 {code}" + (f"：{detail.text[:200]}" if detail is not None and getattr(detail, "text", "") else "")
    if isinstance(exc, (APIConnectionError, TimeoutError)):
        return "无法连接模型服务，请确认 LLMServices/serve.sh 已启动且端口可达。"
    return str(exc)


def _with_retry(operation, *args, **kwargs):
    """Run an SDK call, backing off on transient server-side errors."""
    attempts = 0
    while True:
        try:
            return operation(*args, **kwargs)
        except APIStatusError as e:
            if e.status_code not in RETRY_STATUS:
                raise
            attempts += 1
            if attempts >= 4:
                log.warning("LLM call failed after %d retries: %s", attempts, e)
                raise
            # Honour Retry-After when the server sends one, else exponential backoff.
            retry_after = None
            response = getattr(e, "response", None)
            if response is not None:
                try:
                    retry_after = float(response.headers.get("Retry-After", ""))
                except (TypeError, ValueError):
                    retry_after = None
            delay = retry_after if retry_after else min(RETRY_BASE_DELAY * (2 ** (attempts - 1)), RETRY_MAX_DELAY)
            log.info("LLM %s, retry %d/%d in %.1fs", e.status_code, attempts, 3, delay)
            time.sleep(delay)
        except (TimeoutError, OSError) as e:
            attempts += 1
            if attempts >= 4:
                raise
            delay = min(RETRY_BASE_DELAY * (2 ** (attempts - 1)), RETRY_MAX_DELAY)
            log.info("LLM connection error (%s), retry %d/%d in %.1fs", e, attempts, 3, delay)
            time.sleep(delay)


def _get_client(provider) -> OpenAI:
    """Return an OpenAI client pointed at the given provider.

    Provider is duck-typed (any object with `base_url`, `model`,
    `api_key`); the agent flow passes an `LLMProvider` from the
    database, the settings probe builds a transient one with the
    same shape. Stripping the OpenAI-compat `/chat/completions` suffix
    is needed because the SDK appends its own path; settings typically
    store the full endpoint URL but older configs may have just the
    base.
    """
    base_url = provider.base_url
    api_key = provider.api_key or "not-needed"
    if base_url.endswith("/chat/completions"):
        base_url = base_url.rsplit("/chat/completions", 1)[0]
    return OpenAI(
        api_key=api_key,
        base_url=base_url,
        timeout=300,
        max_retries=0,  # retries are handled by _with_retry so we control backoff
    )


def chat_completion(
    messages: list[dict],
    tools: Optional[list[dict]] = None,
    temperature: float = 0.7,
    max_tokens: Optional[int] = None,
    provider=None,
) -> dict:
    """Send a chat completion request. Returns the raw choice message dict.

    `provider` is duck-typed; the agent flow passes an LLMProvider from
    the database, the settings probe builds a transient one with the
    same shape. Falls back to the main provider when None is given.
    """
    if provider is None:
        provider = state.get_chat_history().get_main_provider()
        if provider is None:
            raise RuntimeError("No LLM provider configured (add one in /settings)")
    client = _get_client(provider)
    settings = state.get_settings()
    kwargs: dict = {
        "model": provider.model,
        "messages": messages,
        "temperature": temperature,
        "max_tokens": max_tokens or settings.max_output_tokens,
    }
    if tools:
        kwargs["tools"] = tools
    resp = _with_retry(client.chat.completions.create, **kwargs)
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


def _get_embedding_client(base_url: str, api_key: str) -> OpenAI:
    if base_url.endswith("/embeddings"):
        base_url = base_url.rsplit("/embeddings", 1)[0]
    return OpenAI(
        api_key=api_key or "not-needed",
        base_url=base_url,
        # Measured ~15s per 1500-character chunk on the local CPU embedding
        # server, so a 16-chunk batch needs several minutes.
        timeout=600,
        max_retries=0,
    )


def get_embeddings(
    texts: list[str],
    base_url: Optional[str] = None,
    api_key: Optional[str] = None,
    model: Optional[str] = None,
) -> list[list[float]]:
    """Batch embed — one API call for all texts. Falls back to per-item on failure.

    base_url / api_key / model override the indexer defaults from
    EMBEDDING_SEARCH_*; the search path passes these so it hits the dedicated
    GPU-backed endpoint without going through the indexer. Callers that omit
    them keep the legacy behaviour of reading state.get_settings().
    """
    if not texts:
        return []
    # Fall back to the is_index row when callers omit fields. The
    # search path always passes its own values, so the fallback
    # only matters for callers that don't know which provider to
    # use (probes, the dim detector). The previous default read
    # AppSettings, but those fields moved to the providers table
    # in the embedding refactor.
    if not base_url or not api_key or not model:
        idx = state.get_chat_history().get_index_embedding_provider()
        if idx is not None:
            base_url = base_url or idx.base_url
            api_key = api_key or idx.api_key
            model = model or idx.model
    if not (base_url and model):
        raise RuntimeError("没有可用的 embedding provider；请到 /settings 添加")
    client = _get_embedding_client(base_url, api_key)
    resp = _with_retry(client.embeddings.create, input=texts, model=model)
    by_index = {item.index: item.embedding for item in resp.data}
    return [by_index.get(i, []) for i in range(len(texts))]


def get_embedding(
    text: str,
    base_url: Optional[str] = None,
    api_key: Optional[str] = None,
    model: Optional[str] = None,
) -> list[float]:
    """Single-text embed (used for queries and dim detection)."""
    return get_embeddings([text], base_url=base_url, api_key=api_key, model=model)[0]


def chat_completion_stream(
    messages: list[dict],
    tools: Optional[list[dict]] = None,
    temperature: float = 0.7,
    max_tokens: Optional[int] = None,
    provider=None,
) -> Iterator[tuple[str, object]]:
    """Stream a chat completion.

    Yields ("token", str) for each content delta and, once the stream ends,
    ("tool_call", {...}) for each accumulated tool call. Tool call fragments
    are reassembled here because the wire format splits the function name and
    its JSON arguments across deltas.

    `provider` is duck-typed; the agent flow passes an LLMProvider from
    the database, the settings probe builds a transient one with the
    same shape. Falls back to the main provider when None is given.
    Tools are still passed through if provided — the caller's choice
    of provider is respected for the entire stream, including any
    subsequent tool-call steps. (Pick a 24B-class provider if your
    turn calls tools; the 350M picks them unreliably.)

    Retries apply only to establishing the stream: once tokens are flowing a
    retry would duplicate them.
    """
    if provider is None:
        provider = state.get_chat_history().get_main_provider()
        if provider is None:
            raise RuntimeError("No LLM provider configured (add one in /settings)")
    client = _get_client(provider)
    settings = state.get_settings()
    kwargs: dict = {
        "model": provider.model,
        "messages": messages,
        "temperature": temperature,
        "max_tokens": max_tokens or settings.max_output_tokens,
        "stream": True,
    }
    if tools:
        kwargs["tools"] = tools
    stream = _with_retry(client.chat.completions.create, **kwargs)

    calls: dict[int, dict] = {}
    for chunk in stream:
        if not chunk.choices:
            continue
        delta = chunk.choices[0].delta
        # Reasoning arrives on its own channel ahead of the answer. Without a
        # separate event type it would either be lost or interleaved into the
        # reply text, which reads as the model thinking out loud mid-sentence.
        reasoning = getattr(delta, "reasoning_content", None)
        if reasoning:
            yield ("reasoning", reasoning)
        content = getattr(delta, "content", None)
        if content:
            yield ("token", content)
        for tc in getattr(delta, "tool_calls", None) or []:
            idx = tc.index if getattr(tc, "index", None) is not None else len(calls)
            slot = calls.setdefault(idx, {"id": "", "name": "", "arguments": ""})
            if getattr(tc, "id", None):
                slot["id"] = tc.id
            fn = getattr(tc, "function", None)
            if fn is not None:
                if getattr(fn, "name", None):
                    slot["name"] = fn.name
                if getattr(fn, "arguments", None):
                    slot["arguments"] += fn.arguments

    for slot in calls.values():
        if slot["name"]:
            yield ("tool_call", slot)
