import os
from pathlib import Path
from datetime import datetime
from fastapi import APIRouter, Query, HTTPException
from fastapi.responses import FileResponse
from ..deps import state
from ..models import FileNode, BrowseResult
from src.config import SUPPORTED_IMAGE_FORMATS, is_image_file

router = APIRouter(prefix="/api", tags=["files"])

THUMB_SIZE = (200, 200)
_thumb_cache: dict[str, str] = {}


@router.get("/files", response_model=BrowseResult)
def browse_files(dir_id: str = Query(...), path: str | None = Query(None)):
    entry = state.get_dir(dir_id)
    if not entry:
        from fastapi import HTTPException
        raise HTTPException(404, "目录不存在")

    base = Path(entry.path)
    target = Path(path) if path else base

    try:
        target = target.resolve()
        base = base.resolve()
        if not str(target).startswith(str(base)):
            raise HTTPException(403, "路径超出目录范围")
    except Exception:
        raise HTTPException(400, "无效路径")

    if not target.exists() or not target.is_dir():
        raise HTTPException(404, f"目录不存在: {target}")

    parent_path: str | None = None
    if target != base:
        parent = target.parent
        parent_path = str(parent) if str(parent).startswith(str(base)) else None

    items: list[FileNode] = []
    try:
        for child in sorted(target.iterdir(), key=lambda p: (not p.is_dir(), p.name.lower())):
            if child.name.startswith("."):
                continue
            thumb_url = None
            if not child.is_dir() and is_image_file(child):
                thumb_url = f"/api/thumbnails?path={_encode_path(str(child))}"

            items.append(FileNode(
                name=child.name,
                path=str(child),
                is_dir=child.is_dir(),
                size=child.stat().st_size if child.is_file() else 0,
                modified=_format_time(child.stat().st_mtime),
                thumbnail_url=thumb_url,
            ))
    except PermissionError:
        pass

    return BrowseResult(
        current_path=str(target),
        parent_path=parent_path,
        items=items,
    )


@router.get("/thumbnails")
def get_thumbnail(path: str = Query(...)):
    img_path = Path(path)
    if not img_path.exists() or not is_image_file(img_path):
        raise HTTPException(404, "文件不存在")

    cached = _thumb_cache.get(path)
    if cached and Path(cached).exists():
        return FileResponse(cached, media_type="image/jpeg")

    try:
        from PIL import Image
        img = Image.open(img_path)
        img.thumbnail(THUMB_SIZE)
        img = img.convert("RGB")

        thumb_dir = Path(__file__).resolve().parent.parent.parent / "data" / "thumbs"
        thumb_dir.mkdir(parents=True, exist_ok=True)
        thumb_path = thumb_dir / f"{img_path.stem}_{img_path.stat().st_size}.jpg"
        img.save(thumb_path, "JPEG", quality=75)
        _thumb_cache[path] = str(thumb_path)
        return FileResponse(thumb_path, media_type="image/jpeg")
    except Exception:
        raise HTTPException(500, "生成缩略图失败")

def _encode_path(p: str) -> str:
    import urllib.parse
    return urllib.parse.quote(p, safe="")


def _format_time(mtime: float) -> str:
    try:
        return datetime.fromtimestamp(mtime).strftime("%Y-%m-%d %H:%M:%S")
    except Exception:
        return ""
