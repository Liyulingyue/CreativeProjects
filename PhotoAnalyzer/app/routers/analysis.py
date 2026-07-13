import threading
import time
import os
from pathlib import Path
from fastapi import APIRouter, HTTPException
from ..deps import state, FOLDER_CACHE_DIR_NAME
from ..models import AnalysisResult, AnalysisJob, PhotoAnalysis
from src.config import is_image_file, get_image_files

router = APIRouter(prefix="/api", tags=["analysis"])


@router.post("/analysis", response_model=AnalysisJob)
def start_analysis(body: dict):
    file_paths = body.get("file_paths", [])
    delay = body.get("delay", 0) / 1000.0

    valid_paths = [p for p in file_paths if Path(p).exists() and is_image_file(p)]
    if not valid_paths:
        raise HTTPException(400, "没有有效的图片路径")

    job = state.create_analysis_job(len(valid_paths))
    t = threading.Thread(target=_run_analysis, args=(job.job_id, valid_paths, delay), daemon=True)
    t.start()
    return job


@router.post("/analysis/folder", response_model=AnalysisJob)
def start_folder_analysis(body: dict):
    dir_id = body.get("dir_id")
    sub_path = body.get("sub_path")
    recursive = body.get("recursive", True)
    delay = body.get("delay", 0) / 1000.0

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

    # In folder mode, ensure this analysis target has a local cache directory.
    if state.get_settings().storage_mode == "folder":
        (target / FOLDER_CACHE_DIR_NAME).mkdir(parents=True, exist_ok=True)

    job = state.create_analysis_job(len(paths))
    t = threading.Thread(target=_run_analysis, args=(job.job_id, paths, delay, str(target)), daemon=True)
    t.start()
    return job


@router.get("/analysis/{job_id}", response_model=AnalysisJob)
def get_analysis_job(job_id: str):
    job = state.get_analysis_job(job_id)
    if not job:
        raise HTTPException(404, "任务不存在")
    return job


@router.get("/results", response_model=list[AnalysisResult])
def list_results():
    return state.list_results()


@router.get("/results/{file_path:path}", response_model=AnalysisResult)
def get_result(file_path: str):
    target = os.path.normcase(os.path.normpath(file_path))
    for r in state.list_results():
        current = os.path.normcase(os.path.normpath(r.file_path))
        if current == target:
            return r
    raise HTTPException(404, "结果不存在")


def _run_analysis(job_id: str, paths: list[str], delay: float, base_dir: str | None = None):
    from src.analyzer import PhotoAnalyzer as _PhotoAnalyzer

    job = state.get_analysis_job(job_id)
    if not job:
        return

    state.update_analysis_job(job_id, status="running")

    settings = state.get_settings()
    try:
        analyzer = _PhotoAnalyzer(
            api_key=settings.api_key or None,
            base_url=settings.base_url or None,
            model=settings.model or None,
            delay_between_requests=delay or settings.delay / 1000.0,
        )
    except Exception as e:
        state.update_analysis_job(job_id, status="failed", finished_at=time.strftime("%Y-%m-%dT%H:%M:%S"))
        return

    all_results: list[AnalysisResult] = []
    for i, p in enumerate(paths):
        state.update_analysis_job(
            job_id,
            progress=i,
            current_file=Path(p).name,
        )

        raw = analyzer.analyze_image(p)
        result = _convert_result(raw)
        all_results.append(result)

    state.update_analysis_job(
        job_id,
        status="completed",
        progress=len(paths),
        results=all_results,
        finished_at=time.strftime("%Y-%m-%dT%H:%M:%S"),
    )
    state.add_results(all_results, base_dir=base_dir)


def _convert_result(raw) -> AnalysisResult:
    data = None
    if raw.success and raw.data:
        data = PhotoAnalysis(**raw.data)
    return AnalysisResult(
        file_path=raw.file_path,
        file_name=raw.file_name,
        success=raw.success,
        error=raw.error,
        data=data,
        reasoning=raw.reasoning,
    )
