import threading
import time
from pathlib import Path
from fastapi import APIRouter, HTTPException
from ..deps import state
from ..models import DedupJob, DedupGroup, DedupItem, DedupStageConfig
from src.config import is_image_file, get_image_files

router = APIRouter(prefix="/api/dedup", tags=["dedup"])


@router.post("", response_model=DedupJob)
def start_dedup(body: dict):
    file_paths = body.get("file_paths")
    dir_id = body.get("dir_id")
    sub_path = body.get("sub_path")
    recursive = body.get("recursive", True)
    stages_config = body.get("stages")

    if file_paths:
        paths = [p for p in file_paths if Path(p).exists() and is_image_file(p)]
    else:
        if not dir_id:
            raise HTTPException(400, "需要 dir_id 或 file_paths")

        entry = state.get_dir(dir_id)
        if not entry:
            raise HTTPException(404, "目录不存在")

        base = Path(entry.path)
        target = Path(sub_path) if sub_path else base

        if not target.exists():
            raise HTTPException(400, f"路径不存在: {target}")

        image_files = get_image_files(target) if recursive else [
            f for f in target.iterdir() if f.is_file() and is_image_file(f)
        ]
        paths = [str(f) for f in image_files]

    if not paths:
        raise HTTPException(400, "目录下没有图片")

    stages = None
    if stages_config:
        stages = [s for s in stages_config if s.get("enabled", True)] if isinstance(stages_config, list) else None

    job = state.create_dedup_job(len(paths))
    t = threading.Thread(target=_run_dedup, args=(job.job_id, paths, stages), daemon=True)
    t.start()
    return job


@router.get("/{job_id}", response_model=DedupJob)
def get_dedup_job(job_id: str):
    job = state.get_dedup_job(job_id)
    if not job:
        raise HTTPException(404, "任务不存在")
    return job


@router.post("/{job_id}/resolve")
def resolve_dedup_groups(job_id: str, body: dict):
    job = state.get_dedup_job(job_id)
    if not job:
        raise HTTPException(404, "任务不存在")

    actions = body.get("actions", [])
    removed = []
    for action in actions:
        for r_path in action.get("remove", []):
            p = Path(r_path)
            if p.exists():
                p.unlink()
                removed.append(r_path)

    return {"removed": removed, "count": len(removed)}


def _run_dedup(job_id: str, paths: list[str], stages_config: list | None):
    job = state.get_dedup_job(job_id)
    if not job:
        return

    state.update_dedup_job(job_id, status="running", stage="初始化")

    settings = state.get_settings()

    try:
        if stages_config:
            from src.dedup.composite import HierarchicalDeduplicator
            dedup = HierarchicalDeduplicator(stages=stages_config)
            state.update_dedup_job(job_id, stage="层次化去重")
            result = dedup.deduplicate(paths)
            groups = _convert_hierarchical(result)
        else:
            from src.dedup.composite import CompositeDeduplicator
            enabled_stages = [s for s in settings.dedup_stages if s.enabled]
            dedup = CompositeDeduplicator(
                use_exif=any(s.type == "exif" for s in enabled_stages),
                use_phash=any(s.type == "phash" for s in enabled_stages),
                use_embedding=any(s.type == "embedding" for s in enabled_stages),
            )
            state.update_dedup_job(job_id, stage="组合去重")
            raw_groups = dedup.group_all(paths)
            groups = _convert_groups(raw_groups)

        state.update_dedup_job(
            job_id,
            status="completed",
            groups=groups,
            groups_count=len(groups),
            stage="完成",
            finished_at=time.strftime("%Y-%m-%dT%H:%M:%S"),
        )
    except Exception as e:
        state.update_dedup_job(
            job_id,
            status="failed",
            stage="失败",
            finished_at=time.strftime("%Y-%m-%dT%H:%M:%S"),
        )


def _convert_groups(raw_groups) -> list[DedupGroup]:
    result = []
    for i, g in enumerate(raw_groups):
        items = []
        for item in g.items:
            items.append(DedupItem(
                path=str(item.path),
                file_name=item.path.name,
                thumbnail_url=f"/api/thumbnails?path={_encode_path(str(item.path))}",
                file_size=item.file_size or (item.path.stat().st_size if item.path.exists() else 0),
                similarity=0.0,
            ))
        result.append(DedupGroup(
            group_id=g.group_id if hasattr(g, "group_id") and g.group_id else f"g_{i}",
            items=items,
            representative=str(g.representative.path) if g.representative else (str(items[0].path) if items else None),
            stage="composite",
        ))
    return result


def _convert_hierarchical(result: dict) -> list[DedupGroup]:
    groups = []
    for entry in result.get("groups", []):
        raw_group = entry["group"]
        stage_name = entry["stage"]
        items = []
        for item in raw_group.items:
            items.append(DedupItem(
                path=str(item.path),
                file_name=item.path.name,
                thumbnail_url=f"/api/thumbnails?path={_encode_path(str(item.path))}",
                file_size=item.file_size or (item.path.stat().st_size if item.path.exists() else 0),
                similarity=0.0,
            ))
        groups.append(DedupGroup(
            group_id=raw_group.group_id if hasattr(raw_group, "group_id") and raw_group.group_id else f"h_{entry['stage']}_{len(groups)}",
            items=items,
            representative=str(raw_group.representative.path) if raw_group.representative else (str(items[0].path) if items else None),
            stage=stage_name,
        ))
    return groups


def _encode_path(p: str) -> str:
    import urllib.parse
    return urllib.parse.quote(p, safe="")
