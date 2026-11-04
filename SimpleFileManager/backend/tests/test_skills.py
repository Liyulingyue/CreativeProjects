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
    from app.skills import ALWAYS_ON_TOOL_NAMES
    names = {t["function"]["name"] for t in reg.get_active_tools()}
    assert names == set(ALWAYS_ON_TOOL_NAMES)
    reg.set_all_tools(True)
    names = {t["function"]["name"] for t in reg.get_active_tools()}
    from app.skills import ALL_TOOL_NAMES, BULK_TOGGLE_EXCLUDED
    # The bulk switch leaves command execution off on purpose.
    assert names == set(ALL_TOOL_NAMES) - set(BULK_TOGGLE_EXCLUDED)


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
        # Toggleable tools are disabled; always-on meta tools stay available.
        body = client.get("/api/skills").json()
        assert all(not t["enabled"] or t["always_on"] for t in body["tools"])
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


def test_executable_defaults_are_conservative():
    """Destructive tools default to plan-only; read/info tools run directly."""
    reg = get_skill_registry()
    reg.set_master(True)
    reg.set_all_tools(True)
    states = {t["name"]: t for t in reg.all_tool_states()}
    assert states["write_file"]["direct_executable"] is True
    assert states["create_directory"]["direct_executable"] is True
    assert states["file_info"]["direct_executable"] is True
    assert states["move_file"]["direct_executable"] is False
    assert states["delete_file"]["direct_executable"] is False
    reg.set_master(False)
    reg.set_all_tools(True)


def test_executable_toggle_and_visibility_interaction():
    reg = get_skill_registry()
    reg.set_master(True)
    reg.set_tool_enabled("delete_file", True)
    assert "delete_file" in reg.plan_only_tool_names()
    reg.set_tool_executable("delete_file", True)
    assert "delete_file" not in reg.plan_only_tool_names()
    assert reg.tool_is_directly_executable("delete_file") is True
    # Hiding the tool also makes it non-executable.
    reg.set_tool_enabled("delete_file", False)
    assert reg.tool_is_directly_executable("delete_file") is False
    # Reset
    reg.set_tool_executable("delete_file", False)
    reg.set_master(False)


def test_agent_guard_rejects_hidden_tool():
    from app.routers.agent import execute_tool as agent_execute_tool
    reg = get_skill_registry()
    reg.set_master(False)
    result = agent_execute_tool("write_file", {"path": "x.txt", "content": "x"})
    assert result["success"] is False
    assert "未启用" in result["error"]


def test_agent_guard_rejects_plan_only_tool():
    from app.routers.agent import execute_tool as agent_execute_tool
    reg = get_skill_registry()
    reg.set_master(True)
    reg.set_tool_enabled("delete_file", True)
    reg.set_tool_executable("delete_file", False)
    result = agent_execute_tool("delete_file", {"path": "whatever.txt"})
    assert result["success"] is False
    assert "仅计划执行" in result["error"]
    reg.set_master(False)


def test_agent_runs_directly_executable_tool():
    from app.routers.agent import execute_tool as agent_execute_tool
    from tests.conftest import create_test_file
    reg = get_skill_registry()
    reg.set_master(True)
    reg.set_tool_enabled("write_file", True)
    reg.set_tool_executable("write_file", True)
    create_test_file("guard_dir/keep.txt", "old")
    result = agent_execute_tool("write_file", {"path": "guard_dir/keep.txt", "content": "new"})
    assert result.get("success") is True, result
    reg.set_master(False)


# ---- file-based skills (SKILL.md + resources) ----

def _make_zip(files: dict[str, bytes]) -> bytes:
    import io
    import zipfile
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as zf:
        for name, data in files.items():
            zf.writestr(name, data)
    return buf.getvalue()


_SKILL_MD = """---
name: Test Zip Skill
description: a test skill
---
# Body
Use the reference file.
"""


def test_import_zip_and_read_skill():
    from app.skills import (
        import_zip_skill, _handle_read_skill, _handle_list_skill_files,
    )
    reg = get_skill_registry()
    data = _make_zip({
        "SKILL.md": _SKILL_MD.encode(),
        "refs/note.txt": b"hello note",
    })
    try:
        info = import_zip_skill(data, "test_zip_skill")
        assert info["id"] == "test_zip_skill"
        assert info["description"] == "a test skill"
        assert "SKILL.md" in info["files"]
        assert "refs/note.txt" in info["files"]

        body = _handle_read_skill("test_zip_skill")
        assert "Body" in body["content"]
        note = _handle_read_skill("test_zip_skill", "refs/note.txt")
        assert note["content"] == "hello note"
        listing = _handle_list_skill_files("test_zip_skill")
        assert "refs/note.txt" in listing["files"]

        assert "test_zip_skill" in {s["id"] for s in reg.all_skills()}

        reg.set_master(True)
        reg.set_enabled("test_zip_skill", True)
        instr = reg.get_active_instructions()
        # Progressive disclosure: metadata + read_skill hint, not the body.
        assert "read_skill" in instr
        assert "Use the reference file" not in instr
    finally:
        reg.set_master(False)
        reg.remove_custom("test_zip_skill")


def test_import_nested_zip_uses_inner_folder():
    from app.skills import import_zip_skill
    reg = get_skill_registry()
    data = _make_zip({
        "mypack/SKILL.md": _SKILL_MD.encode(),
        "mypack/a/b.txt": b"b",
    })
    try:
        info = import_zip_skill(data, "test_nested_skill")
        assert "a/b.txt" in info["files"]
    finally:
        reg.remove_custom("test_nested_skill")


def test_import_files_skill():
    from app.skills import import_files_skill
    reg = get_skill_registry()
    try:
        info = import_files_skill([
            ("pack/SKILL.md", _SKILL_MD.encode()),
            ("pack/ref/a.txt", b"a"),
        ], "test_files_skill")
        assert info["id"] == "test_files_skill"
        assert "ref/a.txt" in info["files"]
    finally:
        reg.remove_custom("test_files_skill")


def test_import_requires_skill_md():
    from app.skills import import_zip_skill
    import pytest
    with pytest.raises(ValueError):
        import_zip_skill(_make_zip({"readme.txt": b"x"}), "no_md_skill")


def test_read_skill_blocks_traversal():
    from app.skills import import_zip_skill, execute_tool
    reg = get_skill_registry()
    try:
        import_zip_skill(_make_zip({"SKILL.md": _SKILL_MD.encode()}), "test_trav_skill")
        result = execute_tool("read_skill", {"skill_id": "test_trav_skill", "file": "../../etc/passwd"})
        assert result["success"] is False
    finally:
        reg.remove_custom("test_trav_skill")


def test_import_zip_via_api():
    reg = get_skill_registry()
    try:
        data = _make_zip({"SKILL.md": _SKILL_MD.encode()})
        res = client.post(
            "/api/skills/import/zip",
            files={"file": ("skill.zip", data, "application/zip")},
            data={"skill_id": "test_api_zip_skill"},
        )
        assert res.status_code == 200, res.text
        assert res.json()["id"] == "test_api_zip_skill"
    finally:
        reg.remove_custom("test_api_zip_skill")


# ---- editing custom skills ----

def test_update_inline_skill():
    reg = get_skill_registry()
    reg.add_custom("upd_inline", "Old", "old instruction", "old desc")
    try:
        out = reg.update_custom("upd_inline", label="New", description="d2",
                                instruction="new instruction")
        assert out["label"] == "New"
        assert out["instruction"] == "new instruction"
        skill = next(s for s in reg.all_skills() if s["id"] == "upd_inline")
        assert skill["label"] == "New"
        assert skill["description"] == "d2"
    finally:
        reg.remove_custom("upd_inline")


def test_update_file_skill_keeps_resources():
    from app.skills import import_zip_skill, discover_dir_skills
    reg = get_skill_registry()
    try:
        import_zip_skill(
            _make_zip({"SKILL.md": _SKILL_MD.encode(), "refs/a.txt": b"a"}),
            "upd_file_skill",
        )
        reg.update_custom("upd_file_skill", label="New Name",
                          description="new desc", instruction="NEW BODY")
        info = discover_dir_skills()["upd_file_skill"]
        assert info["label"] == "New Name"
        assert info["description"] == "new desc"
        assert "NEW BODY" in info["instruction"]
        assert "refs/a.txt" in info["files"]
    finally:
        reg.remove_custom("upd_file_skill")


def test_update_builtin_is_rejected():
    import pytest
    with pytest.raises(ValueError):
        get_skill_registry().update_custom("organize", label="x")


def test_update_skill_via_api():
    reg = get_skill_registry()
    client.post("/api/skills", json={
        "id": "upd_api_skill", "label": "Old", "instruction": "old",
    })
    try:
        res = client.patch("/api/skills/upd_api_skill", json={
            "label": "New", "instruction": "new body",
        })
        assert res.status_code == 200, res.text
        body = res.json()
        assert body["label"] == "New"
        assert body["instruction"] == "new body"
    finally:
        reg.remove_custom("upd_api_skill")


# ---- skill resource files (nested folders) ----

def test_resource_files_support_nested_folders():
    from app.skills import (
        import_zip_skill, discover_dir_skills, read_skill_resource,
        write_skill_resource, add_skill_resources, delete_skill_resource,
    )
    reg = get_skill_registry()
    try:
        import_zip_skill(_make_zip({
            "SKILL.md": _SKILL_MD.encode(),
            "refs/deep/a.txt": b"hello",
        }), "res_folder_skill")
        info = discover_dir_skills()["res_folder_skill"]
        assert "refs/deep/a.txt" in info["files"]

        assert read_skill_resource("res_folder_skill", "refs/deep/a.txt")["content"] == "hello"
        write_skill_resource("res_folder_skill", "refs/deep/a.txt", "updated")
        assert read_skill_resource("res_folder_skill", "refs/deep/a.txt")["content"] == "updated"

        add_skill_resources("res_folder_skill", [
            ("refs/new/b.txt", b"B"),
            ("notes/top.md", b"# t"),
        ])
        info = discover_dir_skills()["res_folder_skill"]
        assert "refs/new/b.txt" in info["files"]
        assert "notes/top.md" in info["files"]

        delete_skill_resource("res_folder_skill", "refs/deep/a.txt")
        assert "refs/deep/a.txt" not in discover_dir_skills()["res_folder_skill"]["files"]
        assert "notes/top.md" in discover_dir_skills()["res_folder_skill"]["files"]
    finally:
        reg.remove_custom("res_folder_skill")


def test_resource_endpoints_reject_skill_md_and_traversal():
    from app.skills import import_zip_skill, read_skill_resource, write_skill_resource
    reg = get_skill_registry()
    try:
        import_zip_skill(_make_zip({"SKILL.md": _SKILL_MD.encode()}), "res_guard_skill")
        with pytest.raises(ValueError):
            read_skill_resource("res_guard_skill", "SKILL.md")
        with pytest.raises(ValueError):
            write_skill_resource("res_guard_skill", "SKILL.md", "x")
        with pytest.raises(ValueError):
            read_skill_resource("res_guard_skill", "../outside.txt")
        with pytest.raises(ValueError):
            write_skill_resource("res_guard_skill", "a/../../outside.txt", "x")
    finally:
        reg.remove_custom("res_guard_skill")


def test_resource_api_upload_read_edit_delete():
    from app.skills import import_zip_skill
    reg = get_skill_registry()
    try:
        import_zip_skill(_make_zip({"SKILL.md": _SKILL_MD.encode()}), "res_api_skill")
        res = client.post("/api/skills/res_api_skill/files", files=[
            ("files", ("refs/a.txt", b"hi", "text/plain")),
            ("files", ("sub/b.txt", b"yo", "text/plain")),
        ])
        assert res.status_code == 200, res.text
        assert set(res.json()["files"]) == {"refs/a.txt", "sub/b.txt"}

        res = client.get("/api/skills/res_api_skill/files/refs/a.txt")
        assert res.status_code == 200, res.text
        assert res.json()["content"] == "hi"

        res = client.put("/api/skills/res_api_skill/files/refs/a.txt",
                         json={"content": "edited"})
        assert res.status_code == 200, res.text
        assert client.get("/api/skills/res_api_skill/files/refs/a.txt").json()["content"] == "edited"

        assert client.delete("/api/skills/res_api_skill/files/sub/b.txt").status_code == 200
        assert client.get("/api/skills/res_api_skill/files/sub/b.txt").status_code == 400
        assert client.get("/api/skills/res_api_skill/files/SKILL.md").status_code == 400
    finally:
        reg.remove_custom("res_api_skill")


# ---- manual skills grow a file tree on first upload ----

def test_inline_skill_promotes_to_file_on_upload():
    reg = get_skill_registry()
    reg.add_custom("promo_skill", "Promo", "original method", "desc")
    try:
        res = client.post("/api/skills/promo_skill/files", files=[
            ("files", ("refs/a.txt", b"hello", "text/plain")),
        ])
        assert res.status_code == 200, res.text
        assert res.json()["files"] == ["refs/a.txt"]

        skills = {s["id"]: s for s in client.get("/api/skills").json()["skills"]}
        skill = skills["promo_skill"]
        assert skill["source"] == "file"
        assert skill["instruction"].strip() == "original method"
        assert "SKILL.md" in skill["files"]
        assert "refs/a.txt" in skill["files"]
        assert "promo_skill" not in reg._custom
    finally:
        reg.remove_custom("promo_skill")


def test_inline_skill_promotes_on_write_resource():
    from app.skills import write_skill_resource, read_skill_resource
    reg = get_skill_registry()
    reg.add_custom("promo_write_skill", "Promo Write", "method", "")
    try:
        write_skill_resource("promo_write_skill", "notes/a.md", "# hi")
        assert read_skill_resource("promo_write_skill", "notes/a.md")["content"] == "# hi"
        assert "promo_write_skill" not in reg._custom
    finally:
        reg.remove_custom("promo_write_skill")


def test_download_skill_file_nested_and_skill_md():
    from app.skills import import_zip_skill
    reg = get_skill_registry()
    try:
        import_zip_skill(_make_zip({
            "SKILL.md": _SKILL_MD.encode(),
            "refs/deep/a.txt": b"download me",
        }), "dl_skill")
        res = client.get("/api/skills/dl_skill/download/refs/deep/a.txt")
        assert res.status_code == 200, res.text
        assert res.content == b"download me"
        assert "attachment" in res.headers["content-disposition"]

        res = client.get("/api/skills/dl_skill/download/SKILL.md")
        assert res.status_code == 200
        assert b"Test Zip Skill" in res.content

        assert client.get("/api/skills/dl_skill/download/../x").status_code in (400, 404)
    finally:
        reg.remove_custom("dl_skill")
