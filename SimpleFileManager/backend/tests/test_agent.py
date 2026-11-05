"""Tests for agent tools (list_directory, read_file, find_files, search_content)."""
from tests.conftest import create_test_file, get_storage_root
from app.routers.agent import (
    tool_list_directory,
    tool_read_file,
    tool_find_files,
    tool_search_content,
)
from app.routers.agent import _require_agent_session


class TestListDirectory:
    def test_list_root(self):
        create_test_file("a.txt", "aaa")
        create_test_file("b.txt", "bbb")
        result = tool_list_directory(".")
        assert result["success"] is True
        names = [i["name"] for i in result["items"]]
        assert "a.txt" in names
        assert "b.txt" in names

    def test_list_subdirectory(self):
        create_test_file("sub/c.txt", "ccc")
        result = tool_list_directory("sub")
        assert result["success"] is True
        assert any(i["name"] == "c.txt" for i in result["items"])

    def test_list_nonexistent(self):
        result = tool_list_directory("no_such_dir")
        assert result["success"] is False

    def test_hidden_files_excluded(self):
        create_test_file(".secret", "hidden")
        result = tool_list_directory(".")
        assert not any(i["name"] == ".secret" for i in result["items"])

    def test_agent_rejects_unknown_session(self):
        import pytest
        with pytest.raises(Exception):
            _require_agent_session("does-not-exist")

    def test_agent_rejects_missing_chat_session(self):
        from app.deps import state
        import pytest
        from fastapi import HTTPException

        history = state.get_chat_history()
        session = history.create_session("agent")
        assert history.get_session(session.id) is not None
        with pytest.raises(HTTPException) as exc:
            _require_agent_session("unknown-session")
        assert exc.value.status_code == 404


class TestReadFile:
    def test_read_text_file(self):
        create_test_file("read.txt", "hello world")
        result = tool_read_file("read.txt")
        assert result["success"] is True
        assert "hello world" in result["content"]

    def test_read_nonexistent(self):
        result = tool_read_file("nope.txt")
        assert result["success"] is False

    def test_read_truncated(self):
        create_test_file("big.txt", "x" * 3000)
        result = tool_read_file("big.txt", limit=1000)
        assert result["success"] is True
        assert result["truncated"] is True
        assert len(result["content"]) <= 1000


class TestFindFiles:
    def test_find_by_name(self):
        create_test_file("screenshot1.png", "")
        create_test_file("screenshot2.png", "")
        create_test_file("doc.txt", "")
        result = tool_find_files("screenshot.*png")
        assert result["success"] is True
        assert result["total"] >= 2

    def test_find_no_matches(self):
        result = tool_find_files("zzzznotexist")
        assert result["success"] is True
        assert result["total"] == 0

    def test_invalid_regex(self):
        result = tool_find_files("[invalid(")
        assert result["success"] is False


class TestSearchContent:
    def test_search_content(self):
        create_test_file("doc1.txt", "The quick brown fox")
        create_test_file("doc2.txt", "Nothing interesting here")
        result = tool_search_content("quick.*fox")
        assert result["success"] is True
        assert result["total"] >= 1
        assert any("doc1.txt" in m["path"] for m in result["matches"])

    def test_search_no_matches(self):
        create_test_file("empty.txt", "nothing here")
        result = tool_search_content("zzzznotfound")
        assert result["success"] is True
        assert result["total"] == 0


class TestPathSafety:
    def test_list_directory_path_escape(self):
        result = tool_list_directory("../../../etc")
        assert result["success"] is False

    def test_read_file_path_escape(self):
        result = tool_read_file("../../../etc/passwd")
        assert result["success"] is False


# ---- the server persists the assistant reply ----

class _FakeProvider:
    id = "fake-provider"
    name = "fake"
    reasoning_budget = 0


def _fake_stream(messages, tools=None, provider=None, reasoning_budget=0, max_tokens=None):
    yield ("token", "你好")
    yield ("token", "，世界")


def _make_agent_session(client):
    return client.post("/api/chat_history/sessions", json={"session_type": "agent"}).json()


def test_stream_persists_the_assistant_reply(monkeypatch):
    import importlib
    from conftest import client
    agent_mod = importlib.import_module("app.routers.agent")

    monkeypatch.setattr(agent_mod, "_resolve_provider", lambda *a, **k: _FakeProvider())
    monkeypatch.setattr(agent_mod, "chat_completion_stream", _fake_stream)

    session = _make_agent_session(client)
    res = client.post("/api/agent/chat/stream", json={
        "message": "hi", "session_id": session["id"],
    })
    assert res.status_code == 200, res.text
    assert "event: done" in res.text

    messages = client.get(f"/api/chat_history/sessions/{session['id']}").json()["messages"]
    assistant = [m for m in messages if m["role"] == "assistant"]
    assert len(assistant) == 1
    assert "你好，世界" in assistant[0]["content"]
    assert assistant[0]["provider_id"] == "fake-provider"


def test_a_disconnected_stream_still_persists_the_partial_reply(monkeypatch):
    import importlib
    from conftest import client
    agent_mod = importlib.import_module("app.routers.agent")
    AgentRequest = agent_mod.AgentRequest

    monkeypatch.setattr(agent_mod, "_resolve_provider", lambda *a, **k: _FakeProvider())
    monkeypatch.setattr(agent_mod, "chat_completion_stream", _fake_stream)

    # Capture the generator the endpoint would hand to StreamingResponse,
    # so the test can drop it the way a reload or a closed tab does.
    captured = {}

    class _CaptureResponse:
        def __init__(self, content, **kwargs):
            captured["generator"] = content
            self.body_iterator = content

    monkeypatch.setattr(agent_mod, "StreamingResponse", _CaptureResponse)

    session = _make_agent_session(client)
    agent_mod.agent_chat_stream(AgentRequest(
        message="hi", session_id=session["id"],
    ))
    generator = captured["generator"]
    # Pull events until the first token has been produced, then drop the
    # connection -- the moment a reload or a closed tab creates.
    saw_token = False
    for event in generator:
        if event.startswith("event: token"):
            saw_token = True
            break
    assert saw_token, "the fake stream never produced a token"
    generator.close()

    messages = client.get(f"/api/chat_history/sessions/{session['id']}").json()["messages"]
    assistant = [m for m in messages if m["role"] == "assistant"]
    assert len(assistant) == 1
    assert "你好" in assistant[0]["content"]
    assert "生成被中断" in assistant[0]["content"]


def test_stopping_before_any_answer_still_leaves_a_trace(monkeypatch):
    import importlib
    from conftest import client
    agent_mod = importlib.import_module("app.routers.agent")
    AgentRequest = agent_mod.AgentRequest

    monkeypatch.setattr(agent_mod, "_resolve_provider", lambda *a, **k: _FakeProvider())

    def reasoning_only(messages, tools=None, provider=None, reasoning_budget=0, max_tokens=None):
        yield ("reasoning", "让我想想")
        yield ("reasoning", "怎么回答")

    monkeypatch.setattr(agent_mod, "chat_completion_stream", reasoning_only)

    captured = {}

    class _CaptureResponse:
        def __init__(self, content, **kwargs):
            captured["generator"] = content
            self.body_iterator = content

    monkeypatch.setattr(agent_mod, "StreamingResponse", _CaptureResponse)

    session = _make_agent_session(client)
    agent_mod.agent_chat_stream(AgentRequest(message="hi", session_id=session["id"]))
    generator = captured["generator"]
    # Take the first reasoning event, then drop the connection while the
    # model is still thinking and has produced no answer text.
    for event in generator:
        if event.startswith("event: reasoning"):
            break
    generator.close()

    messages = client.get(f"/api/chat_history/sessions/{session['id']}").json()["messages"]
    assistant = [m for m in messages if m["role"] == "assistant"]
    assert len(assistant) == 1
    # A dropped connection, not a deliberate stop: say so, because the
    # user did not ask for the turn to end.
    assert assistant[0]["content"] == "（生成被中断，没有产出内容）"
    assert "让我想想" in (assistant[0].get("reasoning") or "")


def test_abort_endpoint_saves_what_the_user_saw(monkeypatch):
    from conftest import client

    session = _make_agent_session(client)
    res = client.post("/api/agent/chat/abort", json={
        "session_id": session["id"], "content": "已经写了一半",
    })
    assert res.status_code == 200, res.text
    body = res.json()
    assert body["success"] is True
    assert body["saved_message_id"]

    messages = client.get(f"/api/chat_history/sessions/{session['id']}").json()["messages"]
    assistant = [m for m in messages if m["role"] == "assistant"]
    assert len(assistant) == 1
    assert "已经写了一半" in assistant[0]["content"]
    assert "生成被中断" in assistant[0]["content"]


def test_abort_then_disconnect_does_not_duplicate(monkeypatch):
    import importlib
    from conftest import client
    agent_mod = importlib.import_module("app.routers.agent")
    AgentRequest = agent_mod.AgentRequest

    monkeypatch.setattr(agent_mod, "_resolve_provider", lambda *a, **k: _FakeProvider())
    monkeypatch.setattr(agent_mod, "chat_completion_stream", _fake_stream)

    captured = {}

    class _CaptureResponse:
        def __init__(self, content, **kwargs):
            captured["generator"] = content
            self.body_iterator = content

    monkeypatch.setattr(agent_mod, "StreamingResponse", _CaptureResponse)

    session = _make_agent_session(client)
    # The browser stops the turn and reports what it had.
    client.post("/api/agent/chat/abort", json={
        "session_id": session["id"], "content": "用户看到的半截",
    })

    agent_mod.agent_chat_stream(AgentRequest(message="hi", session_id=session["id"]))
    generator = captured["generator"]
    for event in generator:
        if event.startswith("event: token"):
            break
    generator.close()

    messages = client.get(f"/api/chat_history/sessions/{session['id']}").json()["messages"]
    assistant = [m for m in messages if m["role"] == "assistant"]
    assert len(assistant) == 1
    assert "用户看到的半截" in assistant[0]["content"]


def test_abort_with_nothing_still_leaves_a_trace(monkeypatch):
    import importlib
    from conftest import client
    agent_mod = importlib.import_module("app.routers.agent")
    AgentRequest = agent_mod.AgentRequest

    monkeypatch.setattr(agent_mod, "_resolve_provider", lambda *a, **k: _FakeProvider())

    def reasoning_only(messages, tools=None, provider=None, reasoning_budget=0, max_tokens=None):
        yield ("reasoning", "想想")

    monkeypatch.setattr(agent_mod, "chat_completion_stream", reasoning_only)

    captured = {}

    class _CaptureResponse:
        def __init__(self, content, **kwargs):
            captured["generator"] = content
            self.body_iterator = content

    monkeypatch.setattr(agent_mod, "StreamingResponse", _CaptureResponse)

    session = _make_agent_session(client)
    client.post("/api/agent/chat/abort", json={
        "session_id": session["id"], "content": "",
    })

    agent_mod.agent_chat_stream(AgentRequest(message="hi", session_id=session["id"]))
    generator = captured["generator"]
    for event in generator:
        if event.startswith("event: reasoning"):
            break
    generator.close()

    messages = client.get(f"/api/chat_history/sessions/{session['id']}").json()["messages"]
    assistant = [m for m in messages if m["role"] == "assistant"]
    # The abort endpoint wrote the placeholder straight away, so the
    # user sees the stopped turn immediately rather than whenever the
    # server happens to notice the dropped connection.
    assert len(assistant) == 1
    assert assistant[0]["content"] == "已停止生成。"
