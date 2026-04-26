export interface NodeInfo {
  node_id: string;
  node_type: 'master' | 'worker';
  alive: boolean;
  http_port: number;
  tcp_port?: number;
  pid?: number;
  log_file?: string;
}

export interface MasterStatus {
  local_expert_count: number;
  workers: number;
  local_experts: string[];
}

export interface WorkerStatus {
  loaded_count: number;
  loaded_experts: string[];
}

export type NodeStatus = MasterStatus | WorkerStatus;

export interface CreateMasterRequest {
  node_id: string;
  manifest_path: string;
  http_port?: number;
  expert_ids?: number[];
}

export interface CreateWorkerRequest {
  node_id: string;
  experts_dir?: string;
  expert_ids?: number[];
  master_url?: string;
  master_id?: string;
}

export interface InferenceRequest {
  master_node_id: string;
  prompt: string;
  max_tokens?: number;
}

export interface InferenceResponse {
  result: string;
  input_tokens: number;
  output_tokens: number;
}

export interface LoadExpertRequest {
  node_id: string;
  expert_id: number;
  layer_id: number;
}

export interface LoadExpertResponse {
  size_mb: number;
  local_experts: string[];
}

export interface LogResponse {
  content: string;
}
