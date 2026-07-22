from pydantic import BaseModel
from typing import Optional


class DirEntry(BaseModel):
    id: str
    path: str
    name: str
    added_at: str


class FileNode(BaseModel):
    name: str
    path: str
    is_dir: bool
    size: int
    modified: str
    thumbnail_url: Optional[str] = None


class BrowseResult(BaseModel):
    current_path: str
    parent_path: Optional[str] = None
    items: list[FileNode]


class PhotoAnalysis(BaseModel):
    score: int
    style: str
    caption: str
    main_objects: list[str]
    blurry: str
    comments: str
    recommendations: str


class AnalysisResult(BaseModel):
    file_path: str
    file_name: str
    success: bool
    error: Optional[str] = None
    data: Optional[PhotoAnalysis] = None
    reasoning: Optional[str] = None


class AnalysisJob(BaseModel):
    job_id: str
    status: str
    total: int
    progress: int
    current_file: Optional[str] = None
    results: list[AnalysisResult] = []
    created_at: str
    finished_at: Optional[str] = None


class DedupItem(BaseModel):
    path: str
    file_name: str
    thumbnail_url: Optional[str] = None
    file_size: int
    similarity: float = 0.0
    metadata: dict = {}
    siblings: list[str] = []


class DedupGroup(BaseModel):
    group_id: str
    items: list[DedupItem]
    representative: Optional[str] = None
    stage: str


class DedupJob(BaseModel):
    job_id: str
    status: str
    total_files: int
    groups_count: int
    groups: list[DedupGroup] = []
    stage: Optional[str] = None
    dir_id: Optional[str] = None
    dir_path: Optional[str] = None
    created_at: str
    finished_at: Optional[str] = None


class ThumbnailJob(BaseModel):
    job_id: str
    status: str
    total: int
    progress: int = 0
    completed: int = 0
    failed: int = 0
    current_file: Optional[str] = None
    created_at: str
    finished_at: Optional[str] = None


class DedupStageConfig(BaseModel):
    type: str
    enabled: bool = True
    params: dict = {}


class AppSettings(BaseModel):
    api_key: str = ""
    base_url: str = "https://api.minimaxi.com/v1"
    model: str = "MiniMax-M3"
    delay: int = 1000
    storage_mode: str = "folder"
    dedup_stages: list[DedupStageConfig] = [
        DedupStageConfig(type="exif", enabled=True, params={"time_window": 5}),
        DedupStageConfig(type="phash", enabled=True, params={"threshold": 8}),
        DedupStageConfig(type="embedding", enabled=False, params={"model": "clip", "threshold": 0.9}),
    ]


class Stats(BaseModel):
    total_photos: int
    analyzed_photos: int
    duplicate_groups: int
    directories: int
