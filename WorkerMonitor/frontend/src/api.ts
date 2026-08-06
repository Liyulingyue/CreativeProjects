import { MonitorSnapshot, AppConfig } from "./types";

let requestId = 0;
const IPC_TIMEOUT_MS = 15000;
const pendingRequests = new Map<string, {
  resolve: (value: unknown) => void;
  reject: (reason: unknown) => void;
  timeout: ReturnType<typeof setTimeout>;
}>();

interface IpcMessage {
  id: string;
  method: string;
  params?: unknown;
}

interface IpcResponse {
  id?: string | null;
  ok: boolean;
  result?: unknown;
  error?: string;
}

declare global {
  interface Window {
    ipc: {
      postMessage: (msg: string) => void;
      invoke: (method: string, params?: unknown) => Promise<unknown>;
      onResponse: (response: IpcResponse) => void;
    };
  }
}

function sendIpc(method: string, params?: unknown): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const ipc = window.ipc;
    if (!ipc || typeof ipc.postMessage !== "function") {
      reject(new Error("IPC bridge unavailable. Please run inside WorkerMonitor desktop app."));
      return;
    }

    const id = String(++requestId);
    const timeout = setTimeout(() => {
      pendingRequests.delete(id);
      reject(new Error(`IPC timeout for method: ${method}`));
    }, IPC_TIMEOUT_MS);

    pendingRequests.set(id, { resolve, reject, timeout });

    const payload: IpcMessage = { id, method, params };
    try {
      ipc.postMessage(JSON.stringify(payload));
    } catch (error) {
      clearTimeout(timeout);
      pendingRequests.delete(id);
      reject(error);
    }
  });
}

if (!window.ipc) {
  window.ipc = {
    postMessage: () => {
      throw new Error("IPC bridge unavailable. Please run inside WorkerMonitor desktop app.");
    },
    invoke: async () => {
      throw new Error("IPC bridge unavailable. Please run inside WorkerMonitor desktop app.");
    },
    onResponse: () => {},
  };
}

window.ipc.invoke = async (method: string, params?: unknown): Promise<unknown> => sendIpc(method, params);
window.ipc.onResponse = (response: IpcResponse) => {
  if (!response.id) {
    return;
  }
  const pending = pendingRequests.get(response.id);
  if (pending) {
    clearTimeout(pending.timeout);
    pendingRequests.delete(response.id);
    if (response.ok) {
      pending.resolve(response.result);
    } else {
      pending.reject(new Error(response.error || "unknown error"));
    }
  }
};

export async function startMonitoring(): Promise<void> {
  await sendIpc("start_monitoring");
}

export async function stopMonitoring(): Promise<void> {
  await sendIpc("stop_monitoring");
}

export async function getMonitorStatus(): Promise<MonitorSnapshot> {
  return await sendIpc("get_status") as MonitorSnapshot;
}

export async function getConfig(): Promise<AppConfig> {
  return await sendIpc("get_config") as AppConfig;
}

export async function saveConfig(config: AppConfig): Promise<void> {
  await sendIpc("save_config", config);
}

export async function dismissBreakAlert(): Promise<void> {
  await sendIpc("dismiss_alert");
}

export async function initDetector(): Promise<void> {
  await sendIpc("init_detector");
}

export interface DetectResult {
  keypoints: Array<{ x: number; y: number; confidence: number }>;
  person_detected: boolean;
}

export async function detectFrame(): Promise<DetectResult> {
  return await sendIpc("detect_frame") as DetectResult;
}

export async function updateDetection(pose: DetectResult): Promise<void> {
  await sendIpc("update_detection", pose);
}

export async function getFrameBase64(): Promise<string> {
  const result = await sendIpc("get_frame_base64") as { frame: string };
  return result.frame;
}
