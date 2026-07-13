import json
import uuid
import time
import threading
import os
from pathlib import Path
from typing import Optional

from .models import DirEntry, AppSettings, AnalysisResult, AnalysisJob, DedupJob

DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DATA_DIR.mkdir(exist_ok=True)

_DIRS_FILE = DATA_DIR / "dirs.json"
_SETTINGS_FILE = DATA_DIR / "settings.json"
_RESULTS_FILE = DATA_DIR / "results.json"
_DEDUP_JOBS_FILE = DATA_DIR / "dedup_jobs.json"

FOLDER_CACHE_DIR_NAME = ".photoanalyzer"
RESULTS_FILE_NAME = "results.json"


def _normalize_dir_path(path: str) -> str:
    return os.path.normcase(os.path.normpath(str(Path(path).resolve())))


def _find_base_dir(file_path: str) -> Optional[Path]:
    p = Path(file_path).resolve()
    for parent in [p.parent] + list(p.parents):
        if (parent / FOLDER_CACHE_DIR_NAME).exists():
            return parent
    return None


def _get_results_folder(file_path: str) -> tuple[str, Path]:
    base = _find_base_dir(file_path)
    if base:
        rel = str(Path(file_path).resolve().relative_to(base))
        return rel, base / FOLDER_CACHE_DIR_NAME
    p = Path(file_path).resolve()
    return str(p), DATA_DIR / "orphan_results"


def _get_results_folder_with_base(file_path: str, base_dir: str) -> tuple[str, Path]:
    base = Path(base_dir).resolve()
    p = Path(file_path).resolve()
    try:
        rel = str(p.relative_to(base))
    except ValueError:
        rel = str(p)
    return rel, base / FOLDER_CACHE_DIR_NAME


def _load_json(path: Path, default=None):
    if not path.exists():
        return default if default is not None else {}
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def _save_json(path: Path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)


class AppState:
    def __init__(self):
        self._lock = threading.Lock()
        self._dirs: dict[str, DirEntry] = {}
        self._settings: AppSettings = AppSettings()
        self._results: list[AnalysisResult] = []
        self._analysis_jobs: dict[str, AnalysisJob] = {}
        self._dedup_jobs: dict[str, DedupJob] = {}
        self._load()

    def _load(self):
        dirs_data = _load_json(_DIRS_FILE, {})
        normalized_seen: set[str] = set()
        deduped_dirs: dict[str, DirEntry] = {}
        for k, v in dirs_data.items():
            entry = DirEntry(**v)
            norm = _normalize_dir_path(entry.path)
            if norm in normalized_seen:
                continue
            normalized_seen.add(norm)
            deduped_dirs[k] = entry
        self._dirs = deduped_dirs
        if len(self._dirs) != len(dirs_data):
            self._persist_dirs()

        settings_data = _load_json(_SETTINGS_FILE, None)
        if settings_data:
            self._settings = AppSettings(**settings_data)

        results_data = _load_json(_RESULTS_FILE, [])
        if self._settings.storage_mode == "folder":
            self._results = []
        else:
            self._results = [AnalysisResult(**r) for r in results_data]

        dedup_jobs_data = _load_json(_DEDUP_JOBS_FILE, {})
        for k, v in dedup_jobs_data.items():
            self._dedup_jobs[k] = DedupJob(**v)

        from src.dedup.cache import cache as feature_cache
        feature_cache.mode = self._settings.storage_mode

    def _persist_dirs(self):
        _save_json(_DIRS_FILE, {k: v.model_dump() for k, v in self._dirs.items()})

    def _persist_settings(self):
        _save_json(_SETTINGS_FILE, self._settings.model_dump())

    def _persist_results(self):
        if self._settings.storage_mode == "folder":
            return
        _save_json(_RESULTS_FILE, [r.model_dump() for r in self._results])

    def _persist_dedup_jobs(self):
        _save_json(_DEDUP_JOBS_FILE, {k: v.model_dump() for k, v in self._dedup_jobs.items()})

    # --- Dirs ---
    def list_dirs(self) -> list[DirEntry]:
        return list(self._dirs.values())

    def add_dir(self, path: str, name: Optional[str] = None) -> DirEntry:
        with self._lock:
            normalized_target = _normalize_dir_path(path)
            for existing in self._dirs.values():
                if _normalize_dir_path(existing.path) == normalized_target:
                    return existing

            dir_id = uuid.uuid4().hex[:12]
            entry = DirEntry(
                id=dir_id,
                path=path,
                name=name or Path(path).name,
                added_at=time.strftime("%Y-%m-%dT%H:%M:%S"),
            )
            self._dirs[dir_id] = entry
            self._persist_dirs()
            return entry

    def get_dir(self, dir_id: str) -> Optional[DirEntry]:
        return self._dirs.get(dir_id)

    def remove_dir(self, dir_id: str) -> bool:
        with self._lock:
            if dir_id in self._dirs:
                del self._dirs[dir_id]
                self._persist_dirs()
                return True
            return False

    # --- Settings ---
    def get_settings(self) -> AppSettings:
        return self._settings

    def update_settings(self, updates: dict) -> AppSettings:
        with self._lock:
            for k, v in updates.items():
                if hasattr(self._settings, k):
                    setattr(self._settings, k, v)
            if "storage_mode" in updates:
                from src.dedup.cache import cache as feature_cache
                feature_cache.mode = updates["storage_mode"]
            self._persist_settings()
            return self._settings

    # --- Results ---
    def list_results(self) -> list[AnalysisResult]:
        if self._settings.storage_mode == "folder":
            all_results: list[AnalysisResult] = []

            def _append_results_from_file(results_file: Path):
                data = _load_json(results_file, {})
                if isinstance(data, dict):
                    iterable = data.values()
                elif isinstance(data, list):
                    iterable = data
                else:
                    iterable = []
                for r_data in iterable:
                    try:
                        all_results.append(AnalysisResult(**r_data))
                    except Exception:
                        pass

            for d in self._dirs.values():
                p = Path(d.path)
                if p.exists():
                    cache_dir = p / FOLDER_CACHE_DIR_NAME
                    results_file = cache_dir / RESULTS_FILE_NAME
                    if results_file.exists():
                        _append_results_from_file(results_file)

            # Also include standalone-file results that are saved under data/orphan_results.
            orphan_results_file = DATA_DIR / "orphan_results" / RESULTS_FILE_NAME
            if orphan_results_file.exists():
                _append_results_from_file(orphan_results_file)

            # De-duplicate by normalized file path to avoid repeated items.
            deduped: dict[str, AnalysisResult] = {}
            for r in all_results:
                normalized = os.path.normcase(os.path.normpath(r.file_path))
                deduped[normalized] = r
            return list(deduped.values())
        return self._results

    def add_results(self, results: list[AnalysisResult], base_dir: Optional[str] = None):
        with self._lock:
            if self._settings.storage_mode == "folder":
                folder_results: dict[Path, dict[str, dict]] = {}

                def _match_registered_base(file_path: str) -> Optional[str]:
                    p = Path(file_path).resolve()
                    candidates: list[Path] = []
                    for d in self._dirs.values():
                        try:
                            base = Path(d.path).resolve()
                            p.relative_to(base)
                            candidates.append(base)
                        except Exception:
                            continue
                    if not candidates:
                        return None
                    # Prefer the most specific registered directory.
                    best = max(candidates, key=lambda x: len(str(x)))
                    return str(best)

                for r in results:
                    effective_base_dir = base_dir or _match_registered_base(r.file_path)
                    if effective_base_dir:
                        rel, folder = _get_results_folder_with_base(r.file_path, effective_base_dir)
                    else:
                        rel, folder = _get_results_folder(r.file_path)
                    if folder not in folder_results:
                        folder_results[folder] = _load_json(folder / RESULTS_FILE_NAME, {})
                    folder_results[folder][rel] = r.model_dump()
                for folder, data in folder_results.items():
                    _save_json(folder / RESULTS_FILE_NAME, data)
                return

            existing_paths = {r.file_path for r in self._results}
            for r in results:
                if r.file_path in existing_paths:
                    self._results = [x for x in self._results if x.file_path != r.file_path]
                self._results.append(r)
            self._persist_results()

    # --- Analysis Jobs ---
    def create_analysis_job(self, total: int) -> AnalysisJob:
        job_id = uuid.uuid4().hex[:12]
        job = AnalysisJob(
            job_id=job_id,
            status="pending",
            total=total,
            progress=0,
            results=[],
            created_at=time.strftime("%Y-%m-%dT%H:%M:%S"),
        )
        self._analysis_jobs[job_id] = job
        return job

    def get_analysis_job(self, job_id: str) -> Optional[AnalysisJob]:
        return self._analysis_jobs.get(job_id)

    def update_analysis_job(self, job_id: str, **kwargs):
        with self._lock:
            job = self._analysis_jobs.get(job_id)
            if job:
                for k, v in kwargs.items():
                    if hasattr(job, k):
                        setattr(job, k, v)

    # --- Dedup Jobs ---
    def create_dedup_job(self, total_files: int, dir_id: Optional[str] = None, dir_path: Optional[str] = None) -> DedupJob:
        job_id = uuid.uuid4().hex[:12]
        job = DedupJob(
            job_id=job_id,
            status="pending",
            total_files=total_files,
            groups_count=0,
            groups=[],
            dir_id=dir_id,
            dir_path=dir_path,
            created_at=time.strftime("%Y-%m-%dT%H:%M:%S"),
        )
        self._dedup_jobs[job_id] = job
        self._persist_dedup_jobs()
        return job

    def get_dedup_job(self, job_id: str) -> Optional[DedupJob]:
        return self._dedup_jobs.get(job_id)

    def get_dedup_jobs_by_dir(self, dir_id: str) -> list[DedupJob]:
        return [j for j in self._dedup_jobs.values() if j.dir_id == dir_id and j.status == "completed"]

    def get_dedup_jobs_by_path(self, dir_path: str) -> list[DedupJob]:
        return [j for j in self._dedup_jobs.values() if j.dir_path == dir_path and j.status == "completed"]

    def get_latest_dedup_job(self, dir_id: Optional[str] = None, dir_path: Optional[str] = None) -> Optional[DedupJob]:
        jobs: list[DedupJob] = []
        if dir_id:
            jobs = self.get_dedup_jobs_by_dir(dir_id)
        if not jobs and dir_path:
            jobs = self.get_dedup_jobs_by_path(dir_path)
        if not jobs and dir_path:
            import os
            norm_target = os.path.normcase(os.path.normpath(dir_path))
            for j in self._dedup_jobs.values():
                if j.status != "completed":
                    continue
                if j.dir_id == dir_id or (j.dir_path and os.path.normcase(os.path.normpath(j.dir_path)) == norm_target):
                    jobs.append(j)
                    continue
                for g in j.groups:
                    for item in g.items:
                        item_norm = os.path.normcase(os.path.normpath(os.path.dirname(item.path)))
                        if item_norm == norm_target or item_norm.startswith(norm_target + os.sep):
                            jobs.append(j)
                            break
                    else:
                        continue
                    break
        if not jobs:
            return None
        return max(jobs, key=lambda j: j.created_at)

    def update_dedup_job(self, job_id: str, **kwargs):
        with self._lock:
            job = self._dedup_jobs.get(job_id)
            if job:
                for k, v in kwargs.items():
                    if hasattr(job, k):
                        setattr(job, k, v)
                self._persist_dedup_jobs()

    # --- Stats ---
    def get_stats(self) -> dict:
        dir_path_set: set[str] = set()
        total_photos = 0
        for d in self._dirs.values():
            p = Path(d.path)
            if p.exists():
                dir_path_set.add(d.path)
                for f in p.rglob("*"):
                    if f.is_file() and f.suffix.lower() in {".jpg", ".jpeg", ".png", ".gif", ".bmp", ".webp", ".tiff"}:
                        total_photos += 1

        dup_groups = sum(
            j.groups_count for j in self._dedup_jobs.values() if j.status == "completed"
        )

        return {
            "total_photos": total_photos,
            "analyzed_photos": len([r for r in self._results if r.success]),
            "duplicate_groups": dup_groups,
            "directories": len(self._dirs),
        }


state = AppState()
