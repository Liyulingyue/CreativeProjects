import os
import shutil
from datetime import datetime
from pathlib import Path
from typing import Optional

from fastapi import APIRouter, HTTPException, UploadFile, File
from fastapi.responses import FileResponse
from ..deps import state, get_storage_root, resolve_under_root, PathOutsideRoot
from ..models import (
    BrowseResult,
    CreateFolderRequest,
    DeleteRequest,
    FileNode,
    FileOperation,
    MoveRequest,
)

fs = APIRouter()

# Preview reads the whole file to slice it by character offset, so this bounds
# how large a file may be before preview refuses it outright.
MAX_PREVIEW_BYTES = 10 * 1024 * 1024


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
    if not target_path.is_relative_to(base_path):
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
    storage_path = str(get_storage_root())
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
    storage_path = str(get_storage_root())
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
    storage_path = str(get_storage_root())
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
    storage_path = str(get_storage_root())
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
    storage_path = str(get_storage_root())
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
    storage_path = str(get_storage_root())
    target = _safe_path(storage_path, path)

    if not target.exists():
        raise HTTPException(status_code=404, detail="Path not found")

    return _path_to_filenode(target)


@fs.get("/tree")
def get_tree(path: Optional[str] = None, depth: int = 2) -> dict:
    storage_path = str(get_storage_root())
    if not path:
        target = Path(storage_path)
    else:
        target = _safe_path(storage_path, path)

    def build_tree(p: Path, current_depth: int) -> dict:
        if current_depth > depth:
            return {"name": p.name, "path": str(p), "is_dir": True, "children": []}
        if not p.is_dir():
            # Leaves carry the metadata a preview needs; the UI opens tree files
            # directly instead of browsing into them.
            try:
                st = p.stat()
                return {
                    "name": p.name,
                    "path": str(p),
                    "is_dir": False,
                    "size": st.st_size,
                    "modified": datetime.fromtimestamp(st.st_mtime).isoformat(),
                }
            except (PermissionError, OSError):
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


@fs.get("/download")
def download_file(path: str):
    try:
        target = resolve_under_root(path)
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    if not target.exists() or not target.is_file():
        raise HTTPException(status_code=404, detail="File not found")
    return FileResponse(
        str(target),
        filename=target.name,
        media_type=_get_mime_type(target),
    )


@fs.post("/upload")
async def upload_files(path: str = "", files: list[UploadFile] = File(...)):
    try:
        target_dir = resolve_under_root(path)
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    if not target_dir.is_dir():
        raise HTTPException(status_code=400, detail="Destination is not a directory")

    uploaded = []
    for f in files:
        raw_filename = f.filename or ""
        filename = Path(raw_filename.replace("\\", "/")).name
        if (not filename or filename in (".", "..")
                or "/" in raw_filename or "\\" in raw_filename):
            raise HTTPException(status_code=400, detail="Invalid upload filename")
        dest = (target_dir / filename).resolve()
        if not dest.is_relative_to(target_dir.resolve()):
            raise HTTPException(status_code=400, detail="Upload destination outside storage root")
        with open(dest, "wb") as out:
            while chunk := await f.read(1024 * 1024):
                out.write(chunk)
        uploaded.append(filename)
    return {"success": True, "uploaded": uploaded, "count": len(uploaded)}


@fs.get("/content")
def get_content(path: str, limit: int = 50000, offset: int = 0):
    """Return one slice of a text file.

    Long documents (a novel is 1.7MB) are served in `limit`-sized character
    segments via `offset`, because reading the whole thing into a modal at once
    both stalls the browser and shows only a fraction of the text. Offsets count
    characters, not bytes, so multi-byte CJK text stays aligned between calls.
    """
    try:
        target = resolve_under_root(path)
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    if not target.exists() or not target.is_file():
        raise HTTPException(status_code=404, detail="File not found")

    size = target.stat().st_size
    if size > MAX_PREVIEW_BYTES:
        raise HTTPException(
            status_code=413,
            detail=f"File too large to preview (max {MAX_PREVIEW_BYTES // 1024 // 1024}MB)",
        )

    limit = max(1000, min(limit, 500_000))
    offset = max(0, offset)
    try:
        # Decoding errors are dropped rather than raised, so the character count
        # matches what the client actually receives.
        text = target.read_text(encoding="utf-8", errors="ignore")
    except (PermissionError, OSError, ValueError) as e:
        raise HTTPException(status_code=500, detail=str(e))

    total_chars = len(text)
    segment = text[offset: offset + limit]
    end = offset + len(segment)
    return {
        "path": str(target),
        "name": target.name,
        "content": segment,
        "offset": offset,
        "next_offset": end if end < total_chars else None,
        "total_chars": total_chars,
        "size": size,
        "mime_type": _get_mime_type(target),
    }
