import { type SystemInfo, type DockerContainer, type StartupItem, type ProcessInfo } from '../types.ts';

const API_BASE = '/api';

export const api = {
  async getSystemInfo(): Promise<SystemInfo> {
    const res = await fetch(`${API_BASE}/system/info`);
    return res.json();
  },
  async getDockerInfo(): Promise<DockerContainer[] | { error: string }> {
    const res = await fetch(`${API_BASE}/system/docker`);
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
  async manageDocker(containerId: string, action: 'start' | 'stop' | 'restart'): Promise<{ status: string }> {
    const res = await fetch(`${API_BASE}/system/docker/${containerId}/${action}`, {
      method: 'POST',
    });
    return res.json();
  }
};
