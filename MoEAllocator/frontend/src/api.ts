import type {
  NodeInfo,
  NodeStatus,
  CreateMasterRequest,
  CreateWorkerRequest,
  InferenceRequest,
  InferenceResponse,
  LoadExpertRequest,
  LoadExpertResponse,
  LogResponse,
} from './types';

const BASE = '/api';

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const opts: RequestInit = {
    method,
    headers: { 'Content-Type': 'application/json' },
  };
  if (body) opts.body = JSON.stringify(body);
  const r = await fetch(BASE + path, opts);
  const data = await r.json();
  if (!r.ok) throw new Error((data as { detail?: string }).detail || JSON.stringify(data));
  return data as T;
}

export const api = {
  getNodes: () => request<NodeInfo[]>('GET', '/nodes'),
  getNode: (nodeId: string) => request<NodeInfo>('GET', `/nodes/${nodeId}`),
  getNodeStatus: (nodeId: string) => request<NodeStatus>('GET', `/nodes/${nodeId}/status`),
  getNodeLogs: (nodeId: string) => request<LogResponse>('GET', `/nodes/${nodeId}/logs`),
  createMaster: (req: CreateMasterRequest) => request<NodeInfo>('POST', '/nodes/master', req),
  createWorker: (req: CreateWorkerRequest) => request<NodeInfo>('POST', '/nodes/worker', req),
  deleteNode: (nodeId: string) => request<void>('DELETE', `/nodes/${nodeId}`),
  inference: (req: InferenceRequest) => request<InferenceResponse>('POST', '/inference', req),
  loadExpert: (req: LoadExpertRequest) => request<LoadExpertResponse>('POST', '/nodes/master/load_expert', req),
  unloadExpert: (req: LoadExpertRequest) => request<void>('POST', '/nodes/master/unload_expert', req),
};
