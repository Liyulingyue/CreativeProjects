import json
import uuid
import time
import threading
from pathlib import Path
from typing import Optional

from .models import DirEntry, AppSettings, AnalysisResult, AnalysisJob, DedupJob

DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DATA_DIR.mkdir(exist_ok=True)

_DIRS_FILE = DATA_DIR / "dirs.json"
_SETTINGS_FILE = DATA_DIR / "settings.json"
_RESULTS_FILE = DATA_DIR / "results.json"
_DEDUP_JOBS_FILE = DATA_DIR / "dedup_jobs.json"


def _load_json(path: Path, default=None):
    if not path.exists():
        return default if default is not None else {}
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def _save_json(path: Path, data):
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
        for k, v in dirs_data.items():
            self._dirs[k] = DirEntry(**v)

        settings_data = _load_json(_SETTINGS_FILE, None)
        if settings_data:
            self._settings = AppSettings(**settings_data)

        results_data = _load_json(_RESULTS_FILE, [])
        self._results = [AnalysisResult(**r) for r in results_data]

        dedup_jobs_data = _load_json(_DEDUP_JOBS_FILE, {})
        for k, v in dedup_jobs_data.items():
            self._dedup_jobs[k] = DedupJob(**v)

    def _persist_dirs(self):
        _save_json(_DIRS_FILE, {k: v.model_dump() for k, v in self._dirs.items()})

    def _persist_settings(self):
        _save_json(_SETTINGS_FILE, self._settings.model_dump())

    def _persist_results(self):
        _save_json(_RESULTS_FILE, [r.model_dump() for r in self._results])

    def _persist_dedup_jobs(self):
        _save_json(_DEDUP_JOBS_FILE, {k: v.model_dump() for k, v in self._dedup_jobs.items()})

    # --- Dirs ---
    def list_dirs(self) -> list[DirEntry]:
        return list(self._dirs.values())

    def add_dir(self, path: str, name: Optional[str] = None) -> DirEntry:
        with self._lock:
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
            self._persist_settings()
            return self._settings

    # --- Results ---
    def list_results(self) -> list[AnalysisResult]:
        return self._results

    def add_results(self, results: list[AnalysisResult]):
        with self._lock:
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
    def create_dedup_job(self, total_files: int) -> DedupJob:
        job_id = uuid.uuid4().hex[:12]
        job = DedupJob(
            job_id=job_id,
            status="pending",
            total_files=total_files,
            groups_count=0,
            groups=[],
            created_at=time.strftime("%Y-%m-%dT%H:%M:%S"),
        )
        self._dedup_jobs[job_id] = job
        self._persist_dedup_jobs()
        return job

    def get_dedup_job(self, job_id: str) -> Optional[DedupJob]:
        return self._dedup_jobs.get(job_id)

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
