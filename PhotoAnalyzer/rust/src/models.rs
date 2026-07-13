use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub id: String,
    pub path: String,
    pub name: String,
    #[serde(rename = "added_at")]
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    #[serde(rename = "is_dir")]
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
    #[serde(rename = "thumbnail_url", skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowseResult {
    #[serde(rename = "current_path")]
    pub current_path: String,
    #[serde(rename = "parent_path", skip_serializing_if = "Option::is_none")]
    pub parent_path: Option<String>,
    pub items: Vec<FileNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoAnalysis {
    pub score: u32,
    pub style: String,
    pub caption: String,
    #[serde(rename = "main_objects")]
    pub main_objects: Vec<String>,
    pub blurry: String,
    pub comments: String,
    pub recommendations: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    #[serde(rename = "file_path")]
    pub file_path: String,
    #[serde(rename = "file_name")]
    pub file_name: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub data: Option<PhotoAnalysis>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisJob {
    #[serde(rename = "job_id")]
    pub job_id: String,
    pub status: String,
    pub total: usize,
    pub progress: usize,
    #[serde(rename = "current_file", skip_serializing_if = "Option::is_none")]
    pub current_file: Option<String>,
    pub results: Vec<AnalysisResult>,
    #[serde(rename = "created_at")]
    pub created_at: String,
    #[serde(rename = "finished_at", skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupGroup {
    #[serde(rename = "group_id")]
    pub group_id: String,
    pub items: Vec<DedupItem>,
    pub representative: Option<String>,
    pub stage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupItem {
    pub path: String,
    #[serde(rename = "file_name")]
    pub file_name: String,
    #[serde(rename = "thumbnail_url", skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(rename = "file_size")]
    pub file_size: u64,
    pub similarity: f64,
    pub metadata: HashMap<String, serde_json::Value>,
    pub siblings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupJob {
    #[serde(rename = "job_id")]
    pub job_id: String,
    pub status: String,
    #[serde(rename = "total_files")]
    pub total_files: usize,
    #[serde(rename = "groups_count")]
    pub groups_count: usize,
    pub groups: Vec<DedupGroup>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(rename = "dir_id", skip_serializing_if = "Option::is_none")]
    pub dir_id: Option<String>,
    #[serde(rename = "dir_path", skip_serializing_if = "Option::is_none")]
    pub dir_path: Option<String>,
    #[serde(rename = "created_at")]
    pub created_at: String,
    #[serde(rename = "finished_at", skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupStageConfig {
    #[serde(rename = "type")]
    pub type_field: String,
    pub enabled: bool,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(rename = "api_key")]
    pub api_key: String,
    #[serde(rename = "base_url")]
    pub base_url: String,
    pub model: String,
    pub delay: u32,
    #[serde(rename = "storage_mode")]
    pub storage_mode: String,
    #[serde(rename = "dedup_stages")]
    pub dedup_stages: Vec<DedupStageConfig>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.minimaxi.com/v1".to_string(),
            model: "MiniMax-M3".to_string(),
            delay: 1000,
            storage_mode: "folder".to_string(),
            dedup_stages: vec![
                DedupStageConfig {
                    type_field: "exif".to_string(),
                    enabled: true,
                    params: serde_json::json!({"time_window": 5}),
                },
                DedupStageConfig {
                    type_field: "phash".to_string(),
                    enabled: true,
                    params: serde_json::json!({"threshold": 5}),
                },
                DedupStageConfig {
                    type_field: "embedding".to_string(),
                    enabled: false,
                    params: serde_json::json!({"model": "clip", "threshold": 0.85}),
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    #[serde(rename = "total_photos")]
    pub total_photos: usize,
    #[serde(rename = "analyzed_photos")]
    pub analyzed_photos: usize,
    #[serde(rename = "duplicate_groups")]
    pub duplicate_groups: usize,
    pub directories: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsEntry {
    pub name: String,
    pub path: String,
    #[serde(rename = "is_dir")]
    pub is_dir: bool,
    #[serde(rename = "children_count", skip_serializing_if = "Option::is_none")]
    pub children_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsBrowseResult {
    #[serde(rename = "current_path")]
    pub current_path: String,
    #[serde(rename = "parent_path", skip_serializing_if = "Option::is_none")]
    pub parent_path: Option<String>,
    pub entries: Vec<FsEntry>,
    pub home: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsSuggestResult {
    pub suggestions: Vec<FsEntry>,
    pub partial: String,
}
