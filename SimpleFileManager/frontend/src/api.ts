import type { AgentPlan, AppSettings, AutoIndexStatus, PlanActionType } from './types';

const API_BASE = '/api';

export async function fetchBrowse(path?: string): Promise<BrowseResult> {
  const url = path ? `${API_BASE}/fs/browse?path=${encodeURIComponent(path)}` : `${API_BASE}/fs/browse`;
  const res = await fetch(url);
  if (!res.ok) throw new Error('Failed to fetch directory');
  return res.json();
}

export async function fetchTree(path?: string, depth?: number): Promise<TreeNode> {
  const params = new URLSearchParams();
  if (path) params.set('path', path);
  if (depth !== undefined) params.set('depth', String(depth));
  const res = await fetch(`${API_BASE}/fs/tree?${params}`);
  if (!res.ok) throw new Error('Failed to fetch tree');
  return res.json();
}

export async function createFolder(path: string, name: string): Promise<FileOperation> {
  const res = await fetch(`${API_BASE}/fs/create_folder`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path, name }),
  });
  if (!res.ok) throw new Error('Failed to create folder');
  return res.json();
}

export async function deletePath(path: string): Promise<FileOperation> {
  const res = await fetch(`${API_BASE}/fs/delete`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path }),
  });
  if (!res.ok) throw new Error('Failed to delete');
  return res.json();
}

export async function movePath(src: string, dest: string): Promise<FileOperation> {
  const res = await fetch(`${API_BASE}/fs/move`, {
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
  const res = await fetch(`${API_BASE}/search/query?${params}`);
  if (!res.ok) throw new Error('Failed to search');
  return res.json();
}

export async function fetchSettings(): Promise<AppSettings> {
  const res = await fetch(`${API_BASE}/settings`);
  if (!res.ok) throw new Error('Failed to fetch settings');
  return res.json();
}

export async function updateSettings(updates: Partial<AppSettings>): Promise<AppSettings> {
  const res = await fetch(`${API_BASE}/settings`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(updates),
  });
  if (!res.ok) throw new Error('Failed to update settings');
  return res.json();
}

export async function checkHealth(): Promise<{ status: string }> {
  const res = await fetch(`${API_BASE}/health`);
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
  children?: TreeNode[];
}

export type { AppSettings };

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  sources?: { file_path: string; score: number }[];
  timestamp: number;
}

export interface ChatSession {
  id: string;
  title: string;
  messages: ChatMessage[];
  updated_at: number;
  session_type: string;
}

export interface ChatHistoryResponse {
  sessions: ChatSession[];
  current_session_id: string | null;
}

export async function fetchChatSessions(sessionType?: string): Promise<ChatHistoryResponse> {
  const params = sessionType ? `?session_type=${sessionType}` : '';
  const res = await fetch(`${API_BASE}/chat_history/sessions${params}`);
  if (!res.ok) throw new Error('Failed to fetch chat sessions');
  return res.json();
}

export async function createChatSession(sessionType: string = 'chat'): Promise<ChatSession> {
  const res = await fetch(`${API_BASE}/chat_history/sessions`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ session_type: sessionType }),
  });
  if (!res.ok) throw new Error('Failed to create chat session');
  return res.json();
}

export async function fetchChatSession(sessionId: string): Promise<ChatSession> {
  const res = await fetch(`${API_BASE}/chat_history/sessions/${sessionId}`);
  if (!res.ok) throw new Error('Failed to fetch chat session');
  return res.json();
}

export async function deleteChatSession(sessionId: string): Promise<void> {
  const res = await fetch(`${API_BASE}/chat_history/sessions/${sessionId}`, {
    method: 'DELETE',
  });
  if (!res.ok) throw new Error('Failed to delete chat session');
}

export async function addChatMessage(
  sessionId: string,
  role: 'user' | 'assistant',
  content: string,
  sources?: { file_path: string; score: number }[]
): Promise<ChatMessage> {
  const res = await fetch(`${API_BASE}/chat_history/messages`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ session_id: sessionId, role, content, sources }),
  });
  if (!res.ok) throw new Error('Failed to add chat message');
  return res.json();
}

export async function updateChatSessionTitle(sessionId: string, title: string): Promise<void> {
  const res = await fetch(`${API_BASE}/chat_history/sessions/${sessionId}/title`, {
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

export interface AgentResponse {
  response: string;
  tool_results?: Array<{
    tool: string;
    arguments: Record<string, unknown>;
    result: unknown;
  }>;
  plans: AgentPlan[];
  steps_used: number;
  available_tools: string[];
}

export async function sendAgentMessage(
  message: string,
  sessionId?: string
): Promise<AgentResponse> {
  const res = await fetch(`${API_BASE}/agent/chat`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ message, session_id: sessionId }),
  });
  if (!res.ok) throw new Error('Failed to send agent message');
  return res.json();
}

export async function getAgentTools(): Promise<{ tools: string[] }> {
  const res = await fetch(`${API_BASE}/agent/tools`);
  if (!res.ok) throw new Error('Failed to get agent tools');
  return res.json();
}

// ---- Plans & Approval ----

export async function fetchPlans(status?: string): Promise<AgentPlan[]> {
  const params = status ? `?status=${encodeURIComponent(status)}` : '';
  const res = await fetch(`${API_BASE}/plans${params}`);
  if (!res.ok) throw new Error('Failed to fetch plans');
  const data = await res.json();
  return data.plans || [];
}

export async function fetchPlan(planId: string): Promise<AgentPlan> {
  const res = await fetch(`${API_BASE}/plans/${planId}`);
  if (!res.ok) throw new Error('Failed to fetch plan');
  return res.json();
}

export async function fetchPlanLog(
  planId: string
): Promise<Array<Record<string, unknown>>> {
  const res = await fetch(`${API_BASE}/plans/${planId}/log`);
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
  const res = await fetch(`${API_BASE}/plans`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  if (!res.ok) throw new Error('Failed to create plan');
  return res.json();
}

export async function approvePlan(planId: string): Promise<AgentPlan> {
  const res = await fetch(`${API_BASE}/plans/${planId}/approve`, { method: 'POST' });
  if (!res.ok) throw new Error('Failed to approve plan');
  return res.json();
}

export async function rejectPlan(planId: string): Promise<AgentPlan> {
  const res = await fetch(`${API_BASE}/plans/${planId}/reject`, { method: 'POST' });
  if (!res.ok) throw new Error('Failed to reject plan');
  return res.json();
}

export async function executePlan(planId: string): Promise<AgentPlan> {
  const res = await fetch(`${API_BASE}/plans/${planId}/execute`, { method: 'POST' });
  if (!res.ok) throw new Error('Failed to execute plan');
  return res.json();
}

export async function deletePlan(planId: string): Promise<void> {
  const res = await fetch(`${API_BASE}/plans/${planId}`, { method: 'DELETE' });
  if (!res.ok) throw new Error('Failed to delete plan');
}

// ---- Auto Indexer ----

export async function fetchAutoIndexStatus(): Promise<AutoIndexStatus> {
  const res = await fetch(`${API_BASE}/settings/auto_index_status`);
  if (!res.ok) throw new Error('Failed to fetch auto index status');
  return res.json();
}

export async function runAutoIndexNow(): Promise<Record<string, number>> {
  const res = await fetch(`${API_BASE}/settings/auto_index/run`, { method: 'POST' });
  if (!res.ok) throw new Error('Failed to run auto index');
  return res.json();
}

// ---- Hybrid Search ----

export interface HybridSearchResult {
  path: string;
  name: string;
  preview: string;
  score: number;
  sources: string[];
}

export interface HybridSearchResponse {
  results: HybridSearchResult[];
  keyword_count: number;
  semantic_count: number;
  query: string;
}

export async function hybridSearch(query: string, topK: number = 10): Promise<HybridSearchResponse> {
  const params = new URLSearchParams({ query, top_k: String(topK) });
  const res = await fetch(`${API_BASE}/search/hybrid?${params}`, { method: 'POST' });
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
  const res = await fetch(`${API_BASE}/digest/generate${params}`, { method: 'POST' });
  if (!res.ok) throw new Error('Failed to generate digest');
  return res.json();
}

export async function fetchLatestDigest(): Promise<Digest> {
  const res = await fetch(`${API_BASE}/digest/latest`);
  if (!res.ok) throw new Error('Failed to fetch latest digest');
  return res.json();
}

export async function fetchDigestList(): Promise<DigestMeta[]> {
  const res = await fetch(`${API_BASE}/digest/list`);
  if (!res.ok) throw new Error('Failed to fetch digest list');
  const data = await res.json();
  return data.digests || [];
}

export async function fetchDigest(date: string): Promise<Digest> {
  const res = await fetch(`${API_BASE}/digest/${date}`);
  if (!res.ok) throw new Error('Failed to fetch digest');
  return res.json();
}
