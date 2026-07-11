from fastapi import APIRouter, HTTPException
from ..deps import state
from ..models import DirEntry

router = APIRouter(prefix="/api/dirs", tags=["dirs"])


@router.get("", response_model=list[DirEntry])
def list_dirs():
    return state.list_dirs()


@router.post("", response_model=DirEntry)
def add_dir(body: dict):
    path = body.get("path")
    if not path:
        raise HTTPException(400, "path is required")
    name = body.get("name")
    p = __import__("pathlib").Path(path)
    if not p.exists():
        raise HTTPException(400, f"路径不存在: {path}")
    if not p.is_dir():
        raise HTTPException(400, f"不是目录: {path}")
    return state.add_dir(path, name)


@router.delete("/{dir_id}")
def remove_dir(dir_id: str):
    if not state.remove_dir(dir_id):
        raise HTTPException(404, "目录不存在")
    return {"ok": True}
