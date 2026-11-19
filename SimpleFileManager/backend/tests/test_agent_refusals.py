"""The last corners: what a tool says when it says no.

Every tool here answers with a dict and never raises. The refusals
are the interesting part: a path outside the storage root, a folder
the person may not open, a tool that is switched off, arguments that
do not fit the tool.

The agent hands these answers to the model, so they have to be
readable: the model has to be able to tell the difference between
"there is nothing there" and "you may not look there".
"""

import importlib
import uuid

import pytest

from app.deps import get_storage_root, set_current_user, state

agent_mod = importlib.import_module("app.routers.agent")


def _make(relative: str, content: str = "hello"):
    path = get_storage_root() / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    return path


def _ordinary_person():
    username = "丁" + uuid.uuid4().hex[:8]
    return state.get_user_store().create_user(username, "ding1234", role="user")


# ---- listing a directory ----


def test_a_folder_the_person_may_not_open_is_refused():
    _make("私密/日记.txt", "秘密")
    user = _ordinary_person()
    state.get_user_store().restrict("私密", user["id"], "none")
    set_current_user(user)
    try:
        out = agent_mod.tool_list_directory("私密")
    finally:
        set_current_user(None)

    assert out["success"] is False
    assert "权限" in out["error"]


def test_a_folder_they_may_open_does_not_reveal_the_folders_beside_it():
    """Reaching one folder is not reaching its neighbours.

    A person granted one folder must not learn the names of the
    folders beside it.
    """
    _make("我的/笔记.txt", "内容")
    _make("别人的/秘密.txt", "不该看见")

    user = _ordinary_person()
    state.get_user_store().restrict("别人的", user["id"], "none")
    set_current_user(user)
    try:
        out = agent_mod.tool_list_directory("")
    finally:
        set_current_user(None)

    assert out["success"] is True
    names = [i["name"] for i in out["items"]]
    assert "我的" in names
    assert "别人的" not in names


def test_listing_something_that_is_not_a_directory_says_so():
    _make("一个文件.txt", "内容")
    out = agent_mod.tool_list_directory("一个文件.txt")
    assert out["success"] is False
    assert "not a directory" in out["error"].lower()


def test_listing_a_folder_that_is_not_there_says_so():
    out = agent_mod.tool_list_directory("根本没有这个")
    assert out["success"] is False
    assert "not found" in out["error"].lower()


# ---- finding files ----


def test_a_pattern_that_matches_nothing_is_not_an_error():
    out = agent_mod.tool_find_files("根本没有这个.*")
    assert out["success"] is True
    assert out["total"] == 0


def test_a_pattern_that_does_not_compile_is_an_error():
    out = agent_mod.tool_find_files("[没有闭合")
    assert out["success"] is False


# ---- searching content ----


def test_searching_a_folder_that_is_not_there_says_so():
    out = agent_mod.tool_search_content("什么都行", path="根本没有这个")
    assert out["success"] is False
    assert "not found" in out["error"].lower()


def test_searching_outside_the_storage_root_is_refused():
    out = agent_mod.tool_search_content("什么都行", path="../../etc")
    assert out["success"] is False
    assert "storage root" in out["error"]


# ---- the tool dispatcher ----


def test_arguments_that_do_not_fit_the_tool_are_an_error_not_a_crash():
    """The model guessed the arguments.

    It has to be told, in terms it can act on, rather than crashing
    the turn.
    """
    out = agent_mod.execute_tool("read_file", {"nonexistent_argument": 1})
    assert out["success"] is False
    assert "Invalid arguments" in out["error"]


def test_a_tool_that_is_switched_off_is_refused():
    """A model can still emit a call for a tool it was not given.

    The two axes are enforced here rather than trusting the model:
    visible (is it switched on) and executable (may it run directly).
    """
    out = agent_mod.execute_tool("一个不存在的技能工具", {})
    assert out["success"] is False
    assert "未启用" in out["error"] or "工具" in out["error"]


# ---- the plan ----


def test_a_plan_that_would_write_where_it_may_not_is_refused():
    """A plan cannot do what the person asking may not do."""
    _make("公开/放得下吗.txt", "")
    user = _ordinary_person()
    state.get_user_store().restrict("禁止写入", user["id"], "none")
    set_current_user(user)
    try:
        out = agent_mod.tool_submit_plan(
            "把文件搬过去",
            actions=[{
                "action_type": "move",
                "source_path": "公开/放得下吗.txt",
                "target_path": "禁止写入/放得下吗.txt",
                "reason": "分类",
            }],
        )
    finally:
        set_current_user(None)

    assert out["success"] is False
    assert "not allowed to do" in out["error"] or "may not write" in out["error"]


def test_a_plan_whose_action_is_not_valid_is_refused():
    """An action the plan runner does not understand is refused here,
    not halfway through applying the plan."""
    out = agent_mod.tool_submit_plan(
        "胡闹的计划",
        actions=[{"action_type": "凭空捏造", "reason": "试试"}],
    )
    assert out["success"] is False
    assert out["error"]
