use axum::{
    body::Bytes,
    extract::{Path as AxumPath, Query},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use image::imageops::FilterType;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use tokio::sync::RwLock as AsyncRwLock;
use crate::models::ThumbnailJob;
use crate::paths::thumbs_dir;

static THUMB_CACHE: Lazy<RwLock<HashMap<String, String>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

static THUMB_JOBS: Lazy<AsyncRwLock<HashMap<String, ThumbnailJob>>> =
    Lazy::new(|| AsyncRwLock::new(HashMap::new()));

const THUMBNAIL_SIZE: u32 = 200;

pub async fn get_thumbnail(
    Query(params): Query<ThumbnailQuery>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    let path = Path::new(&params.path);

    if !path.exists() || !path.is_file() {
        return Err((StatusCode::NOT_FOUND, "文件不存在"));
    }

    if !is_supported_image(path) {
        return Err((StatusCode::BAD_REQUEST, "不支持的图片格式"));
    }

    if params
        .full
        .as_deref()
        .map(is_truthy_flag)
        .unwrap_or(false)
    {
        let content_type = image_media_type(path);
        return read_file_response(path, content_type)
            .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "读取原图失败"));
    }

    let cache_key = format!("thumb_{}", &params.path);
    if let Some(cached_path) = THUMB_CACHE.read().get(&cache_key).cloned() {
        let cached = PathBuf::from(cached_path);
        if cached.exists() {
            if let Some(resp) = read_file_response(&cached, "image/jpeg") {
                return Ok(resp);
            }
        }
    }

    let thumb_dir = thumbs_dir();
    let _ = std::fs::create_dir_all(&thumb_dir);

    let size = path.metadata().map(|m| m.len()).unwrap_or(0);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("thumb");
    let thumb_path = thumb_dir.join(format!("{}_{}.jpg", stem, size));

    if thumb_path.exists() {
        THUMB_CACHE
            .write()
            .insert(cache_key, thumb_path.to_string_lossy().to_string());
        return read_file_response(&thumb_path, "image/jpeg")
            .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "读取缓存缩略图失败"));
    }

    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(axum::body::Body::empty())
        .unwrap())
}

#[derive(serde::Deserialize)]
pub struct ThumbnailQuery {
    path: String,
    full: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct StartThumbnailBatchRequest {
    paths: Vec<String>,
}

pub async fn start_thumbnail_batch(
    Json(req): Json<StartThumbnailBatchRequest>,
) -> Result<Json<ThumbnailJob>, (StatusCode, &'static str)> {
    let valid_paths: Vec<String> = req
        .paths
        .into_iter()
        .filter(|p| Path::new(p).exists() && is_supported_image(Path::new(p)))
        .collect();

    if valid_paths.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "没有有效的图片路径"));
    }

    let job_id = uuid::Uuid::new_v4().to_string();
    let job = ThumbnailJob {
        job_id: job_id.clone(),
        status: "pending".to_string(),
        total: valid_paths.len(),
        progress: 0,
        completed: 0,
        failed: 0,
        current_file: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        finished_at: None,
    };

    {
        let mut jobs = THUMB_JOBS.write().await;
        jobs.insert(job_id.clone(), job.clone());
    }

    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        run_thumbnail_batch(job_id_clone, valid_paths).await;
    });

    Ok(Json(job))
}

async fn run_thumbnail_batch(job_id: String, paths: Vec<String>) {
    {
        let mut jobs = THUMB_JOBS.write().await;
        if let Some(job) = jobs.get_mut(&job_id) {
            job.status = "running".to_string();
        }
    }

    let thumb_dir = thumbs_dir();
    let _ = std::fs::create_dir_all(&thumb_dir);

    for (i, p) in paths.iter().enumerate() {
        {
            let jobs = THUMB_JOBS.read().await;
            if let Some(job) = jobs.get(&job_id) {
                if job.status == "canceled" {
                    break;
                }
            }
        }

        {
            let mut jobs = THUMB_JOBS.write().await;
            if let Some(job) = jobs.get_mut(&job_id) {
                job.progress = i;
                job.current_file = Some(Path::new(p).file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default());
            }
        }

        let img_path = Path::new(p);
        let cache_key = format!("thumb_{}", p);
        let size = img_path.metadata().map(|m| m.len()).unwrap_or(0);
        let stem = img_path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let thumb_path = thumb_dir.join(format!("{}_{}.jpg", stem, size));

        match generate_thumbnail_sync(img_path, &thumb_path) {
            Ok(_) => {
                THUMB_CACHE
                    .write()
                    .insert(cache_key, thumb_path.to_string_lossy().to_string());
                let mut jobs = THUMB_JOBS.write().await;
                if let Some(job) = jobs.get_mut(&job_id) {
                    job.completed += 1;
                }
            }
            Err(_) => {
                let mut jobs = THUMB_JOBS.write().await;
                if let Some(job) = jobs.get_mut(&job_id) {
                    job.failed += 1;
                }
            }
        }
    }

    let mut jobs = THUMB_JOBS.write().await;
    if let Some(job) = jobs.get_mut(&job_id) {
        job.status = "completed".to_string();
        job.finished_at = Some(chrono::Utc::now().to_rfc3339());
    }
}

fn generate_thumbnail_sync(path: &Path, thumb_path: &Path) -> Result<(), String> {
    if thumb_path.exists() {
        return Ok(());
    }

    let img = image::open(path).map_err(|_| "无法打开图片")?;
    let thumbnail = img.resize(THUMBNAIL_SIZE, THUMBNAIL_SIZE, FilterType::Triangle);

    let mut bytes = Vec::new();
    let mut cursor = Cursor::new(&mut bytes);
    thumbnail
        .write_to(&mut cursor, image::ImageFormat::Jpeg)
        .map_err(|_| "生成缩略图失败")?;

    std::fs::write(thumb_path, &bytes).map_err(|_| "保存缩略图失败")?;
    Ok(())
}

pub async fn get_thumbnail_job(
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<ThumbnailJob>, (StatusCode, &'static str)> {
    let jobs = THUMB_JOBS.read().await;
    jobs.get(&job_id)
        .cloned()
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "任务不存在"))
}

pub async fn cancel_thumbnail_job(
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<ThumbnailJob>, (StatusCode, &'static str)> {
    let mut jobs = THUMB_JOBS.write().await;
    if let Some(job) = jobs.get_mut(&job_id) {
        job.status = "canceled".to_string();
        job.finished_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(Json(job.clone()))
    } else {
        Err((StatusCode::NOT_FOUND, "任务不存在"))
    }
}

fn is_truthy_flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "t" | "yes" | "y" | "on"
    )
}

fn read_file_response(path: &Path, content_type: &str) -> Option<Response> {
    let bytes = std::fs::read(path).ok()?;
    let mut resp = Response::new(Bytes::from(bytes).into());
    let header_value = HeaderValue::from_str(content_type).ok()?;
    resp.headers_mut().insert(header::CONTENT_TYPE, header_value);
    Some(resp)
}

fn is_supported_image(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "tiff"
    )
}

fn image_media_type(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "tiff" => "image/tiff",
        _ => "application/octet-stream",
    }
}