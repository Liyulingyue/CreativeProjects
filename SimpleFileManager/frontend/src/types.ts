export interface FileNode {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified: string;
  created: string;
  extension: string;
  mime_type: string;
}

export interface BrowseResult {
  current_path: string;
  parent_path: string | null;
  items: FileNode[];
  total_count: number;
  dirs_count: number;
  files_count: number;
}

export interface FileOperation {
  success: boolean;
  message: string;
  path?: string;
}

export interface SearchResult {
  items: FileNode[];
  total: number;
  query: string;
}

export interface AppSettings {
  openai_api_key: string;
  openai_base_url: string;
  embedding_model: string;
  index_interval: number;
  storage_path: string;
  theme: string;
}
