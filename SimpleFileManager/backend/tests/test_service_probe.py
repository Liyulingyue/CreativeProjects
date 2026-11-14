"""The health probe must not lie about a busy model, nor hold the cores."""

import threading
import time

from app.cpu_pool import get_big_pool
import importlib

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
