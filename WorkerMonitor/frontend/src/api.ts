import { MonitorSnapshot, AppConfig } from "./types";

const BASE = "http://127.0.0.1:8080";

async function post<T, R>(path: string, body: T): Promise<R> {
  const res = await fetch(`${BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`${path} failed: ${res.status}`);
  return res.json();
}

async function get<R>(path: string): Promise<R> {
  const res = await fetch(`${BASE}${path}`);
  if (!res.ok) throw new Error(`${path} failed: ${res.status}`);
  return res.json();
}

export async function startMonitoring(): Promise<void> {
  await post("/api/monitoring/start", {});
}

export async function stopMonitoring(): Promise<void> {
  await post("/api/monitoring/stop", {});
}

export async function updatePresence(present: boolean): Promise<MonitorSnapshot> {
  return await post("/api/presence", { present });
}

export async function getMonitorStatus(): Promise<MonitorSnapshot> {
  return await get<MonitorSnapshot>("/api/status");
}

export async function getConfig(): Promise<AppConfig> {
  return await get<AppConfig>("/api/config");
}

export async function saveConfig(config: AppConfig): Promise<void> {
  await post("/api/config", config);
}

export async function dismissBreakAlert(): Promise<void> {
  await post("/api/alert/dismiss", {});
}

export async function reportPosture(
  score: number,
  headForward: boolean,
  headTilt: boolean,
  shoulderUneven: boolean,
  slouching: boolean
): Promise<void> {
  await post("/api/posture", {
    score,
    head_forward: headForward,
    head_tilt: headTilt,
    shoulder_uneven: shoulderUneven,
    slouching,
  });
}
