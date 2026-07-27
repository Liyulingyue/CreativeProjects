import { invoke } from "@tauri-apps/api/core";
import { MonitorSnapshot, AppConfig } from "./types";

export async function startMonitoring(): Promise<void> {
  await invoke("start_monitoring");
}

export async function stopMonitoring(): Promise<void> {
  await invoke("stop_monitoring");
}

export async function updatePresence(present: boolean): Promise<MonitorSnapshot> {
  return await invoke<MonitorSnapshot>("update_presence", { present });
}

export async function getMonitorStatus(): Promise<MonitorSnapshot> {
  return await invoke<MonitorSnapshot>("get_monitor_status");
}

export async function getConfig(): Promise<AppConfig> {
  return await invoke<AppConfig>("get_config");
}

export async function saveConfig(config: AppConfig): Promise<void> {
  await invoke("save_config", { config });
}

export async function dismissBreakAlert(): Promise<void> {
  await invoke("dismiss_break_alert");
}

export async function reportPosture(
  score: number,
  headForward: boolean,
  headTilt: boolean,
  shoulderUneven: boolean,
  slouching: boolean
): Promise<void> {
  await invoke("report_posture", {
    score,
    headForward,
    headTilt,
    shoulderUneven,
    slouching,
  });
}
