export type TabType = "images" | "results" | "settings";

export interface FileEntry {
  id: string;
  file: File;
  thumb?: string;
}

export interface AnalysisLog {
  fileName: string;
  status: "success" | "failed";
  score?: number;
  error?: string;
}