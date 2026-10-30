export interface FileNode {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified: string;
  created: string;
  extension: string;
  mime_type: string;
}

export interface BrowseResult {
  current_path: string;
  parent_path: string | null;
  items: FileNode[];
  total_count: number;
  dirs_count: number;
  files_count: number;
  /** Free space on the disk holding the storage root. */
  disk?: {
    ok: boolean;
    total?: number;
    used?: number;
    free?: number;
    used_pct?: number;
    detail?: string;
  } | null;
  /**
   * A key for the little pictures of this folder. A picture element
   * cannot send an Authorization header, so the key is in the URL --
   * and it opens the pictures of this folder and nothing else.
   */
  thumbnail_token?: string | null;
  /** Which of these files the person has starred. */
  favorite_paths?: string[];
}

export interface FileOperation {
  success: boolean;
  message: string;
  path?: string;
}

export interface SearchResult {
  items: FileNode[];
  total: number;
  query: string;
}

/**
 * One configured LLM endpoint. The is_main flag is a singleton — at
 * most one provider carries it, and the chat UI uses it as the
 * default for newly-created sessions. The chat dropdown lets the
 * user pick any provider (main or otherwise) per turn.
 *
 * `max_tokens` is an optional per-provider output cap. When set, the
 * effective limit is min(provider.max_tokens, system_max_tokens). When
 * null the system ceiling applies by itself.
 */
export interface LLMProvider {
  id: string;
  name: string;
  base_url: string;
  model: string;
  api_key: string;
  is_main: boolean;
  max_tokens: number | null;
  /** Cap on the model's thinking, in tokens. -1 = uncapped. */
  reasoning_budget: number;
  created_at: number;
  updated_at: number;
}

/** Payload for POST /api/providers. The server assigns id and timestamps. */
export type LLMProviderCreate = Omit<LLMProvider, 'id' | 'is_main' | 'created_at' | 'updated_at'>;

/** Payload for PATCH /api/providers/{id}. All fields optional; is_main
 *  triggers the singleton-promote/demote logic in the service. */
export type LLMProviderUpdate = Partial<LLMProviderCreate> & { is_main?: boolean };

/**
 * One configured embedding endpoint. Two role flags hang off the
 * row: `is_index` (the background indexer reads this, singleton
 * — at least one row must always carry it) and `is_search`
 * (the runtime search/chat path reads this, may be empty).
 * `dim` is the operator-declared dimension; "AUTO" lets the
 * server probe on first use, an explicit integer pins it so a
 * dim change requires an explicit (and visible) reindex.
 */
export interface EmbeddingProvider {
  id: string;
  name: string;
  base_url: string;
  model: string;
  dim: string;
  api_key: string;
  is_index: boolean;
  is_search: boolean;
  created_at: number;
  updated_at: number;
}

/** Payload for POST /api/embedding_providers. */
export type EmbeddingProviderCreate = Omit<
  EmbeddingProvider,
  'id' | 'is_index' | 'is_search' | 'created_at' | 'updated_at' | 'dim'
> & { dim?: string };

/** Payload for PATCH /api/embedding_providers/{id}. All fields optional;
 *  is_index / is_search trigger the singleton-promote/demote logic. */
export type EmbeddingProviderUpdate = Partial<EmbeddingProviderCreate> & {
  is_index?: boolean;
  is_search?: boolean;
};

export interface AppSettings {
  // Embedding endpoints (base_url / model / dim / api_key) and the
  // dedicated search override moved to the embedding_providers table
  // (CRUD at /api/embedding_providers, rendered in /settings). Same
  // shape as the LLM refactor: two role singletons (is_index,
  // is_search) instead of one.
  index_interval: number;
  auto_index_enabled: boolean;
  index_debounce_seconds: number;
  index_active_hours: string;
  max_agent_steps: number;
  storage_path: string;
  theme: string;
  auto_digest_enabled: boolean;
  auto_digest_mode: 'scheduled' | 'interval';
  auto_digest_hour: number;
  auto_digest_interval_hours: number;
  max_context_tokens: number;
  max_output_tokens: number;
  digest_max_tokens: number;
  /**
   * Toggles LFM2.5-A2B's thinking/scratchpad mode. LFM does not honour
   * request-level thinking-off, so flipping this causes the backend to
   * write THINKING_ENABLED to .env and call `serve.sh restart`. The chat
   * UI does not need to do anything special — when the restarted server
   * stops emitting reasoning_content, the activity panel naturally
   * collapses to tool traces only.
   */
  thinking_enabled: boolean;
}

export interface AutoIndexStatus {
  enabled: boolean;
  running: boolean;
  interval: number;
  debounce_seconds: number;
  last_scan_time: string | null;
  last_scan_files: number;
  indexed_files: number;
  pending_changes: number;
  pending_vector_files: number;
  yielded_for_interaction: boolean;
  paused_remaining: number;
  paused_indefinitely: boolean;
  active_hours: string;
  in_active_hours: boolean;
  watcher?: {
    watching: boolean;
    filesystem: string;
    directories: number;
    events: number;
    overflows: number;
    degraded: boolean;
    error: string | null;
    since: number | null;
  } | null;
}

export type PlanActionType = 'move' | 'rename' | 'create_folder' | 'delete';

export type PlanStatus =
  | 'pending'
  | 'approved'
  | 'rejected'
  | 'executed'
  | 'executed_with_errors'
  | 'failed';

export interface PlanAction {
  id: string;
  action_type: PlanActionType;
  source_path: string | null;
  target_path: string | null;
  reason: string;
  status: 'pending' | 'done' | 'failed' | 'skipped';
  result: string | null;
}

export interface AgentPlan {
  id: string;
  title: string;
  summary: string;
  status: PlanStatus;
  source: string;
  actions: PlanAction[];
  created_at: number;
  decided_at: number | null;
  executed_at: number | null;
}

export interface Skill {
  id: string;
  label: string;
  description: string;
  instruction?: string;
  enabled: boolean;
  builtin: boolean;
}

export interface SkillsState {
  master: boolean;
  skills: Skill[];
}
