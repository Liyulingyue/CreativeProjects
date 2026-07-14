use axum::{extract::{Path as AxumPath, Query, State}, Json};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::fs;
use std::path::Path as StdPath;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::models::{DedupGroup, DedupJob};
use crate::paths::{features_dir, FOLDER_CACHE_DIR_NAME};
use crate::services::{AppState, DedupJobUpdate};

pub async fn start_dedup(
    State(state): State<Arc<AppState>>,
    Json(body): Json<StartDedupRequest>,
) -> Result<Json<DedupJob>, (axum::http::StatusCode, &'static str)> {
    let (paths, dir_path) = if let Some(ref dir_id) = body.dir_id {
        let entry = state.get_dir(dir_id).ok_or((axum::http::StatusCode::NOT_FOUND, "目录不存在"))?;
        let base = StdPath::new(&entry.path);
        let target = body.sub_path.as_ref().map(|s| base.join(s)).unwrap_or_else(|| base.to_path_buf());
        if !target.exists() {
            return Err((axum::http::StatusCode::BAD_REQUEST, "路径不存在"));
        }
        let files = collect_image_files(&target, body.recursive.unwrap_or(true));
        (files, Some(target.to_string_lossy().to_string()))
    } else if let Some(ref paths) = body.file_paths {
        let valid: Vec<String> = paths
            .iter()
            .filter(|p| StdPath::new(p.as_str()).exists() && is_image_file(p))
            .cloned()
            .collect();
        (valid, None)
    } else {
        return Err((axum::http::StatusCode::BAD_REQUEST, "需要 dir_id 或 file_paths"));
    };

    if paths.is_empty() {
        return Err((axum::http::StatusCode::BAD_REQUEST, "目录下没有图片"));
    }

    let job = state.create_dedup_job(paths.len(), body.dir_id.as_deref(), dir_path.as_deref());
    let state_clone = state.clone();
    let job_id = job.job_id.clone();
    tokio::spawn(async move {
        run_dedup_job(state_clone, job_id, paths).await;
    });

    Ok(Json(job))
}

#[derive(serde::Deserialize)]
pub struct StartDedupRequest {
    dir_id: Option<String>,
    sub_path: Option<String>,
    recursive: Option<bool>,
    file_paths: Option<Vec<String>>,
}

pub async fn get_dedup_job(
    State(state): State<Arc<AppState>>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<DedupJob>, (axum::http::StatusCode, &'static str)> {
    state.get_dedup_job(&job_id)
        .map(Json)
        .ok_or((axum::http::StatusCode::NOT_FOUND, "任务不存在"))
}

pub async fn get_dedup_by_dir(
    State(state): State<Arc<AppState>>,
    AxumPath(dir_id): AxumPath<String>,
) -> Result<Json<DedupJob>, (axum::http::StatusCode, &'static str)> {
    state.get_latest_dedup_job(&dir_id, None)
        .map(Json)
        .ok_or((axum::http::StatusCode::NOT_FOUND, "该目录暂无去重结果"))
}

pub async fn resolve_dedup(
    State(state): State<Arc<AppState>>,
    AxumPath(job_id): AxumPath<String>,
    Json(body): Json<ResolveDedupRequest>,
) -> Result<Json<ResolveResponse>, (axum::http::StatusCode, &'static str)> {
    let actions = body.actions.as_ref();
    let mut all_removed = Vec::new();

    if let Some(actions_list) = actions {
        for action in actions_list {
            for path in &action.remove {
                let p = StdPath::new(path);
                if p.exists() && p.is_file() {
                    let _ = std::fs::remove_file(p);
                    all_removed.push(path.clone());
                }
            }
        }
    }

    if !all_removed.is_empty() {
        if let Some(job) = state.get_dedup_job(&job_id) {
            let removed_set: HashMap<String, ()> = all_removed.iter().map(|s| (s.clone(), ())).collect();
            let updated_groups: Vec<DedupGroup> = job.groups.into_iter()
                .map(|mut g| {
                    g.items.retain(|i| !removed_set.contains_key(&i.path));
                    g
                })
                .filter(|g| g.items.len() > 1)
                .collect();

            state.update_dedup_job(&job_id, DedupJobUpdate {
                status: None,
                stage: None,
                groups: Some(updated_groups),
                finished_at: None,
            });
        }
    }

    let count = all_removed.len();
    Ok(Json(ResolveResponse { removed: all_removed, count }))
}

#[derive(serde::Deserialize)]
pub struct ResolveDedupRequest {
    actions: Option<Vec<ResolveAction>>,
}

#[derive(serde::Deserialize)]
pub struct ResolveAction {
    remove: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct ResolveResponse {
    removed: Vec<String>,
    count: usize,
}

pub async fn get_cache_stats() -> Json<CacheStats> {
    Json(HashMap::new())
}

pub type CacheStats = HashMap<String, usize>;

pub async fn get_cache_stats_with_state(
    State(state): State<Arc<AppState>>,
) -> Json<CacheStats> {
    let settings = state.get_settings();
    let stats = if settings.storage_mode == "folder" {
        stats_folder_mode(&state)
    } else {
        stats_project_mode()
    };
    Json(stats)
}

pub async fn get_cache_entries(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CacheEntriesQuery>,
) -> Json<Vec<CacheEntry>> {
    let settings = state.get_settings();
    let entries = if settings.storage_mode == "folder" {
        list_entries_folder_mode(&state, params.feature_type.as_deref())
    } else {
        list_entries_project_mode(params.feature_type.as_deref())
    };
    Json(entries)
}

#[derive(serde::Deserialize)]
pub struct CacheEntriesQuery {
    #[serde(rename = "feature_type")]
    feature_type: Option<String>,
}

#[derive(serde::Serialize)]
pub struct CacheEntry {
    cache_key: String,
    feature_type: String,
    file_path: String,
    mtime: f64,
    data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_dir: Option<String>,
}

pub async fn clear_cache(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ClearCacheRequest>,
) -> Json<ClearCacheResponse> {
    let settings = state.get_settings();
    if settings.storage_mode == "folder" {
        clear_folder_cache(&state, body.feature_type.as_deref());
    } else {
        clear_project_cache(body.feature_type.as_deref());
    }
    Json(ClearCacheResponse {
        cleared: body.feature_type.clone().unwrap_or_else(|| "all".to_string()),
    })
}

#[derive(serde::Deserialize)]
pub struct ClearCacheRequest {
    #[serde(rename = "feature_type")]
    feature_type: Option<String>,
}

#[derive(serde::Serialize)]
pub struct ClearCacheResponse {
    cleared: String,
}

pub async fn delete_cache_entry(
    State(state): State<Arc<AppState>>,
    AxumPath(cache_key): AxumPath<String>,
) -> Result<Json<DeleteCacheEntryResponse>, (axum::http::StatusCode, &'static str)> {
    let settings = state.get_settings();
    let deleted = if settings.storage_mode == "folder" {
        delete_cache_entry_folder(&state, &cache_key)
    } else {
        delete_cache_entry_project(&cache_key)
    };

    if !deleted {
        return Err((axum::http::StatusCode::NOT_FOUND, "缓存条目不存在"));
    }
    Ok(Json(DeleteCacheEntryResponse { deleted: cache_key }))
}

#[derive(serde::Serialize)]
pub struct DeleteCacheEntryResponse {
    deleted: String,
}

pub async fn export_cache_to_folder(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ExportImportRequest>,
) -> Json<ExportImportResponse> {
    Json(export_project_cache_to_folder(&state, body.dir_paths.as_deref()))
}

#[derive(serde::Deserialize)]
pub struct ExportImportRequest {
    #[serde(rename = "dir_paths")]
    dir_paths: Option<Vec<String>>,
}

#[derive(serde::Serialize)]
pub struct ExportImportResponse {
    migrated: usize,
    directories: usize,
}

pub async fn import_cache_from_folder(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ExportImportRequest>,
) -> Json<ExportImportResponse> {
    Json(import_folder_cache_to_project(&state, body.dir_paths.as_deref()))
}

fn collect_image_files(dir: &StdPath, recursive: bool) -> Vec<String> {
    let mut files = Vec::new();
    let walker = if recursive {
        walkdir::WalkDir::new(dir)
    } else {
        walkdir::WalkDir::new(dir).max_depth(1)
    };

    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().map(|s| s.to_string_lossy().to_lowercase()) {
                if matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "tiff") {
                    files.push(path.to_string_lossy().to_string());
                }
            }
        }
    }
    files
}

fn is_image_file(path: &str) -> bool {
    let ext = StdPath::new(path)
        .extension()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "tiff")
}

async fn run_dedup_job(state: Arc<AppState>, job_id: String, paths: Vec<String>) {
    state.update_dedup_job(
        &job_id,
        DedupJobUpdate {
            status: Some("running".to_string()),
            stage: Some("组合去重".to_string()),
            groups: None,
            finished_at: None,
        },
    );

    // Keep behavior deterministic and lightweight: group files with same stem as potential duplicates.
    let mut by_stem: HashMap<String, Vec<String>> = HashMap::new();
    for p in &paths {
        let stem = StdPath::new(p)
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        by_stem.entry(stem).or_default().push(p.clone());
    }

    let mut groups: Vec<DedupGroup> = Vec::new();
    for (idx, items) in by_stem.values().filter(|v| v.len() > 1).enumerate() {
        let mut dedup_items = Vec::new();
        for p in items {
            let path = PathBuf::from(p);
            let file_name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let file_size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            dedup_items.push(crate::models::DedupItem {
                path: p.clone(),
                file_name,
                thumbnail_url: Some(format!("/api/thumbnails?path={}", encode_uri(p))),
                file_size,
                similarity: 0.0,
                metadata: HashMap::new(),
                siblings: Vec::new(),
            });
        }

        groups.push(DedupGroup {
            group_id: format!("g_{}", idx),
            representative: dedup_items.first().map(|x| x.path.clone()),
            items: dedup_items,
            stage: "composite".to_string(),
        });
    }

    tokio::time::sleep(Duration::from_millis(50)).await;

    state.update_dedup_job(
        &job_id,
        DedupJobUpdate {
            status: Some("completed".to_string()),
            stage: Some("完成".to_string()),
            groups: Some(groups),
            finished_at: Some(chrono::Utc::now().to_rfc3339()),
        },
    );
}

fn project_cache_dir() -> PathBuf {
    features_dir()
}

fn load_json(path: &StdPath) -> serde_json::Value {
    if !path.exists() {
        return serde_json::json!({});
    }
    match fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| serde_json::json!({})),
        Err(_) => serde_json::json!({}),
    }
}

fn save_json(path: &StdPath, value: &serde_json::Value) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(content) = serde_json::to_string_pretty(value) {
        let _ = fs::write(path, content);
    }
}

fn encode_uri(input: &str) -> String {
    let mut result = String::new();
    for c in input.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

fn stats_project_mode() -> CacheStats {
    let mut result: CacheStats = HashMap::new();
    let cache_dir = project_cache_dir();

    let hashes = load_json(&cache_dir.join("hashes.json"));
    if let Some(obj) = hashes.as_object() {
        for entry in obj.values() {
            let t = entry.get("type").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
            *result.entry(t).or_insert(0) += 1;
        }
    }

    let exif = load_json(&cache_dir.join("exif.json"));
    if let Some(obj) = exif.as_object() {
        result.insert("exif".to_string(), obj.len());
    }

    let emb_dir = cache_dir.join("embeddings");
    if emb_dir.exists() {
        if let Ok(rd) = fs::read_dir(&emb_dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let v = load_json(&p);
                let model = v.get("model").and_then(|x| x.as_str()).unwrap_or("unknown");
                let key = format!("emb_{}", model);
                *result.entry(key).or_insert(0) += 1;
            }
        }
    }

    result
}

fn stats_folder_mode(state: &AppState) -> CacheStats {
    let mut result: CacheStats = HashMap::new();
    for cache_dir in scan_folder_cache_dirs(state, None) {
        let hashes = load_json(&cache_dir.join("hashes.json"));
        if let Some(obj) = hashes.as_object() {
            for entry in obj.values() {
                let t = entry.get("type").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                *result.entry(t).or_insert(0) += 1;
            }
        }

        let exif = load_json(&cache_dir.join("exif.json"));
        if let Some(obj) = exif.as_object() {
            *result.entry("exif".to_string()).or_insert(0) += obj.len();
        }

        let emb_dir = cache_dir.join("embeddings");
        if emb_dir.exists() {
            if let Ok(rd) = fs::read_dir(&emb_dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.extension().and_then(|s| s.to_str()) != Some("json") {
                        continue;
                    }
                    let v = load_json(&p);
                    let model = v.get("model").and_then(|x| x.as_str()).unwrap_or("unknown");
                    let key = format!("emb_{}", model);
                    *result.entry(key).or_insert(0) += 1;
                }
            }
        }
    }
    result
}

fn list_entries_project_mode(feature_type: Option<&str>) -> Vec<CacheEntry> {
    let mut entries = Vec::new();
    let cache_dir = project_cache_dir();

    if feature_type.is_none() || feature_type.unwrap_or_default().starts_with("hash_") {
        let hashes = load_json(&cache_dir.join("hashes.json"));
        if let Some(obj) = hashes.as_object() {
            for (k, v) in obj {
                let ft = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
                if let Some(filter) = feature_type {
                    if filter != ft {
                        continue;
                    }
                }
                entries.push(CacheEntry {
                    cache_key: k.clone(),
                    feature_type: ft.to_string(),
                    file_path: v.get("file_path").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    mtime: v.get("mtime").and_then(|x| x.as_f64()).unwrap_or(0.0),
                    data: v.get("data").cloned().unwrap_or(serde_json::json!({})),
                    base_dir: None,
                });
            }
        }
    }

    if feature_type.is_none() || feature_type == Some("exif") {
        let exif = load_json(&cache_dir.join("exif.json"));
        if let Some(obj) = exif.as_object() {
            for (k, v) in obj {
                entries.push(CacheEntry {
                    cache_key: k.clone(),
                    feature_type: "exif".to_string(),
                    file_path: v.get("file_path").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    mtime: v.get("mtime").and_then(|x| x.as_f64()).unwrap_or(0.0),
                    data: v.get("data").cloned().unwrap_or(serde_json::json!({})),
                    base_dir: None,
                });
            }
        }
    }

    if feature_type.is_none() || feature_type.unwrap_or_default().starts_with("emb_") {
        let emb_dir = cache_dir.join("embeddings");
        if emb_dir.exists() {
            if let Ok(rd) = fs::read_dir(&emb_dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.extension().and_then(|s| s.to_str()) != Some("json") {
                        continue;
                    }
                    let v = load_json(&p);
                    let model = v.get("model").and_then(|x| x.as_str()).unwrap_or("unknown");
                    let ft = format!("emb_{}", model);
                    if let Some(filter) = feature_type {
                        if filter != ft {
                            continue;
                        }
                    }
                    entries.push(CacheEntry {
                        cache_key: p.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string(),
                        feature_type: ft,
                        file_path: v.get("file_path").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                        mtime: v.get("mtime").and_then(|x| x.as_f64()).unwrap_or(0.0),
                        data: serde_json::json!({
                            "model": model,
                            "dim": v.get("embedding").and_then(|x| x.as_array()).map(|a| a.len()).unwrap_or(0)
                        }),
                        base_dir: None,
                    });
                }
            }
        }
    }

    entries
}

fn list_entries_folder_mode(state: &AppState, feature_type: Option<&str>) -> Vec<CacheEntry> {
    let mut entries = Vec::new();
    for cache_dir in scan_folder_cache_dirs(state, None) {
        let base = cache_dir.parent().unwrap_or_else(|| StdPath::new(""));

        let hashes = load_json(&cache_dir.join("hashes.json"));
        if let Some(obj) = hashes.as_object() {
            for (k, v) in obj {
                let ft = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
                if let Some(filter) = feature_type {
                    if filter != ft {
                        continue;
                    }
                }
                let rel = v.get("file_path").and_then(|x| x.as_str()).unwrap_or("");
                entries.push(CacheEntry {
                    cache_key: k.clone(),
                    feature_type: ft.to_string(),
                    file_path: base.join(rel).to_string_lossy().to_string(),
                    mtime: v.get("mtime").and_then(|x| x.as_f64()).unwrap_or(0.0),
                    data: v.get("data").cloned().unwrap_or(serde_json::json!({})),
                    base_dir: Some(base.to_string_lossy().to_string()),
                });
            }
        }

        let exif = load_json(&cache_dir.join("exif.json"));
        if let Some(obj) = exif.as_object() {
            for (k, v) in obj {
                if let Some(filter) = feature_type {
                    if filter != "exif" {
                        continue;
                    }
                }
                let rel = v.get("file_path").and_then(|x| x.as_str()).unwrap_or("");
                entries.push(CacheEntry {
                    cache_key: k.clone(),
                    feature_type: "exif".to_string(),
                    file_path: base.join(rel).to_string_lossy().to_string(),
                    mtime: v.get("mtime").and_then(|x| x.as_f64()).unwrap_or(0.0),
                    data: v.get("data").cloned().unwrap_or(serde_json::json!({})),
                    base_dir: Some(base.to_string_lossy().to_string()),
                });
            }
        }

        let emb_dir = cache_dir.join("embeddings");
        if emb_dir.exists() {
            if let Ok(rd) = fs::read_dir(&emb_dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.extension().and_then(|s| s.to_str()) != Some("json") {
                        continue;
                    }
                    let v = load_json(&p);
                    let model = v.get("model").and_then(|x| x.as_str()).unwrap_or("unknown");
                    let ft = format!("emb_{}", model);
                    if let Some(filter) = feature_type {
                        if filter != ft {
                            continue;
                        }
                    }
                    let rel = v.get("file_path").and_then(|x| x.as_str()).unwrap_or("");
                    entries.push(CacheEntry {
                        cache_key: p.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string(),
                        feature_type: ft,
                        file_path: base.join(rel).to_string_lossy().to_string(),
                        mtime: v.get("mtime").and_then(|x| x.as_f64()).unwrap_or(0.0),
                        data: serde_json::json!({
                            "model": model,
                            "dim": v.get("embedding").and_then(|x| x.as_array()).map(|a| a.len()).unwrap_or(0)
                        }),
                        base_dir: Some(base.to_string_lossy().to_string()),
                    });
                }
            }
        }
    }
    entries
}

fn clear_project_cache(feature_type: Option<&str>) {
    let cache_dir = project_cache_dir();
    let hashes_file = cache_dir.join("hashes.json");
    let exif_file = cache_dir.join("exif.json");
    let emb_dir = cache_dir.join("embeddings");

    match feature_type {
        None => {
            save_json(&hashes_file, &serde_json::json!({}));
            save_json(&exif_file, &serde_json::json!({}));
            if emb_dir.exists() {
                let _ = fs::remove_dir_all(&emb_dir);
            }
        }
        Some(ft) if ft.starts_with("hash_") => {
            let mut hashes = load_json(&hashes_file);
            if let Some(obj) = hashes.as_object_mut() {
                obj.retain(|_, v| v.get("type").and_then(|x| x.as_str()) != Some(ft));
            }
            save_json(&hashes_file, &hashes);
        }
        Some("exif") => save_json(&exif_file, &serde_json::json!({})),
        Some(ft) if ft.starts_with("emb_") => {
            let model = ft.trim_start_matches("emb_");
            if emb_dir.exists() {
                if let Ok(rd) = fs::read_dir(&emb_dir) {
                    for e in rd.flatten() {
                        let p = e.path();
                        if p.extension().and_then(|s| s.to_str()) == Some("json") {
                            let v = load_json(&p);
                            if v.get("model").and_then(|x| x.as_str()) == Some(model) {
                                let _ = fs::remove_file(p);
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn clear_folder_cache(state: &AppState, feature_type: Option<&str>) {
    for cache_dir in scan_folder_cache_dirs(state, None) {
        match feature_type {
            None => {
                let _ = fs::remove_dir_all(&cache_dir);
            }
            Some(ft) if ft.starts_with("hash_") => {
                let path = cache_dir.join("hashes.json");
                let mut hashes = load_json(&path);
                if let Some(obj) = hashes.as_object_mut() {
                    obj.retain(|_, v| v.get("type").and_then(|x| x.as_str()) != Some(ft));
                }
                save_json(&path, &hashes);
            }
            Some("exif") => save_json(&cache_dir.join("exif.json"), &serde_json::json!({})),
            Some(ft) if ft.starts_with("emb_") => {
                let model = ft.trim_start_matches("emb_");
                let emb_dir = cache_dir.join("embeddings");
                if emb_dir.exists() {
                    if let Ok(rd) = fs::read_dir(&emb_dir) {
                        for e in rd.flatten() {
                            let p = e.path();
                            let v = load_json(&p);
                            if v.get("model").and_then(|x| x.as_str()) == Some(model) {
                                let _ = fs::remove_file(p);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn delete_cache_entry_project(cache_key: &str) -> bool {
    let cache_dir = project_cache_dir();
    let hashes_file = cache_dir.join("hashes.json");
    let exif_file = cache_dir.join("exif.json");

    let mut hashes = load_json(&hashes_file);
    if let Some(obj) = hashes.as_object_mut() {
        if obj.remove(cache_key).is_some() {
            save_json(&hashes_file, &hashes);
            return true;
        }
    }

    let mut exif = load_json(&exif_file);
    if let Some(obj) = exif.as_object_mut() {
        if obj.remove(cache_key).is_some() {
            save_json(&exif_file, &exif);
            return true;
        }
    }

    let emb_file = cache_dir.join("embeddings").join(format!("{}.json", cache_key));
    if emb_file.exists() {
        let _ = fs::remove_file(emb_file);
        return true;
    }

    false
}

fn delete_cache_entry_folder(state: &AppState, cache_key: &str) -> bool {
    for cache_dir in scan_folder_cache_dirs(state, None) {
        let hashes_file = cache_dir.join("hashes.json");
        let mut hashes = load_json(&hashes_file);
        if let Some(obj) = hashes.as_object_mut() {
            if obj.remove(cache_key).is_some() {
                save_json(&hashes_file, &hashes);
                return true;
            }
        }

        let exif_file = cache_dir.join("exif.json");
        let mut exif = load_json(&exif_file);
        if let Some(obj) = exif.as_object_mut() {
            if obj.remove(cache_key).is_some() {
                save_json(&exif_file, &exif);
                return true;
            }
        }

        let emb_file = cache_dir.join("embeddings").join(format!("{}.json", cache_key));
        if emb_file.exists() {
            let _ = fs::remove_file(emb_file);
            return true;
        }
    }
    false
}

fn export_project_cache_to_folder(state: &AppState, dir_paths: Option<&[String]>) -> ExportImportResponse {
    let entries = list_entries_project_mode(None);
    let mut groups: HashMap<String, Vec<CacheEntry>> = HashMap::new();

    for entry in entries {
        let fp = PathBuf::from(&entry.file_path);
        let mut matched_base: Option<PathBuf> = None;
        if let Some(paths) = dir_paths {
            for dp in paths {
                let base = PathBuf::from(dp);
                if fp.starts_with(&base) {
                    matched_base = Some(base);
                    break;
                }
            }
            if matched_base.is_none() {
                continue;
            }
        } else {
            matched_base = fp.parent().map(|p| p.to_path_buf());
        }

        if let Some(base) = matched_base {
            groups.entry(base.to_string_lossy().to_string()).or_default().push(entry);
        }
    }

    let mut migrated = 0usize;
    let directories = groups.len();
    for (base_str, items) in groups {
        let base = PathBuf::from(base_str);
        let cache_dir = base.join(FOLDER_CACHE_DIR_NAME);
        let _ = fs::create_dir_all(&cache_dir);

        let hashes_file = cache_dir.join("hashes.json");
        let exif_file = cache_dir.join("exif.json");
        let emb_dir = cache_dir.join("embeddings");

        let mut hashes = load_json(&hashes_file);
        let mut exif = load_json(&exif_file);
        if hashes.as_object().is_none() {
            hashes = serde_json::json!({});
        }
        if exif.as_object().is_none() {
            exif = serde_json::json!({});
        }

        for entry in items {
            let rel = PathBuf::from(&entry.file_path)
                .strip_prefix(&base)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or(entry.file_path.clone());
            let key = path_key(&rel, entry.mtime);
            if entry.feature_type.starts_with("hash_") {
                if let Some(obj) = hashes.as_object_mut() {
                    obj.insert(
                        key,
                        serde_json::json!({
                            "type": entry.feature_type,
                            "file_path": rel,
                            "mtime": entry.mtime,
                            "data": entry.data,
                        }),
                    );
                }
            } else if entry.feature_type == "exif" {
                if let Some(obj) = exif.as_object_mut() {
                    obj.insert(
                        key,
                        serde_json::json!({
                            "file_path": rel,
                            "mtime": entry.mtime,
                            "data": entry.data,
                        }),
                    );
                }
            } else if entry.feature_type.starts_with("emb_") {
                let _ = fs::create_dir_all(&emb_dir);
                let model = entry.feature_type.trim_start_matches("emb_");
                let out = emb_dir.join(format!("{}_{}.json", key, model));
                save_json(
                    &out,
                    &serde_json::json!({
                        "model": model,
                        "file_path": rel,
                        "mtime": entry.mtime,
                        "embedding": [],
                    }),
                );
            }
            migrated += 1;
        }

        save_json(&hashes_file, &hashes);
        save_json(&exif_file, &exif);
    }

    ExportImportResponse { migrated, directories }
}

fn import_folder_cache_to_project(state: &AppState, dir_paths: Option<&[String]>) -> ExportImportResponse {
    let cache_dir = project_cache_dir();
    let _ = fs::create_dir_all(&cache_dir);
    let hashes_file = cache_dir.join("hashes.json");
    let exif_file = cache_dir.join("exif.json");
    let emb_dir = cache_dir.join("embeddings");
    let _ = fs::create_dir_all(&emb_dir);

    let mut hashes = load_json(&hashes_file);
    let mut exif = load_json(&exif_file);
    if hashes.as_object().is_none() {
        hashes = serde_json::json!({});
    }
    if exif.as_object().is_none() {
        exif = serde_json::json!({});
    }

    let cache_dirs = scan_folder_cache_dirs(state, dir_paths);
    let mut migrated = 0usize;
    for cache in &cache_dirs {
        let base = cache.parent().unwrap_or_else(|| StdPath::new(""));

        let hv = load_json(&cache.join("hashes.json"));
        if let Some(obj) = hv.as_object() {
            for v in obj.values() {
                let rel = v.get("file_path").and_then(|x| x.as_str()).unwrap_or("");
                let abs = base.join(rel).to_string_lossy().to_string();
                let mtime = v.get("mtime").and_then(|x| x.as_f64()).unwrap_or(0.0);
                let key = path_key(&abs, mtime);
                if let Some(dst) = hashes.as_object_mut() {
                    dst.insert(
                        key,
                        serde_json::json!({
                            "type": v.get("type").and_then(|x| x.as_str()).unwrap_or(""),
                            "file_path": abs,
                            "mtime": mtime,
                            "data": v.get("data").cloned().unwrap_or(serde_json::json!({})),
                        }),
                    );
                }
                migrated += 1;
            }
        }

        let ev = load_json(&cache.join("exif.json"));
        if let Some(obj) = ev.as_object() {
            for v in obj.values() {
                let rel = v.get("file_path").and_then(|x| x.as_str()).unwrap_or("");
                let abs = base.join(rel).to_string_lossy().to_string();
                let mtime = v.get("mtime").and_then(|x| x.as_f64()).unwrap_or(0.0);
                let key = path_key(&abs, mtime);
                if let Some(dst) = exif.as_object_mut() {
                    dst.insert(
                        key,
                        serde_json::json!({
                            "file_path": abs,
                            "mtime": mtime,
                            "data": v.get("data").cloned().unwrap_or(serde_json::json!({})),
                        }),
                    );
                }
                migrated += 1;
            }
        }
    }

    save_json(&hashes_file, &hashes);
    save_json(&exif_file, &exif);
    ExportImportResponse {
        migrated,
        directories: cache_dirs.len(),
    }
}

fn scan_folder_cache_dirs(state: &AppState, dir_paths: Option<&[String]>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let selected: Vec<PathBuf> = if let Some(paths) = dir_paths {
        paths.iter().map(PathBuf::from).collect()
    } else {
        state.list_dirs().into_iter().map(|d| PathBuf::from(d.path)).collect()
    };

    for base in selected {
        let cache = base.join(FOLDER_CACHE_DIR_NAME);
        if cache.exists() {
            dirs.push(cache);
        }
    }
    dirs
}

fn path_key(path: &str, mtime: f64) -> String {
    let mut hasher = DefaultHasher::new();
    format!("{}:{}", path, mtime).hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
