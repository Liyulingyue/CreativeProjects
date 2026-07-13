use axum::{extract::Query, Json};
use std::path::Path;

use crate::models::{FsBrowseResult, FsEntry, FsSuggestResult};

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "bmp", "webp", "tiff"];
const RAW_EXTENSIONS: &[&str] = &["cr2", "arw", "dng", "nef", "orf", "rw2", "pef", "srw", "raf"];

fn get_home() -> String {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| "C:\\".to_string())
}

fn get_common_roots() -> Vec<String> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    {
        for letter in 'A'..='Z' {
            let drive = format!("{}:\\", letter);
            if Path::new(&drive).exists() {
                roots.push(drive);
            }
        }
    }
    #[cfg(not(windows))]
    {
        if Path::new("/mnt").exists() {
            roots.push("/mnt".to_string());
        }
        if Path::new("/media").exists() {
            roots.push("/media".to_string());
        }
        if Path::new("/Volumes").exists() {
            roots.push("/Volumes".to_string());
        }
        if Path::new("/home").exists() {
            roots.push("/home".to_string());
        }
    }
    roots
}

fn count_children(path: &Path) -> Option<usize> {
    path.read_dir().ok().map(|entries| {
        entries.filter_map(|e| e.ok())
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .count()
    })
}

pub async fn browse_fs(
    Query(params): Query<FsBrowseQuery>,
) -> Result<Json<FsBrowseResult>, (axum::http::StatusCode, &'static str)> {
    let home = get_home();
    let common_roots = get_common_roots();
    let path = params.path.as_deref().unwrap_or("");

    if path.is_empty() {
        let mut entries = Vec::new();
        entries.push(FsEntry {
            name: "~ (Home)".to_string(),
            path: home.clone(),
            is_dir: true,
            children_count: count_children(Path::new(&home)),
        });
        for root in &common_roots {
            let rp = Path::new(root);
            if rp.exists() {
                entries.push(FsEntry {
                    name: root.clone(),
                    path: root.clone(),
                    is_dir: true,
                    children_count: count_children(rp),
                });
            }
        }
        return Ok(Json(FsBrowseResult {
            current_path: String::new(),
            parent_path: None,
            entries,
            home,
        }));
    }

    let target = Path::new(&path);
    if !target.exists() {
        return Err((axum::http::StatusCode::BAD_REQUEST, "path not found"));
    }
    if !target.is_dir() {
        return Err((axum::http::StatusCode::BAD_REQUEST, "not a directory"));
    }

    let parent_path = target.parent()
        .filter(|p| *p != target)
        .map(|p| p.to_string_lossy().to_string());

    let mut entries = Vec::new();
    let dirs_only = params.dirs_only.unwrap_or(true);
    if let Ok(children) = target.read_dir() {
        for child in children.filter_map(|e| e.ok()) {
            let name = child.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = child.path();
            let is_dir = path.is_dir();
            if dirs_only && !is_dir {
                continue;
            }
            entries.push(FsEntry {
                name,
                path: path.to_string_lossy().to_string(),
                is_dir,
                children_count: if is_dir { count_children(&path) } else { None },
            });
        }
    }

    entries.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(Json(FsBrowseResult {
        current_path: target.to_string_lossy().to_string(),
        parent_path,
        entries,
        home,
    }))
}

#[derive(serde::Deserialize)]
pub struct FsBrowseQuery {
    #[serde(default)]
    path: Option<String>,
    #[serde(rename = "dirs_only", default)]
    dirs_only: Option<bool>,
}

pub async fn suggest_path(
    Query(params): Query<FsSuggestQuery>,
) -> Json<FsSuggestResult> {
    let home = get_home();
    let common_roots = get_common_roots();

    if params.q.is_empty() {
        let mut suggestions = common_roots.iter()
            .map(|r| FsEntry {
                name: r.clone(),
                path: r.clone(),
                is_dir: true,
                children_count: None,
            })
            .collect::<Vec<_>>();
        suggestions.insert(0, FsEntry {
            name: "~".to_string(),
            path: home.clone(),
            is_dir: true,
            children_count: None,
        });
        return Json(FsSuggestResult {
            suggestions,
            partial: String::new(),
        });
    }

    let expanded = if params.q.starts_with('~') {
        params.q.replacen('~', &home, 1)
    } else {
        params.q.clone()
    };

    let p = Path::new(&expanded);
    let (parent, prefix) = if p.is_dir() && params.q.ends_with('/') {
        (p, String::new())
    } else {
        let parent = p.parent().unwrap_or(p);
        let prefix = p.file_name()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        (parent, prefix)
    };

    if !parent.is_dir() {
        return Json(FsSuggestResult {
            suggestions: vec![],
            partial: params.q,
        });
    }

    let mut suggestions = Vec::new();
    if let Ok(children) = parent.read_dir() {
        for child in children.filter_map(|e| e.ok()) {
            let file_name = child.file_name();
            let name = file_name.to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = child.path();
            if !path.is_dir() {
                continue;
            }
            let lower_name = name.to_lowercase();
            if !prefix.is_empty() && !lower_name.starts_with(&prefix) {
                continue;
            }
            suggestions.push(FsEntry {
                name,
                path: path.to_string_lossy().to_string(),
                is_dir: true,
                children_count: count_children(&path),
            });
        }
    }

    suggestions.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Json(FsSuggestResult {
        suggestions,
        partial: params.q,
    })
}

#[derive(serde::Deserialize)]
pub struct FsSuggestQuery {
    q: String,
}
