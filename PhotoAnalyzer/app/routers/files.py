import os
import uuid
import time
import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from datetime import datetime
from fastapi import APIRouter, Query, HTTPException
from fastapi.responses import FileResponse, Response
from ..deps import state
from ..models import FileNode, BrowseResult, ThumbnailJob
from src.config import SUPPORTED_IMAGE_FORMATS, is_image_file

router = APIRouter(prefix="/api", tags=["files"])

THUMB_SIZE = (200, 200)
_thumb_cache: dict[str, str] = {}
_thumb_jobs: dict[str, ThumbnailJob] = {}
_thumb_executor = ThreadPoolExecutor(max_workers=4)

RAW_FORMATS = {".cr2", ".cr3", ".arw", ".dng", ".nef", ".orf", ".rw2", ".pef", ".raf", ".3fr", ".ai", ".eps"}


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
        stem_map: dict[str, list[Path]] = {}

        for child in sorted(target.iterdir(), key=lambda p: p.name.lower()):
            if child.name.startswith("."):
                continue
            if child.is_dir():
                items.append(FileNode(
                    name=child.name,
                    path=str(child),
                    is_dir=True,
                    size=0,
                    modified="",
                    thumbnail_url=None,
                ))
                continue

            stem = child.stem
            suffix = child.suffix.lower()

            if is_image_file(child) or suffix in RAW_FORMATS:
                if stem not in stem_map:
                    stem_map[stem] = []
                stem_map[stem].append(child)

        for stem, paths in stem_map.items():
            paths.sort(key=lambda p: (0 if p.suffix.lower() == ".jpg" else 1, p.suffix.lower(), p.name.lower()))
            best = paths[0]
            thumb_url = f"/api/thumbnails?path={_encode_path(str(best))}" if best.suffix.lower() in {".jpg", ".jpeg", ".png", ".webp"} else None
            items.append(FileNode(
                name=best.name,
                path=str(best),
                is_dir=False,
                size=best.stat().st_size,
                modified=_format_time(best.stat().st_mtime),
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
def get_thumbnail(path: str = Query(...), full: bool = Query(False)):
    img_path = Path(path)
    if not img_path.exists() or not is_image_file(img_path):
        raise HTTPException(404, "文件不存在")

    if full:
        return FileResponse(img_path, media_type=f"image/{img_path.suffix.lstrip('.').lower()}")

    cache_key = f"thumb_{path}"
    cached = _thumb_cache.get(cache_key)
    if cached and Path(cached).exists():
        return FileResponse(cached, media_type="image/jpeg")

    thumb_dir = Path(__file__).resolve().parent.parent.parent / "data" / "thumbs"
    thumb_dir.mkdir(parents=True, exist_ok=True)
    thumb_path = thumb_dir / f"{img_path.stem}_{img_path.stat().st_size}.jpg"

    if thumb_path.exists():
        _thumb_cache[cache_key] = str(thumb_path)
        return FileResponse(thumb_path, media_type="image/jpeg")

    return Response(status_code=204)


@router.post("/thumbnails/batch")
def start_thumbnail_batch(paths: list[str]):
    valid_paths = [p for p in paths if Path(p).exists() and is_image_file(Path(p))]
    if not valid_paths:
        raise HTTPException(400, "没有有效的图片路径")

    job_id = str(uuid.uuid4())
    job = ThumbnailJob(
        job_id=job_id,
        status="pending",
        total=len(valid_paths),
        completed=0,
        failed=0,
        created_at=time.strftime("%Y-%m-%dT%H:%M:%S"),
    )
    _thumb_jobs[job_id] = job

    def generate_thumbnails():
        _thumb_jobs[job_id].status = "running"
        for i, p in enumerate(valid_paths):
            if _thumb_jobs[job_id].status == "canceled":
                break
            _thumb_jobs[job_id].current_file = Path(p).name
            _thumb_jobs[job_id].progress = i
            try:
                img_path = Path(p)
                cache_key = f"thumb_{p}"
                thumb_dir = Path(__file__).resolve().parent.parent.parent / "data" / "thumbs"
                thumb_dir.mkdir(parents=True, exist_ok=True)
                thumb_path = thumb_dir / f"{img_path.stem}_{img_path.stat().st_size}.jpg"
                if not thumb_path.exists():
                    from PIL import Image, ImageOps
                    img = Image.open(img_path)
                    img = ImageOps.exif_transpose(img)
                    img.thumbnail(THUMB_SIZE)
                    img = img.convert("RGB")
                    img.save(thumb_path, "JPEG", quality=75)
                _thumb_cache[cache_key] = str(thumb_path)
                _thumb_jobs[job_id].completed += 1
            except Exception:
                _thumb_jobs[job_id].failed += 1

        _thumb_jobs[job_id].status = "completed"
        _thumb_jobs[job_id].finished_at = time.strftime("%Y-%m-%dT%H:%M:%S")

    _thumb_executor.submit(generate_thumbnails)
    return job


@router.get("/thumbnails/batch/{job_id}")
def get_thumbnail_job(job_id: str):
    job = _thumb_jobs.get(job_id)
    if not job:
        raise HTTPException(404, "任务不存在")
    return job


@router.post("/thumbnails/batch/{job_id}/cancel")
def cancel_thumbnail_job(job_id: str):
    job = _thumb_jobs.get(job_id)
    if not job:
        raise HTTPException(404, "任务不存在")
    job.status = "canceled"
    job.finished_at = time.strftime("%Y-%m-%dT%H:%M:%S")
    return job

def _encode_path(p: str) -> str:
    import urllib.parse
    return urllib.parse.quote(p, safe="")


@router.delete("/files")
def delete_file(path: str = Query(...)):
    p = Path(path)
    if not p.exists():
        raise HTTPException(404, "文件不存在")
    if not p.is_file():
        raise HTTPException(400, "不是文件")
    try:
        p.unlink()
        return {"deleted": [str(p)], "count": 1}
    except Exception as e:
        raise HTTPException(500, f"删除失败: {e}")


@router.get("/files/siblings")
def get_file_siblings(path: str = Query(...)):
    p = Path(path)
    if not p.exists():
        raise HTTPException(404, "文件不存在")
    parent = p.parent
    stem = p.stem
    siblings = []
    for f in parent.iterdir():
        if f.is_file() and f.stem == stem and f != p:
            siblings.append(str(f))
    return {"siblings": siblings, "count": len(siblings)}


RAW_EXTENSIONS = {".cr2", ".arw", ".dng", ".nef", ".orf", ".rw2", ".pef", ".srw", ".raf"}


@router.get("/files/orphaned-raws")
def get_orphaned_raws(dir_id: str = Query(...)):
    from ..deps import state
    entry = state.get_dir(dir_id)
    if not entry:
        raise HTTPException(404, "目录不存在")
    target = Path(entry.path)
    orphaned = []
    for f in target.rglob("*"):
        if not f.is_file():
            continue
        ext = f.suffix.lower()
        if ext not in RAW_EXTENSIONS:
            continue
        stem = f.stem
        jpg_path = f.parent / f"{stem}.jpg"
        jpg_path_upper = f.parent / f"{stem}.JPG"
        if not jpg_path.exists() and not jpg_path_upper.exists():
            orphaned.append(str(f))
    return {"orphaned": orphaned, "count": len(orphaned)}


@router.delete("/files/orphaned-raws")
def delete_orphaned_raws(dir_id: str = Query(...)):
    from ..deps import state
    entry = state.get_dir(dir_id)
    if not entry:
        raise HTTPException(404, "目录不存在")
    target = Path(entry.path)
    deleted = []
    not_found = []
    for f in target.rglob("*"):
        if not f.is_file():
            continue
        ext = f.suffix.lower()
        if ext not in RAW_EXTENSIONS:
            continue
        stem = f.stem
        jpg_path = f.parent / f"{stem}.jpg"
        jpg_path_upper = f.parent / f"{stem}.JPG"
        if not jpg_path.exists() and not jpg_path_upper.exists():
            if f.exists():
                f.unlink()
                deleted.append(str(f))
            else:
                not_found.append(str(f))
    return {"deleted": deleted, "not_found": not_found, "count": len(deleted)}


def _format_time(mtime: float) -> str:
    try:
        return datetime.fromtimestamp(mtime).strftime("%Y-%m-%d %H:%M:%S")
    except Exception:
        return ""
