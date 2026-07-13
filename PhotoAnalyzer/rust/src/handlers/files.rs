use axum::{extract::{Query, State}, Json};
use std::collections::HashMap;
use std::sync::Arc;
use std::path::Path;

use crate::models::{BrowseResult, FileNode};
use crate::services::AppState;

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "bmp", "webp", "tiff"];
const RAW_EXTENSIONS: &[&str] = &["cr2", "cr3", "arw", "dng", "nef", "orf", "rw2", "pef", "srw", "raf", "3fr", "ai", "eps"];

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

    let base = Path::new(&dir.path).to_path_buf();
    let parent_path = if target != base {
        target.parent().map(|p| p.to_string_lossy().to_string())
    } else {
        None
    };

    let mut items = Vec::new();
    let mut stem_map: HashMap<String, Vec<std::path::PathBuf>> = HashMap::new();

    let read_dir = match std::fs::read_dir(&target) {
        Ok(rd) => rd,
        Err(_) => return Err((axum::http::StatusCode::INTERNAL_SERVER_ERROR, "failed to read directory")),
    };

    for entry in read_dir.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = path.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        if name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            let modified = path.metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| {
                    chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                        .unwrap_or_default()
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string()
                })
                .unwrap_or_default();

            items.push(FileNode {
                name,
                path: path.to_string_lossy().to_string(),
                is_dir: true,
                size: 0,
                modified,
                thumbnail_url: None,
            });
            continue;
        }

        let ext = path.extension()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        if is_image_file(&ext) || RAW_EXTENSIONS.contains(&ext.as_str()) {
            let stem = path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            stem_map.entry(stem).or_default().push(path);
        }
    }

    let mut stem_keys: Vec<_> = stem_map.keys().cloned().collect();
    stem_keys.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

    for stem in stem_keys {
        let mut paths = stem_map.get(&stem).unwrap().clone();
        paths.sort_by(|a, b| {
            let a_ext = a.extension().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
            let b_ext = b.extension().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
            let a_is_jpg = a_ext == "jpg";
            let b_is_jpg = b_ext == "jpg";
            if a_is_jpg != b_is_jpg {
                std::cmp::Ordering::Less
            } else {
                a_ext.cmp(&b_ext)
            }
        });

        let best = &paths[0];
        let best_ext = best.extension()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let thumbnail_url = if best_ext == "jpg" || best_ext == "jpeg" || best_ext == "png" || best_ext == "webp" {
            Some(format!("/api/thumbnails?path={}", encode_uri(&best.to_string_lossy())))
        } else {
            None
        };

        let metadata = best.metadata().ok();
        let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = metadata
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| {
                chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                    .unwrap_or_default()
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_default();

        items.push(FileNode {
            name: best.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
            path: best.to_string_lossy().to_string(),
            is_dir: false,
            size,
            modified,
            thumbnail_url,
        });
    }

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

    let read_dir = match std::fs::read_dir(parent) {
        Ok(rd) => rd,
        Err(_) => return Ok(Json(SiblingsResponse { siblings: vec![], count: 0 })),
    };

    for entry in read_dir.filter_map(|e| e.ok()) {
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
    let mut not_found = Vec::new();

    for entry in walkdir::WalkDir::new(target)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().map(|s| s.to_string_lossy().to_lowercase()) {
                if RAW_EXTENSIONS.contains(&ext.as_str()) {
                    let jpg_path = path.with_extension("jpg");
                    let jpg_path_upper = path.with_extension("JPG");

                    if !jpg_path.exists() && !jpg_path_upper.exists() {
                        if path.exists() {
                            let _ = std::fs::remove_file(path);
                            deleted.push(path.to_string_lossy().to_string());
                        } else {
                            not_found.push(path.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
    }

    let count = deleted.len();
    Ok(Json(DeleteOrphanedResponse {
        deleted,
        not_found,
        count,
    }))
}

#[derive(serde::Serialize)]
pub struct DeleteOrphanedResponse {
    deleted: Vec<String>,
    not_found: Vec<String>,
    count: usize,
}

fn is_image_file(ext: &str) -> bool {
    IMAGE_EXTENSIONS.contains(&ext)
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
