"""Tests for the big-core lock around the model, and the paths around it.

The lock is the reason a picture index build cannot make the model slow
down. It is also the easiest thing to break by accident: delete the two
`with` lines in llm_client and nothing else fails.
"""

import threading
import time

import pytest

from app.cpu_pool import CpuPool, CpuPoolBusy, get_big_pool


class FakeStream:
    def __init__(self, chunks):
        self._chunks = list(chunks)

    def __iter__(self):
        return iter(self._chunks)


class FakeChunk:
    def __init__(self, content):
        self.choices = [type("C", (), {"delta": type(
            "D", (), {"content": content, "reasoning_content": None,
                      "tool_calls": None})()})()]


class FakeCompletions:
    def __init__(self, recorder):
        self._recorder = recorder

    def create(self, **kwargs):
        self._recorder.append(kwargs)
        return FakeStream([FakeChunk("hello"), FakeChunk(" world")])


def _fake_client(monkeypatch, recorder):
    from app import llm_client

    class FakeChat:
        completions = FakeCompletions(recorder)

    class FakeClient:
        chat = FakeChat()

    monkeypatch.setattr(llm_client, "_get_client", lambda provider: FakeClient())
    class FakeState:
        def get_settings(self):
            return type("X", (), {"max_output_tokens": 64})()

        def get_chat_history(self):
            return type("C", (), {"get_main_provider": lambda: None})()

    monkeypatch.setattr(llm_client, "state", FakeState())
    return llm_client


def test_the_model_takes_the_big_pool_for_the_whole_stream(monkeypatch):
    """While the model is generating, nobody else may have the cores."""
    recorder = []
    llm_client = _fake_client(monkeypatch, recorder)

    from app import llm_client as lc

    provider = type("P", (), {"model": "m", "max_tokens": None, "reasoning_budget": None})()
    stream = lc.chat_completion_stream([{"role": "user", "content": "hi"}], provider=provider)

    # The generator has not started yet, so the pool is free.
    with get_big_pool().try_hold(timeout=0) as got:
        assert got is True

    events = []
    for kind, payload in stream:
        events.append((kind, payload))
        # Mid-stream the pool belongs to the model.
        with get_big_pool().try_hold(timeout=0) as got:
            assert got is False, "the model must hold the cores while it generates"
    assert [e[1] for e in events] == ["hello", " world"]


def test_the_pool_is_free_again_when_the_stream_ends(monkeypatch):
    recorder = []
    llm_client = _fake_client(monkeypatch, recorder)
    provider = type("P", (), {"model": "m", "max_tokens": None, "reasoning_budget": None})()
    list(llm_client.chat_completion_stream([{"role": "user", "content": "hi"}], provider=provider))
    with get_big_pool().try_hold(timeout=0) as got:
        assert got is True


def test_closing_the_stream_early_gives_the_cores_back(monkeypatch):
    """The model asks for a tool and the generator is closed right there."""
    recorder = []
    llm_client = _fake_client(monkeypatch, recorder)
    provider = type("P", (), {"model": "m", "max_tokens": None, "reasoning_budget": None})()
    stream = llm_client.chat_completion_stream([{"role": "user", "content": "hi"}], provider=provider)
    next(iter(stream))
    stream.close()
    with get_big_pool().try_hold(timeout=0) as got:
        assert got is True


def test_the_request_goes_out_while_the_pool_is_held(monkeypatch):
    """The server starts computing the moment it receives the request.

    Asking first and locking afterwards would let the model run on the
    cores while a picture index build believes it has them.
    """
    recorder = []
    llm_client = _fake_client(monkeypatch, recorder)
    provider = type("P", (), {"model": "m", "max_tokens": None, "reasoning_budget": None})()
    stream = llm_client.chat_completion_stream([{"role": "user", "content": "hi"}], provider=provider)
    next(iter(stream))
    assert recorder, "the request should have been sent"
    with get_big_pool().try_hold(timeout=0) as got:
        assert got is False
    stream.close()


def test_a_busy_pool_makes_the_model_say_so(monkeypatch):
    """Waiting for the cores has a deadline, and the message is readable."""
    recorder = []
    llm_client = _fake_client(monkeypatch, recorder)
    monkeypatch.setattr(llm_client, "BIG_POOL_TIMEOUT", 0.2)
    provider = type("P", (), {"model": "m", "max_tokens": None, "reasoning_budget": None})()

    held = threading.Event()
    released = threading.Event()

    def holder():
        with get_big_pool().hold():
            held.set()
            released.wait(5)

    t = threading.Thread(target=holder, daemon=True)
    t.start()
    assert held.wait(5)
    try:
        with pytest.raises(CpuPoolBusy):
            list(llm_client.chat_completion_stream(
                [{"role": "user", "content": "hi"}], provider=provider))
        message = llm_client._friendly_error(CpuPoolBusy("x"))
        assert "忙" in message
    finally:
        released.set()
        t.join(5)


def test_a_remote_provider_does_not_lock_the_local_cores(monkeypatch):
    """A cloud model does not use these cores, so it must not lock them."""
    recorder = []
    llm_client = _fake_client(monkeypatch, recorder)
    remote = type("P", (), {"model": "m", "max_tokens": None, "reasoning_budget": None,
                             "base_url": "https://api.example.com/v1"})()
    local = type("P", (), {"model": "m", "max_tokens": None, "reasoning_budget": None,
                           "base_url": "http://127.0.0.1:9010/v1"})()

    assert llm_client._is_local_provider(remote) is False
    assert llm_client._is_local_provider(local) is True

    stream = llm_client.chat_completion_stream([{"role": "user", "content": "hi"}], provider=remote)
    next(iter(stream))
    # The cores are free: a remote round trip does not take them.
    with get_big_pool().try_hold(timeout=0) as got:
        assert got is True
    stream.close()


def test_a_lan_address_on_this_box_is_still_local(monkeypatch):
    """A llama-server reached by its LAN address uses these cores."""
    llm_client = _fake_client(monkeypatch, [])
    lan = type("P", (), {"base_url": "http://192.168.1.50:9010/v1"})()
    public = type("P", (), {"base_url": "https://api.example.com/v1"})()
    lookalike = type("P", (), {"base_url": "https://localhost.example.com/v1"})()
    assert llm_client._is_local_provider(lan) is True
    assert llm_client._is_local_provider(public) is False
    assert llm_client._is_local_provider(lookalike) is False


class TestHowBigAModelIs:
    """The setting says how long a conversation may get.

    The model may be smaller than that: the chat page lets a person
    pick the small model, and it holds a quarter of what the setting
    assumes. Asking the model how big it is costs one request.
    """

    def test_the_model_is_asked_how_much_it_can_hold(self, monkeypatch):
        import json
        import threading
        from http.server import BaseHTTPRequestHandler, HTTPServer

        from app import llm_client

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_GET(self):
                body = json.dumps(
                    {"default_generation_settings": {"n_ctx": 4096}}
                ).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        server = HTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            url = f"http://127.0.0.1:{server.server_address[1]}/v1"
            llm_client._CONTEXT_SIZES.clear()
            size = llm_client.provider_context_size(type("P", (), {"base_url": url})())
            assert size == 4096
            # The answer is remembered: the model is not asked twice.
            llm_client._CONTEXT_SIZES.clear()
            llm_client.provider_context_size(type("P", (), {"base_url": url})())
            assert url.split("/v1")[0] in llm_client._CONTEXT_SIZES
        finally:
            server.shutdown()
            server.server_close()

    def test_a_model_that_will_not_say_gives_no_answer(self, monkeypatch):
        from app import llm_client

        llm_client._CONTEXT_SIZES.clear()
        size = llm_client.provider_context_size(
            type("P", (), {"base_url": "http://127.0.0.1:1/v1"})()
        )
        assert size is None

    def test_the_conversation_is_cut_to_what_the_model_can_hold(self, monkeypatch):
        from app import llm_client
        from app.routers.agent import context_budget

        # The setting says 32768; this model holds 8192.
        monkeypatch.setattr(
            llm_client, "provider_context_size",
            lambda provider: 8192,
        )
        budget = context_budget(type("P", (), {"base_url": "http://x/v1"})())

        assert budget < 8192
        assert budget >= 2000


class TestTheModelSaysItIsTooFull:
    """A model that cannot hold the prompt says so in English.

    The user is told, in Chinese, what to do about it.
    """

    def _context_error(self):
        import httpx2 as httpx
        from openai import APIStatusError

        request = httpx.Request("POST", "http://model/v1/chat/completions")
        response = httpx.Response(
            400,
            request=request,
            text=('{"error":{"code":400,"message":"request (20009 tokens) '
                  'exceeds the available context size (8192 tokens)"}}'),
        )
        return APIStatusError("bad request", response=response, body=None)

    def test_the_complaint_about_the_context_is_translated(self):
        from app.llm_client import _friendly_error

        message = _friendly_error(self._context_error())

        assert "装不下" in message
        assert "exceeds the available context" not in message

    def test_other_bad_requests_still_say_what_the_server_said(self):
        from app.llm_client import _friendly_error

        import httpx2 as httpx
        from openai import APIStatusError

        request = httpx.Request("POST", "http://model/v1/chat/completions")
        response = httpx.Response(400, request=request, text="模型名不认识")
        error = APIStatusError("bad request", response=response, body=None)

        message = _friendly_error(error)

        assert "400" in message
