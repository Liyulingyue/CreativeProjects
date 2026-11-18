"""The agent's tools: what they hand back, and what they refuse.

The agent is the part of the service that reaches into the user's
files on the model's behalf. Its tools had almost no tests of their
own -- the streaming and the persistence were covered, the tools were
not -- and a tool that quietly returns the wrong thing does not fail
loudly: the model just goes on believing it.

These are about the edges: a path that leaves the storage root, a
file that is not there, a regular expression that does not compile, a
plan naming a file that has moved, a person who may not read the file
the model asked about.
"""

import importlib
import uuid

import pytest

from app.deps import get_storage_root, set_current_user, state

# app.routers.agent is the router; the module has to be asked for by name.
agent = importlib.import_module("app.routers.agent")


def _ordinary_person():
    """A person with an account of their own.

    A fresh name every time: the users live in a database that
    outlives the test.
    """
    username = "乙" + uuid.uuid4().hex[:8]
    return state.get_user_store().create_user(username, "yi1234", role="user")


def _make(relative: str, content: str = "hello"):
    path = get_storage_root() / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    return path


# ---- read_file ----


def test_reading_a_file_gives_its_text_back():
    _make("agent/读我.txt", "这里有一行字")
    out = agent.tool_read_file("agent/读我.txt")
    assert out["success"] is True
    assert "这里有一行字" in out["content"]
    assert out["path"].endswith("读我.txt")


def test_a_path_outside_the_storage_root_is_refused():
    out = agent.tool_read_file("../../../etc/passwd")
    assert out["success"] is False
    assert "storage root" in out["error"]


def test_a_file_that_is_not_there_says_so():
    out = agent.tool_read_file("agent/没有这个.txt")
    assert out["success"] is False
    assert "not found" in out["error"].lower()


def test_a_long_file_can_be_read_in_pieces():
    """The tool has to be able to hand over the rest of a file.

    A novel is two and a half megabytes; a single read cannot hold it,
    so the tool says where it stopped and where to carry on.
    """
    _make("agent/长文件.txt", "abcdefghijklmnopqrstuvwxyz" * 4)
    first = agent.tool_read_file("agent/长文件.txt", limit=20)
    assert first["success"] is True
    assert first["truncated"] is True
    assert first["content"] == "abcdefghijklmnopqrst"

    second = agent.tool_read_file("agent/长文件.txt", limit=20, offset=20)
    assert second["content"] == "uvwxyzabcdefghijklmn"
    assert second["content"] != first["content"]


def test_a_file_somebody_is_shut_out_of_cannot_be_read():
    """The agent reads on the model's behalf, but as the person asking."""
    from app.deps import state

    _make("私密/日记.txt", "秘密")
    user = _ordinary_person()
    state.get_user_store().restrict("私密", user["id"], "none")
    set_current_user(user)
    try:
        out = agent.tool_read_file("私密/日记.txt")
        assert out["success"] is False
        assert "权限" in out["error"]
    finally:
        set_current_user(None)


# ---- search_content ----


def test_searching_content_finds_the_line():
    _make("agent/代码.py", "first line\nsecond needle line\nthird line")
    out = agent.tool_search_content("needle")
    assert out["success"] is True
    assert out["total"] == 1
    assert out["matches"][0]["line"] == 2
    assert "needle" in out["matches"][0]["content"]


def test_a_regular_expression_that_does_not_compile_is_an_error_not_a_crash():
    out = agent.tool_search_content("[unclosed")
    assert out["success"] is False
    assert "regex" in out["error"].lower()


def test_a_pattern_that_is_absurdly_long_is_refused():
    out = agent.tool_search_content("x" * 500)
    assert out["success"] is False
    assert "200" in out["error"]


def test_searching_outside_the_storage_root_is_refused():
    out = agent.tool_search_content("needle", path="../../etc")
    assert out["success"] is False
    assert "storage root" in out["error"]


def test_the_search_does_not_look_inside_files_the_person_may_not_read():
    from app.deps import state

    _make("私密/秘密.txt", "needle inside")
    _make("公开/普通.txt", "needle outside")
    user = _ordinary_person()
    state.get_user_store().restrict("私密", user["id"], "none")
    set_current_user(user)
    try:
        out = agent.tool_search_content("needle")
    finally:
        set_current_user(None)

    assert out["success"] is True
    found = [m["path"] for m in out["matches"]]
    assert any("公开" in p for p in found)
    assert not any("私密" in p for p in found)


# ---- semantic_search ----


def test_a_search_that_finds_nothing_says_so():
    """An empty index is not an error, it is an empty answer with a hint."""
    out = agent.tool_semantic_search("找不到的东西")
    assert out["success"] is True
    assert out["matches"] == []
    assert "hint" in out


def test_the_path_filter_narrows_the_search_to_one_file():
    from app.search_engine import search_engine

    _make("agent/甲.txt", "针 needle 在这里")
    _make("agent/乙.txt", "针 needle 也在这里")
    search_engine.upsert_file(str(get_storage_root() / "agent" / "甲.txt"), "针 needle 在这里")
    search_engine.upsert_file(str(get_storage_root() / "agent" / "乙.txt"), "针 needle 也在这里")

    out = agent.tool_semantic_search("needle", path="agent/甲.txt")
    assert out["success"] is True
    for m in out["matches"]:
        assert "甲" in m["file"]


# ---- submit_plan ----


def test_a_plan_with_no_actions_is_refused():
    out = agent.tool_submit_plan("空计划")
    assert out["success"] is False
    assert "action" in out["error"].lower()


def test_a_plan_naming_a_file_that_is_not_there_is_refused():
    """A half-applied plan is worse than no plan.

    The source has to exist, or the plan would create its target
    folder and then fail every single move.
    """
    out = agent.tool_submit_plan(
        "搬文件",
        actions=[{
            "action_type": "move",
            "source_path": "根本/不存在的文件.txt",
            "target_path": "别处/文件.txt",
            "reason": "试试",
        }],
    )
    assert out["success"] is False
    assert "not found" in out["error"].lower()


def test_a_plan_naming_an_ambiguous_file_says_where_the_file_is():
    _make("一处/同名.txt", "内容")
    _make("另一处/同名.txt", "内容")
    out = agent.tool_submit_plan(
        "搬文件",
        actions=[{
            "action_type": "move",
            "source_path": "同名.txt",
            "target_path": "别处/同名.txt",
            "reason": "试试",
        }],
    )
    assert out["success"] is False
    assert "more than one place" in out["error"]


def test_an_ordinary_person_may_not_submit_a_command():
    """Running a command is the administrator's call."""
    from app.deps import state

    user = _ordinary_person()
    set_current_user(user)
    try:
        out = agent.tool_submit_plan(
            "跑命令",
            actions=[{
                "action_type": "run",
                "command": "echo hello",
                "reason": "试试",
            }],
        )
    finally:
        set_current_user(None)

    assert out["success"] is False
    assert "管理员" in out["error"]


def test_a_good_plan_is_recorded():
    _make("agent/源文件.txt", "内容")
    out = agent.tool_submit_plan(
        "整理文件",
        summary="把源文件搬到目标目录",
        actions=[{
            "action_type": "move",
            "source_path": "agent/源文件.txt",
            "target_path": "agent/目标/源文件.txt",
            "reason": "分类",
        }],
    )
    assert out["success"] is True
    assert out["action_count"] == 1
    plan = state.get_plan_store().get_plan(out["plan_id"])
    assert plan is not None
    assert plan.title == "整理文件"
    assert len(plan.actions) == 1


# ---- the history that goes back to the model ----


def test_a_tool_call_that_never_got_an_answer_is_dropped():
    """A turn that stopped half way leaves the model holding a tool call
    with no reply, and chat templates refuse to render that."""
    messages = [
        {"role": "user", "content": "干活"},
        {"role": "assistant", "tool_calls": [{"id": "call-1", "function": {"name": "x"}}]},
        {"role": "tool", "tool_call_id": "call-1", "content": "结果"},
        {"role": "assistant", "tool_calls": [{"id": "call-2", "function": {"name": "y"}}]},
    ]
    out = agent._drop_dangling_tool_calls(messages)
    assert [m.get("role") for m in out] == ["user", "assistant", "tool"]
    assert out[1]["tool_calls"][0]["id"] == "call-1"


def test_a_tool_reply_with_no_call_is_dropped():
    messages = [
        {"role": "user", "content": "干活"},
        {"role": "tool", "tool_call_id": "call-99", "content": "没人问过我"},
    ]
    out = agent._drop_dangling_tool_calls(messages)
    assert [m.get("role") for m in out] == ["user"]


def test_an_intact_conversation_is_left_alone():
    messages = [
        {"role": "user", "content": "你好"},
        {"role": "assistant", "content": "你好"},
    ]
    assert agent._drop_dangling_tool_calls(messages) == messages


def test_a_summary_that_cannot_be_made_is_empty_not_a_crash():
    """The compaction calls the model. If the model is down, the
    conversation carries on without a summary."""
    def broken(*args, **kwargs):
        raise RuntimeError("模型没在")

    original = agent.chat_completion
    agent.chat_completion = broken
    try:
        assert agent._compress_to_memory([{"role": "user", "content": "很久以前说的话"}]) == ""
    finally:
        agent.chat_completion = original
