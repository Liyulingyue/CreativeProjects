from pydantic import BaseModel
from typing import Optional


class FileNode(BaseModel):
    name: str
    path: str
    is_dir: bool
    size: int
    modified: str
    created: str
    extension: str
    mime_type: str


class BrowseResult(BaseModel):
    current_path: str
    parent_path: Optional[str] = None
    items: list[FileNode]
    total_count: int
    dirs_count: int
    files_count: int


class FileOperation(BaseModel):
    success: bool
    message: str
    path: Optional[str] = None


class CreateFolderRequest(BaseModel):
    path: str
    name: str


class MoveRequest(BaseModel):
    src: str
    dest: str


class DeleteRequest(BaseModel):
    path: str


class SearchRequest(BaseModel):
    query: str
    path: Optional[str] = None
    limit: int = 50


class SearchResult(BaseModel):
    items: list[FileNode]
    total: int
    query: str


class AppSettings(BaseModel):
    openai_api_key: str = ""
    openai_base_url: str = "https://api.minimaxi.com/v1"
    embedding_model: str = "text-embedding-3-small"
    index_interval: int = 300
    storage_path: str = "/data"
    theme: str = "light"


class IndexStats(BaseModel):
    total_files: int
    total_dirs: int
    indexed_files: int
    indexed_dirs: int
    last_index_time: Optional[str] = None
    storage_used: int


class IndexStatus(BaseModel):
    is_indexing: bool
    progress: float
    current_path: Optional[str] = None
