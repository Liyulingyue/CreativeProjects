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
