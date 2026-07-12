import os
from pathlib import Path
from datetime import datetime
from fastapi import APIRouter, Query, HTTPException
from pydantic import BaseModel
from typing import Optional

router = APIRouter(prefix="/api/fs", tags=["filesystem"])


class FsEntry(BaseModel):
    name: str
    path: str
    is_dir: bool
    children_count: Optional[int] = None


class FsBrowseResult(BaseModel):
    current_path: str
    parent_path: Optional[str] = None
    entries: list[FsEntry]
    home: str


HOME = str(Path.home())
COMMON_ROOTS: list[str] = []
PATH_SEP = "\\" if os.name == "nt" else "/"

if os.name == "nt":
    for letter in "ABCDEFGHIJKLMNOPQRSTUVWXYZ":
        drive = f"{letter}:\\"
        if Path(drive).exists():
            COMMON_ROOTS.append(drive)
    if Path(HOME).drive:
        drive_root = Path(HOME).drive + "\\"
        if drive_root not in COMMON_ROOTS:
            COMMON_ROOTS.insert(0, drive_root)
else:
    if Path("/mnt").exists():
        COMMON_ROOTS.append("/mnt")
    if Path("/media").exists():
        COMMON_ROOTS.append("/media")
    if Path("/Volumes").exists():
        COMMON_ROOTS.append("/Volumes")
    if Path("/home").exists():
        COMMON_ROOTS.append("/home")


def _is_readable(p: Path) -> bool:
    try:
        return os.access(p, os.R_OK)
    except Exception:
        return False


def _count_children(p: Path) -> Optional[int]:
    try:
        return sum(1 for _ in p.iterdir() if not _.name.startswith("."))
    except (PermissionError, OSError):
        return None


@router.get("/browse", response_model=FsBrowseResult)
def browse_fs(path: str = Query(default=""), dirs_only: bool = Query(default=True)):
    if not path:
        entries = []
        entries.append(FsEntry(name="~ (Home)", path=HOME, is_dir=True, children_count=_count_children(Path(HOME))))
        for root in COMMON_ROOTS:
            rp = Path(root)
            if rp.exists() and _is_readable(rp):
                entries.append(FsEntry(name=root, path=root, is_dir=True, children_count=_count_children(rp)))
        return FsBrowseResult(current_path="", parent_path=None, entries=entries, home=HOME)

    target = Path(path).resolve()
    if not target.exists():
        raise HTTPException(400, f"路径不存在: {path}")
    if not target.is_dir():
        raise HTTPException(400, f"不是目录: {path}")
    if not _is_readable(target):
        raise HTTPException(403, f"无权限访问: {path}")

    parent = str(target.parent) if target.parent != target else None

    entries: list[FsEntry] = []
    try:
        for child in sorted(target.iterdir(), key=lambda p: (not p.is_dir(), p.name.lower())):
            if child.name.startswith("."):
                continue
            if not _is_readable(child):
                continue
            if dirs_only and not child.is_dir():
                continue
            entries.append(FsEntry(
                name=child.name,
                path=str(child),
                is_dir=child.is_dir(),
                children_count=_count_children(child) if child.is_dir() else None,
            ))
    except PermissionError:
        pass

    return FsBrowseResult(current_path=str(target), parent_path=parent, entries=entries, home=HOME)


class FsSuggestResult(BaseModel):
    suggestions: list[FsEntry]
    partial: str


@router.get("/suggest", response_model=FsSuggestResult)
def suggest_path(q: str = Query(default="")):
    if not q:
        entries = [FsEntry(name=r, path=r, is_dir=True) for r in COMMON_ROOTS]
        entries.insert(0, FsEntry(name="~", path=HOME, is_dir=True))
        return FsSuggestResult(suggestions=entries, partial="")

    expanded = q.replace("~", HOME, 1) if q.startswith("~") else q
    p = Path(expanded)

    if p.is_dir() and _is_readable(p) and q.endswith("/"):
        parent = p
        prefix = ""
    else:
        parent = p.parent
        prefix = p.name.lower()

    if not parent.is_dir() or not _is_readable(parent):
        return FsSuggestResult(suggestions=[], partial=q)

    suggestions: list[FsEntry] = []
    try:
        for child in sorted(parent.iterdir(), key=lambda c: c.name.lower()):
            if child.name.startswith("."):
                continue
            if not _is_readable(child):
                continue
            if not child.is_dir():
                continue
            if prefix and not child.name.lower().startswith(prefix):
                continue
            suggestions.append(FsEntry(
                name=child.name,
                path=str(child),
                is_dir=True,
                children_count=_count_children(child),
            ))
    except (PermissionError, OSError):
        pass

    return FsSuggestResult(suggestions=suggestions, partial=q)
