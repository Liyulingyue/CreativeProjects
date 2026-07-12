use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub id: String,
    pub path: String,
    pub name: String,
    pub added_at: String,
}

#[derive(Debug, Clone)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BrowseResult {
    pub current_path: String,
    pub parent_path: Option<String>,
    pub items: Vec<FileNode>,
}

#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub file_path: String,
    pub success: bool,
    pub data: Option<AnalysisData>,
}

#[derive(Debug, Clone)]
pub struct AnalysisData {
    pub score: u32,
    pub blurry: String,
    pub style: String,
}

#[derive(Debug, Clone)]
pub struct DedupGroup {
    pub group_id: String,
    pub items: Vec<DedupItem>,
    pub representative: Option<String>,
    pub stage: String,
}

#[derive(Debug, Clone)]
pub struct DedupItem {
    pub path: String,
    pub file_name: String,
    pub thumbnail_url: Option<String>,
    pub file_size: u64,
    pub similarity: f64,
    pub metadata: HashMap<String, serde_json::Value>,
    pub siblings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DedupJob {
    pub job_id: String,
    pub status: String,
    pub total_files: usize,
    pub groups_count: usize,
    pub groups: Vec<DedupGroup>,
    pub stage: Option<String>,
    pub dir_id: Option<String>,
    pub dir_path: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

pub struct AppState {
    pub dirs: RwLock<HashMap<String, DirEntry>>,
    pub results: RwLock<HashMap<String, AnalysisResult>>,
    pub dedup_jobs: RwLock<HashMap<String, DedupJob>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            dirs: RwLock::new(HashMap::new()),
            results: RwLock::new(HashMap::new()),
            dedup_jobs: RwLock::new(HashMap::new()),
        }
    }

    pub fn add_dir(&self, path: &str, name: Option<&str>) -> DirEntry {
        let id = Uuid::new_v4().to_string()[..12].to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let path_buf = PathBuf::from(path);
        let entry_name = name.map(|s| s.to_string()).unwrap_or_else(|| {
            path_buf.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string())
        });
        let entry = DirEntry {
            id: id.clone(),
            path: path.to_string(),
            name: entry_name,
            added_at: now,
        };
        self.dirs.write().insert(id, entry.clone());
        entry
    }

    pub fn get_dir(&self, id: &str) -> Option<DirEntry> {
        self.dirs.read().get(id).cloned()
    }

    pub fn list_dirs(&self) -> Vec<DirEntry> {
        self.dirs.read().values().cloned().collect()
    }

    pub fn remove_dir(&self, id: &str) -> bool {
        self.dirs.write().remove(id).is_some()
    }

    pub fn add_result(&self, result: AnalysisResult) {
        self.results.write().insert(result.file_path.clone(), result);
    }

    pub fn get_result(&self, path: &str) -> Option<AnalysisResult> {
        self.results.read().get(path).cloned()
    }

    pub fn list_results(&self) -> Vec<AnalysisResult> {
        self.results.read().values().cloned().collect()
    }

    pub fn create_dedup_job(&self, total_files: usize, dir_id: Option<&str>, dir_path: Option<&str>) -> DedupJob {
        let job_id = Uuid::new_v4().to_string()[..12].to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let job = DedupJob {
            job_id: job_id.clone(),
            status: "pending".to_string(),
            total_files,
            groups_count: 0,
            groups: vec![],
            stage: Some("初始化".to_string()),
            dir_id: dir_id.map(|s| s.to_string()),
            dir_path: dir_path.map(|s| s.to_string()),
            created_at: now,
            finished_at: None,
        };
        self.dedup_jobs.write().insert(job_id, job.clone());
        job
    }

    pub fn get_dedup_job(&self, job_id: &str) -> Option<DedupJob> {
        self.dedup_jobs.read().get(job_id).cloned()
    }

    pub fn update_dedup_job(&self, job_id: &str, updates: DedupJobUpdate) {
        if let Some(job) = self.dedup_jobs.write().get_mut(job_id) {
            if let Some(status) = updates.status {
                job.status = status;
            }
            if let Some(stage) = updates.stage {
                job.stage = Some(stage);
            }
            if let Some(groups) = updates.groups {
                job.groups = groups;
                job.groups_count = groups.len();
            }
            if let Some(finished_at) = updates.finished_at {
                job.finished_at = Some(finished_at);
            }
        }
    }

    pub fn get_latest_dedup_job(&self, dir_id: &str, dir_path: Option<&str>) -> Option<DedupJob> {
        let jobs: Vec<_> = self.dedup_jobs.read().values()
            .filter(|j| {
                let dir_id_match = j.dir_id.as_deref() == Some(dir_id);
                let path_match = dir_path.map(|p| j.dir_path.as_deref() == Some(p)).unwrap_or(false);
                j.status == "completed" && (dir_id_match || path_match)
            })
            .cloned()
            .collect();
        jobs.into_iter().max_by_key(|j| j.created_at.clone())
    }
}

pub struct DedupJobUpdate {
    pub status: Option<String>,
    pub stage: Option<String>,
    pub groups: Option<Vec<DedupGroup>>,
    pub finished_at: Option<String>,
}

impl Default for DedupJobUpdate {
    fn default() -> Self {
        Self {
            status: None,
            stage: None,
            groups: None,
            finished_at: None,
        }
    }
}
