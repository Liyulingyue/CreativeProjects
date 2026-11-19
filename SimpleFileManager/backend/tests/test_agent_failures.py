"""The agent when things go wrong, and the last few corners.

A model that fails half way through a turn, a Word document the agent
has to read, a machine with too many conversations open. These are the
paths that only run when something has already gone wrong, which is
exactly why they need a test: they are the paths nobody exercises by
accident.
"""

import importlib
import json
import time

import pytest

from conftest import client

agent_mod = importlib.import_module("app.routers.agent")


class _FakeProvider:
    id = "fake-provider"
    name = "fake"
    reasoning_budget = 0
    base_url = "http://127.0.0.1:9010/v1"
    model = "fake-model"
    api_key = "not-needed"
    max_tokens = None


def _session():
    return client.post("/api/chat_history/sessions", json={"session_type": "agent"}).json()


def _events(text: str) -> list[tuple[str, dict]]:
    out = []
    for block in text.split("\n\n"):
        if not block.strip():
            continue
        name = None
        data = None
        for line in block.splitlines():
            if line.startswith("event: "):
                name = line[len("event: "):]
            elif line.startswith("data: "):
                data = json.loads(line[len("data: "):])
        if name:
            out.append((name, data or {}))
    return out


def _run(message: str, stream) -> tuple[list[tuple[str, dict]], str]:
    monkeypatch = pytest.MonkeyPatch()
    monkeypatch.setattr(agent_mod, "_resolve_provider", lambda *a, **k: _FakeProvider())
    monkeypatch.setattr(agent_mod, "chat_completion_stream", stream)
    try:
        session = _session()["id"]
        res = client.post("/api/agent/chat/stream", json={
            "message": message, "session_id": session,
        })
        assert res.status_code == 200, res.text
        return _events(res.text), session
    finally:
        monkeypatch.undo()


def _saved(session_id: str) -> list[dict]:
    messages = client.get(f"/api/chat_history/sessions/{session_id}").json()["messages"]
    return [m for m in messages if m["role"] == "assistant"]


# ---- when the model falls over ----


def test_a_model_that_fails_mid_answer_leaves_what_the_user_saw():
    """The answer so far is not thrown away.

    The model produced three words and then the connection died. The
    person saw those three words, so those three words are what gets
    saved -- with a note that the turn was cut short.
    """
    def stream(messages, tools=None, provider=None, **kwargs):
        yield ("token", "说到一半")
        raise RuntimeError("模型掉线了")

    events, session_id = _run("讲个故事", stream)

    errors = [d for n, d in events if n == "error"]
    assert errors, "the failure was not reported"
    assert "模型掉线了" in errors[0]["message"]

    saved = _saved(session_id)
    assert saved
    assert "说到一半" in saved[-1]["content"]
    assert "生成被中断" in saved[-1]["content"]


def test_a_model_that_fails_before_saying_anything_leaves_a_trace():
    """Nothing to show, but the turn is not invisible.

    The person asked something and got nothing back. The saved message
    says what went wrong, so the conversation does not look like the
    request never happened.
    """
    def stream(messages, tools=None, provider=None, **kwargs):
        raise RuntimeError("一开始就坏了")
        yield  # pragma: no cover - never reached

    events, session_id = _run("讲个故事", stream)

    errors = [d for n, d in events if n == "error"]
    assert errors
    saved = _saved(session_id)
    assert saved
    assert "发生错误" in saved[-1]["content"]


# ---- reading a document ----


def test_a_word_document_is_read_as_text_not_as_mojibake():
    """A .docx carries its text in a ZIP of XML.

    Decoded as UTF-8 it comes back as rubbish, and the model would
    answer about rubbish.
    """
    import docx

    from app.deps import get_storage_root

    path = get_storage_root() / "合同.docx"
    document = docx.Document()
    document.add_paragraph("合同里的关键条款：付款期限为三十天")
    document.save(str(path))

    out = agent_mod.tool_read_file("合同.docx")
    assert out["success"] is True
    assert "付款期限为三十天" in out["content"]
    assert "�" not in out["content"]


def test_a_document_is_read_in_pieces_too():
    """The same offset and limit as a text file."""
    import docx

    from app.deps import get_storage_root

    path = get_storage_root() / "长合同.docx"
    document = docx.Document()
    document.add_paragraph("甲乙双方就下列事项达成一致：" * 20)
    document.save(str(path))

    first = agent_mod.tool_read_file("长合同.docx", limit=20)
    assert first["success"] is True
    assert first["truncated"] is True
    assert first["content"] == ("甲乙双方就下列事项达成一致：" * 2)[:20]
    assert first["next_offset"] == 20

    second = agent_mod.tool_read_file("长合同.docx", limit=20, offset=20)
    assert second["content"] != first["content"]


def test_a_document_that_cannot_be_opened_says_why():
    """A Word file that is really a text file.

    The user renamed it, or it was half-copied. The agent should get an
    error it can act on, not a crash.
    """
    from app.deps import get_storage_root

    path = get_storage_root() / "假的.docx"
    path.write_text("这不是 Word 文件", encoding="utf-8")

    out = agent_mod.tool_read_file("假的.docx")
    assert out["success"] is False
    assert out["error"]


# ---- the conversations the machine keeps open ----


def test_too_many_open_conversations_are_dropped_oldest_first():
    """Memory cannot grow for ever.

    The least recently used conversation is the one that goes.
    """
    monkeypatch = pytest.MonkeyPatch()
    monkeypatch.setattr(agent_mod, "_MAX_SESSIONS", 3)
    try:
        agent_mod.sessions.clear()
        agent_mod._session_used.clear()
        for i in range(6):
            sid = f"sid-{i}"
            agent_mod.sessions[sid] = agent_mod.Session()
            agent_mod._session_used[sid] = time.time() + i
        agent_mod._evict_idle_sessions()
        assert len(agent_mod.sessions) == 3
        # The three newest are the ones that stayed.
        assert set(agent_mod.sessions) == {"sid-3", "sid-4", "sid-5"}
    finally:
        monkeypatch.undo()
        agent_mod.sessions.clear()
        agent_mod._session_used.clear()


def test_a_conversation_nobody_has_touched_for_an_hour_is_dropped():
    monkeypatch = pytest.MonkeyPatch()
    monkeypatch.setattr(agent_mod, "_SESSION_TTL_SECONDS", 10)
    try:
        agent_mod.sessions.clear()
        agent_mod._session_used.clear()
        agent_mod.sessions["old"] = agent_mod.Session()
        agent_mod._session_used["old"] = time.time() - 3600
        agent_mod.sessions["new"] = agent_mod.Session()
        agent_mod._session_used["new"] = time.time()
        agent_mod._evict_idle_sessions()
        assert "old" not in agent_mod.sessions
        assert "new" in agent_mod.sessions
    finally:
        monkeypatch.undo()
        agent_mod.sessions.clear()
        agent_mod._session_used.clear()


def test_a_conversation_is_rebuilt_from_what_was_saved():
    """A cold start: the in-memory conversation is gone, the database
    is not. The model should not start from nothing as if the person
    had never asked."""
    from app.deps import state

    chat_svc = state.get_chat_history()
    session = chat_svc.create_session("agent")
    chat_svc.add_message(session.id, "user", "我刚才问的是什么")
    chat_svc.add_message(session.id, "assistant", "你问的是红楼梦")

    agent_mod.sessions.pop(session.id, None)
    agent_mod._session_used.pop(session.id, None)

    restored = agent_mod._restore_session(session.id)
    contents = [m["content"] for m in restored.history]
    assert "我刚才问的是什么" in contents
    assert "你问的是红楼梦" in contents


# ---- small corners ----


def test_an_empty_request_is_not_an_action_request():
    """The rule that decides whether the model is asked for a plan.

    Nothing in, nothing out.
    """
    assert agent_mod._looks_like_action_request("") is False
    assert agent_mod._looks_like_action_request("把文件整理一下") is True
    assert agent_mod._looks_like_action_request("你好") is False
