use axum::{extract::{Path as AxumPath, Query, State}, Json};
use std::collections::HashMap;
use std::path::Path as StdPath;
use std::sync::Arc;

use crate::models::{DedupGroup, DedupJob};
use crate::services::{AppState, DedupJobUpdate};

pub async fn start_dedup(
    State(state): State<Arc<AppState>>,
    Json(body): Json<StartDedupRequest>,
) -> Result<Json<DedupJob>, (axum::http::StatusCode, &'static str)> {
    let (total_files, dir_path) = if let Some(ref dir_id) = body.dir_id {
        let entry = state.get_dir(dir_id).ok_or((axum::http::StatusCode::NOT_FOUND, "directory not found"))?;
        let base = StdPath::new(&entry.path);
        let target = body.sub_path.as_ref().map(|s| base.join(s)).unwrap_or_else(|| base.to_path_buf());
        if !target.exists() {
            return Err((axum::http::StatusCode::BAD_REQUEST, "path not found"));
        }
        let files = collect_image_files(&target, body.recursive.unwrap_or(true));
        (files.len(), Some(target.to_string_lossy().to_string()))
    } else if let Some(ref paths) = body.file_paths {
        (paths.len(), None)
    } else {
        return Err((axum::http::StatusCode::BAD_REQUEST, "need dir_id or file_paths"));
    };

    let job = state.create_dedup_job(total_files, body.dir_id.as_deref(), dir_path.as_deref());

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
        .ok_or((axum::http::StatusCode::NOT_FOUND, "job not found"))
}

pub async fn get_dedup_by_dir(
    State(state): State<Arc<AppState>>,
    AxumPath(dir_id): AxumPath<String>,
) -> Result<Json<DedupJob>, (axum::http::StatusCode, &'static str)> {
    state.get_latest_dedup_job(&dir_id, None)
        .map(Json)
        .ok_or((axum::http::StatusCode::NOT_FOUND, "no dedup result for this directory"))
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
    Json(CacheStats {
        total_entries: 0,
        total_size: 0,
    })
}

#[derive(serde::Serialize)]
pub struct CacheStats {
    pub total_entries: usize,
    pub total_size: usize,
}

pub async fn get_cache_entries(
    Query(params): Query<CacheEntriesQuery>,
) -> Json<Vec<CacheEntry>> {
    Json(vec![])
}

#[derive(serde::Deserialize)]
pub struct CacheEntriesQuery {
    #[serde(rename = "feature_type")]
    feature_type: Option<String>,
}

#[derive(serde::Serialize)]
pub struct CacheEntry {
    key: String,
    feature_type: String,
    size: usize,
    created_at: String,
}

pub async fn clear_cache(
    Json(body): Json<ClearCacheRequest>,
) -> Json<ClearCacheResponse> {
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
    AxumPath(cache_key): AxumPath<String>,
) -> Result<Json<DeleteCacheEntryResponse>, (axum::http::StatusCode, &'static str)> {
    Ok(Json(DeleteCacheEntryResponse { deleted: cache_key }))
}

#[derive(serde::Serialize)]
pub struct DeleteCacheEntryResponse {
    deleted: String,
}

pub async fn export_cache_to_folder(
    Json(body): Json<ExportImportRequest>,
) -> Json<ExportImportResponse> {
    Json(ExportImportResponse {
        migrated: 0,
        directories: 0,
    })
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
    Json(body): Json<ExportImportRequest>,
) -> Json<ExportImportResponse> {
    Json(ExportImportResponse {
        migrated: 0,
        directories: 0,
    })
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
