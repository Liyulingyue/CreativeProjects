from pydantic import BaseModel
from typing import Optional

# Default cap on a model's thinking, in tokens, for a provider that does
# not name one of its own.
#
# A reasoning model is free to think for the whole max_tokens before it
# does anything useful. On this box that is minutes of generation per
# step: one turn spent 10k tokens of thinking and 13 minutes, and the
# budget that actually mattered was the thinking, not the clock -- the
# same question with a 256-token thinking budget ran seven times faster
# and still called its tools.
#
# -1 leaves the model's thinking unrestricted.
DEFAULT_REASONING_BUDGET = 1024


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
    # Free space on the disk holding the storage root, so the browser
    # can say when the disk is nearly full.
    disk: Optional[dict] = None
    # A key for the little pictures of this folder. A picture element
    # cannot send an Authorization header, so the key is in the URL --
    # and it opens the pictures of this folder and nothing else.
    thumbnail_token: Optional[str] = None
    # Which of these files the person has starred, so the browser
    # knows without asking again.
    favorite_paths: list[str] = []


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
    # False sends the file to the recycle bin, where it can be put back.
    # True destroys it outright.
    permanent: bool = False


class SearchRequest(BaseModel):
    query: str
    path: Optional[str] = None
    limit: int = 50


class SearchResult(BaseModel):
    items: list[FileNode]
    total: int
    query: str


class AppSettings(BaseModel):
    # Note: LLM endpoints (base_url / model / api_key) are no longer
    # stored here. They live in the `llm_providers` table now (see
    # /api/providers) and are managed through the Settings UI's
    # Providers list. The router and llm_client read from the table
    # at request time, so flipping a provider is instant — no
    # server-level restart needed.
    #
    # Embedding endpoints have moved to the `embedding_providers`
    # table for the same reason. Two roles hang off the table:
    #   - is_index: which endpoint the background indexer uses.
    #     Singleton — at least one must always exist.
    #   - is_search: optional dedicated endpoint for the runtime
    #     search/chat path. Falls back to the is_index row when
    #     unset. Operators without a GPU search endpoint can leave
    #     it set and search will share the index endpoint.
    # dim must match across the index + search pair; changing it
    # requires re-indexing (the indexer would otherwise reject the
    # next batch as soon as the row goes in).
    index_interval: int = 300
    auto_index_enabled: bool = True
    # How long a deleted file waits in the recycle bin before it is
    # removed for good. 0 keeps everything until the bin is emptied by
    # hand, which is the default: on a family NAS the undo window is
    # worth more than the disk it costs.
    trash_retention_days: int = 0
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
    # Building the picture index is a deliberate job: it uses the same cores
    # as the model, so it runs in a window instead of all day. The window is
    # "HH:MM-HH:MM"; "all" lets it run to the end.
    image_index_enabled: bool = False
    image_index_window: str = "09:00-09:30"
    max_context_tokens: int = 32768
    # Ceiling on a single model reply. Left unset, llama-server generates until
    # EOS or until it exhausts the slot's context, which in the agent loop
    # silently eats the room the next turn needs. The budget covers the model's
    # reasoning tokens too, so values below ~1024 starve the visible answer.
    max_output_tokens: int = 4096
    # Digests summarise a whole day of changes, so they get a larger budget.
    digest_max_tokens: int = 8192
    # Toggle for LFM2.5-A2B's thinking mode. LFM does not support
    # request-level thinking-off, so flipping this does NOT take effect
    # immediately — the backend writes THINKING_ENABLED to .env and calls
    # `LLMServices/serve.sh restart`, which re-launches llama-server with
    # --reasoning off. Restart is several seconds; in-flight chats on
    # port 9010 will fail. UI must surface this prominently.
    thinking_enabled: bool = True


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


class LLMProvider(BaseModel):
    """One row per LLM endpoint the operator has configured.

    `is_main` is a singleton flag — at most one provider carries it. The
    first provider the system sees (or the first one the operator
    creates on an empty table) is automatically promoted to main, so
    new sessions always have somewhere to land. The UI treats the
    main as the default for fresh sessions; the user can pick a
    different one per turn via the chat dropdown.

    `max_tokens` is an optional per-provider output cap. When set, the
    effective limit is min(provider.max_tokens, system_max_tokens) so
    the provider cap never exceeds the system ceiling but can tighten
    it further. When null the system ceiling applies by itself.

    `reasoning_budget` caps the model's thinking, in tokens. It is a
    property of the model rather than of the app -- a 24B on CPU needs a
    different allowance than a 350M -- so each provider carries its own.
    -1 means the model's thinking is not capped at all.
    """
    id: str
    name: str
    base_url: str
    model: str
    api_key: str = ""
    is_main: bool = False
    max_tokens: Optional[int] = None
    reasoning_budget: int = DEFAULT_REASONING_BUDGET
    created_at: int = 0
    updated_at: int = 0


class LLMProviderCreate(BaseModel):
    name: str
    base_url: str
    model: str
    api_key: str = ""
    max_tokens: Optional[int] = None
    reasoning_budget: Optional[int] = None


class LLMProviderUpdate(BaseModel):
    name: Optional[str] = None
    base_url: Optional[str] = None
    model: Optional[str] = None
    api_key: Optional[str] = None
    # The router enforces the singleton: setting is_main=True unmarks
    # the rest of the table in the same transaction.
    is_main: Optional[bool] = None
    max_tokens: Optional[int] = None
    reasoning_budget: Optional[int] = None


class EmbeddingProvider(BaseModel):
    """One row per embedding endpoint the operator has configured.

    Two role flags hang off the row:
      - `is_index`: the background indexer reads from this row. Singleton.
        At least one provider must always carry it; the system refuses to
        demote the last one.
      - `is_search`: optional dedicated endpoint for the runtime search /
        chat path. Singleton but may be empty — in that case the search
        path transparently falls back to the is_index row.

    `dim` is the operator-declared embedding dimension. "AUTO" lets the
    server probe the model on first startup; an explicit integer pins it
    so a dim change requires an explicit (and visible) reindex. The
    indexer refuses to start a new run when the configured dim disagrees
    with the dim already in vector_db.
    """
    id: str
    name: str
    base_url: str
    model: str
    dim: str = "AUTO"
    api_key: str = ""
    is_index: bool = False
    is_search: bool = False
    created_at: int = 0
    updated_at: int = 0


class EmbeddingProviderCreate(BaseModel):
    name: str
    base_url: str
    model: str
    # Server-side default is "AUTO" so the operator doesn't have to know
    # the dim when adding a new provider; the indexer probes on first use.
    dim: str = "AUTO"
    api_key: str = ""


class EmbeddingProviderUpdate(BaseModel):
    name: Optional[str] = None
    base_url: Optional[str] = None
    model: Optional[str] = None
    dim: Optional[str] = None
    api_key: Optional[str] = None
    # The router layer enforces singletons: setting is_index=True
    # unmarks the rest of the table, and is_index=False is refused when
    # this is the only main. is_search is a simple toggle with no
    # "last one" rule (search can be empty).
    is_index: Optional[bool] = None
    is_search: Optional[bool] = None


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
    # Which provider actually served the reply. Resolved server-side
    # from the request override or the session default at message-
    # send time; never empty for assistant messages. The chat UI
    # looks up the provider by id to render the "via <name>" badge.
    provider_id: Optional[str] = None
    # Plans submitted by the agent during this turn. The chat UI
    # uses these to render an inline "批准并执行" button next to
    # the plan summary so the user doesn't have to navigate to the
    # approval center. None for messages without plans; an empty
    # list for the (rare) case where the agent called submit_plan
    # but the call failed and no plan was recorded.
    plans: Optional[list[dict]] = None
    # Legacy field kept around so old rows (saved before the providers
    # refactor) still parse. New writes never set it.
    use_fast: bool = False


class ChatSession(BaseModel):
    id: str
    title: str = ""
    messages: list[ChatMessage] = []
    updated_at: int
    session_type: str = "chat"
    # Default provider for this session. Set on session create from
    # the main provider, then mutable from the chat UI — picking a
    # different provider in the dropdown updates this and all
    # subsequent messages in the session go through it. The agent
    # endpoint reads this when the request does not override.
    provider_id: Optional[str] = None


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
    # Which provider served the reply. Resolved server-side from
    # request override or session default; the chat UI looks the
    # provider up to render the "via <name>" badge.
    provider_id: Optional[str] = None
    # Plans submitted by the agent during this turn. The chat UI
    # uses these to render an inline "批准并执行" button so the
    # user doesn't have to navigate to the approval center.
    plans: Optional[list[dict]] = None
    # Legacy field. New writes never set it; the router ignores it
    # so old messages still parse and new code stays symmetric.
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
    action_type: str  # move | rename | create_folder | delete | run
    source_path: Optional[str] = None
    target_path: Optional[str] = None
    command: Optional[str] = None
    cwd: Optional[str] = None
    reason: str = ""
    status: str = "pending"  # pending | done | failed | skipped
    result: Optional[str] = None


class PlanActionInput(BaseModel):
    action_type: str
    source_path: Optional[str] = None
    target_path: Optional[str] = None
    command: Optional[str] = None
    cwd: Optional[str] = None
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
    # Whether the kernel is telling us about changes instead of us
    # having to look. None when it could not be started here.
    watcher: Optional[dict] = None


# ---- Auth ----

class LoginRequest(BaseModel):
    password: str
    # Empty means the administrator, so an installation that has only
    # ever had one password needs no change.
    username: str = ""


class LoginResponse(BaseModel):
    token: str
    requires_auth: bool
    username: str = ""
    role: str = ""


class ChangePasswordRequest(BaseModel):
    old_password: str
    new_password: str


class AuthStatus(BaseModel):
    requires_auth: bool
    has_password: bool
