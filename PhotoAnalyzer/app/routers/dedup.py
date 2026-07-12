import threading
import time
from pathlib import Path
from fastapi import APIRouter, HTTPException
from ..deps import state
from ..models import DedupJob, DedupGroup, DedupItem, DedupStageConfig
from src.config import is_image_file, get_image_files

router = APIRouter(prefix="/api/dedup", tags=["dedup"])


@router.get("/cache/stats")
def cache_stats():
    from src.dedup.cache import cache as feature_cache
    return feature_cache.stats()


@router.get("/cache/entries")
def cache_entries(feature_type: str | None = None):
    from src.dedup.cache import cache as feature_cache
    return feature_cache.list_entries(feature_type)


@router.post("/cache/clear")
def cache_clear(body: dict | None = None):
    from src.dedup.cache import cache as feature_cache
    feature_type = (body or {}).get("feature_type")
    feature_cache.clear(feature_type)
    return {"cleared": feature_type or "all"}


@router.delete("/cache/entries/{cache_key:path}")
def cache_delete_entry(cache_key: str):
    from src.dedup.cache import cache as feature_cache
    ok = feature_cache.delete_entry(cache_key)
    if not ok:
        raise HTTPException(404, "缓存条目不存在")
    return {"deleted": cache_key}


@router.post("/cache/export-to-folder")
def cache_export_to_folder(body: dict | None = None):
    from src.dedup.cache import cache as feature_cache
    dir_paths = (body or {}).get("dir_paths")
    return feature_cache.export_to_folder(dir_paths)


@router.post("/cache/import-from-folder")
def cache_import_from_folder(body: dict | None = None):
    from src.dedup.cache import cache as feature_cache
    dir_paths = (body or {}).get("dir_paths")
    return feature_cache.import_from_folder(dir_paths)


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

    job = state.create_dedup_job(len(paths), dir_id, str(base))
    t = threading.Thread(target=_run_dedup, args=(job.job_id, paths, stages), daemon=True)
    t.start()
    return job


@router.get("/{job_id}", response_model=DedupJob)
def get_dedup_job(job_id: str):
    job = state.get_dedup_job(job_id)
    if not job:
        raise HTTPException(404, "任务不存在")
    return job


@router.get("/by-dir/{dir_id}", response_model=DedupJob)
def get_dedup_job_by_dir(dir_id: str):
    entry = state.get_dir(dir_id)
    dir_path = entry.path if entry else None
    job = state.get_latest_dedup_job(dir_id, dir_path)
    if not job:
        raise HTTPException(404, "该目录暂无去重结果")
    return job


@router.post("/{job_id}/resolve")
def resolve_dedup_groups(job_id: str, body: dict):
    job = state.get_dedup_job(job_id)
    if not job:
        raise HTTPException(404, "任务不存在")

    actions = body.get("actions", [])
    all_removed_paths = set()

    for action in actions:
        for r_path in action.get("remove", []):
            all_removed_paths.add(r_path)

    if all_removed_paths:
        updated_groups = []
        for group in job.groups:
            updated_items = [item for item in group.items if item.path not in all_removed_paths]
            if len(updated_items) > 1:
                g = group.model_copy(deep=True)
                g.items = updated_items
                updated_groups.append(g)
        state.update_dedup_job(job_id, groups=updated_groups, groups_count=len(updated_groups))

    return {"removed": list(all_removed_paths), "count": len(all_removed_paths)}


def _run_dedup(job_id: str, paths: list[str], stages_config: list | None):
    job = state.get_dedup_job(job_id)
    if not job:
        return

    state.update_dedup_job(job_id, status="running", stage="初始化")

    settings = state.get_settings()

    path_objs = [Path(p) for p in paths]
    families = _build_file_families(path_objs)
    family_paths = _families_to_paths(families)

    try:
        if stages_config:
            from src.dedup.composite import HierarchicalDeduplicator
            dedup = HierarchicalDeduplicator(stages=stages_config)
            state.update_dedup_job(job_id, stage="层次化去重")
            result = dedup.deduplicate(family_paths)
            groups = _convert_hierarchical(result, families)
        else:
            from src.dedup.composite import CompositeDeduplicator
            enabled_stages = [s for s in settings.dedup_stages if s.enabled]
            dedup = CompositeDeduplicator(
                use_exif=any(s.type == "exif" for s in enabled_stages),
                use_phash=any(s.type == "phash" for s in enabled_stages),
                use_embedding=any(s.type == "embedding" for s in enabled_stages),
            )
            state.update_dedup_job(job_id, stage="组合去重")
            raw_groups = dedup.group_all(family_paths)
            groups = _convert_groups(raw_groups, families)

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


def _convert_groups(raw_groups, families: dict[str, list[Path]] | None = None) -> list[DedupGroup]:
    result = []
    for i, g in enumerate(raw_groups):
        items = []
        for item in g.items:
            siblings = _get_all_siblings(item.path, families) if families else []
            items.append(DedupItem(
                path=str(item.path),
                file_name=item.path.name,
                thumbnail_url=f"/api/thumbnails?path={_encode_path(str(item.path))}",
                file_size=item.file_size or (item.path.stat().st_size if item.path.exists() else 0),
                similarity=0.0,
                siblings=siblings,
            ))
        result.append(DedupGroup(
            group_id=g.group_id if hasattr(g, "group_id") and g.group_id else f"g_{i}",
            items=items,
            representative=str(g.representative.path) if g.representative else (str(items[0].path) if items else None),
            stage="composite",
        ))
    return result


def _convert_hierarchical(result: dict, families: dict[str, list[Path]] | None = None) -> list[DedupGroup]:
    groups = []
    for entry in result.get("groups", []):
        raw_group = entry["group"]
        stage_name = entry["stage"]
        items = []
        for item in raw_group.items:
            siblings = _get_all_siblings(item.path, families) if families else []
            items.append(DedupItem(
                path=str(item.path),
                file_name=item.path.name,
                thumbnail_url=f"/api/thumbnails?path={_encode_path(str(item.path))}",
                file_size=item.file_size or (item.path.stat().st_size if item.path.exists() else 0),
                similarity=0.0,
                siblings=siblings,
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


RAW_FORMATS = {".cr2", ".cr3", ".arw", ".dng", ".nef", ".orf", ".rw2", ".pef"}


def _get_stem(path: Path) -> str:
    return path.stem


def _build_file_families(paths: list[Path]) -> dict[str, list[Path]]:
    families: dict[str, list[Path]] = {}
    for p in paths:
        stem = _get_stem(p)
        if stem not in families:
            families[stem] = []
        families[stem].append(p)
    for stem in families:
        families[stem].sort(key=lambda x: (0 if x.suffix.lower() == ".jpg" else 1, x.suffix.lower()))
    return families


def _families_to_paths(families: dict[str, list[Path]]) -> list[Path]:
    return [family[0] for family in families.values()]


def _get_all_siblings(path: Path, families: dict[str, list[Path]]) -> list[str]:
    stem = _get_stem(path)
    return [str(p) for p in families.get(stem, [])]
