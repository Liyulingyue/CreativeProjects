"""Skill management: list, toggle, upload, remove."""

from __future__ import annotations

from typing import Optional

from fastapi import APIRouter, Depends, File, Form, HTTPException, UploadFile
from pydantic import BaseModel

from ..skills import (
    MAX_SKILL_BYTES,
    get_skill_registry,
    import_files_skill,
    import_zip_skill,
)
from .auth import require_admin

skills_router = APIRouter()

MASTER_DEFAULT = False


class AddSkillRequest(BaseModel):
    id: str
    label: str
    instruction: str
    description: str = ""


class UpdateSkillRequest(BaseModel):
    label: Optional[str] = None
    description: Optional[str] = None
    instruction: Optional[str] = None


class ToggleRequest(BaseModel):
    enabled: bool


class MasterToggleRequest(BaseModel):
    enabled: bool


class ExecutableToggleRequest(BaseModel):
    executable: bool


@skills_router.get("")
def list_skills() -> dict:
    reg = get_skill_registry()
    return {
        "master": reg.master_enabled,
        "skills": reg.all_skills(),
        "tools": reg.all_tool_states(),
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


@skills_router.patch("/{skill_id}")
def update_skill(skill_id: str, req: UpdateSkillRequest, _admin=Depends(require_admin)) -> dict:
    reg = get_skill_registry()
    try:
        return reg.update_custom(
            skill_id, label=req.label, description=req.description,
            instruction=req.instruction,
        )
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))


@skills_router.get("/tools")
def list_tools() -> dict:
    return {"tools": get_skill_registry().all_tool_states()}


@skills_router.patch("/tools/{tool_name}/toggle")
def toggle_tool(tool_name: str, req: ToggleRequest, _admin=Depends(require_admin)) -> dict:
    reg = get_skill_registry()
    try:
        reg.set_tool_enabled(tool_name, req.enabled)
    except ValueError as e:
        raise HTTPException(status_code=404, detail=str(e))
    return {"name": tool_name, "enabled": req.enabled}


@skills_router.patch("/tools/{tool_name}/executable")
def toggle_tool_executable(
    tool_name: str, req: ExecutableToggleRequest, _admin=Depends(require_admin)
) -> dict:
    reg = get_skill_registry()
    try:
        reg.set_tool_executable(tool_name, req.executable)
    except ValueError as e:
        raise HTTPException(status_code=404, detail=str(e))
    return {"name": tool_name, "executable": req.executable}


@skills_router.patch("/tools/all")
def toggle_all_tools(req: ToggleRequest, _admin=Depends(require_admin)) -> dict:
    reg = get_skill_registry()
    reg.set_all_tools(req.enabled)
    return {"success": True, "all_enabled": req.enabled}


@skills_router.post("/import/zip")
async def import_zip_skill_endpoint(
    file: UploadFile = File(...),
    skill_id: str = Form(""),
    _admin=Depends(require_admin),
) -> dict:
    data = await file.read()
    if len(data) > MAX_SKILL_BYTES:
        raise HTTPException(status_code=413, detail="ZIP 过大（上限 50MB）")
    try:
        return import_zip_skill(data, skill_id or None)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))


@skills_router.post("/import/files")
async def import_files_endpoint(
    files: list[UploadFile] = File(...),
    skill_id: str = Form(""),
    _admin=Depends(require_admin),
) -> dict:
    entries: list[tuple[str, bytes]] = []
    total = 0
    for upload in files:
        data = await upload.read()
        total += len(data)
        if total > MAX_SKILL_BYTES:
            raise HTTPException(status_code=413, detail="技能包过大（上限 50MB）")
        entries.append((upload.filename or "", data))
    try:
        return import_files_skill(entries, skill_id or None)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
