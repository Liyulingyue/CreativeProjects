"""The health probe must not lie about a busy model, nor hold the cores."""

import threading
import time

from app.cpu_pool import get_big_pool
import importlib

from tests.conftest import client

settings_mod = importlib.import_module("app.routers.settings")


class FakeProvider:
    name = "Model24B"
    base_url = "http://127.0.0.1:9010/v1/chat/completions"
    api_key = ""
    model = "LFM2.5-24B-A2B-Q4_K_M"
    reasoning_budget = None


def _with_local_provider(monkeypatch):
    monkeypatch.setattr(
        settings_mod.state.get_chat_history(), "get_main_provider",
        lambda: FakeProvider())


def test_a_busy_model_is_not_reported_as_broken(monkeypatch):
    """The sidebar asks every minute; a busy model is not a broken one."""
    _with_local_provider(monkeypatch)
    calls = []

    def fake_chat_completion(**kwargs):
        calls.append(kwargs)
        # What the real path raises when the cores are not free in time.
        from app.cpu_pool import CpuPoolBusy
        raise CpuPoolBusy("CPU 池在 2.0 秒内没有被释放")

    import app.llm_client as llm_client
    monkeypatch.setattr(llm_client, "chat_completion", fake_chat_completion)

    ok, detail = settings_mod._probe_llm()
    assert ok is True, detail
    assert "忙" in detail, detail
    # And it gave up quickly instead of waiting a minute.
    assert calls[0]["pool_timeout"] <= 5


def test_the_probe_asks_for_a_deadline(monkeypatch):
    """A provider that never answers must not tie up a worker thread."""
    _with_local_provider(monkeypatch)
    seen = {}

    def fake_chat_completion(**kwargs):
        seen.update(kwargs)
        return {"content": "ok"}

    import app.llm_client as llm_client
    monkeypatch.setattr(llm_client, "chat_completion", fake_chat_completion)

    ok, detail = settings_mod._probe_llm()
    assert ok is True
    assert seen.get("request_timeout") is not None


class TestTheHealthEndpoint:
    """What the health check says, and what it answers with."""

    def test_the_liveness_probe_answers_while_the_service_is_up(self):
        """A separate, simpler question: is this process answering?

        The container that runs the frontend waits for the backend to be
        healthy before it starts. If the full health check answered
        that question -- and it answers 503 when the model is down --
        then a model being restarted would keep the whole application
        unreachable.
        """
        res = client.get("/api/health/live")
        assert res.status_code == 200
        assert res.json()["status"] == "alive"

    def test_the_health_check_looks_at_the_memory(self):
        """The models take most of this machine's memory.

        When it runs out, the kernel starts killing processes, and this
        service is one of them. Saying so while there is still time is
        the point of a health check.
        """
        res = client.get("/api/health")
        assert res.status_code in (200, 503)
        memory = res.json()["checks"]["memory"]
        assert "available_mb" in memory
        assert "total_mb" in memory
