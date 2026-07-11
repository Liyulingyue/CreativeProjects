import { request } from "./client";
import type { AppSettings, Stats } from "./types";

export async function getSettings(): Promise<AppSettings> {
  return request<AppSettings>("/settings");
}

export async function updateSettings(settings: Partial<AppSettings>): Promise<AppSettings> {
  return request<AppSettings>("/settings", {
    method: "PUT",
    body: JSON.stringify(settings),
  });
}

export async function getStats(): Promise<Stats> {
  return request<Stats>("/stats");
}
