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

export interface AppSettings {
  openai_api_key: string;
  openai_base_url: string;
  embedding_model: string;
  index_interval: number;
  storage_path: string;
  theme: string;
}

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
