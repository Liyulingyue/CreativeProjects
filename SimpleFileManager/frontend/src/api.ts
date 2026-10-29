import type {
  AgentPlan, AppSettings, AutoIndexStatus,
  LLMProvider, LLMProviderCreate, LLMProviderUpdate,
  EmbeddingProvider, EmbeddingProviderCreate, EmbeddingProviderUpdate,
  PlanActionType,
} from './types';
import { authFetch, getToken as getAuthToken } from './auth';

const API_BASE = '/api';

export async function fetchBrowse(path?: string): Promise<BrowseResult> {
  const url = path ? `${API_BASE}/fs/browse?path=${encodeURIComponent(path)}` : `${API_BASE}/fs/browse`;
  const res = await authFetch(url);
  if (!res.ok) throw new Error('Failed to fetch directory');
  return res.json();
}

export async function fetchTree(path?: string, depth?: number): Promise<TreeNode> {
  const params = new URLSearchParams();
  if (path) params.set('path', path);
  if (depth !== undefined) params.set('depth', String(depth));
  const res = await authFetch(`${API_BASE}/fs/tree?${params}`);
  if (!res.ok) throw new Error('Failed to fetch tree');
  return res.json();
}

export async function createFolder(path: string, name: string): Promise<FileOperation> {
  const res = await authFetch(`${API_BASE}/fs/create_folder`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path, name }),
  });
  if (!res.ok) throw new Error('Failed to create folder');
  return res.json();
}

export async function deletePath(path: string, permanent = false): Promise<FileOperation> {
  const res = await authFetch(`${API_BASE}/fs/delete`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path, permanent }),
  });
  if (!res.ok) throw new Error('Failed to delete');
  return res.json();
}

/** One entry in the recycle bin. */
export interface TrashEntry {
  id: string;
  name: string;
  original_path: string;
  rel_path: string;
  deleted_at: number;
  deleted_at_human: string;
  is_dir: boolean;
  size: number;
  incomplete?: boolean;
}

export interface TrashListing {
  entries: TrashEntry[];
  count: number;
  size: number;
}

export async function fetchTrash(): Promise<TrashListing> {
  const res = await authFetch(`${API_BASE}/fs/trash`);
  if (!res.ok) throw new Error('Failed to fetch the recycle bin');
  return res.json();
}

export async function restoreTrashEntry(id: string): Promise<FileOperation> {
  const res = await authFetch(`${API_BASE}/fs/trash/${id}/restore`, { method: 'POST' });
  if (!res.ok) throw new Error('Failed to restore');
  return res.json();
}

export async function purgeTrashEntry(id: string): Promise<FileOperation> {
  const res = await authFetch(`${API_BASE}/fs/trash/${id}`, { method: 'DELETE' });
  if (!res.ok) throw new Error('Failed to delete permanently');
  return res.json();
}

export async function emptyTrash(): Promise<FileOperation> {
  const res = await authFetch(`${API_BASE}/fs/trash/empty`, { method: 'POST' });
  if (!res.ok) throw new Error('Failed to empty the recycle bin');
  return res.json();
}

export interface BatchResult {
  success: boolean;
  action: string;
  succeeded: string[];
  failed: { path: string; error: string }[];
  count: number;
  failed_count: number;
}

/**
 * Do the same thing to a lot of files in one request.
 *
 * The browser used to send a request per file, which for a thousand
 * files is a thousand round trips and a thousand chances to lose
 * track of where it got to.
 */
export async function batchOperation(
  action: 'delete' | 'move' | 'copy',
  paths: string[],
  dest?: string,
  overwrite: 'never' | 'replace' | 'rename' = 'never',
): Promise<BatchResult> {
  const res = await authFetch(`${API_BASE}/fs/batch`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ action, paths, dest: dest ?? '', overwrite }),
  });
  if (!res.ok) {
    let detail = 'Batch failed';
    try { const body = await res.json(); if (typeof body?.detail === 'string') detail = body.detail; } catch { /* keep default */ }
    throw new Error(detail);
  }
  return res.json();
}

export async function movePath(src: string, dest: string): Promise<FileOperation> {
  const res = await authFetch(`${API_BASE}/fs/move`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ src, dest }),
  });
  if (!res.ok) throw new Error('Failed to move');
  return res.json();
}

export async function searchFiles(query: string, path?: string, limit?: number): Promise<SearchResult> {
  const params = new URLSearchParams({ query });
  if (path) params.set('path', path);
  if (limit) params.set('limit', String(limit));
  const res = await authFetch(`${API_BASE}/search/query?${params}`);
  if (!res.ok) throw new Error('Failed to search');
  return res.json();
}

export async function fetchSettings(): Promise<AppSettings> {
  const res = await authFetch(`${API_BASE}/settings`);
  if (!res.ok) throw new Error('Failed to fetch settings');
  return res.json();
}

export async function updateSettings(updates: Partial<AppSettings>): Promise<AppSettings> {
  const res = await authFetch(`${API_BASE}/settings`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(updates),
  });
  if (!res.ok) throw new Error('Failed to update settings');
  return res.json();
}

export async function checkHealth(): Promise<{ status: string }> {
  const res = await authFetch(`${API_BASE}/health`);
  if (!res.ok) throw new Error('Failed to check health');
  return res.json();
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
}

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

export interface TreeNode {
  name: string;
  path: string;
  is_dir: boolean;
  /** Present on leaves, so a tree click can open the preview directly. */
  size?: number;
  modified?: string;
  children?: TreeNode[];
}

export type { AppSettings };

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  /** Ordered reasoning/tool transcript, persisted so history keeps its shape. */
  activity?: { kind: 'reasoning' | 'tool'; tool?: string; status?: 'running' | 'done'; args?: Record<string, unknown>; result?: unknown; text?: string }[];
  /** Flat scratchpad, kept for older messages saved before `activity`. */
  reasoning?: string;
  sources?: { file_path: string; score: number }[];
  timestamp: number;
  /** Per-turn performance, captured by the agent SSE generator and persisted
   *  via the same /chat_history/messages call. Zeros for user messages and
   *  for legacy rows saved before the perf columns existed. */
  ttft_ms?: number;
  total_ms?: number;
  tokens?: number;
  tps?: number;
  /** Which provider actually served this assistant reply. Resolved
   *  server-side from the request override or the session default;
   *  the chat UI looks the provider up to render the "via <name>"
   *  badge. Empty for user messages. */
  provider_id?: string | null;
  /** Plans submitted by the agent during this turn. Drives the
   *  inline approve-and-execute button rendered next to the chat
   *  bubble so the user doesn't have to navigate to the approval
   *  center for a single-turn plan. */
  plans?: AgentPlan[];
  /** Legacy field kept around so old rows (saved before the providers
   *  refactor) still parse. New writes never set it; the chat UI no
   *  longer reads it. */
  use_fast?: boolean;
}

export interface ChatSession {
  id: string;
  title: string;
  messages: ChatMessage[];
  updated_at: number;
  session_type: string;
  /** Default provider for this session. Set on session create from
   *  the main provider, then mutable from the chat UI — picking a
   *  different provider in the dropdown updates this and all
   *  subsequent messages in the session go through it. The agent
   *  endpoint reads this when the request does not override. */
  provider_id?: string | null;
}

export interface ChatHistoryResponse {
  sessions: ChatSession[];
  current_session_id: string | null;
}

export async function fetchChatSessions(sessionType?: string): Promise<ChatHistoryResponse> {
  const params = sessionType ? `?session_type=${sessionType}` : '';
  const res = await authFetch(`${API_BASE}/chat_history/sessions${params}`);
  if (!res.ok) throw new Error('Failed to fetch chat sessions');
  return res.json();
}

export async function createChatSession(sessionType: string = 'chat'): Promise<ChatSession> {
  const res = await authFetch(`${API_BASE}/chat_history/sessions`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ session_type: sessionType }),
  });
  if (!res.ok) throw new Error('Failed to create chat session');
  return res.json();
}

export async function fetchChatSession(sessionId: string): Promise<ChatSession> {
  const res = await authFetch(`${API_BASE}/chat_history/sessions/${sessionId}`);
  if (!res.ok) throw new Error('Failed to fetch chat session');
  return res.json();
}

export async function deleteChatSession(sessionId: string): Promise<void> {
  const res = await authFetch(`${API_BASE}/chat_history/sessions/${sessionId}`, {
    method: 'DELETE',
  });
  if (!res.ok) throw new Error('Failed to delete chat session');
}

export async function addChatMessage(
  sessionId: string,
  role: 'user' | 'assistant',
  content: string,
  sources?: { file_path: string; score: number }[],
  reasoning?: string,
  activity?: unknown[],
  perf?: { ttft_ms?: number; total_ms?: number; tokens?: number; tps?: number },
  providerId?: string | null,
  plans?: AgentPlan[]
): Promise<ChatMessage> {
  const res = await authFetch(`${API_BASE}/chat_history/messages`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      session_id: sessionId, role, content,
      sources, reasoning: reasoning ?? '', activity: activity ?? [],
      ...(perf ?? {}),
      // Persist which provider served the reply. The chat bubble reads
      // this to render the "via <name>" badge so the user can tell at a
      // glance which model served the reply, including after a reload.
      provider_id: providerId ?? null,
      // Persist agent plans so the bubble can offer an inline
      // "批准并执行" button without the user navigating to the
      // approval center. None for messages without plans.
      plans: plans ?? null,
    }),
  });
  if (!res.ok) throw new Error('Failed to add chat message');
  return res.json();
}

export async function updateChatSessionTitle(sessionId: string, title: string): Promise<void> {
  const res = await authFetch(`${API_BASE}/chat_history/sessions/${sessionId}/title`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ session_id: sessionId, title }),
  });
  if (!res.ok) throw new Error('Failed to update chat session title');
}

export interface AgentMessage {
  id: string;
  role: 'user' | 'assistant' | 'tool';
  content: string;
  tool_calls?: Array<{
    name: string;
    arguments: Record<string, unknown>;
    result?: unknown;
  }>;
  timestamp: number;
}

export interface AgentToolDisplay {
  name: string;
  risk_label: string;
  display: {
    title?: string;
    summary?: string;
    risk?: 'read' | 'approval';
    params?: Record<string, string>;
  };
}

export async function getAgentTools(): Promise<{
  tools: string[];
  definitions: AgentToolDefinition[];
  display: AgentToolDisplay[];
}> {
  const res = await authFetch(`${API_BASE}/agent/tools`);
  if (!res.ok) throw new Error('Failed to get agent tools');
  return res.json();
}

// ---- LLM providers ----

export async function listProviders(): Promise<LLMProvider[]> {
  const res = await authFetch(`${API_BASE}/providers`);
  if (!res.ok) throw new Error('Failed to fetch providers');
  return res.json();
}

export async function createProvider(input: LLMProviderCreate): Promise<LLMProvider> {
  const res = await authFetch(`${API_BASE}/providers`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  if (!res.ok) {
    let detail = 'Failed to create provider';
    try { const body = await res.json(); if (typeof body?.detail === 'string') detail = body.detail; } catch { /* keep default */ }
    throw new Error(detail);
  }
  return res.json();
}

export async function updateProvider(id: string, patch: LLMProviderUpdate): Promise<LLMProvider> {
  const res = await authFetch(`${API_BASE}/providers/${id}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(patch),
  });
  if (!res.ok) {
    let detail = 'Failed to update provider';
    try { const body = await res.json(); if (typeof body?.detail === 'string') detail = body.detail; } catch { /* keep default */ }
    throw new Error(detail);
  }
  return res.json();
}

export async function deleteProvider(id: string): Promise<void> {
  const res = await authFetch(`${API_BASE}/providers/${id}`, { method: 'DELETE' });
  if (!res.ok) {
    let detail = 'Failed to delete provider';
    try { const body = await res.json(); if (typeof body?.detail === 'string') detail = body.detail; } catch { /* keep default */ }
    throw new Error(detail);
  }
}

export async function setMainProvider(id: string): Promise<LLMProvider> {
  const res = await authFetch(`${API_BASE}/providers/${id}/set_main`, { method: 'POST' });
  if (!res.ok) {
    let detail = 'Failed to set main';
    try { const body = await res.json(); if (typeof body?.detail === 'string') detail = body.detail; } catch { /* keep default */ }
    throw new Error(detail);
  }
  return res.json();
}

// ---- Embedding providers ----

export async function listEmbeddingProviders(): Promise<EmbeddingProvider[]> {
  const res = await authFetch(`${API_BASE}/embedding_providers`);
  if (!res.ok) throw new Error('Failed to fetch embedding providers');
  return res.json();
}

export async function createEmbeddingProvider(input: EmbeddingProviderCreate): Promise<EmbeddingProvider> {
  const res = await authFetch(`${API_BASE}/embedding_providers`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  if (!res.ok) {
    let detail = 'Failed to create embedding provider';
    try { const body = await res.json(); if (typeof body?.detail === 'string') detail = body.detail; } catch { /* keep default */ }
    throw new Error(detail);
  }
  return res.json();
}

export async function updateEmbeddingProvider(id: string, patch: EmbeddingProviderUpdate): Promise<EmbeddingProvider> {
  const res = await authFetch(`${API_BASE}/embedding_providers/${id}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(patch),
  });
  if (!res.ok) {
    let detail = 'Failed to update embedding provider';
    try { const body = await res.json(); if (typeof body?.detail === 'string') detail = body.detail; } catch { /* keep default */ }
    throw new Error(detail);
  }
  return res.json();
}

export async function deleteEmbeddingProvider(id: string): Promise<void> {
  const res = await authFetch(`${API_BASE}/embedding_providers/${id}`, { method: 'DELETE' });
  if (!res.ok) {
    let detail = 'Failed to delete embedding provider';
    try { const body = await res.json(); if (typeof body?.detail === 'string') detail = body.detail; } catch { /* keep default */ }
    throw new Error(detail);
  }
}

// ---- Agent streaming (SSE) ----

export type AgentStreamEvent =
  | { type: 'meta'; steps_max: number; provider_id?: string; provider_name?: string }
  | { type: 'step'; step: number; of: number }
  | { type: 'reasoning'; text: string }
  | { type: 'token'; text: string }
  | { type: 'tool_start'; tool: string; args?: Record<string, unknown> }
  | { type: 'tool'; tool: string; args?: Record<string, unknown>; result: unknown }
  | { type: 'done'; response: string; reasoning?: string; plans: AgentPlan[]; steps_used: number; ttft_ms?: number; total_ms?: number; tokens?: number; tps?: number; provider_id?: string; provider_name?: string }
  | { type: 'error'; message: string };

export interface AgentToolParam {
  type: string;
  description?: string;
}

export interface AgentToolDefinition {
  function: {
    name: string;
    description: string;
    parameters?: { properties?: Record<string, AgentToolParam> };
  };
}

/**
 * Consume /agent/chat/stream. `signal` lets the caller abort mid-generation;
 * the backend treats the disconnect as a normal stop and keeps the partial text.
 */
export async function streamAgentMessage(
  message: string,
  sessionId: string | undefined,
  onEvent: (event: AgentStreamEvent) => void,
  signal?: AbortSignal,
  options?: { providerId?: string | null }
): Promise<void> {
  const res = await authFetch(`${API_BASE}/agent/chat/stream`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      message,
      session_id: sessionId,
      // Per-turn provider override. Resolution precedence on the
      // server: this id > session.provider_id > main. A stale or
      // unknown id falls through silently rather than 400-ing.
      provider_id: options?.providerId ?? null,
    }),
    signal,
  });
  if (!res.ok || !res.body) {
    throw new Error(res.status === 401 ? '登录已过期' : `请求失败 (${res.status})`);
  }

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });

    // SSE frames are separated by a blank line
    const frames = buffer.split('\n\n');
    buffer = frames.pop() ?? '';
    for (const frame of frames) {
      let event = 'message';
      const dataLines: string[] = [];
      for (const line of frame.split('\n')) {
        if (line.startsWith('event:')) event = line.slice(6).trim();
        else if (line.startsWith('data:')) dataLines.push(line.slice(5).trim());
      }
      if (!dataLines.length) continue;
      try {
        onEvent({ type: event, ...JSON.parse(dataLines.join('\n')) } as AgentStreamEvent);
      } catch {
        // ignore malformed frame rather than killing the stream
      }
    }
  }
}

// ---- Plans & Approval ----

export async function fetchPlans(status?: string): Promise<AgentPlan[]> {
  const params = status ? `?status=${encodeURIComponent(status)}` : '';
  const res = await authFetch(`${API_BASE}/plans${params}`);
  if (!res.ok) throw new Error('Failed to fetch plans');
  const data = await res.json();
  return data.plans || [];
}

export async function fetchPlan(planId: string): Promise<AgentPlan> {
  const res = await authFetch(`${API_BASE}/plans/${planId}`);
  if (!res.ok) throw new Error('Failed to fetch plan');
  return res.json();
}

export async function fetchPlanLog(
  planId: string
): Promise<Array<Record<string, unknown>>> {
  const res = await authFetch(`${API_BASE}/plans/${planId}/log`);
  if (!res.ok) throw new Error('Failed to fetch plan log');
  const data = await res.json();
  return data.log || [];
}

export interface CreatePlanInput {
  title: string;
  summary?: string;
  source?: string;
  actions: Array<{
    action_type: PlanActionType;
    source_path?: string | null;
    target_path?: string | null;
    reason?: string;
  }>;
}

export async function createPlan(input: CreatePlanInput): Promise<AgentPlan> {
  const res = await authFetch(`${API_BASE}/plans`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  if (!res.ok) throw new Error('Failed to create plan');
  return res.json();
}

/**
 * Turn a non-OK response into an Error carrying the server's own message.
 * The backend explains *why* a call was refused ("Plan is executed, cannot
 * approve", "Plan not found") and that is the part the user needs — a bare
 * "Failed to reject plan" hides both the cause and the difference between a
 * missing plan and a plan in the wrong state.
 */
async function planError(res: Response, fallback: string): Promise<Error> {
  let detail = '';
  try {
    const body = await res.json();
    if (body && typeof body.detail === 'string') detail = body.detail;
  } catch {
    // Not JSON, or the body was already consumed. Keep the fallback.
  }
  if (res.status === 404) return new Error('这个计划已不存在，可能已被删除');
  return new Error(detail || fallback);
}

export async function approvePlan(planId: string): Promise<AgentPlan> {
  const res = await authFetch(`${API_BASE}/plans/${planId}/approve`, { method: 'POST' });
  if (!res.ok) throw await planError(res, 'Failed to approve plan');
  return res.json();
}

export async function rejectPlan(planId: string): Promise<AgentPlan> {
  const res = await authFetch(`${API_BASE}/plans/${planId}/reject`, { method: 'POST' });
  if (!res.ok) throw await planError(res, 'Failed to reject plan');
  return res.json();
}

export async function executePlan(planId: string): Promise<AgentPlan> {
  const res = await authFetch(`${API_BASE}/plans/${planId}/execute`, { method: 'POST' });
  if (!res.ok) throw await planError(res, 'Failed to execute plan');
  return res.json();
}

export async function deletePlan(planId: string): Promise<void> {
  const res = await authFetch(`${API_BASE}/plans/${planId}`, { method: 'DELETE' });
  if (!res.ok) throw await planError(res, 'Failed to delete plan');
}

// ---- Auto Indexer ----

export async function fetchAutoIndexStatus(): Promise<AutoIndexStatus> {
  const res = await authFetch(`${API_BASE}/settings/auto_index_status`);
  if (!res.ok) throw new Error('Failed to fetch auto index status');
  return res.json();
}

// ---- Service Health ----

export interface ServiceProbe {
  ok: boolean;
  detail?: string;
  latency_ms?: number;
}

export async function fetchServiceHealth(): Promise<Record<string, { ok: boolean }>> {
  const res = await authFetch(`${API_BASE}/settings/service_health`);
  if (!res.ok) throw new Error('Failed to fetch service health');
  return res.json();
}

export async function testConnections(): Promise<Record<string, ServiceProbe>> {
  const res = await authFetch(`${API_BASE}/settings/test_connection`, { method: 'POST' });
  if (!res.ok) throw new Error('Failed to test connections');
  return res.json();
}

export interface IndexStats {
  total_files: number;
  total_dirs: number;
  indexed_files: number;
  indexed_dirs: number;
  last_index_time: string | null;
  storage_used: number;
  unsupported_count: number;
  failed_count: number;
}

/** Files present in storage but absent from the index, split by cause. */
export interface IndexSkipped {
  unsupported: { path: string; ext: string }[];
  failed: { path: string; reason: string }[];
  oversized: { path: string; size: string }[];
  /** Keyword-searchable but still being embedded, chunk by chunk. */
  partial_vector: { path: string; done: number; total: number; percent: number }[];
}

export async function fetchIndexStats(): Promise<IndexStats> {
  const res = await authFetch(`${API_BASE}/settings/index_stats`);
  if (!res.ok) throw new Error('Failed to fetch index stats');
  return res.json();
}

export async function fetchIndexSkipped(): Promise<IndexSkipped> {
  const res = await authFetch(`${API_BASE}/settings/index_skipped`);
  if (!res.ok) throw new Error('Failed to fetch skipped files');
  const raw = await res.json() as Partial<IndexSkipped>;
  // Defaults matter: a backend that predates a category returns no key for it,
  // and indexing it as undefined crashed the page into a blank screen.
  return {
    unsupported: raw.unsupported ?? [],
    failed: raw.failed ?? [],
    oversized: raw.oversized ?? [],
    partial_vector: raw.partial_vector ?? [],
  };
}

export async function pauseAutoIndex(seconds: number): Promise<{ paused: boolean; seconds: number }> {
  const res = await authFetch(`${API_BASE}/settings/auto_index/pause`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ seconds }),
  });
  if (!res.ok) throw new Error('Failed to pause indexing');
  return res.json();
}

export async function resumeAutoIndex(): Promise<{ paused: boolean }> {
  const res = await authFetch(`${API_BASE}/settings/auto_index/resume`, { method: 'POST' });
  if (!res.ok) throw new Error('Failed to resume indexing');
  return res.json();
}

export async function runAutoIndexNow(): Promise<Record<string, number>> {
  const res = await authFetch(`${API_BASE}/settings/auto_index/run`, { method: 'POST' });
  if (!res.ok) throw new Error('Failed to run auto index');
  return res.json();
}


export interface RagStreamHandlers {
  onSearching?: () => void;
  onSources?: (sources: { file_path: string; score: number }[]) => void;
  onToken?: (text: string) => void;
  /** The model's scratchpad for the summary, kept separate from the answer. */
  onReasoning?: (text: string) => void;
}

/**
 * Stream the grounded answer from /rag/query/stream.
 *
 * Generation runs at roughly 30 tok/s on CPU, so the answer takes over a
 * minute; streaming leaves the user reading it instead of watching a spinner.
 */
export async function streamRagQuery(
  question: string,
  handlers: RagStreamHandlers,
  topK = 5,
  signal?: AbortSignal
): Promise<string> {
  const res = await authFetch(`${API_BASE}/rag/query/stream`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ question, top_k: topK }),
    signal,
  });
  if (!res.ok || !res.body) throw new Error(`请求失败 (${res.status})`);

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  let answer = '';

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    const frames = buffer.split('\n\n');
    buffer = frames.pop() ?? '';
    for (const frame of frames) {
      let event = 'message';
      const dataLines: string[] = [];
      for (const line of frame.split('\n')) {
        if (line.startsWith('event:')) event = line.slice(6).trim();
        else if (line.startsWith('data:')) dataLines.push(line.slice(5).trim());
      }
      if (!dataLines.length) continue;
      try {
        const payload = JSON.parse(dataLines.join('\n'));
        if (event === 'meta') handlers.onSearching?.();
        else if (event === 'sources') handlers.onSources?.(payload.sources || []);
        else if (event === 'reasoning') handlers.onReasoning?.(payload.text || '');
        else if (event === 'token') { answer += payload.text || ''; handlers.onToken?.(payload.text || ''); }
        else if (event === 'done') answer = payload.answer || answer;
        else if (event === 'error') throw new Error(payload.message);
      } catch (e) {
        if (e instanceof Error && /模型|连接|请求/.test(e.message)) throw e;
      }
    }
  }
  return answer;
}

// ---- Hybrid Search ----

export interface HybridSearchResult {
  path: string;
  name: string;
  preview: string;
  score: number;
  sources: string[];
  /** Which chunk matched — the unit both channels index. */
  chunk_idx: number;
  /** Length of the matched chunk, so the snippet can say how much is behind it. */
  text_len?: number;
}

export interface HybridSearchResponse {
  results: HybridSearchResult[];
  keyword_count: number;
  semantic_count: number;
  query: string;
}

export type SearchMode = 'hybrid' | 'keyword' | 'semantic';

export async function hybridSearch(
  query: string,
  topK: number = 10,
  mode: SearchMode = 'hybrid'
): Promise<HybridSearchResponse> {
  const params = new URLSearchParams({ query, top_k: String(topK), mode });
  const res = await authFetch(`${API_BASE}/search/hybrid?${params}`, { method: 'POST' });
  if (!res.ok) throw new Error('Failed to search');
  return res.json();
}

// ---- Knowledge Digest ----

export interface DigestMeta {
  date: string;
  files_count: number;
  generated_by: string;
  created_at: number;
}

export interface Digest extends DigestMeta {
  content: string | null;
}

export async function generateDigest(date?: string): Promise<{ success: boolean; message?: string; date?: string; files_count?: number }> {
  const params = date ? `?date=${date}` : '';
  const res = await authFetch(`${API_BASE}/digest/generate${params}`, { method: 'POST' });
  if (!res.ok) throw new Error('Failed to generate digest');
  return res.json();
}

export async function fetchLatestDigest(): Promise<Digest> {
  const res = await authFetch(`${API_BASE}/digest/latest`);
  if (!res.ok) throw new Error('Failed to fetch latest digest');
  return res.json();
}

export async function fetchDigestList(): Promise<DigestMeta[]> {
  const res = await authFetch(`${API_BASE}/digest/list`);
  if (!res.ok) throw new Error('Failed to fetch digest list');
  const data = await res.json();
  return data.digests || [];
}

export async function fetchDigest(date: string): Promise<Digest> {
  const res = await authFetch(`${API_BASE}/digest/${date}`);
  if (!res.ok) throw new Error('Failed to fetch digest');
  return res.json();
}

// ---- File Download / Upload / Content ----

export async function downloadZip(path: string): Promise<void> {
  const res = await authFetch(`${API_BASE}/fs/download-zip?path=${encodeURIComponent(path)}`);
  if (!res.ok) throw new Error('Failed to build the archive');
  const blob = await res.blob();
  const url = URL.createObjectURL(blob);
  const name = (path.split(/[/\\]/).pop() || 'archive') + '.zip';
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
}

/**
 * Download a file.
 *
 * Where the browser can write straight to disk, the file is streamed
 * there as it arrives, so a large one does not have to fit in memory.
 * Everywhere else the response is collected and handed over as a
 * blob, which is how it has always worked and is the only thing
 * available.
 */
export async function downloadFile(
  path: string,
  onProgress?: (loaded: number, total: number) => void,
): Promise<void> {
  const res = await authFetch(`${API_BASE}/fs/download?path=${encodeURIComponent(path)}`);
  if (!res.ok) throw new Error('Failed to download');

  const name = path.split(/[/\\]/).pop() || 'download';
  const total = Number(res.headers.get('content-length') || 0);

  // Chromium can write the bytes to a file the user chooses as they
  // arrive. Everything else gets the blob.
  const picker = (window as unknown as {
    showSaveFilePicker?: (opts: unknown) => Promise<{
      createWritable: () => Promise<{
        write: (data: BufferSource) => Promise<void>;
        close: () => Promise<void>;
        abort?: () => Promise<void>;
      }>;
    }>;
  }).showSaveFilePicker;

  if (picker && res.body) {
    let handle;
    try {
      handle = await picker({ suggestedName: name });
    } catch {
      // The user closed the picker. That is an answer, not a failure.
      return;
    }
    const writable = await handle.createWritable();
    const reader = res.body.getReader();
    let loaded = 0;
    try {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        await writable.write(value);
        loaded += value.byteLength;
        onProgress?.(loaded, total);
      }
      await writable.close();
    } catch (e) {
      await writable.abort?.();
      throw e;
    }
    return;
  }

  const blob = await res.blob();
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
  onProgress?.(blob.size, blob.size);
}

/** The piece size for a chunked upload. */
export const UPLOAD_CHUNK_SIZE = 4 * 1024 * 1024;

/** What to do when the name is already taken. */
export type OverwritePolicy = 'never' | 'replace' | 'rename';

export interface UploadConflict {
  message: string;
  existing?: { name: string; size: number; modified?: string };
}

/** A file that is already at the destination. */
export class UploadConflictError extends Error {
  conflict: UploadConflict;
  constructor(conflict: UploadConflict) {
    super(conflict.message);
    this.conflict = conflict;
  }
}

/**
 * Upload one file in pieces.
 *
 * The pieces are kept on the server until the file is finished, so a
 * connection that drops part way through resumes instead of starting
 * over -- which is the difference between a 40MB upload working on a
 * bad connection and not.
 */
export async function uploadFileChunked(
  path: string,
  file: File,
  onProgress?: (loaded: number, total: number) => void,
  overwrite: OverwritePolicy = 'never',
): Promise<{ name: string; path: string; size: number }> {
  const total = Math.max(1, Math.ceil(file.size / UPLOAD_CHUNK_SIZE));
  const initRes = await authFetch(
    `${API_BASE}/fs/upload/init?path=${encodeURIComponent(path)}` +
    `&name=${encodeURIComponent(file.name)}&size=${file.size}&total_chunks=${total}`,
    { method: 'POST' },
  );
  if (!initRes.ok) throw new Error((await detailOf(initRes)) || '无法开始上传');
  const init = await initRes.json();
  const uploadId: string = init.upload_id;

  let sent = 0;
  for (let index = 0; index < total; index++) {
    const start = index * UPLOAD_CHUNK_SIZE;
    const blob = file.slice(start, Math.min(start + UPLOAD_CHUNK_SIZE, file.size));
    const res = await authFetch(
      `${API_BASE}/fs/upload/chunk?upload_id=${uploadId}&index=${index}`,
      { method: 'POST', headers: { 'Content-Type': 'application/octet-stream' }, body: blob },
    );
    if (!res.ok) {
      // The server is still holding the pieces, so the upload can be
      // picked up again rather than thrown away.
      throw new Error((await detailOf(res)) || `第 ${index + 1} 块上传失败`);
    }
    sent += blob.size;
    onProgress?.(sent, file.size);
  }

  const doneRes = await authFetch(
    `${API_BASE}/fs/upload/complete?upload_id=${uploadId}&overwrite=${overwrite}`,
    { method: 'POST' },
  );
  if (doneRes.status === 409) {
    const body = await doneRes.json().catch(() => ({}));
    const detail = body?.detail ?? {};
    throw new UploadConflictError({
      message: detail.message || '同名文件已存在',
      existing: detail.existing,
    });
  }
  if (!doneRes.ok) throw new Error((await detailOf(doneRes)) || '上传失败');
  return doneRes.json();
}

async function detailOf(res: Response): Promise<string> {
  try {
    const body = await res.json();
    const d = body?.detail;
    if (typeof d === 'string') return d;
    if (d?.message) return d.message;
  } catch { /* the body was not json */ }
  return '';
}

/** Throw away an upload that will not be finished, so the server
 *  does not keep its pieces around for a day. */
export async function abandonUpload(uploadId: string): Promise<void> {
  await authFetch(`${API_BASE}/fs/upload?upload_id=${uploadId}`, { method: 'DELETE' }).catch(() => {});
}

export async function uploadFiles(
  path: string,
  files: FileList,
  onProgress?: (loaded: number, total: number) => void,
): Promise<{ success: boolean; uploaded: string[]; count: number }> {
  const formData = new FormData();
  for (const file of Array.from(files)) {
    formData.append('files', file);
  }
  // XMLHttpRequest is used because fetch has no upload progress support
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open('POST', `${API_BASE}/fs/upload?path=${encodeURIComponent(path)}`);
    const token = getAuthToken();
    if (token) xhr.setRequestHeader('Authorization', `Bearer ${token}`);
    xhr.upload.onprogress = (e) => {
      if (e.lengthComputable && onProgress) onProgress(e.loaded, e.total);
    };
    xhr.onload = () => {
      if (xhr.status >= 200 && xhr.status < 300) {
        try { resolve(JSON.parse(xhr.responseText)); } catch { resolve({ success: true, uploaded: [], count: 0 }); }
      } else {
        reject(new Error(`Upload failed: ${xhr.status}`));
      }
    };
    xhr.onerror = () => reject(new Error('Upload failed'));
    xhr.send(formData);
  });
}

export interface Share {
  id: string;
  path?: string;
  name: string;
  is_dir: boolean;
  size: number;
  created_at: number;
  expires_at: number;
  has_password: boolean;
  max_downloads: number;
  downloads: number;
  last_access: number;
  revoked: boolean;
  expired: boolean;
  used_up: boolean;
  active: boolean;
}

/** One earlier version of a file. */
export interface FileVersion {
  id: number;
  size: number;
  created_at: number;
  note: string;
  human: string;
}

export async function fetchVersions(path: string): Promise<{ path: string; versions: FileVersion[] }> {
  const res = await authFetch(`${API_BASE}/fs/versions?path=${encodeURIComponent(path)}`);
  if (!res.ok) throw new Error('读取历史版本失败');
  return res.json();
}

export async function restoreVersion(id: number, path?: string): Promise<{ path: string; size: number }> {
  const q = path ? `?path=${encodeURIComponent(path)}` : '';
  const res = await authFetch(`${API_BASE}/fs/versions/${id}/restore${q}`, { method: 'POST' });
  if (!res.ok) {
    let detail = '还原失败';
    try { const body = await res.json(); if (typeof body?.detail === 'string') detail = body.detail; } catch { /* keep default */ }
    throw new Error(detail);
  }
  return res.json();
}

export async function deleteVersion(id: number): Promise<void> {
  const res = await authFetch(`${API_BASE}/fs/versions/${id}`, { method: 'DELETE' });
  if (!res.ok) throw new Error('删除版本失败');
}

/** Make a file or directory reachable by link. */
export async function createShare(
  path: string,
  expiryDays: number,
  password = '',
  maxDownloads = 0,
): Promise<Share> {
  const res = await authFetch(`${API_BASE}/shares`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path, expiry_days: expiryDays, password, max_downloads: maxDownloads }),
  });
  if (!res.ok) {
    let detail = '创建分享失败';
    try { const body = await res.json(); if (typeof body?.detail === 'string') detail = body.detail; } catch { /* keep default */ }
    throw new Error(detail);
  }
  return res.json();
}

export async function fetchShares(): Promise<{ shares: Share[]; count: number }> {
  const res = await authFetch(`${API_BASE}/shares`);
  if (!res.ok) throw new Error('加载分享列表失败');
  return res.json();
}

export async function revokeShare(id: string): Promise<void> {
  const res = await authFetch(`${API_BASE}/shares/${id}`, { method: 'DELETE' });
  if (!res.ok) throw new Error('停止分享失败');
}

export async function setShareExpiry(id: string, expiryDays: number): Promise<void> {
  const res = await authFetch(`${API_BASE}/shares/${id}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ expiry_days: expiryDays }),
  });
  if (!res.ok) throw new Error('修改有效期失败');
}

/** What a visitor sees when they open a share link. */
export interface PublicShareInfo {
  name: string;
  is_dir: boolean;
  size: number;
  has_password: boolean;
  expires_at: number;
  downloads: number;
  max_downloads: number;
  locked: boolean;
}

/**
 * The public side of a share.
 *
 * These calls carry no token on purpose: the point of a share is that
 * someone without a login can open it.
 */
export async function fetchPublicShare(id: string): Promise<PublicShareInfo> {
  const res = await fetch(`${API_BASE}/share/${encodeURIComponent(id)}`);
  if (!res.ok) {
    let detail = '链接无效';
    try { const body = await res.json(); if (typeof body?.detail === 'string') detail = body.detail; } catch { /* keep default */ }
    throw new Error(detail);
  }
  return res.json();
}

export async function unlockPublicShare(id: string, password: string): Promise<void> {
  const res = await fetch(`${API_BASE}/share/${encodeURIComponent(id)}/unlock`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ password }),
  });
  if (!res.ok) {
    let detail = '密码不正确';
    try { const body = await res.json(); if (typeof body?.detail === 'string') detail = body.detail; } catch { /* keep default */ }
    throw new Error(detail);
  }
}

export async function fetchPublicShareListing(id: string): Promise<{
  name: string;
  items: { name: string; is_dir: boolean; size: number; modified: string }[];
  count: number;
}> {
  const res = await fetch(`${API_BASE}/share/${encodeURIComponent(id)}/list`);
  if (!res.ok) throw new Error('无法读取目录');
  return res.json();
}

/** A file rendered for reading in the browser. */
export interface FilePreviewData {
  name: string;
  kind: 'text' | 'document';
  content: string;
  truncated: boolean;
  /** A spreadsheet's first sheet, as a grid. */
  rows?: string[][] | null;
}

/** The content of a document, extracted on the server. */
export async function fetchFilePreview(path: string): Promise<FilePreviewData> {
  const res = await authFetch(`${API_BASE}/fs/preview?path=${encodeURIComponent(path)}`);
  if (!res.ok) {
    let detail = '无法预览这个文件';
    try {
      const body = await res.json();
      if (typeof body?.detail === 'string') detail = body.detail;
    } catch { /* keep the default */ }
    throw new Error(detail);
  }
  return res.json();
}

/**
 * A short-lived URL for a media file.
 *
 * A <video> or <audio> element cannot send an Authorization header,
 * so the server issues a token that is good for this one file for a
 * few minutes. The player then streams the file, rather than the
 * browser holding all of it in memory first.
 */
export async function fetchStreamUrl(path: string): Promise<string> {
  const res = await authFetch(`${API_BASE}/fs/stream-token`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path }),
  });
  if (!res.ok) throw new Error('无法播放这个文件');
  const data = await res.json();
  return data.url;
}

export interface FileContent {
  path: string;
  name: string;
  content: string;
  offset: number;
  /** null once the whole file has been served. */
  next_offset: number | null;
  total_chars: number;
  size: number;
  mime_type: string;
}

export async function fetchFileContent(path: string, offset = 0, limit = 50000): Promise<FileContent> {
  const params = new URLSearchParams({ path, offset: String(offset), limit: String(limit) });
  const res = await authFetch(`${API_BASE}/fs/content?${params}`);
  if (!res.ok) {
    // Surface the server's reason (e.g. too large) instead of a generic line.
    let detail = 'Failed to fetch file content';
    try {
      const body = await res.json();
      if (typeof body?.detail === 'string') detail = body.detail;
    } catch { /* keep default */ }
    throw new Error(detail);
  }
  return res.json();
}

export async function copyPath(src: string, dest: string): Promise<FileOperation> {
  const res = await authFetch(`${API_BASE}/fs/copy`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ src, dest }),
  });
  if (!res.ok) throw new Error('Failed to copy');
  return res.json();
}
