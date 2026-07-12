import { request } from "./client";
import type { DedupJob, DedupStageConfig, CacheStats, CacheEntry } from "./types";

export async function getCacheStats(): Promise<CacheStats> {
  return request<CacheStats>("/dedup/cache/stats");
}

export async function getCacheEntries(featureType?: string): Promise<CacheEntry[]> {
  const params = featureType ? `?feature_type=${encodeURIComponent(featureType)}` : "";
  return request<CacheEntry[]>(`/dedup/cache/entries${params}`);
}

export async function clearCache(featureType?: string): Promise<{ cleared: string }> {
  return request<{ cleared: string }>("/dedup/cache/clear", {
    method: "POST",
    body: JSON.stringify({ feature_type: featureType || null }),
  });
}

export async function deleteCacheEntry(cacheKey: string): Promise<{ deleted: string }> {
  return request<{ deleted: string }>(`/dedup/cache/entries/${encodeURIComponent(cacheKey)}`, {
    method: "DELETE",
  });
}

export async function startDedupFolder(
  dirId: string,
  options?: {
    subPath?: string;
    recursive?: boolean;
    stages?: DedupStageConfig[];
  }
): Promise<DedupJob> {
  return request<DedupJob>("/dedup", {
    method: "POST",
    body: JSON.stringify({
      dir_id: dirId,
      sub_path: options?.subPath,
      recursive: options?.recursive ?? true,
      stages: options?.stages,
    }),
  });
}

export async function startDedupPaths(
  filePaths: string[],
  options?: {
    stages?: DedupStageConfig[];
  }
): Promise<DedupJob> {
  return request<DedupJob>("/dedup", {
    method: "POST",
    body: JSON.stringify({
      file_paths: filePaths,
      stages: options?.stages,
    }),
  });
}

export async function getDedupJob(jobId: string): Promise<DedupJob> {
  return request<DedupJob>(`/dedup/${jobId}`);
}

export async function resolveDedupGroups(
  jobId: string,
  actions: { group_id: string; keep: string; remove: string[] }[]
): Promise<void> {
  return request<void>(`/dedup/${jobId}/resolve`, {
    method: "POST",
    body: JSON.stringify({ actions }),
  });
}
