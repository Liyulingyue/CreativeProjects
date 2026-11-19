"""The agent's main loop: the model, the tools, and the rules between.

The agent is a loop. The model says something, the tools run, the
results go back to the model, and it says something else. On top of
that sit the rules that keep the loop honest: a model that repeats
itself is stopped, a model that searches for something that is not
there is stopped, a model that claims to have submitted a plan without
calling the tool is corrected, and a turn that runs out of steps still
answers.

None of that had a test of its own. The streaming and the persistence
were covered; the loop was not.
"""

import importlib
import json

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


def _last_session_id() -> str:
    sessions = client.get("/api/chat_history/sessions").json()
    items = sessions.get("sessions", sessions) if isinstance(sessions, dict) else sessions
    return items[-1]["id"]


def _events(text: str) -> list[tuple[str, dict]]:
    """The SSE stream, as a list of (event, data)."""
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


def _run(message: str, stream, session_id: str | None = None) -> tuple[list[tuple[str, dict]], str]:
    """Drive one turn with a scripted model.

    Returns the events and the session it happened in.
    """
    monkeypatch = pytest.MonkeyPatch()
    monkeypatch.setattr(agent_mod, "_resolve_provider", lambda *a, **k: _FakeProvider())
    monkeypatch.setattr(agent_mod, "chat_completion_stream", stream)
    try:
        session = session_id or _session()["id"]
        res = client.post("/api/agent/chat/stream", json={
            "message": message, "session_id": session,
        })
        assert res.status_code == 200, res.text
        return _events(res.text), session
    finally:
        monkeypatch.undo()


def _saved_reply(session_id: str) -> str:
    """What the person ends up reading.

    The closing answer of a stopped turn is not streamed -- it rides
    along with the saved reply, and the page shows the saved reply
    when the turn ends. So this is where to look for it.
    """
    messages = client.get(f"/api/chat_history/sessions/{session_id}").json()["messages"]
    assistant = [m for m in messages if m["role"] == "assistant"]
    assert assistant, "nothing was saved"
    return assistant[-1]["content"]


def _tool_call(name: str, arguments: dict, call_id: str = "call-1"):
    return ("tool_call", {
        "id": call_id,
        "name": name,
        "arguments": json.dumps(arguments, ensure_ascii=False),
    })


def _answer(text: str):
    return ("token", text)


# ---- the ordinary turn ----


def test_a_turn_without_tools_just_answers():
    events, _ = _run("你好", lambda *a, **k: iter([_answer("你好呀")]))
    names = [n for n, _ in events]
    assert "token" in names
    assert "done" in names
    tokens = [d["text"] for n, d in events if n == "token"]
    assert "你好呀" in "".join(tokens)


def test_a_tool_call_is_run_and_the_result_goes_back_to_the_model():
    """The round trip: the model asks, the tool answers, the model replies.

    The tool really runs -- this is not a mocked tool -- and its result
    reaches the model on the next call.
    """
    from app.deps import get_storage_root

    (get_storage_root() / "工具测试").mkdir(parents=True, exist_ok=True)
    (get_storage_root() / "工具测试" / "a.txt").write_text("内容", encoding="utf-8")

    seen_messages = []

    def stream(messages, tools=None, provider=None, **kwargs):
        seen_messages.append(list(messages))
        if len(seen_messages) == 1:
            yield _tool_call("list_directory", {"path": "工具测试"})
        else:
            yield _answer("这个目录里有 a.txt")

    events, _ = _run("看看工具测试目录里有什么", stream)

    names = [n for n, _ in events]
    assert "tool_start" in names
    assert "tool" in names

    tool_events = [d for n, d in events if n == "tool"]
    assert tool_events[0]["tool"] == "list_directory"
    assert tool_events[0]["result"]["success"] is True
    assert any("a.txt" in str(i["name"]) for i in tool_events[0]["result"]["items"])

    # The second call to the model carries the tool's result.
    assert len(seen_messages) >= 2
    tool_messages = [m for m in seen_messages[1] if m.get("role") == "tool"]
    assert tool_messages, "the tool result never reached the model"
    assert "a.txt" in tool_messages[0]["content"]


def test_a_tool_call_with_broken_arguments_is_not_run():
    """A model that emits nonsense arguments gets an error, not a crash."""
    def stream(messages, tools=None, provider=None, **kwargs):
        yield ("tool_call", {"id": "call-1", "name": "list_directory", "arguments": "{not json"})
        yield _answer("我看看")

    events, _ = _run("列一下目录", stream)
    tool_events = [d for n, d in events if n == "tool"]
    assert tool_events
    assert tool_events[0]["result"]["success"] is False
    assert "Malformed" in tool_events[0]["result"]["error"]


def test_no_provider_is_reported_clearly():
    """No model configured: say so, in Chinese, and stop."""
    monkeypatch = pytest.MonkeyPatch()
    monkeypatch.setattr(agent_mod, "_resolve_provider", lambda *a, **k: None)
    try:
        session = _session()
        res = client.post("/api/agent/chat/stream", json={
            "message": "你好", "session_id": session["id"],
        })
        assert res.status_code == 200, res.text
        events = _events(res.text)
        errors = [d for n, d in events if n == "error"]
        assert errors
        assert "provider" in errors[0]["message"]
    finally:
        monkeypatch.undo()


# ---- the rules that stop a loop from going in circles ----


def test_the_same_call_three_times_stops_the_turn():
    """A model that repeats itself verbatim is stuck, not refining.

    The turn ends with an answer, and says why it stopped.
    """
    def stream(messages, tools=None, provider=None, **kwargs):
        # The same call, word for word, every step.
        yield _tool_call("search_content", {"pattern": "同样的东西"})

    events, _ = _run("找一样东西", stream)

    steps = [d for n, d in events if n == "step"]
    assert any(d.get("stopped") == "loop" for d in steps), steps

    tool_events = [d for n, d in events if n == "tool"]
    assert any("重复了三次" in str(d["result"].get("error", "")) for d in tool_events)


def test_three_empty_searches_stop_the_turn():
    """Searching for something that is not there, three ways.

    The words change every time, so comparing the calls exactly would
    never catch it. What they have in common is that they found
    nothing -- and that is the signal.
    """
    calls = []

    def stream(messages, tools=None, provider=None, **kwargs):
        calls.append(1)
        if len(calls) == 1:
            yield _tool_call("search_content", {"pattern": "垂杨柳"})
        elif len(calls) == 2:
            yield _tool_call("search_content", {"pattern": "倒拔垂杨柳"})
        elif len(calls) == 3:
            yield _tool_call("search_content", {"pattern": "倒拔垂杨柳 第五回"})
        else:
            # The closing pass: the model is told to answer with what
            # it has, and this time it does.
            yield _answer("资料里没有这一回")

    events, session_id = _run("林黛玉倒拔垂杨柳是哪一回", stream)

    steps = [d for n, d in events if n == "step"]
    assert any(d.get("stopped") == "empty" for d in steps), steps

    # And the turn still answers: it says the content is not there.
    assert "资料里没有这一回" in _saved_reply(session_id)


def test_a_turn_that_runs_out_of_steps_still_answers():
    """The step budget is not the end of the turn.

    The model gets one last pass with the tools taken away, so the turn
    ends with an answer instead of an apology about the limit.
    """
    from app.deps import state

    original = state.get_settings().max_agent_steps
    state.get_settings().max_agent_steps = 2
    calls = []

    def stream(messages, tools=None, provider=None, **kwargs):
        calls.append(list(tools or []))
        if len(calls) <= 2:
            yield _tool_call("search_content", {"pattern": f"第{len(calls)}次"})
        else:
            yield _answer("我找到了一些东西")

    try:
        events, session_id = _run("找东西", stream)
    finally:
        state.get_settings().max_agent_steps = original

    # The last pass had no tools: the model was asked to answer with
    # what it had.
    assert calls[-1] == [], "the closing pass still had tools"
    assert "我找到了一些东西" in _saved_reply(session_id)


# ---- the plan guard ----


def test_a_model_that_describes_an_action_without_calling_the_tool_is_corrected():
    """Saying "已提交计划" is not submitting a plan.

    The model is told to use the tool, and gets another turn. If it
    keeps describing instead of doing, the answer says plainly that
    nothing was submitted.
    """
    calls = []

    def stream(messages, tools=None, provider=None, **kwargs):
        calls.append(list(messages))
        yield _answer("已为您提交整理计划，ID 是 abcdef1234567890abcdef12")

    events, session_id = _run("帮我把文件整理一下", stream)

    # The first answer was not accepted: the model was corrected and
    # given another turn.
    assert len(calls) >= 2, "the model was not corrected"
    correction = [m for m in calls[1] if m.get("role") == "user"]
    assert correction
    assert "submit_plan" in correction[-1]["content"]

    # And if it still only describes, the saved answer says plainly
    # that nothing was submitted. The notice rides along with the
    # saved reply -- the page shows the saved reply when the turn
    # ends -- so this is where the person reads it.
    assert "没有提交任何整理计划" in _saved_reply(session_id)


def test_a_model_that_calls_the_tool_is_left_alone():
    """When the plan really is submitted, nothing is added to the answer."""
    def stream(messages, tools=None, provider=None, **kwargs):
        if len([m for m in messages if m.get("role") == "tool"]) == 0:
            yield _tool_call("submit_plan", {
                "title": "整理文件",
                "summary": "把文件分门别类",
                "actions": [{
                    "action_type": "move",
                    "source_path": "整理/a.txt",
                    "target_path": "整理/归类/a.txt",
                    "reason": "分类",
                }],
            })
        else:
            yield _answer("已经提交了计划")

    from app.deps import get_storage_root
    (get_storage_root() / "整理").mkdir(parents=True, exist_ok=True)
    (get_storage_root() / "整理" / "a.txt").write_text("x", encoding="utf-8")

    events, _ = _run("帮我把整理/a.txt 归类", stream)

    tokens = "".join(d["text"] for n, d in events if n == "token")
    assert "没有提交任何整理计划" not in tokens

    tool_events = [d for n, d in events if n == "tool"]
    assert any(d["tool"] == "submit_plan" and d["result"].get("success") for d in tool_events)


# ---- the pieces around the loop ----


def test_the_tool_list_keeps_the_schema_and_the_chinese_together():
    """The frontend holds no tool knowledge of its own.

    It gets the raw schema the model consumes and the Chinese copy to
    show, from one place, so the two cannot drift apart.
    """
    out = agent_mod.get_tools()
    assert out["tools"]
    assert out["definitions"]
    assert out["display"]

    names = {d["name"] for d in out["display"]}
    assert "read_file" in names
    for d in out["display"]:
        assert d["display"], f"{d['name']} has no Chinese copy"
        assert d["risk_label"]


def test_the_provider_of_the_turn_is_remembered_for_the_next_one():
    """The pick sticks to the session.

    A person who chose a model for this conversation keeps it for the
    next message; the dropdown can change it back. An id that no
    longer points at anything falls through instead of failing.
    """
    from app.deps import state

    chat_svc = state.get_chat_history()
    chosen = chat_svc.create_provider("选的那个", "http://127.0.0.1:9011/v1", "m1")
    other = chat_svc.create_provider("另一个", "http://127.0.0.1:9012/v1", "m2")

    # The turn's own choice wins.
    assert agent_mod._resolve_provider(chat_svc, chosen.id, other.id).id == chosen.id
    # Then the session's.
    assert agent_mod._resolve_provider(chat_svc, None, other.id).id == other.id
    # An id that points at nothing falls through rather than failing.
    assert agent_mod._resolve_provider(chat_svc, "已经删掉的", other.id).id == other.id
