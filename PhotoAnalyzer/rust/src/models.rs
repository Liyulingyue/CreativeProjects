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
pub struct AnalysisResult {
    #[serde(rename = "file_path")]
    pub file_path: String,
    pub success: bool,
    pub data: Option<AnalysisData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisData {
    pub score: u32,
    pub blurry: String,
    pub style: String,
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
