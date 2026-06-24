export type TabType = "images" | "results" | "settings";

export interface FileEntry {
  id: string;
  file: File;
  thumb?: string;
}