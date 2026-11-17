"""Unified LLM client wrapping the openai SDK.

All LLM calls (chat, agent, RAG, digest) go through here so retry logic,
timeout, and error handling are consistent.
"""

import logging
import threading
import time
from typing import Iterator, Optional

from openai import APIConnectionError, APIStatusError, OpenAI

from .deps import state
from .cpu_pool import CpuPoolBusy, get_big_pool
from .interaction import interaction

log = logging.getLogger(__name__)

# llama-server runs a small number of slots, so bursts (indexer digest + chat +
# several browser tabs) get answered with 429/503 while a slot frees up. The
# openai SDK retries too, but on its own schedule and with no logging, so the
# user just sees a raw "429 Too Many Requests" surface error.
RETRY_STATUS = {408, 409, 425, 429, 500, 502, 503, 504}
RETRY_BASE_DELAY = 1.5
RETRY_MAX_DELAY = 20.0


def _with_pool(operation, pool_timeout: Optional[float] = None, **kwargs):
    """Make one request while holding the big cores."""
    # A reader's lease: the llama-server can serve more than one request
    # at a time (--parallel 2), and it queues the rest itself. The lock
    # does not need to be stricter than the server -- two conversations
    # should not lock each other out.
    with get_big_pool().hold(timeout=BIG_POOL_TIMEOUT if pool_timeout is None
                             else pool_timeout, shared=True):
        return operation(**kwargs)


class _null_context:
    """A context manager that does nothing.

    A remote provider does not take the local cores, so it does not need
    the lock; this stands in for it.
    """

    def __enter__(self):
        return None

    def __exit__(self, *exc):
        return False


# How many tokens a model can hold at once, per base URL. A llama-server
# says so on /props; a remote service usually does not, and then the
# setting is what we go by.
_CONTEXT_SIZES: dict[str, int] = {}


def provider_context_size(provider) -> Optional[int]:
    """How many tokens this model can hold at once.

    The setting says 32768, which is right for the model this box
    usually runs -- but a person can pick the small model from the
    chat page, and that one holds 8192. Sending it a longer prompt
    than it can take is rejected outright, so it is worth asking.

    Returns None when the model will not say.
    """
    base_url = (getattr(provider, "base_url", "") or "").strip()
    if not base_url:
        return None
    root = base_url.split("/v1")[0].rstrip("/")
    if not root.startswith("http"):
        return None
    if root in _CONTEXT_SIZES:
        return _CONTEXT_SIZES[root]
    try:
        import json
        import urllib.request

        with urllib.request.urlopen(f"{root}/props", timeout=3) as resp:
            data = json.loads(resp.read())
        n_ctx = int(
            (data.get("default_generation_settings") or {}).get("n_ctx")
            or (data.get("meta") or {}).get("n_ctx")
            or 0
        )
    except Exception:
        return None
    if n_ctx <= 0:
        return None
    _CONTEXT_SIZES[root] = n_ctx
    return n_ctx


def _friendly_error(exc: Exception) -> str:
    """Turn transport/SDK errors into something worth showing in the UI."""
    if isinstance(exc, CpuPoolBusy):
        # The holder could be another conversation, the daily digest or a
        # picture index build -- saying which one would be a guess.
        return ("服务器正在忙（模型正在生成，或图片索引构建中），稍等一会儿再试。")
    if isinstance(exc, APIStatusError):
        code = exc.status_code
        if code == 402:
            return ("远程模型服务额度用尽了。请在聊天页把 provider 换成本地的 Model24B。")
        if code == 429:
            return "模型服务繁忙（429），已自动重试多次仍被拒绝。请稍后再试，或在设置里确认 llama-server 已用 --parallel 启动。"
        if code in (500, 502, 503, 504):
            return f"模型服务暂时不可用（{code}），请检查 LLMServices/serve.sh 是否在运行。"
        if code == 404:
            return "模型服务返回 404，请检查设置里的 Base URL 与模型名称是否匹配。"
        detail = getattr(exc, "response", None)
        text = detail.text if detail is not None and getattr(detail, "text", "") else ""
        if "exceeds the available context" in text or "exceed_context_size" in text:
            # The conversation outgrew the model. Saying so is more
            # useful than showing the raw complaint.
            return ("这一轮太长了，这个模型装不下（它的上下文比对话短）。"
                    "换个大一点的模型，或者开一个新对话。")
        return f"模型服务返回 {code}" + (f"：{text[:200]}" if text else "")
    if isinstance(exc, (APIConnectionError, TimeoutError)):
        return "无法连接模型服务，请确认 LLMServices/serve.sh 已启动且端口可达。"
    return str(exc)


# How long a request waits for the big cores. A picture index build or a
# long answer can have them for a while; a user should not wait forever
# without being told.
BIG_POOL_TIMEOUT = 60.0

# How long a streaming client may take to accept a chunk before the
# cores are given back. A client that stops reading -- the browser tab
# went to the background, the phone fell asleep -- stops the server
# generating too: the socket buffer fills up and the model waits. The
# cores are then idle while the lock says the model is using them, and
# that is a box nobody can use.
SLOW_CLIENT_SECONDS = 8.0

# How often a running answer says "still busy". The idle window is 45
# seconds, so this has to be well inside it.
INTERACTION_RENEW_SECONDS = 10.0


# The host names that mean "this machine".
LOCAL_HOSTS = {"127.0.0.1", "localhost", "::1", "0.0.0.0", "host.docker.internal"}


def _is_local_provider(provider) -> bool:
    """Whether the provider runs on this machine.

    A remote model does not use the local cores, so it has no business
    locking them: a slow round trip to a cloud provider would otherwise
    stop the picture index and every local search for the length of it.

    The host name is compared exactly. A substring test would call
    "https://localhost.example.com" local and "http://192.168.1.50:9010"
    remote -- and the second one is a llama-server on this very box,
    addressed by its LAN address.
    """
    url = (getattr(provider, "base_url", "") or "").strip()
    if not url:
        return True
    try:
        from urllib.parse import urlparse
        host = (urlparse(url).hostname or "").lower()
    except Exception:  # noqa: BLE001
        host = url.lower()
    if not host:
        return True
    if host in LOCAL_HOSTS:
        return True
    # An address on this machine's own network is this machine too.
    if host.startswith("192.168.") or host.startswith("10.") or host.startswith("172."):
        return True
    return False


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
    reasoning_budget: Optional[int] = None,
    provider=None,
    pool_timeout: Optional[float] = None,
    request_timeout: Optional[float] = None,
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
    system_cap = max_tokens or settings.max_output_tokens
    if provider.max_tokens:
        effective_max = min(provider.max_tokens, system_cap)
    else:
        effective_max = system_cap
    kwargs: dict = {
        "model": provider.model,
        "messages": messages,
        "temperature": temperature,
        "max_tokens": effective_max,
    }
    if tools:
        kwargs["tools"] = tools
    if request_timeout is not None:
        # A probe that hangs is worse than one that gives up: the sidebar
        # asks every minute, and a provider that never answers would tie
        # up a worker thread for as long as it likes.
        kwargs["timeout"] = request_timeout
    if reasoning_budget is not None and reasoning_budget >= 0:
        # llama-server's token budget for thinking. Without it a reasoning
        # model is free to think for the whole max_tokens, and on this box
        # that is minutes of generation before it does anything useful.
        kwargs["extra_body"] = {"reasoning_budget": reasoning_budget}
    # The big cores have one owner at a time: the model, a picture search,
    # or a picture index build. The model has priority -- it is what the
    # user is waiting for -- so it waits for the lock rather than taking
    # the cores away from a running build.
    if _is_local_provider(provider):
        # The lock covers the request, not the backoff between attempts.
        # A server that answers 429 twice would otherwise keep the cores
        # for the twenty seconds it sleeps between tries.
        resp = _with_retry(
            lambda **kw: _with_pool(client.chat.completions.create, **kw,
                                    pool_timeout=pool_timeout),
            **kwargs)
    else:
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


class _Lease:
    """A lease on the big cores that a stalled client cannot keep.

    A streaming answer holds the cores for as long as it lasts. If the
    client stops reading -- the browser tab went to the background, the
    phone fell asleep -- the server stops generating as well: the socket
    buffer fills up and the model waits. The cores are then idle while
    the lease says the model is using them, and that is a box nobody can
    use.

    The client that stops reading is the one that cannot be asked: a
    generator suspended at a yield does not run any more, so it cannot
    notice that it has been asleep. A watchdog notices instead: while the
    lease has not been touched for a while, it is given back. The answer
    picks it up again when the client comes back.
    """

    def __init__(self, pool, timeout: float, idle_seconds: Optional[float] = None):
        self.pool = pool
        self.timeout = timeout
        # Read at the moment the lease is made, not when this function
        # was defined: the number is a setting, and a default argument
        # would freeze it at import time.
        self.idle_seconds = SLOW_CLIENT_SECONDS if idle_seconds is None else idle_seconds
        self.held = False
        self._lock = threading.Lock()
        # When the holder last did something.
        self._touched = time.monotonic()
        self._watching = False

    def acquire(self) -> bool:
        with self._lock:
            if self.held:
                return True
            got = self.pool.acquire(self.timeout, shared=True)
            self.held = got
            if got:
                self._touched = time.monotonic()
                self._start_watching()
            return got

    def release(self) -> None:
        with self._lock:
            if self.held:
                self.held = False
                self.pool.release()

    def touch(self) -> None:
        """The holder is alive and working."""
        with self._lock:
            self._touched = time.monotonic()

    def _start_watching(self) -> None:
        if self._watching:
            return
        self._watching = True
        threading.Thread(target=self._watch, daemon=True,
                         name="cpu-lease-watchdog").start()

    def _watch(self) -> None:
        while True:
            # Look often enough to be worth the thread: a lease that is
            # given back after a minute is no use to anybody.
            time.sleep(min(1.0, max(0.05, self.idle_seconds / 2)))
            with self._lock:
                if not self.held:
                    continue
                idle = time.monotonic() - self._touched
                if idle <= self.idle_seconds:
                    continue
                log.info("the client has not read for %.0fs -- giving the "
                         "cores back", idle)
                self.held = False
                self.pool.release()


def chat_completion_stream(
    messages: list[dict],
    tools: Optional[list[dict]] = None,
    temperature: float = 0.7,
    max_tokens: Optional[int] = None,
    reasoning_budget: Optional[int] = None,
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
    system_cap = max_tokens or settings.max_output_tokens
    if provider.max_tokens:
        effective_max = min(provider.max_tokens, system_cap)
    else:
        effective_max = system_cap
    kwargs: dict = {
        "model": provider.model,
        "messages": messages,
        "temperature": temperature,
        "max_tokens": effective_max,
        "stream": True,
    }
    if tools:
        kwargs["tools"] = tools
    if reasoning_budget is not None and reasoning_budget >= 0:
        # See chat_completion: a token budget for the model's thinking.
        kwargs["extra_body"] = {"reasoning_budget": reasoning_budget}
    calls: dict[int, dict] = {}
    # The lock is held for the whole stream, not just for the request: the
    # cores are busy until the last token is out. Closing the generator
    # early -- which is what happens when the model asks for a tool -- ends
    # the with-block and hands the cores back.
    #
    # The request goes out inside the block on purpose. The server starts
    # computing the moment it receives it, so asking first and locking
    # afterwards would let the model run on the cores while a picture
    # index build believes it has them.
    pool = get_big_pool() if _is_local_provider(provider) else None
    lease = _Lease(pool, BIG_POOL_TIMEOUT) if pool else None
    if pool:
        # Before the request goes out: a reasoning model can think for
        # minutes before the first token, and the indexers should keep
        # their hands off the cores during that time too.
        interaction.mark_active()
        if not lease.acquire():
            # The same answer as before: the cores are busy and the wait
            # is over. Silently answering without the lease would let a
            # picture index build take the cores mid-answer.
            raise CpuPoolBusy(f"CPU 池在 {BIG_POOL_TIMEOUT} 秒内没有被释放")
    try:
        # The retry stays inside the lock here, unlike the single-shot
        # path: the lock has to cover the whole stream, so it cannot be
        # given up between attempts.
        stream = _with_retry(client.chat.completions.create, **kwargs)
        # A long answer can run for minutes. The "someone is talking"
        # signal is only good for 45 seconds, so it is renewed while the
        # tokens keep coming -- otherwise the indexers would decide the
        # machine is idle and take the cores away from the answer.
        last_mark = time.monotonic()
        for chunk in stream:
            now = time.monotonic()
            # Only a local answer occupies the local cores, so only a
            # local answer should keep the indexers standing down. A
            # cloud model uses no CPU here at all.
            if pool and now - last_mark >= INTERACTION_RENEW_SECONDS:
                interaction.mark_active()
                last_mark = now
            if not chunk.choices:
                continue
            delta = chunk.choices[0].delta
            # Reasoning arrives on its own channel ahead of the answer. Without a
            # separate event type it would either be lost or interleaved into the
            # reply text, which reads as the model thinking out loud mid-sentence.
            reasoning = getattr(delta, "reasoning_content", None)
            if reasoning:
                if lease:
                    lease.touch()
                yield ("reasoning", reasoning)
                if lease:
                    lease.acquire()
            content = getattr(delta, "content", None)
            if content:
                # How long did the client take to take this chunk? A
                # client that has stopped reading stops the server
                # generating as well, and the cores are then idle while
                # the lease says they are busy.
                if lease:
                    lease.touch()
                yield ("token", content)
                if lease:
                    # The watchdog may have given the cores back while
                    # the client was away. Take them again for the rest
                    # of the answer.
                    lease.acquire()
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

    finally:
        if lease:
            lease.release()

    for slot in calls.values():
        if slot["name"]:
            yield ("tool_call", slot)
