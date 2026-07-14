import { request } from "./client";
import type { DirEntry, BrowseResult } from "./types";

export async function listDirs(): Promise<DirEntry[]> {
  return request<DirEntry[]>("/dirs");
}

export async function addDir(path: string, name?: string): Promise<DirEntry> {
  return request<DirEntry>("/dirs", {
    method: "POST",
    body: JSON.stringify({ path, name }),
  });
}

export async function removeDir(id: string): Promise<void> {
  return request<void>(`/dirs/${id}`, { method: "DELETE" });
}

export async function browseFiles(
  dirId: string,
  subPath?: string
): Promise<BrowseResult> {
  const params = new URLSearchParams();
  params.set("dir_id", dirId);
  if (subPath) params.set("path", subPath);
  return request<BrowseResult>(`/files?${params.toString()}`);
}

export async function getFileSiblings(path: string): Promise<{ siblings: string[]; count: number }> {
  return request<{ siblings: string[]; count: number }>(`/files/siblings?path=${encodeURIComponent(path)}`);
}

export async function getOrphanedRaws(dirId: string): Promise<{ orphaned: string[]; count: number }> {
  return request<{ orphaned: string[]; count: number }>(`/files/orphaned-raws?dir_id=${encodeURIComponent(dirId)}`);
}

export async function deleteOrphanedRaws(dirId: string): Promise<{ deleted: string[]; not_found: string[]; count: number }> {
  return request<{ deleted: string[]; not_found: string[]; count: number }>(`/files/orphaned-raws?dir_id=${encodeURIComponent(dirId)}`, { method: "DELETE" });
}

export async function deleteFile(path: string): Promise<{ deleted: string[]; not_found?: string[]; count?: number }> {
  return request<{ deleted: string[]; not_found?: string[]; count?: number }>(`/files?path=${encodeURIComponent(path)}`, { method: "DELETE" });
}
