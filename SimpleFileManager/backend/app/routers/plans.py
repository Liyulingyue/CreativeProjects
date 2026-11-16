"""Plan & Approve engine.

Plans are created by the agent (or from organizer suggestions). They are
executed only through the /execute endpoint after explicit user approval.
Every executed action is recorded in an execution log.
"""

import logging
import os
import shutil
from pathlib import Path

from fastapi import APIRouter, HTTPException

from ..deps import (state, resolve_under_root, PathOutsideRoot, PathNotAllowed,
                    validate_plan_action, current_user, get_storage_root, session_owner)
from ..models import AgentPlan, CreatePlanRequest

log = logging.getLogger(__name__)

plans = APIRouter(prefix="/api/plans", tags=["plans"])


@plans.get("")
def list_plans(status: str = None, limit: int = 50) -> dict:
    return {"plans": [p.model_dump() for p in state.get_plan_store().list_plans(status, limit)]}


@plans.get("/{plan_id}")
def get_plan(plan_id: str) -> AgentPlan:
    plan = state.get_plan_store().get_plan(plan_id)
    if not plan:
        raise HTTPException(status_code=404, detail="找不到这份计划")
    return plan


@plans.get("/{plan_id}/log")
def get_plan_log(plan_id: str) -> dict:
    if not state.get_plan_store().get_plan(plan_id):
        raise HTTPException(status_code=404, detail="找不到这份计划")
    return {"log": state.get_plan_store().get_log(plan_id)}


def _is_admin() -> bool:
    """Whether the person asking may plan a shell command.

    An open installation (no accounts) is a single-user install, so
    there is nobody to tell apart.
    """
    user = current_user()
    if user is None:
        return True
    return user.get("role") == "admin"


@plans.post("")
def create_plan(req: CreatePlanRequest) -> AgentPlan:
    if not req.actions:
        raise HTTPException(status_code=400, detail="计划里至少要有一个操作")
    if any(a.action_type == "run" for a in req.actions) and not _is_admin():
        raise HTTPException(status_code=403, detail="只有管理员可以创建命令执行计划")
    for a in req.actions:
        error = validate_plan_action(a.model_dump())
        if error:
            raise HTTPException(status_code=400, detail=error)
    return state.get_plan_store().create_plan(
        title=req.title,
        summary=req.summary,
        source=req.source,
        actions=[a.model_dump() for a in req.actions],
        owner=session_owner(),
    )


@plans.post("/{plan_id}/approve")
def approve_plan(plan_id: str) -> AgentPlan:
    plan = _get_plan_or_404(plan_id)
    # A plan that failed or partially failed can be re-approved for a retry.
    # That is the normal recovery path: the cause was usually a path that
    # went stale, and the executor now resolves those. Re-running only
    # touches the actions that did not already succeed.
    if plan.status not in ("pending", "rejected", "failed", "executed_with_errors"):
        raise HTTPException(status_code=400, detail=f"计划现在是 {plan.status}，不能批准")
    updated = state.get_plan_store().update_status(plan_id, "approved", decided=True)
    assert updated is not None
    return updated


@plans.post("/{plan_id}/reject")
def reject_plan(plan_id: str) -> AgentPlan:
    plan = _get_plan_or_404(plan_id)
    if plan.status not in ("pending", "approved"):
        raise HTTPException(status_code=400, detail=f"Plan is {plan.status}, cannot reject")
    updated = state.get_plan_store().update_status(plan_id, "rejected", decided=True)
    assert updated is not None
    return updated


@plans.post("/{plan_id}/execute")
def execute_plan(plan_id: str) -> AgentPlan:
    plan = _get_plan_or_404(plan_id)
    if plan.status != "approved":
        raise HTTPException(status_code=400, detail=f"计划要先批准才能执行（现在是 {plan.status}）")

    if any(a.action_type == "run" for a in plan.actions) and not _is_admin():
        raise HTTPException(status_code=403, detail="只有管理员可以执行命令计划")

    store = state.get_plan_store()
    succeeded, failed, skipped = 0, 0, 0

    for action in plan.actions:
        # On a retry, actions that already succeeded are left alone. Re-running
        # them would fail on "target already exists" and would mask the real
        # problem, which is the actions that did not go through.
        if action.status == "done":
            skipped += 1
            continue
        try:
            message = _execute_action(action.action_type, action.source_path, action.target_path,
                                      action.command, action.cwd)
            store.update_action(action.id, "done", message)
            store.add_log(plan_id, action.id, action.action_type,
                          action.source_path or action.command, action.target_path, True, message)
            succeeded += 1
        except Exception as e:
            message = str(e)
            store.update_action(action.id, "failed", message)
            store.add_log(plan_id, action.id, action.action_type,
                          action.source_path or action.command, action.target_path, False, message)
            failed += 1

    if failed == 0 and succeeded > 0:
        final_status = "executed"
    elif succeeded > 0:
        final_status = "executed_with_errors"
    elif skipped > 0 and failed == 0:
        # Everything had already succeeded from an earlier attempt.
        final_status = "executed"
    else:
        final_status = "failed"
    updated = store.update_status(plan_id, final_status, executed=True)
    assert updated is not None
    return updated


@plans.delete("/{plan_id}")
def delete_plan(plan_id: str) -> dict:
    _get_plan_or_404(plan_id)
    state.get_plan_store().delete_plan(plan_id)
    return {"success": True}


def _get_plan_or_404(plan_id: str) -> AgentPlan:
    plan = state.get_plan_store().get_plan(plan_id)
    if not plan:
        raise HTTPException(status_code=404, detail="找不到这份计划")
    return plan


def _resolve_existing_source(source: str) -> str:
    """Return the real path of a source that has moved since the plan was made.

    Plans are drafted from a snapshot of the file system, so a source path
    can go stale between submission and approval. When the recorded path is
    dead but exactly one file with that name exists elsewhere, use that one
    — the user's intent ("move the novel") is unambiguous. When the name is
    ambiguous we leave the path alone so the action fails loudly rather than
    picking arbitrarily.
    """
    try:
        if resolve_under_root(source).exists():
            return source
    except PathOutsideRoot:
        return source
    name = Path(source).name
    if not name:
        return source
    root = get_storage_root()
    matches: list[str] = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames
                       if not d.startswith(".") and d != ".simplefilemanager"]
        for fn in filenames:
            if fn == name:
                try:
                    rel = (Path(dirpath) / fn).relative_to(root).as_posix()
                except ValueError:
                    continue
                matches.append(rel)
                if len(matches) > 1:
                    return source
    return matches[0] if len(matches) == 1 else source


def _execute_action(action_type: str, source: str, target: str,
                    command: str = "", cwd: str = "") -> str:
    """Execute a single approved action. Paths are relative to the storage root.

    Raises on any failure so the caller can record it.
    """
    if action_type == "run":
        return _execute_run(command, cwd)

    if action_type == "create_folder":
        if not target:
            raise ValueError("create_folder requires target_path")
        dest = _safe(target, "write")
        # Idempotent: a re-run of a partially-failed plan would otherwise
        # fail this action because the folder survived the first attempt.
        if dest.is_dir():
            return f"文件夹已经在了：{target}"
        if dest.exists():
            raise FileExistsError(f"那里已经有个不是文件夹的东西：{target}")
        dest.mkdir(parents=True, exist_ok=False)
        return f"已建立文件夹 {target}"

    if action_type == "delete":
        if not source:
            raise ValueError("delete requires source_path")
        source = _resolve_existing_source(source)
        src = _safe(source, "write")
        if not src.exists():
            raise FileNotFoundError(f"找不到这个文件：{source}")
        # To the bin, not to nowhere. A tidy-up plan that was approved
        # once should not be the last chance to get a file back.
        from ..trash import TrashError, TrashService

        try:
            TrashService(Path(get_storage_root())).trash(src)
        except TrashError as e:
            raise ValueError(str(e)) from e
        return f"已把 {source} 移进回收站"

    if action_type in ("move", "rename"):
        if not source or not target:
            raise ValueError(f"{action_type} requires source_path and target_path")
        source = _resolve_existing_source(source)
        src = _safe(source, "write")
        dest = _safe(target, "write")
        if not src.exists():
            raise FileNotFoundError(f"找不到这个文件：{source}")
        if dest.exists():
            raise FileExistsError(f"那里已经有东西了：{target}")
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(src), str(dest))
        # The indexes have to know the file has a new address, or a
        # search keeps offering the path it came from.
        try:
            from .fs import _follow_the_move
            _follow_the_move(src, dest)
        except Exception:
            log.warning("the indexes did not follow a plan's move", exc_info=True)
        return f"已把 {source} 移到 {target}"

    raise ValueError(f"不认识的操作类型：{action_type}")


def _execute_run(command: str, cwd: str = "") -> str:
    """Run an approved command inside the sandbox.

    The approval screen showed the user this exact command, so it runs
    as written -- but inside the same sandbox the model would have used,
    so an approved command still cannot reach the network, the API keys
    or anything outside the storage root.
    """
    from ..deps import DATA_DIR
    from ..sandbox import SandboxError, resolve_cwd, run

    command = (command or "").strip()
    if not command:
        raise ValueError("run requires command")
    try:
        root = get_storage_root().resolve()
        target = resolve_cwd(root, DATA_DIR.resolve(), cwd or "")
        result = run(command, target, timeout=120)
    except SandboxError as e:
        raise ValueError(str(e)) from e

    stdout = (result["stdout"] or "").strip()
    stderr = (result["stderr"] or "").strip()
    parts = [f"$ {command}"]
    if stdout:
        parts.append(stdout)
    if stderr:
        parts.append(stderr)
    if result["timed_out"]:
        raise ValueError("命令超时，已终止\n" + "\n".join(parts))
    if result["returncode"] != 0:
        raise ValueError(f"命令失败，退出码 {result['returncode']}\n" + "\n".join(parts))
    return "\n".join(parts)


def _safe(path: str, mode: str = "read") -> Path:
    """A path inside the storage root that the caller may reach.

    A plan is a set of file operations somebody asked for and
    somebody else approved. Before it touches anything, the paths are
    checked against the person running it -- a plan must not be a way
    around what a person is allowed to do.
    """
    try:
        return resolve_under_root(path, mode)
    except PathOutsideRoot:
        raise ValueError(f"路径在存储根目录之外：{path}")
    except PathNotAllowed as e:
        raise ValueError(str(e) or "没有访问这个位置的权限")
