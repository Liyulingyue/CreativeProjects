import { request } from "./client";
import type { FsBrowseResult, FsSuggestResult } from "./types";

export async function browseFs(path?: string, dirsOnly = true): Promise<FsBrowseResult> {
  const params = new URLSearchParams();
  if (path) params.set("path", path);
  params.set("dirs_only", String(dirsOnly));
  return request<FsBrowseResult>(`/fs/browse?${params.toString()}`);
}

export async function suggestPath(q: string): Promise<FsSuggestResult> {
  const params = new URLSearchParams();
  params.set("q", q);
  return request<FsSuggestResult>(`/fs/suggest?${params.toString()}`);
}
