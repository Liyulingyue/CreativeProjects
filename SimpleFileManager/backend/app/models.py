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
    # Dedicated embedding endpoint for the search/chat path. Falls back to
    # the indexer endpoint when unset, so cloud-only deployments keep
    # working. On this box we point it at a GPU-backed llama-server so user
    # queries have a dedicated low-latency path independent of the indexer's
    # batched work on the A520 LITTLE cores.
    search_api_key: str = ""
    search_base_url: str = ""
    search_model: str = ""
    # Optional fast-path LLM. When the user toggles "fast" on a chat
    # turn, /api/agent/chat/stream reads LLM_FAST_BASE_URL and routes that
    # single turn to the smaller model. Defaults to the 350M llama-server
    # on 9013 that LLMServices/serve.sh brings up. The fields are kept
    # blank by default so a cloud-only deployment stays working without
    # the optional service.
    llm_fast_api_key: str = ""
    llm_fast_base_url: str = "http://127.0.0.1:9013/v1"
    # Default matches the GGUF that LLMServices/serve.sh auto-loads on
    # 9013 (see pick_llm_fast_model there). Keeping the basename
    # rather than the absolute path so the same default works for
    # local dev, remote deploys, and docker — the OpenAI client sends
    # this verbatim and llama-server matches by file basename. Override
    # here if you point the fast service at a different GGUF.
    llm_fast_model: str = "LFM2.5-350M-Q4_0.gguf"
    index_interval: int = 300
    auto_index_enabled: bool = True
    index_debounce_seconds: int = 10
    # "all", or an "HH:MM-HH:MM" window during which background indexing may
    # run. May wrap past midnight (23:00-07:00).
    index_active_hours: str = "all"
    max_agent_steps: int = 8
    storage_path: str = "./data"
    theme: str = "light"
    auto_digest_enabled: bool = False
    auto_digest_mode: str = "scheduled"  # scheduled | interval
    auto_digest_hour: int = 23
    auto_digest_interval_hours: int = 6
    max_context_tokens: int = 32768
    # Ceiling on a single model reply. Left unset, llama-server generates until
    # EOS or until it exhausts the slot's context, which in the agent loop
    # silently eats the room the next turn needs. The budget covers the model's
    # reasoning tokens too, so values below ~1024 starve the visible answer.
    max_output_tokens: int = 4096
    # Digests summarise a whole day of changes, so they get a larger budget.
    digest_max_tokens: int = 8192


class IndexStats(BaseModel):
    total_files: int
    total_dirs: int
    indexed_files: int
    indexed_dirs: int
    last_index_time: Optional[str] = None
    storage_used: int
    # Files that exist but cannot be searched, split by cause so the user can
    # tell "we do not parse this format" from "this file failed to parse".
    unsupported_count: int = 0
    failed_count: int = 0


class SkippedFiles(BaseModel):
    unsupported: list[dict] = []
    failed: list[dict] = []


class IndexStatus(BaseModel):
    is_indexing: bool
    progress: float
    current_path: Optional[str] = None


class ChatMessage(BaseModel):
    id: str
    role: str
    content: str
    # Ordered reasoning/tool transcript. Persisting it keeps the interleaving
    # ("think, tool, think, tool, answer") that a flat reasoning blob cannot
    # express, so a reopened conversation still shows what ran in what order.
    activity: list[dict] = []
    # Flat scratchpad, kept so messages saved before `activity` still render.
    reasoning: str = ""
    sources: Optional[list[dict]] = None
    timestamp: int
    # Per-turn performance, captured by the agent SSE generator. Defaults are 0
    # so messages saved before the perf columns existed still parse; user
    # messages always have zeros (the request is one-shot, not streamed).
    ttft_ms: int = 0          # request start -> first "token" event
    total_ms: int = 0         # request start -> "done" event
    tokens: int = 0           # streamed tokens for the assistant turn
    tps: float = 0.0          # tokens / (total - ttft) over the decode phase
    # True when the assistant reply came from the fast-path LLM (350M
    # instead of 24B). The chat UI renders a "⚡快" badge when this is
    # set. Defaults to False for legacy rows and for all user messages.
    use_fast: bool = False


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
    activity: list[dict] = []
    reasoning: str = ""
    sources: Optional[list[dict]] = None
    # Per-turn performance; same semantics as ChatMessage. The agent endpoint
    # measures these server-side and the frontend forwards them; chat-query
    # (non-streaming) leaves them at 0.
    ttft_ms: int = 0
    total_ms: int = 0
    tokens: int = 0
    tps: float = 0.0
    # True when the assistant message was produced by the fast-path LLM.
    # The frontend always sends this; older clients may omit it, in
    # which case Pydantic defaults to False.
    use_fast: bool = False


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
    # Files whose vectorisation is still outstanding
    pending_vector_files: int = 0
    # The last scan stood down because the user was chatting or searching
    yielded_for_interaction: bool = False
    # Manual pause. Seconds remaining until indexing resumes; 0 when not paused.
    paused_remaining: int = 0
    paused_indefinitely: bool = False
    active_hours: str = "all"
    in_active_hours: bool = True


# ---- Auth ----

class LoginRequest(BaseModel):
    password: str


class LoginResponse(BaseModel):
    token: str
    requires_auth: bool


class ChangePasswordRequest(BaseModel):
    old_password: str
    new_password: str


class AuthStatus(BaseModel):
    requires_auth: bool
    has_password: bool
