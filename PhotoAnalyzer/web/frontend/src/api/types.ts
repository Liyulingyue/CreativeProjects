export interface PhotoAnalysis {
  score: number;
  style: string;
  caption: string;
  main_objects: string[];
  blurry: string;
  comments: string;
  recommendations: string;
}

export interface AnalysisResult {
  file_path: string;
  file_name: string;
  success: boolean;
  error: string | null;
  data: PhotoAnalysis | null;
  reasoning: string | null;
}

export interface DirEntry {
  id: string;
  path: string;
  name: string;
  added_at: string;
}

export interface FileNode {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified: string;
  thumbnail_url: string | null;
}

export interface BrowseResult {
  current_path: string;
  parent_path: string | null;
  items: FileNode[];
}

export interface AnalysisJob {
  job_id: string;
  status: "pending" | "running" | "completed" | "failed" | "canceled";
  total: number;
  progress: number;
  current_file: string | null;
  results: AnalysisResult[];
  created_at: string;
  finished_at: string | null;
}

export interface DedupGroup {
  group_id: string;
  items: DedupItem[];
  representative: string | null;
  stage: string;
}

export interface DedupItem {
  path: string;
  file_name: string;
  thumbnail_url: string | null;
  file_size: number;
  similarity: number;
  metadata: Record<string, unknown>;
  siblings: string[];
}

export interface DedupJob {
  job_id: string;
  status: "pending" | "running" | "completed" | "failed";
  total_files: number;
  groups_count: number;
  groups: DedupGroup[];
  stage: string | null;
  created_at: string;
  finished_at: string | null;
}

export interface AppSettings {
  api_key: string;
  base_url: string;
  model: string;
  delay: number;
  storage_mode: "project" | "folder";
  dedup_stages: DedupStageConfig[];
}

export interface DedupStageConfig {
  type: "exif" | "phash" | "embedding";
  enabled: boolean;
  params: Record<string, unknown>;
}

export interface Stats {
  total_photos: number;
  analyzed_photos: number;
  duplicate_groups: number;
  directories: number;
}

export interface FsEntry {
  name: string;
  path: string;
  is_dir: boolean;
  children_count: number | null;
}

export interface FsBrowseResult {
  current_path: string;
  parent_path: string | null;
  entries: FsEntry[];
  home: string;
}

export interface FsSuggestResult {
  suggestions: FsEntry[];
  partial: string;
}

export interface CacheStats {
  [featureType: string]: number;
}

export interface CacheEntry {
  cache_key: string;
  feature_type: string;
  file_path: string;
  mtime: number;
  data: Record<string, unknown>;
}

export interface ThumbnailJob {
  job_id: string;
  status: "pending" | "running" | "completed" | "failed" | "canceled";
  total: number;
  progress: number;
  completed: number;
  failed: number;
  current_file: string | null;
  created_at: string;
  finished_at: string | null;
}
