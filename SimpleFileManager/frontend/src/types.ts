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

export interface AppSettings {
  llm_api_key: string;
  llm_base_url: string;
  llm_model: string;
  embedding_api_key: string;
  embedding_base_url: string;
  embedding_model: string;
  embedding_dim: string;
  index_interval: number;
  auto_index_enabled: boolean;
  index_debounce_seconds: number;
  max_agent_steps: number;
  storage_path: string;
  theme: string;
  auto_digest_enabled: boolean;
  auto_digest_mode: 'scheduled' | 'interval';
  auto_digest_hour: number;
  auto_digest_interval_hours: number;
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
