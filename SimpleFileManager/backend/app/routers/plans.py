"""Plan & Approve engine.

Plans are created by the agent (or from organizer suggestions). They are
executed only through the /execute endpoint after explicit user approval.
Every executed action is recorded in an execution log.
"""

import shutil
from pathlib import Path

from fastapi import APIRouter, HTTPException

from ..deps import state, resolve_under_root, PathOutsideRoot, validate_plan_action
from ..models import AgentPlan, CreatePlanRequest

plans = APIRouter(prefix="/api/plans", tags=["plans"])


@plans.get("")
def list_plans(status: str = None, limit: int = 50) -> dict:
    return {"plans": [p.model_dump() for p in state.get_plan_store().list_plans(status, limit)]}


@plans.get("/{plan_id}")
def get_plan(plan_id: str) -> AgentPlan:
    plan = state.get_plan_store().get_plan(plan_id)
    if not plan:
        raise HTTPException(status_code=404, detail="Plan not found")
    return plan


@plans.get("/{plan_id}/log")
def get_plan_log(plan_id: str) -> dict:
    if not state.get_plan_store().get_plan(plan_id):
        raise HTTPException(status_code=404, detail="Plan not found")
    return {"log": state.get_plan_store().get_log(plan_id)}


@plans.post("")
def create_plan(req: CreatePlanRequest) -> AgentPlan:
    if not req.actions:
        raise HTTPException(status_code=400, detail="Plan must contain at least one action")
    for a in req.actions:
        error = validate_plan_action(a.model_dump())
        if error:
            raise HTTPException(status_code=400, detail=error)
    return state.get_plan_store().create_plan(
        title=req.title,
        summary=req.summary,
        source=req.source,
        actions=[a.model_dump() for a in req.actions],
    )


@plans.post("/{plan_id}/approve")
def approve_plan(plan_id: str) -> AgentPlan:
    plan = _get_plan_or_404(plan_id)
    if plan.status not in ("pending", "rejected"):
        raise HTTPException(status_code=400, detail=f"Plan is {plan.status}, cannot approve")
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
        raise HTTPException(status_code=400, detail=f"Plan must be approved before execution (current: {plan.status})")

    store = state.get_plan_store()
    succeeded, failed = 0, 0

    for action in plan.actions:
        try:
            message = _execute_action(action.action_type, action.source_path, action.target_path)
            store.update_action(action.id, "done", message)
            store.add_log(plan_id, action.id, action.action_type,
                          action.source_path, action.target_path, True, message)
            succeeded += 1
        except Exception as e:
            message = str(e)
            store.update_action(action.id, "failed", message)
            store.add_log(plan_id, action.id, action.action_type,
                          action.source_path, action.target_path, False, message)
            failed += 1

    final_status = "executed" if failed == 0 else ("executed_with_errors" if succeeded > 0 else "failed")
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
        raise HTTPException(status_code=404, detail="Plan not found")
    return plan


def _execute_action(action_type: str, source: str, target: str) -> str:
    """Execute a single approved action. Paths are relative to the storage root.

    Raises on any failure so the caller can record it.
    """
    if action_type == "create_folder":
        if not target:
            raise ValueError("create_folder requires target_path")
        dest = _safe(target)
        if dest.exists():
            raise FileExistsError(f"Already exists: {target}")
        dest.mkdir(parents=True, exist_ok=False)
        return f"Created folder {target}"

    if action_type == "delete":
        if not source:
            raise ValueError("delete requires source_path")
        src = _safe(source)
        if not src.exists():
            raise FileNotFoundError(f"Source not found: {source}")
        if src.is_dir():
            shutil.rmtree(src)
        else:
            src.unlink()
        return f"Deleted {source}"

    if action_type in ("move", "rename"):
        if not source or not target:
            raise ValueError(f"{action_type} requires source_path and target_path")
        src = _safe(source)
        dest = _safe(target)
        if not src.exists():
            raise FileNotFoundError(f"Source not found: {source}")
        if dest.exists():
            raise FileExistsError(f"Target already exists: {target}")
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(src), str(dest))
        return f"Moved {source} -> {target}"

    raise ValueError(f"Unknown action_type: {action_type}")


def _safe(path: str) -> Path:
    try:
        return resolve_under_root(path)
    except PathOutsideRoot:
        raise ValueError(f"Path outside storage root: {path}")
