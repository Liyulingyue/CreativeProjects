import os
import shutil
from pathlib import Path
from typing import Optional

from fastapi import APIRouter, HTTPException
from ..deps import state
from ..models import (
    BrowseResult,
    CreateFolderRequest,
    DeleteRequest,
    FileNode,
    FileOperation,
    MoveRequest,
)

fs = APIRouter()


def _get_mime_type(path: Path) -> str:
    ext = path.suffix.lower()
    mime_types = {
        ".txt": "text/plain",
        ".md": "text/markdown",
        ".json": "application/json",
        ".xml": "application/xml",
        ".html": "text/html",
        ".css": "text/css",
        ".js": "application/javascript",
        ".ts": "application/typescript",
        ".py": "text/x-python",
        ".rs": "text/x-rust",
        ".go": "text/x-go",
        ".java": "text/x-java",
        ".c": "text/x-c",
        ".cpp": "text/x-c++",
        ".h": "text/x-c-header",
        ".jpg": "image/jpeg",
        ".jpeg": "image/jpeg",
        ".png": "image/png",
        ".gif": "image/gif",
        ".bmp": "image/bmp",
        ".webp": "image/webp",
        ".svg": "image/svg+xml",
        ".ico": "image/x-icon",
        ".mp3": "audio/mpeg",
        ".wav": "audio/wav",
        ".ogg": "audio/ogg",
        ".mp4": "video/mp4",
        ".avi": "video/x-msvideo",
        ".mkv": "video/x-matroska",
        ".mov": "video/quicktime",
        ".pdf": "application/pdf",
        ".zip": "application/zip",
        ".tar": "application/x-tar",
        ".gz": "application/gzip",
        ".7z": "application/x-7z-compressed",
        ".rar": "application/vnd.rar",
        ".doc": "application/msword",
        ".docx": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ".xls": "application/vnd.ms-excel",
        ".xlsx": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ".ppt": "application/vnd.ms-powerpoint",
        ".pptx": "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    }
    return mime_types.get(ext, "application/octet-stream")


def _safe_path(base: str, target: str) -> Path:
    base_path = Path(base).resolve()
    target_path = Path(target).resolve()
    if not str(target_path).startswith(str(base_path)):
        raise HTTPException(status_code=400, detail="Path outside storage root")
    return target_path


def _path_to_filenode(path: Path) -> FileNode:
    import datetime
    stat = path.stat()
    modified_dt = datetime.datetime.fromtimestamp(stat.st_mtime)
    created_dt = datetime.datetime.fromtimestamp(stat.st_ctime)
    return FileNode(
        name=path.name,
        path=str(path),
        is_dir=path.is_dir(),
        size=stat.st_size if path.is_file() else 0,
        modified=modified_dt.strftime("%Y-%m-%dT%H:%M:%S"),
        created=created_dt.strftime("%Y-%m-%dT%H:%M:%S"),
        extension=path.suffix.lower(),
        mime_type=_get_mime_type(path),
    )


@fs.get("/browse")
def browse(path: Optional[str] = None) -> BrowseResult:
    storage_path = state.get_settings().storage_path
    if not path:
        target = Path(storage_path)
    else:
        target = _safe_path(storage_path, path)

    if not target.exists():
        raise HTTPException(status_code=404, detail="Path not found")
    if not target.is_dir():
        raise HTTPException(status_code=400, detail="Path is not a directory")

    items = []
    dirs_count = 0
    files_count = 0

    for entry in sorted(target.iterdir(), key=lambda x: (not x.is_dir(), x.name.lower())):
        if entry.name.startswith('.'):
            continue
        try:
            node = _path_to_filenode(entry)
            items.append(node)
            if node.is_dir:
                dirs_count += 1
            else:
                files_count += 1
        except (PermissionError, OSError):
            continue

    parent = str(target.parent) if target != Path(storage_path) else None

    return BrowseResult(
        current_path=str(target),
        parent_path=parent,
        items=items,
        total_count=len(items),
        dirs_count=dirs_count,
        files_count=files_count,
    )


@fs.post("/create_folder")
def create_folder(req: CreateFolderRequest) -> FileOperation:
    storage_path = state.get_settings().storage_path
    target = _safe_path(storage_path, Path(req.path) / req.name)

    try:
        target.mkdir(parents=True, exist_ok=False)
        return FileOperation(success=True, message="Folder created", path=str(target))
    except FileExistsError:
        raise HTTPException(status_code=409, detail="Folder already exists")
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@fs.post("/move")
def move(req: MoveRequest) -> FileOperation:
    storage_path = state.get_settings().storage_path
    src = _safe_path(storage_path, req.src)
    dest = _safe_path(storage_path, req.dest)

    if not src.exists():
        raise HTTPException(status_code=404, detail="Source not found")

    try:
        shutil.move(str(src), str(dest))
        return FileOperation(success=True, message="File moved", path=str(dest))
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@fs.post("/copy")
def copy(req: MoveRequest) -> FileOperation:
    storage_path = state.get_settings().storage_path
    src = _safe_path(storage_path, req.src)
    dest = _safe_path(storage_path, req.dest)

    if not src.exists():
        raise HTTPException(status_code=404, detail="Source not found")

    try:
        if src.is_dir():
            shutil.copytree(str(src), str(dest), dirs_exist_ok=False)
        else:
            shutil.copy2(str(src), str(dest))
        return FileOperation(success=True, message="File copied", path=str(dest))
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@fs.post("/delete")
def delete(req: DeleteRequest) -> FileOperation:
    storage_path = state.get_settings().storage_path
    target = _safe_path(storage_path, req.path)

    if not target.exists():
        raise HTTPException(status_code=404, detail="Path not found")

    try:
        if target.is_dir():
            shutil.rmtree(str(target))
        else:
            target.unlink()
        return FileOperation(success=True, message="Deleted", path=str(target))
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@fs.get("/info")
def get_info(path: str) -> FileNode:
    storage_path = state.get_settings().storage_path
    target = _safe_path(storage_path, path)

    if not target.exists():
        raise HTTPException(status_code=404, detail="Path not found")

    return _path_to_filenode(target)


@fs.get("/tree")
def get_tree(path: Optional[str] = None, depth: int = 2) -> dict:
    storage_path = state.get_settings().storage_path
    if not path:
        target = Path(storage_path)
    else:
        target = _safe_path(storage_path, path)

    def build_tree(p: Path, current_depth: int) -> dict:
        if current_depth > depth:
            return {"name": p.name, "path": str(p), "is_dir": True, "children": []}
        if not p.is_dir():
            return {"name": p.name, "path": str(p), "is_dir": False}
        children = []
        try:
            for child in sorted(p.iterdir(), key=lambda x: (not x.is_dir(), x.name.lower())):
                if child.name.startswith('.'):
                    continue
                try:
                    children.append(build_tree(child, current_depth + 1))
                except (PermissionError, OSError):
                    continue
        except (PermissionError, OSError):
            pass
        return {"name": p.name, "path": str(p), "is_dir": True, "children": children}

    return build_tree(target, 0)
