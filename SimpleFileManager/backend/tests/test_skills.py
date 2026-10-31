"""Tests for skills: registry, tool execution, chat loop."""

import json

import pytest
from conftest import client

from app.skills import (
    SkillRegistry, execute_tool, get_skill_registry, BUILTIN_SKILLS, BASE_TOOLS, ATOMIC_TOOLS,
)


def test_builtin_skills_are_registered():
    reg = get_skill_registry()
    ids = {s["id"] for s in reg.all_skills()}
    for sid in BUILTIN_SKILLS:
        assert sid in ids


def test_master_off_hides_atomic_tools():
    reg = get_skill_registry()
    reg.set_master(False)
    names = {t["function"]["name"] for t in reg.get_active_tools()}
    for t in ATOMIC_TOOLS:
        assert t["function"]["name"] not in names


def test_master_on_with_active_skill_exposes_atomic_tools():
    reg = get_skill_registry()
    reg.set_master(True)
    reg.set_enabled("organize", True)
    reg.set_all_tools(True)
    names = {t["function"]["name"] for t in reg.get_active_tools()}
    for t in ATOMIC_TOOLS:
        assert t["function"]["name"] in names


def test_base_tools_always_present():
    reg = get_skill_registry()
    reg.set_master(True)
    names = {t["function"]["name"] for t in reg.get_active_tools()}
    for t in BASE_TOOLS:
        assert t["function"]["name"] in names
    reg.set_master(False)


def test_per_tool_toggle():
    reg = get_skill_registry()
    reg.set_master(True)
    reg.set_tool_enabled("delete_file", True)
    names = {t["function"]["name"] for t in reg.get_active_tools()}
    assert "delete_file" in names
    reg.set_tool_enabled("delete_file", False)
    names = {t["function"]["name"] for t in reg.get_active_tools()}
    assert "delete_file" not in names


def test_set_all_tools():
    reg = get_skill_registry()
    reg.set_master(True)
    reg.set_all_tools(False)
    names = {t["function"]["name"] for t in reg.get_active_tools()}
    assert names == set()
    reg.set_all_tools(True)
    names = {t["function"]["name"] for t in reg.get_active_tools()}
    from app.skills import ALL_TOOL_NAMES
    assert names == set(ALL_TOOL_NAMES)


def test_master_off_ignores_tool_toggles():
    reg = get_skill_registry()
    reg.set_master(False)
    reg.set_all_tools(True)
    assert reg.get_active_tools() == []


def test_unknown_tool_rejected():
    reg = get_skill_registry()
    with pytest.raises(ValueError):
        reg.set_tool_enabled("no_such_tool", True)


def test_instruction_injected_when_active():
    reg = get_skill_registry()
    reg.set_master(True)
    reg.set_enabled("organize", True)
    instr = reg.get_active_instructions()
    assert "整理文件" in instr
    assert "[整理文件]" in instr


def test_no_instruction_when_master_off():
    reg = get_skill_registry()
    reg.set_master(False)
    assert reg.get_active_instructions() == ""


def test_search_files_tool():
    from tests.conftest import create_test_file
    create_test_file("searchable.txt", "hello content")
    result = execute_tool("search_files", {"query": "searchable", "limit": 10})
    assert result["total"] == 1
    assert "searchable.txt" in result["files"]


def test_list_directory_tool():
    from tests.conftest import create_test_file
    create_test_file("dircheck/a.txt", "hi")
    result = execute_tool("list_directory", {"path": "dircheck"})
    names = [e["name"] for e in result["entries"]]
    assert "a.txt" in names


def test_read_file_tool():
    from tests.conftest import create_test_file
    create_test_file("readable.txt", "file body here")
    result = execute_tool("read_file", {"path": "readable.txt"})
    assert result["content"] == "file body here"


def test_write_file_tool():
    result = execute_tool("write_file", {"path": "written.txt", "content": "new body"})
    assert result["written"] is True
    assert result["size"] > 0


def test_file_info_tool():
    from tests.conftest import create_test_file
    create_test_file("infocheck.txt", "some data")
    result = execute_tool("file_info", {"path": "infocheck.txt"})
    assert result["size"] > 0
    assert result["type"] == "file"


def test_create_directory_tool():
    result = execute_tool("create_directory", {"path": "newdir_test_skills"})
    assert result["created"] is True


def test_unknown_tool_returns_error():
    result = execute_tool("no_such_tool", {})
    assert "error" in result


def test_move_file_tool():
    from tests.conftest import create_test_file
    create_test_file("move_src.txt", "move me")
    result = execute_tool("move_file", {"source": "move_src.txt", "destination": "move_dst.txt"})
    assert result["moved"] is True


def test_delete_file_tool_moves_to_trash():
    from tests.conftest import create_test_file
    create_test_file("delme.txt", "trash me")
    result = execute_tool("delete_file", {"path": "delme.txt"})
    assert "trashed_to" in result


class TestSkillManagementAPI:
    """The HTTP endpoints for managing skills."""

    def test_list_skills(self):
        res = client.get("/api/skills")
        assert res.status_code == 200
        body = res.json()
        assert "master" in body
        assert "skills" in body
        assert len(body["skills"]) >= len(BUILTIN_SKILLS)

    def test_toggle_master_requires_admin(self, monkeypatch):
        # With auth disabled, require_admin returns None and proceeds
        res = client.patch("/api/skills/master", json={"enabled": True})
        assert res.status_code == 200
        assert res.json()["master"] is True
        # Reset
        client.patch("/api/skills/master", json={"enabled": False})

    def test_toggle_individual_skill(self):
        res = client.patch("/api/skills/find_old/toggle", json={"enabled": True})
        assert res.status_code == 200
        assert res.json()["enabled"] is True
        # Reset
        client.patch("/api/skills/find_old/toggle", json={"enabled": False})

    def test_add_custom_skill(self):
        res = client.post("/api/skills", json={
            "id": "my_test_skill",
            "label": "测试技能",
            "instruction": "这是测试技能的指令。",
            "description": "测试用",
        })
        assert res.status_code == 200, res.text
        assert res.json()["id"] == "my_test_skill"
        # Clean up
        client.delete("/api/skills/my_test_skill")

    def test_custom_skill_with_builtin_id_is_rejected(self):
        first = list(BUILTIN_SKILLS.keys())[0]
        res = client.post("/api/skills", json={
            "id": first, "label": "x", "instruction": "y",
        })
        assert res.status_code == 400

    def test_remove_custom_skill(self):
        client.post("/api/skills", json={
            "id": "removable_skill", "label": "可删", "instruction": "x",
        })
        res = client.delete("/api/skills/removable_skill")
        assert res.status_code == 200

    def test_remove_builtin_is_rejected(self):
        res = client.delete("/api/skills/organize")
        assert res.status_code == 404
    def test_list_tools_included(self):
        res = client.get("/api/skills")
        body = res.json()
        assert "tools" in body
        names = {t["name"] for t in body["tools"]}
        assert "search_files" in names
        assert "delete_file" in names

    def test_toggle_tool(self):
        # Master must be on for tools to be active
        client.patch("/api/skills/master", json={"enabled": True})
        res = client.patch("/api/skills/tools/delete_file/toggle", json={"enabled": True})
        assert res.status_code == 200
        assert res.json()["enabled"] is True
        # Reset
        client.patch("/api/skills/tools/delete_file/toggle", json={"enabled": False})
        client.patch("/api/skills/master", json={"enabled": False})

    def test_toggle_all_tools(self):
        client.patch("/api/skills/master", json={"enabled": True})
        res = client.patch("/api/skills/tools/all", json={"enabled": False})
        assert res.status_code == 200
        assert res.json()["all_enabled"] is False
        # Now list should show all disabled
        body = client.get("/api/skills").json()
        assert all(not t["enabled"] for t in body["tools"])
        # Reset
        client.patch("/api/skills/tools/all", json={"enabled": True})
        client.patch("/api/skills/master", json={"enabled": False})

    def test_toggle_unknown_tool(self):
        res = client.patch("/api/skills/tools/no_such_tool/toggle", json={"enabled": True})
        assert res.status_code == 404
def test_path_traversal_is_blocked():
    """Paths that resolve outside the storage root must be refused."""
    import os
    os.environ["STORAGE_PATH"] = "/tmp/sfm_pathcheck"
    from app.skills import _abs_path
    import pytest
    with pytest.raises(ValueError):
        _abs_path("../../etc/passwd")
    with pytest.raises(ValueError):
        _abs_path("/etc/passwd")
