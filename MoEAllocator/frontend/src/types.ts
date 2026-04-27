export interface NodeInfo {
  node_id: string;
  node_type: 'master' | 'worker';
  alive: boolean;
  http_port: number;
  tcp_port?: number;
  pid?: number;
  log_file?: string;
  is_remote?: boolean;
  master_id?: string;
}

export interface AddRemoteNodeRequest {
  node_id: string;
  node_type: string;
  base_url: string;
  tcp_port?: number;
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
  host?: string;
  expert_ids?: number[];
  python_env?: string;
  custom_python?: string;
}

export interface CreateWorkerRequest {
  node_id: string;
  experts_dir?: string;
  expert_ids?: number[];
  master_url?: string;
  python_env?: string;
  custom_python?: string;
  host?: string;
}

export interface DetectedPythonEnvs {
  [key: string]: string;
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
  loaded_count: number;
  local_experts: string[];
}

export interface LogResponse {
  content: string;
}
