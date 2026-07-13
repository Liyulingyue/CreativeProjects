use crate::models::*;
use parking_lot::RwLock;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const FOLDER_CACHE_DIR_NAME: &str = ".photoanalyzer";
const RESULTS_FILE_NAME: &str = "results.json";

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data")
}

fn dirs_file() -> PathBuf {
    data_dir().join("dirs.json")
}

fn settings_file() -> PathBuf {
    data_dir().join("settings.json")
}

fn results_file() -> PathBuf {
    data_dir().join("results.json")
}

fn orphan_results_file() -> PathBuf {
    data_dir().join("orphan_results").join(RESULTS_FILE_NAME)
}

fn load_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str::<T>(&content).ok()
}

fn save_json<T: Serialize>(path: &Path, data: &T) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(content) = serde_json::to_string_pretty(data) {
        let _ = fs::write(path, content);
    }
}

fn normalize_path_for_compare(path: &str) -> String {
    #[cfg(windows)]
    {
        path.replace('/', "\\").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        path.to_string()
    }
}

fn find_base_dir(file_path: &str) -> Option<PathBuf> {
    let p = Path::new(file_path).canonicalize().ok()?;
    let mut current = p.parent().map(|x| x.to_path_buf());
    while let Some(parent) = current {
        if parent.join(FOLDER_CACHE_DIR_NAME).exists() {
            return Some(parent);
        }
        current = parent.parent().map(|x| x.to_path_buf());
    }
    None
}

fn read_results_from_file(path: &Path) -> Vec<AnalysisResult> {
    let Some(content) = fs::read_to_string(path).ok() else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&content) else {
        return Vec::new();
    };

    match value {
        Value::Array(arr) => arr
            .into_iter()
            .filter_map(|v| serde_json::from_value::<AnalysisResult>(v).ok())
            .collect(),
        Value::Object(obj) => obj
            .into_values()
            .filter_map(|v| serde_json::from_value::<AnalysisResult>(v).ok())
            .collect(),
        _ => Vec::new(),
    }
}

pub struct AppState {
    pub dirs: RwLock<HashMap<String, DirEntry>>,
    pub results: RwLock<HashMap<String, AnalysisResult>>,
    pub analysis_jobs: RwLock<HashMap<String, AnalysisJob>>,
    pub dedup_jobs: RwLock<HashMap<String, DedupJob>>,
    pub settings: RwLock<AppSettings>,
}

impl AppState {
    pub fn new() -> Self {
        let _ = fs::create_dir_all(data_dir());

        let settings = load_json::<AppSettings>(&settings_file()).unwrap_or_default();
        let dirs = load_json::<HashMap<String, DirEntry>>(&dirs_file()).unwrap_or_default();

        let mut results_map: HashMap<String, AnalysisResult> = HashMap::new();
        if settings.storage_mode != "folder" {
            let loaded = load_json::<Vec<AnalysisResult>>(&results_file()).unwrap_or_default();
            for r in loaded {
                results_map.insert(r.file_path.clone(), r);
            }
        }

        Self {
            dirs: RwLock::new(dirs),
            results: RwLock::new(results_map),
            analysis_jobs: RwLock::new(HashMap::new()),
            dedup_jobs: RwLock::new(HashMap::new()),
            settings: RwLock::new(settings),
        }
    }

    fn persist_dirs(&self) {
        let dirs = self.dirs.read().clone();
        save_json(&dirs_file(), &dirs);
    }

    fn persist_settings(&self) {
        let settings = self.settings.read().clone();
        save_json(&settings_file(), &settings);
    }

    fn persist_project_results(&self) {
        let settings = self.settings.read().clone();
        if settings.storage_mode == "folder" {
            return;
        }
        let results: Vec<AnalysisResult> = self.results.read().values().cloned().collect();
        save_json(&results_file(), &results);
    }

    pub fn get_settings(&self) -> AppSettings {
        self.settings.read().clone()
    }

    pub fn update_settings(&self, settings: AppSettings) {
        *self.settings.write() = settings;
        self.persist_settings();
    }

    pub fn get_stats(&self) -> Stats {
        let analyzed_photos = self.list_results().into_iter().filter(|r| r.success).count();
        let duplicate_groups: usize = self
            .dedup_jobs
            .read()
            .values()
            .filter(|j| j.status == "completed")
            .map(|j| j.groups_count)
            .sum();

        Stats {
            total_photos: 0,
            analyzed_photos,
            duplicate_groups,
            directories: self.dirs.read().len(),
        }
    }

    pub fn add_dir(&self, path: &str, name: Option<&str>) -> DirEntry {
        let id = Uuid::new_v4().to_string()[..12].to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let path_buf = std::path::PathBuf::from(path);
        let entry_name = name.map(String::from).unwrap_or_else(|| {
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
        self.persist_dirs();
        entry
    }

    pub fn get_dir(&self, id: &str) -> Option<DirEntry> {
        self.dirs.read().get(id).cloned()
    }

    pub fn list_dirs(&self) -> Vec<DirEntry> {
        self.dirs.read().values().cloned().collect()
    }

    pub fn remove_dir(&self, id: &str) -> bool {
        let removed = self.dirs.write().remove(id).is_some();
        if removed {
            self.persist_dirs();
        }
        removed
    }

    pub fn add_result(&self, result: AnalysisResult) {
        self.add_results(vec![result]);
    }

    pub fn get_result(&self, path: &str) -> Option<AnalysisResult> {
        let target = normalize_path_for_compare(path);
        self.list_results()
            .into_iter()
            .find(|r| normalize_path_for_compare(&r.file_path) == target)
    }

    pub fn list_results(&self) -> Vec<AnalysisResult> {
        let settings = self.settings.read().clone();
        if settings.storage_mode != "folder" {
            return self.results.read().values().cloned().collect();
        }

        let mut merged: HashMap<String, AnalysisResult> = HashMap::new();

        for dir in self.dirs.read().values() {
            let path = PathBuf::from(&dir.path)
                .join(FOLDER_CACHE_DIR_NAME)
                .join(RESULTS_FILE_NAME);
            for r in read_results_from_file(&path) {
                merged.insert(normalize_path_for_compare(&r.file_path), r);
            }
        }

        for r in read_results_from_file(&orphan_results_file()) {
            merged.insert(normalize_path_for_compare(&r.file_path), r);
        }

        merged.into_values().collect()
    }

    pub fn add_results(&self, results: Vec<AnalysisResult>) {
        let settings = self.settings.read().clone();

        if settings.storage_mode != "folder" {
            let mut guard = self.results.write();
            for result in results {
                guard.insert(result.file_path.clone(), result);
            }
            drop(guard);
            self.persist_project_results();
            return;
        }

        let dirs: Vec<DirEntry> = self.dirs.read().values().cloned().collect();

        for result in results {
            let file_path = PathBuf::from(&result.file_path);
            let canonical = file_path.canonicalize().unwrap_or(file_path.clone());

            let mut chosen_base: Option<PathBuf> = None;
            let mut best_len = 0usize;
            for dir in &dirs {
                let base = PathBuf::from(&dir.path);
                let base_canonical = base.canonicalize().unwrap_or(base.clone());
                if canonical.starts_with(&base_canonical) {
                    let l = base_canonical.to_string_lossy().len();
                    if l > best_len {
                        best_len = l;
                        chosen_base = Some(base_canonical);
                    }
                }
            }

            if chosen_base.is_none() {
                chosen_base = find_base_dir(&result.file_path);
            }

            let (results_path, key) = if let Some(base) = chosen_base {
                let cache_dir = base.join(FOLDER_CACHE_DIR_NAME);
                let _ = fs::create_dir_all(&cache_dir);
                let rel = canonical
                    .strip_prefix(&base)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| canonical.to_string_lossy().to_string());
                (cache_dir.join(RESULTS_FILE_NAME), rel)
            } else {
                (orphan_results_file(), canonical.to_string_lossy().to_string())
            };

            let mut data: HashMap<String, AnalysisResult> = load_json(&results_path).unwrap_or_default();
            data.insert(key, result);
            save_json(&results_path, &data);
        }
    }

    pub fn create_analysis_job(&self, total: usize) -> AnalysisJob {
        let job_id = Uuid::new_v4().to_string()[..12].to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let job = AnalysisJob {
            job_id: job_id.clone(),
            status: "pending".to_string(),
            total,
            progress: 0,
            current_file: None,
            results: vec![],
            created_at: now,
            finished_at: None,
        };
        self.analysis_jobs.write().insert(job_id, job.clone());
        job
    }

    pub fn get_analysis_job(&self, job_id: &str) -> Option<AnalysisJob> {
        self.analysis_jobs.read().get(job_id).cloned()
    }

    pub fn update_analysis_job(&self, job_id: &str, updates: AnalysisJobUpdate) {
        if let Some(job) = self.analysis_jobs.write().get_mut(job_id) {
            if let Some(status) = updates.status {
                job.status = status;
            }
            if let Some(progress) = updates.progress {
                job.progress = progress;
            }
            if let Some(current_file) = updates.current_file {
                job.current_file = Some(current_file);
            }
            if let Some(results) = updates.results {
                job.results = results;
            }
            if let Some(finished_at) = updates.finished_at {
                job.finished_at = Some(finished_at);
            }
        }
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
            dir_id: dir_id.map(String::from),
            dir_path: dir_path.map(String::from),
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
                job.groups_count = groups.len();
                job.groups = groups;
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

pub struct AnalysisJobUpdate {
    pub status: Option<String>,
    pub progress: Option<usize>,
    pub current_file: Option<String>,
    pub results: Option<Vec<AnalysisResult>>,
    pub finished_at: Option<String>,
}
