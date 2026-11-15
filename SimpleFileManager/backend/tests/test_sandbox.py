"""Tests for the sandboxed command runner and the run_command tool."""

import os
import shutil
from pathlib import Path

import pytest

from app.sandbox import (
    SandboxError,
    resolve_cwd,
    run,
    sandbox_available,
)

pytestmark = pytest.mark.skipif(
    not sandbox_available(),
    reason="bwrap is not available on this machine",
)


def _root():
    from app.deps import DATA_DIR, get_storage_root
    return get_storage_root().resolve(), DATA_DIR.resolve()


def test_runs_a_command_and_captures_output():
    root, _ = _root()
    cwd = resolve_cwd(root, _root()[1], "")
    result = run("echo hello; echo oops >&2", cwd, timeout=20)
    assert result["returncode"] == 0
    assert result["stdout"].strip() == "hello"
    assert result["stderr"].strip() == "oops"
    assert result["timed_out"] is False


def test_non_zero_exit_is_reported_not_raised():
    root, _ = _root()
    cwd = resolve_cwd(root, _root()[1], "")
    result = run("exit 7", cwd, timeout=20)
    assert result["returncode"] == 7


def test_environment_is_cleared():
    root, _ = _root()
    cwd = resolve_cwd(root, _root()[1], "")
    os.environ["SFM_TEST_SECRET"] = "leak-me"
    try:
        result = run("echo secret=${SFM_TEST_SECRET:-none}", cwd, timeout=20)
    finally:
        os.environ.pop("SFM_TEST_SECRET", None)
    assert result["stdout"].strip() == "secret=none"


def test_manager_directory_is_hidden():
    root, data_dir = _root()
    cwd = resolve_cwd(root, data_dir, "")
    result = run(f"ls {data_dir} | wc -l", cwd, timeout=20)
    assert result["stdout"].strip() == "0"


def test_cannot_write_outside_the_storage_root():
    root, data_dir = _root()
    cwd = resolve_cwd(root, data_dir, "")
    # The root of the sandbox is a throwaway tmpfs, so a write there
    # succeeds but leaves nothing behind on the host.
    result = run("touch /escape.txt && echo written", cwd, timeout=20)
    assert result["stdout"].strip() == "written"
    assert not os.path.exists("/escape.txt")


def test_timeout_kills_the_command():
    root, data_dir = _root()
    cwd = resolve_cwd(root, data_dir, "")
    result = run("sleep 30", cwd, timeout=2)
    assert result["timed_out"] is True


def test_cwd_must_be_inside_the_storage_root():
    root, data_dir = _root()
    with pytest.raises(SandboxError):
        resolve_cwd(root, data_dir, "/etc")
    with pytest.raises(SandboxError):
        resolve_cwd(root, data_dir, "../..")
    with pytest.raises(SandboxError):
        resolve_cwd(root, data_dir, "no-such-dir")


def test_cwd_cannot_be_the_manager_directory():
    root, data_dir = _root()
    rel = str(data_dir.relative_to(root))
    with pytest.raises(SandboxError):
        resolve_cwd(root, data_dir, rel)


def test_command_length_is_capped():
    root, data_dir = _root()
    cwd = resolve_cwd(root, data_dir, "")
    with pytest.raises(SandboxError):
        run("x" * 9000, cwd, timeout=20)


def test_run_command_tool_is_disabled_by_default():
    from app.skills import get_skill_registry
    reg = get_skill_registry()
    state = next(t for t in reg.all_tool_states() if t["name"] == "run_command")
    assert state["danger"] == "high"
    assert state["enabled"] is False
    # high danger defaults to plan-only, so even when switched on the
    # model cannot call it on its own.
    assert state["direct_executable"] is False


def test_run_command_tool_executes_through_the_registry():
    from app.skills import execute_tool
    result = execute_tool("run_command", {"command": "echo tool-ok"})
    assert result["success"] is True
    assert "tool-ok" in result["stdout"]


def test_run_command_tool_rejects_a_bad_cwd():
    from app.skills import execute_tool
    result = execute_tool("run_command", {"command": "echo hi", "cwd": "/etc"})
    assert result["success"] is False
    assert "error" in result


# ---- the run action inside a plan ----

def test_validate_run_action_requires_a_command():
    from app.deps import validate_plan_action
    assert validate_plan_action({"action_type": "run", "command": ""})
    assert validate_plan_action({"action_type": "run"}) is not None
    assert validate_plan_action({"action_type": "run", "command": "echo hi"}) is None
    assert validate_plan_action({"action_type": "run", "command": "echo hi", "cwd": "/etc"})


def test_plan_with_a_run_action_executes_after_approval():
    from conftest import client
    create = client.post("/api/plans", json={
        "title": "run a command",
        "summary": "",
        "source": "manual",
        "actions": [
            {"action_type": "run", "command": "echo plan-ok", "cwd": "", "reason": "test"},
        ],
    })
    assert create.status_code == 200, create.text
    plan_id = create.json()["id"]
    action = create.json()["actions"][0]
    assert action["action_type"] == "run"
    assert action["command"] == "echo plan-ok"

    assert client.post(f"/api/plans/{plan_id}/approve").json()["status"] == "approved"
    executed = client.post(f"/api/plans/{plan_id}/execute")
    assert executed.status_code == 200, executed.text
    body = executed.json()
    assert body["status"] == "executed"
    assert "plan-ok" in body["actions"][0]["result"]

    log = client.get(f"/api/plans/{plan_id}/log").json()["log"]
    assert log and log[0]["action_type"] == "run"


def test_failing_run_action_is_recorded_as_failed():
    from conftest import client
    create = client.post("/api/plans", json={
        "title": "failing command",
        "summary": "",
        "source": "manual",
        "actions": [{"action_type": "run", "command": "exit 5", "reason": "test"}],
    })
    plan_id = create.json()["id"]
    client.post(f"/api/plans/{plan_id}/approve")
    executed = client.post(f"/api/plans/{plan_id}/execute")
    assert executed.json()["status"] == "failed"
    assert "5" in (executed.json()["actions"][0]["result"] or "")


def test_run_command_tool_is_admin_only(monkeypatch):
    import importlib
    agent_mod = importlib.import_module("app.routers.agent")
    monkeypatch.setattr(agent_mod, "current_user", lambda: {"role": "user", "id": "u2"})
    result = agent_mod.execute_tool("run_command", {"command": "echo hi"})
    assert result["success"] is False
    assert "管理员" in result["error"]

    submitted = agent_mod.tool_submit_plan(
        title="t", actions=[{"action_type": "run", "command": "echo hi"}])
    assert submitted["success"] is False
    assert "管理员" in submitted["error"]


def test_run_command_tool_works_for_admin(monkeypatch):
    import importlib
    from app.skills import get_skill_registry
    agent_mod = importlib.import_module("app.routers.agent")
    reg = get_skill_registry()
    was_master = reg.master_enabled
    reg.set_master(True)
    reg.set_tool_enabled("run_command", True)
    reg.set_tool_executable("run_command", True)
    monkeypatch.setattr(agent_mod, "current_user", lambda: {"role": "admin", "id": "u1"})
    try:
        result = agent_mod.execute_tool("run_command", {"command": "echo admin-ok"})
        assert result["success"] is True
        assert "admin-ok" in result["stdout"]
    finally:
        reg.set_tool_enabled("run_command", False)
        reg.set_tool_executable("run_command", False)
        reg.set_master(was_master)


def test_run_command_tool_is_hidden_until_it_is_switched_on():
    import importlib
    from app.skills import get_skill_registry
    agent_mod = importlib.import_module("app.routers.agent")
    reg = get_skill_registry()
    was_master = reg.master_enabled
    reg.set_master(True)
    reg.set_tool_enabled("run_command", False)
    try:
        result = agent_mod.execute_tool("run_command", {"command": "echo hidden"})
        assert result["success"] is False
        assert "未启用" in result.get("error", "")
    finally:
        reg.set_master(was_master)


def test_plan_only_run_command_cannot_be_called_directly():
    import importlib
    from app.skills import get_skill_registry
    agent_mod = importlib.import_module("app.routers.agent")
    reg = get_skill_registry()
    was_master = reg.master_enabled
    reg.set_master(True)
    reg.set_tool_enabled("run_command", True)
    reg.set_tool_executable("run_command", False)
    try:
        result = agent_mod.execute_tool("run_command", {"command": "echo nope"})
        assert result["success"] is False
        assert "计划" in result.get("error", "")
        assert "run_command" in reg.plan_only_tool_names()
    finally:
        reg.set_tool_enabled("run_command", False)
        reg.set_master(was_master)


def test_plan_creation_with_run_is_admin_only(monkeypatch):
    import importlib
    from fastapi import HTTPException
    plans_mod = importlib.import_module("app.routers.plans")
    monkeypatch.setattr(plans_mod, "current_user", lambda: {"role": "user", "id": "u2"})

    class _Action:
        action_type = "run"

    class _Req:
        actions = [_Action()]
        title = "t"
        summary = ""
        source = "manual"

    try:
        plans_mod.create_plan(_Req())
    except HTTPException as e:
        assert e.status_code == 403
    else:
        raise AssertionError("expected a 403 for a non-admin")


def test_bulk_toggle_leaves_the_command_tool_alone():
    from app.skills import get_skill_registry
    reg = get_skill_registry()
    was_master = reg.master_enabled
    reg.set_master(True)
    reg.set_tool_enabled("run_command", False)
    try:
        reg.set_all_tools(True)
        state = next(t for t in reg.all_tool_states() if t["name"] == "run_command")
        assert state["enabled"] is False
        # ...while an ordinary skill tool does follow the bulk switch.
        other = next(t for t in reg.all_tool_states() if t["name"] == "write_file")
        assert other["enabled"] is True
    finally:
        reg.set_all_tools(False)
        reg.set_master(was_master)


def test_the_command_cannot_take_the_machine_with_it():
    """One command must not be able to take the whole box.

    This machine has 30 GB of RAM and 8 GB of swap. A python that asks
    for twenty gigabytes of it would starve the server, and the killer
    does not know that the backend matters more than the command. The
    limit is on the address space, which the kernel can refuse.
    """
    root, _ = _root()
    cwd = resolve_cwd(root, _root()[1], "")
    result = run(
        "python3 -c \"x=bytearray(8*1024*1024*1024)\"",
        cwd, timeout=60,
    )
    assert result["returncode"] != 0, "8 GB was allocated without complaint"
    assert "MemoryError" in result["stderr"] or "Cannot allocate" in result["stderr"], \
        result["stderr"][:200]


def test_an_ordinary_command_still_fits_in_the_memory():
    """The limit must not get in the way of real work."""
    root, _ = _root()
    cwd = resolve_cwd(root, _root()[1], "")
    result = run("python3 -c 'print(1+1)'", cwd, timeout=30)
    assert result["returncode"] == 0
    assert result["stdout"].strip() == "2"


def test_the_command_has_no_network():
    """The sandbox has no network at all."""
    root, _ = _root()
    cwd = resolve_cwd(root, _root()[1], "")
    result = run(
        "python3 - <<'EOF'\n"
        "import socket\n"
        "try:\n"
        "    socket.create_connection(('1.1.1.1', 53), timeout=3)\n"
        "    print('REACHED')\n"
        "except Exception as e:\n"
        "    print('NO NETWORK:', type(e).__name__)\n"
        "EOF",
        cwd, timeout=30,
    )
    assert "REACHED" not in result["stdout"], result["stdout"][:200]


def test_the_system_directories_are_read_only():
    """A command may write to the storage root and nowhere else."""
    root, _ = _root()
    cwd = resolve_cwd(root, _root()[1], "")
    result = run("touch /etc/sfm-should-not-be-here 2>&1; ls /etc/sfm-should-not-be-here 2>&1",
                 cwd, timeout=30)
    assert "No such file" in result["stdout"] or "Read-only" in result["stdout"], \
        result["stdout"][:200]
    assert not Path("/etc/sfm-should-not-be-here").exists()


def test_a_command_cannot_write_an_endless_file():
    """The size of a file a command writes is limited."""
    root, _ = _root()
    cwd = resolve_cwd(root, _root()[1], "")
    # Written in pieces: one big allocation would be refused by the
    # memory limit before the file limit ever came up.
    result = run(
        "python3 -c \"\n"
        "f=open('big.bin','wb')\n"
        "chunk=b'x'*(1024*1024)\n"
        "try:\n"
        "    for i in range(12000): f.write(chunk)\n"
        "except Exception as e:\n"
        "    print('STOPPED:', e)\n"
        "\" 2>&1; echo done",
        cwd, timeout=120,
    )
    big = cwd / "big.bin"
    try:
        assert big.exists() and big.stat().st_size <= 5 * 1024 * 1024 * 1024, \
            "写出来的文件太大了"
    finally:
        big.unlink(missing_ok=True)


def test_the_output_is_capped():
    """A command that prints for ever does not fill the page."""
    root, _ = _root()
    cwd = resolve_cwd(root, _root()[1], "")
    result = run("python3 -c \"print('x'*20000000)\"", cwd, timeout=60)
    assert len(result["stdout"]) <= 64 * 1024 + 10, len(result["stdout"])
    assert result["truncated"] is True
