"""The agent's tools, at the edges.

The loop is covered in test_agent_loop.py. This file is about the
parts around it: what the agent hands back when a search turns up
something from more than one book, how a conversation that has grown
too long is cut down, and which of the results a person is even
allowed to see.

Two of these are about permission. The index holds every document on
the machine, including the ones this person may not open; the agent
reads the index, so it has to filter what comes back.
"""

import importlib
import json

import pytest

from app.deps import get_storage_root, set_current_user, state

agent_mod = importlib.import_module("app.routers.agent")


def _make(relative: str, content: str = "hello"):
    path = get_storage_root() / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    return path


def _ordinary_person():
    """A person with an account of their own."""
    import uuid

    username = "丙" + uuid.uuid4().hex[:8]
    return state.get_user_store().create_user(username, "bing1234", role="user")


# ---- semantic search: the answers, and the warnings ----


class _FakeSearch:
    """A search engine that returns what the test says it returns."""

    def __init__(self, results):
        self.results = results
        self.calls = []

    def hybrid_search(self, query, top_k=10, path_filter="", **kwargs):
        self.calls.append((query, top_k, path_filter))
        if path_filter:
            return {"results": [r for r in self.results if path_filter in r["path"]]}
        return {"results": list(self.results)}

    def indexed_file_names(self):
        return [r["name"] for r in self.results]


def _result(path: str, name: str, preview: str = "一段话"):
    """A search hit, in the shape the search engine really returns.

    The path is absolute: that is what the index stores.
    """
    return {
        "path": str(get_storage_root() / path),
        "name": name,
        "preview": preview,
        "chunk_idx": 0,
        "sources": ["vector"],
    }


def test_the_passages_are_handed_over_with_the_file_they_are_from(monkeypatch):
    """The model has to know which book each passage came from."""
    from app.search_engine import search_engine

    fake = _FakeSearch([_result("甲.txt", "甲.txt", "甲书里的一段")])
    monkeypatch.setattr(search_engine, "hybrid_search", fake.hybrid_search)
    monkeypatch.setattr(search_engine, "indexed_file_names", fake.indexed_file_names)

    out = agent_mod.tool_semantic_search("找一段")
    assert out["success"] is True
    assert len(out["matches"]) == 1
    match = out["matches"][0]
    assert match["file"] == "甲.txt"
    assert match["text"] == "甲书里的一段"
    assert match["matched_by"] == ["vector"]


def test_passages_from_several_files_are_flagged(monkeypatch):
    """Semantic neighbours do not know where one book ends.

    An unfiltered search can hand back passages from works the caller
    never asked about, and the model may quietly attribute them to the
    book in question. The tool says so.
    """
    from app.search_engine import search_engine

    fake = _FakeSearch([
        _result("甲.txt", "甲.txt"),
        _result("乙.txt", "乙.txt"),
    ])
    monkeypatch.setattr(search_engine, "hybrid_search", fake.hybrid_search)
    monkeypatch.setattr(search_engine, "indexed_file_names", fake.indexed_file_names)

    out = agent_mod.tool_semantic_search("林黛玉")
    assert out["success"] is True
    assert out.get("files_matched") == ["乙.txt", "甲.txt"]
    assert "warning" in out
    assert "re-run with path" in out["warning"]


def test_the_search_is_narrowed_when_the_question_names_one_file(monkeypatch):
    """The question says which book; the filter follows.

    A smaller model does not reliably pass the path even when told to,
    so the tool works it out from the question itself.
    """
    from app.search_engine import search_engine

    _make("红楼梦.txt", "书里的一段")
    fake = _FakeSearch([_result("红楼梦.txt", "红楼梦.txt")])
    monkeypatch.setattr(search_engine, "hybrid_search", fake.hybrid_search)
    monkeypatch.setattr(search_engine, "indexed_file_names", fake.indexed_file_names)

    out = agent_mod.tool_semantic_search("红楼梦里林黛玉说了什么")
    assert out["success"] is True
    assert out.get("auto_filtered_to") == "红楼梦.txt"
    # And the filter really was passed down to the engine.
    assert fake.calls[0][2] == "红楼梦.txt"


def test_a_search_that_finds_nothing_says_what_to_do(monkeypatch):
    from app.search_engine import search_engine

    fake = _FakeSearch([])
    monkeypatch.setattr(search_engine, "hybrid_search", fake.hybrid_search)
    monkeypatch.setattr(search_engine, "indexed_file_names", fake.indexed_file_names)

    out = agent_mod.tool_semantic_search("找不到")
    assert out["success"] is True
    assert out["matches"] == []
    assert "hint" in out


# ---- the permission filter ----


def test_a_result_the_person_may_not_open_is_dropped(monkeypatch):
    """The index holds every book; the agent reads the index.

    A person shut out of a folder must not be handed passages from
    it, and the model must not be told the passage is there.
    """
    from app.search_engine import search_engine

    _make("私密/日记.txt", "秘密")
    _make("公开/说明.txt", "普通")

    fake = _FakeSearch([
        _result("私密/日记.txt", "日记.txt"),
        _result("公开/说明.txt", "说明.txt"),
    ])
    monkeypatch.setattr(search_engine, "hybrid_search", fake.hybrid_search)
    monkeypatch.setattr(search_engine, "indexed_file_names", fake.indexed_file_names)

    user = _ordinary_person()
    state.get_user_store().restrict("私密", user["id"], "none")
    set_current_user(user)
    try:
        out = agent_mod.tool_semantic_search("找一段")
    finally:
        set_current_user(None)

    files = [m["file"] for m in out["matches"]]
    assert "说明.txt" in files
    assert "日记.txt" not in files


def test_a_name_in_the_index_is_only_offered_if_the_file_can_be_opened():
    """The index is used to guess which file the question is about.

    A name the person may not open must not be guessed for them: the
    model would then try to read a file it was never allowed to see.
    """
    _make("私密/日记.txt", "秘密")
    _make("公开/说明.txt", "普通")

    user = _ordinary_person()
    state.get_user_store().restrict("私密", user["id"], "none")
    set_current_user(user)
    try:
        assert agent_mod._indexed_name_is_readable("说明.txt") is True
        assert agent_mod._indexed_name_is_readable("日记.txt") is False
        assert agent_mod._indexed_name_is_readable("根本没有这个.txt") is False
    finally:
        set_current_user(None)


# ---- the conversation that has grown too long ----


def test_a_long_conversation_is_cut_down_to_fit_the_model(monkeypatch):
    """A conversation cannot grow for ever.

    When it no longer fits the model, the old turns are summarised and
    dropped, so the conversation carries on instead of being refused.
    """
    agent = importlib.import_module("app.routers.agent")

    session = agent.Session()
    for i in range(40):
        session.history.append({"role": "user", "content": f"第{i}个问题 " + "内容" * 200})
        session.history.append({"role": "assistant", "content": "回答 " * 200})

    summarised = []
    monkeypatch.setattr(agent, "_compress_to_memory", lambda msgs: summarised.append(msgs) or "早先的对话摘要")

    class _Tiny:
        """A model with a very small context."""

        id = "tiny"
        name = "tiny"
        reasoning_budget = 0

    changed = session.compact(_Tiny())
    assert changed is True
    assert summarised, "the old turns were not summarised"
    assert "早先的对话摘要" in session.memory
    # And it now fits.
    from app.routers.agent import context_budget, estimate_messages_tokens

    assert estimate_messages_tokens(session.build_messages()) <= context_budget(_Tiny())


def test_a_short_conversation_is_left_alone(monkeypatch):
    agent = importlib.import_module("app.routers.agent")

    session = agent.Session()
    session.history.append({"role": "user", "content": "你好"})
    session.history.append({"role": "assistant", "content": "你好"})

    def never(msgs):
        raise AssertionError("a short conversation must not be summarised")

    monkeypatch.setattr(agent, "_compress_to_memory", never)
    assert session.compact() is False


# ---- the pictures ----


def test_looking_for_pictures_that_do_not_exist_says_what_to_do(monkeypatch):
    """The picture index may not be built yet.

    The tool says so, and says where to build it, instead of
    returning an empty list that reads like "there are none".
    """
    import app.image_search as image_search

    def nothing(query, limit=8):
        return []

    monkeypatch.setattr(image_search, "search_by_text", nothing)
    out = agent_mod.tool_search_images("一只猫")
    assert out["success"] is True
    assert out["results"] == []
    assert "图片索引" in out["note"]


def test_the_picture_model_being_down_is_reported_clearly(monkeypatch):
    import app.image_search as image_search

    def unavailable(query, limit=8):
        raise image_search.SearchUnavailable("图片模型不可用：连不上")

    monkeypatch.setattr(image_search, "search_by_text", unavailable)
    out = agent_mod.tool_search_images("一只猫")
    assert out["success"] is False
    assert "图片模型不可用" in out["error"]


# ---- the plan claim ----


def test_a_made_up_plan_id_is_recognised():
    """The model sometimes invents an id instead of calling the tool.

    A long hex run next to a plan reference is the signature of that.
    """
    assert agent_mod._claims_submitted_plan(
        "已为您提交整理计划，ID 是 abcdef1234567890abcdef1234"
    ) is True
    assert agent_mod._claims_submitted_plan("整理好了，一共 3 个文件") is False


def test_a_claim_is_not_read_out_of_an_ordinary_sentence():
    """The word "提交" alone is not a claim.

    The check is deliberately loose -- it only decides where the
    correction notice lands, and a false positive costs a sentence in
    the wrong order. But a reply that never mentions a plan is not a
    claim.
    """
    assert agent_mod._claims_submitted_plan("我把提交说明写好了") is False
    assert agent_mod._claims_submitted_plan("整理好了，一共 3 个文件") is False


def test_a_made_up_id_next_to_a_plan_is_a_claim():
    """The id is the giveaway, even without a claim verb."""
    assert agent_mod._claims_submitted_plan(
        "计划已完成，详见 abcdef1234567890abcdef1234"
    ) is True
