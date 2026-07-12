use axum::{extract::{Query, State}, Json};
use std::sync::Arc;
use std::path::Path;

use crate::models::{BrowseResult, FileNode};
use crate::services::AppState;

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "bmp", "webp", "tiff"];
const RAW_EXTENSIONS: &[&str] = &["cr2", "arw", "dng", "nef", "orf", "rw2", "pef", "srw", "raf"];

pub async fn browse_files(
    State(state): State<Arc<AppState>>,
    Query(params): Query<BrowseQuery>,
) -> Result<Json<BrowseResult>, (axum::http::StatusCode, &'static str)> {
    let dir = state.get_dir(&params.dir_id).ok_or((axum::http::StatusCode::NOT_FOUND, "directory not found"))?;

    let target = params.path.as_ref()
        .map(Path::new)
        .unwrap_or_else(|| Path::new(&dir.path))
        .to_path_buf();

    if !target.exists() {
        return Err((axum::http::StatusCode::NOT_FOUND, "path not found"));
    }

    let mut items = Vec::new();
    let parent_path = target.parent().map(|p| p.to_string_lossy().to_string());

    for entry in walkdir::WalkDir::new(&target)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path == target {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let name = path.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let modified = metadata.modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| {
                chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                    .unwrap_or_default()
                    .to_rfc3339()
            })
            .unwrap_or_default();

        let is_dir = metadata.is_dir();
        let size = if !is_dir { metadata.len() } else { 0 };

        let thumbnail_url = if !is_dir && is_image_file(&name) {
            Some(format!("/api/thumbnails?path={}", encode_uri(&path.to_string_lossy())))
        } else {
            None
        };

        items.push(FileNode {
            name,
            path: path.to_string_lossy().to_string(),
            is_dir,
            size,
            modified,
            thumbnail_url,
        });
    }

    items.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(Json(BrowseResult {
        current_path: target.to_string_lossy().to_string(),
        parent_path,
        items,
    }))
}

#[derive(serde::Deserialize)]
pub struct BrowseQuery {
    dir_id: String,
    path: Option<String>,
}

pub async fn delete_file(
    Query(params): Query<DeleteQuery>,
) -> Result<Json<DeleteResponse>, (axum::http::StatusCode, &'static str)> {
    let path = Path::new(&params.path);

    if !path.exists() {
        return Err((axum::http::StatusCode::NOT_FOUND, "file not found"));
    }

    if !path.is_file() {
        return Err((axum::http::StatusCode::BAD_REQUEST, "not a file"));
    }

    std::fs::remove_file(path).map_err(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "delete failed"))?;

    Ok(Json(DeleteResponse {
        deleted: params.path,
        count: 1,
    }))
}

#[derive(serde::Deserialize)]
pub struct DeleteQuery {
    path: String,
}

#[derive(serde::Serialize)]
pub struct DeleteResponse {
    deleted: String,
    count: usize,
}

pub async fn get_siblings(
    Query(params): Query<DeleteQuery>,
) -> Result<Json<SiblingsResponse>, (axum::http::StatusCode, &'static str)> {
    let p = Path::new(&params.path);

    if !p.exists() {
        return Err((axum::http::StatusCode::NOT_FOUND, "file not found"));
    }

    let parent = p.parent().ok_or((axum::http::StatusCode::NOT_FOUND, "no parent"))?;
    let stem = p.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut siblings = Vec::new();

    for entry in walkdir::WalkDir::new(parent)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path == p {
            continue;
        }
        if path.is_file() {
            if let Some(entry_stem) = path.file_stem().map(|s| s.to_string_lossy().to_string()) {
                if entry_stem == stem {
                    siblings.push(path.to_string_lossy().to_string());
                }
            }
        }
    }

    let count = siblings.len();
    Ok(Json(SiblingsResponse {
        siblings,
        count,
    }))
}

#[derive(serde::Serialize)]
pub struct SiblingsResponse {
    siblings: Vec<String>,
    count: usize,
}

pub async fn get_orphaned_raws(
    State(state): State<Arc<AppState>>,
    Query(params): Query<OrphanedQuery>,
) -> Result<Json<OrphanedResponse>, (axum::http::StatusCode, &'static str)> {
    let entry = state.get_dir(&params.dir_id).ok_or((axum::http::StatusCode::NOT_FOUND, "directory not found"))?;

    let target = Path::new(&entry.path);
    let mut orphaned = Vec::new();

    for entry in walkdir::WalkDir::new(target)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().map(|s| s.to_string_lossy().to_lowercase()) {
                if RAW_EXTENSIONS.contains(&ext.as_str()) {
                    let _stem = path.file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();

                    let jpg_path = path.with_extension("jpg");
                    let jpg_path_upper = path.with_extension("JPG");

                    if !jpg_path.exists() && !jpg_path_upper.exists() {
                        orphaned.push(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    let count = orphaned.len();
    Ok(Json(OrphanedResponse {
        orphaned,
        count,
    }))
}

#[derive(serde::Deserialize)]
pub struct OrphanedQuery {
    dir_id: String,
}

#[derive(serde::Serialize)]
pub struct OrphanedResponse {
    orphaned: Vec<String>,
    count: usize,
}

pub async fn delete_orphaned_raws(
    State(state): State<Arc<AppState>>,
    Query(params): Query<OrphanedQuery>,
) -> Result<Json<DeleteOrphanedResponse>, (axum::http::StatusCode, &'static str)> {
    let entry = state.get_dir(&params.dir_id).ok_or((axum::http::StatusCode::NOT_FOUND, "directory not found"))?;

    let target = Path::new(&entry.path);
    let mut deleted = Vec::new();

    for entry in walkdir::WalkDir::new(target)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().map(|s| s.to_string_lossy().to_lowercase()) {
                if RAW_EXTENSIONS.contains(&ext.as_str()) {
                    let _stem = path.file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();

                    let jpg_path = path.with_extension("jpg");
                    let jpg_path_upper = path.with_extension("JPG");

                    if !jpg_path.exists() && !jpg_path_upper.exists() {
                        if path.exists() {
                            let _ = std::fs::remove_file(path);
                            deleted.push(path.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
    }

    let count = deleted.len();
    Ok(Json(DeleteOrphanedResponse {
        deleted,
        not_found: vec![],
        count,
    }))
}

#[derive(serde::Serialize)]
pub struct DeleteOrphanedResponse {
    deleted: Vec<String>,
    not_found: Vec<String>,
    count: usize,
}

fn is_image_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    IMAGE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
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
