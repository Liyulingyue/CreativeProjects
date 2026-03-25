import { type SystemInfo, type DockerContainer, type StartupItem, type ProcessInfo, type NetworkInfo } from '../types.ts';

const API_BASE = '';

export const api = {
  async getSystemInfo(): Promise<SystemInfo> {
    const res = await fetch(`${API_BASE}/system/info`);
    return res.json();
  },
  async getDockerInfo(): Promise<DockerContainer[] | { error: string; need_auth?: boolean }> {
    const res = await fetch(`${API_BASE}/system/docker`, { signal: AbortSignal.timeout(10000) });
    return res.json();
  },
  async dockerAuth(password: string): Promise<{ status: string; message?: string }> {
    const res = await fetch(`${API_BASE}/system/docker/auth`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ password }),
    });
    return res.json();
  },
  async dockerLogout(): Promise<{ status: string }> {
    const res = await fetch(`${API_BASE}/system/docker/auth`, { method: 'DELETE' });
    return res.json();
  },
  async getStartupInfo(): Promise<{ load_avg: number[], startup_items: StartupItem[] }> {
    const res = await fetch(`${API_BASE}/system/startup`);
    return res.json();
  },
  async getProcesses(): Promise<ProcessInfo[]> {
    const res = await fetch(`${API_BASE}/system/processes`);
    return res.json();
  },
  async getNetworkInfo(): Promise<NetworkInfo> {
    const res = await fetch(`${API_BASE}/system/network`);
    return res.json();
  },
  async killProcess(pid: number): Promise<{ status: string; message?: string }> {
    const res = await fetch(`${API_BASE}/system/processes/${pid}/kill`, { method: 'POST' });
    return res.json();
  },
  async manageDocker(containerId: string, action: 'start' | 'stop' | 'restart'): Promise<{ status: string; message?: string; need_auth?: boolean }> {
    const res = await fetch(`${API_BASE}/system/docker/${containerId}/${action}`, {
      method: 'POST',
    });
    return res.json();
  },
  async getServiceLogs(name: string, lines: number = 100): Promise<{ name: string; logs: string[]; error?: string }> {
    const res = await fetch(`${API_BASE}/system/startup/${encodeURIComponent(name)}/logs?lines=${lines}`);
    return res.json();
  }
};
