import { request } from "./client";
import type { AnalysisJob, AnalysisResult } from "./types";

export async function startAnalysis(
  filePaths: string[],
  options?: { delay?: number }
): Promise<AnalysisJob> {
  return request<AnalysisJob>("/analysis", {
    method: "POST",
    body: JSON.stringify({ file_paths: filePaths, delay: options?.delay }),
  });
}

export async function startFolderAnalysis(
  dirId: string,
  subPath?: string,
  options?: { recursive?: boolean; delay?: number }
): Promise<AnalysisJob> {
  return request<AnalysisJob>("/analysis/folder", {
    method: "POST",
    body: JSON.stringify({
      dir_id: dirId,
      sub_path: subPath,
      recursive: options?.recursive ?? true,
      delay: options?.delay,
    }),
  });
}

export async function getAnalysisJob(jobId: string): Promise<AnalysisJob> {
  return request<AnalysisJob>(`/analysis/${jobId}`);
}

export async function listResults(): Promise<AnalysisResult[]> {
  return request<AnalysisResult[]>("/results");
}

export async function getResult(filePath: string): Promise<AnalysisResult> {
  return request<AnalysisResult>(`/results/${encodeURIComponent(filePath)}`);
}
