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
 */
export interface LLMProvider {
  id: string;
  name: string;
  base_url: string;
  model: string;
  api_key: string;
  is_main: boolean;
  created_at: number;
  updated_at: number;
}

/** Payload for POST /api/providers. The server assigns id and timestamps. */
export type LLMProviderCreate = Omit<LLMProvider, 'id' | 'is_main' | 'created_at' | 'updated_at'>;

/** Payload for PATCH /api/providers/{id}. All fields optional; is_main
 *  triggers the singleton-promote/demote logic in the service. */
export type LLMProviderUpdate = Partial<LLMProviderCreate> & { is_main?: boolean };

export interface AppSettings {
  embedding_api_key: string;
  embedding_base_url: string;
  embedding_model: string;
  embedding_dim: string;
  /**
   * Dedicated embedding endpoint for the search/chat path. By default
   * unset, so a cloud-only deployment keeps working. On this box we
   * point it at the GPU-backed llama-server (port 9012) so user queries
   * have a low-latency path that does not queue behind the indexer's
   * batched work on the A520 LITTLE cores.
   */
  search_api_key: string;
  search_base_url: string;
  search_model: string;
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
