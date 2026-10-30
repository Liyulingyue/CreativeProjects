"""Skill management: list, toggle, upload, remove."""

from __future__ import annotations

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel

from ..skills import get_skill_registry
from .auth import require_admin

skills_router = APIRouter()

MASTER_DEFAULT = False


class AddSkillRequest(BaseModel):
    id: str
    label: str
    instruction: str
    description: str = ""


class ToggleRequest(BaseModel):
    enabled: bool


class MasterToggleRequest(BaseModel):
    enabled: bool


@skills_router.get("")
def list_skills() -> dict:
    reg = get_skill_registry()
    return {
        "master": reg.master_enabled,
        "skills": reg.all_skills(),
    }


@skills_router.patch("/master")
def toggle_master(req: MasterToggleRequest, _admin=Depends(require_admin)) -> dict:
    reg = get_skill_registry()
    reg.set_master(req.enabled)
    return {"master": reg.master_enabled}


@skills_router.patch("/{skill_id}/toggle")
def toggle_skill(skill_id: str, req: ToggleRequest, _admin=Depends(require_admin)) -> dict:
    reg = get_skill_registry()
    ids = {s["id"] for s in reg.all_skills()}
    if skill_id not in ids:
        raise HTTPException(status_code=404, detail="技能不存在")
    reg.set_enabled(skill_id, req.enabled)
    return {"id": skill_id, "enabled": req.enabled}


@skills_router.post("")
def add_skill(req: AddSkillRequest, _admin=Depends(require_admin)) -> dict:
    reg = get_skill_registry()
    try:
        return reg.add_custom(req.id, req.label, req.instruction, req.description)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))


@skills_router.delete("/{skill_id}")
def remove_skill(skill_id: str, _admin=Depends(require_admin)) -> dict:
    reg = get_skill_registry()
    if not reg.remove_custom(skill_id):
        raise HTTPException(status_code=404, detail="技能不存在或是内置技能")
    return {"success": True, "id": skill_id}
