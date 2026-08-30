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
    llm_api_key: str = ""
    llm_base_url: str = "https://api.minimaxi.com/v1"
    llm_model: str = "gpt-4o-mini"
    embedding_api_key: str = ""
    embedding_base_url: str = "https://api.minimaxi.com/v1"
    embedding_model: str = "text-embedding-3-small"
    embedding_dim: str = "AUTO"
    index_interval: int = 300
    auto_index_enabled: bool = True
    index_debounce_seconds: int = 10
    max_agent_steps: int = 8
    storage_path: str = "./data"
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


class ChatMessage(BaseModel):
    id: str
    role: str
    content: str
    sources: Optional[list[dict]] = None
    timestamp: int


class ChatSession(BaseModel):
    id: str
    title: str = ""
    messages: list[ChatMessage] = []
    updated_at: int
    session_type: str = "chat"


class ChatHistoryResponse(BaseModel):
    sessions: list[ChatSession]
    current_session_id: Optional[str] = None


class CreateSessionRequest(BaseModel):
    session_type: str = "chat"


class AddMessageRequest(BaseModel):
    session_id: str
    role: str
    content: str
    sources: Optional[list[dict]] = None


class UpdateTitleRequest(BaseModel):
    session_id: str
    title: str


class FileSnapshot(BaseModel):
    id: str
    path: str
    name: str
    is_dir: bool
    size: int
    modified: str
    snapshot_date: str


class SnapshotRecord(BaseModel):
    id: str
    date: str
    total_files: int
    total_dirs: int
    files: list[FileSnapshot]


class FileChange(BaseModel):
    path: str
    name: str
    change_type: str
    size: int
    modified: str


class Suggestion(BaseModel):
    id: str
    type: str
    priority: str
    message: str
    source_path: Optional[str] = None
    target_path: Optional[str] = None
    reason: str


class CompareResponse(BaseModel):
    date_from: str
    date_to: str
    added_files: list[FileChange]
    added_dirs: list[FileChange]
    deleted_files: list[FileChange]
    deleted_dirs: list[FileChange]
    suggestions: list[Suggestion]


# ---- Agent Plan & Approval ----

class PlanAction(BaseModel):
    id: str
    action_type: str  # move | rename | create_folder | delete
    source_path: Optional[str] = None
    target_path: Optional[str] = None
    reason: str = ""
    status: str = "pending"  # pending | done | failed | skipped
    result: Optional[str] = None


class PlanActionInput(BaseModel):
    action_type: str
    source_path: Optional[str] = None
    target_path: Optional[str] = None
    reason: str = ""


class AgentPlan(BaseModel):
    id: str
    title: str
    summary: str = ""
    status: str = "pending"  # pending | approved | rejected | executed | executed_with_errors | failed
    source: str = "agent"  # agent | suggestions | manual
    actions: list[PlanAction] = []
    created_at: int
    decided_at: Optional[int] = None
    executed_at: Optional[int] = None


class CreatePlanRequest(BaseModel):
    title: str
    summary: str = ""
    source: str = "manual"
    actions: list[PlanActionInput]


# ---- Auto Indexer ----

class AutoIndexStatus(BaseModel):
    enabled: bool
    running: bool
    interval: int
    debounce_seconds: int
    last_scan_time: Optional[str] = None
    last_scan_files: int = 0
    indexed_files: int = 0
    pending_changes: int = 0
