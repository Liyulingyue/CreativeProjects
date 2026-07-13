use axum::{
    body::Bytes,
    extract::Query,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use image::imageops::FilterType;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use crate::paths::thumbs_dir;

static THUMB_CACHE: Lazy<RwLock<std::collections::HashMap<String, String>>> =
    Lazy::new(|| RwLock::new(std::collections::HashMap::new()));

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

    let img = match image::open(path) {
        Ok(img) => img,
        Err(_) => return Err((StatusCode::BAD_REQUEST, "无法打开图片")),
    };

    let thumbnail = img.resize(THUMBNAIL_SIZE, THUMBNAIL_SIZE, FilterType::Triangle);

    let mut bytes = Vec::new();
    let mut cursor = Cursor::new(&mut bytes);
    if thumbnail
        .write_to(&mut cursor, image::ImageFormat::Jpeg)
        .is_err()
    {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "生成缩略图失败"));
    }

    let _ = std::fs::write(&thumb_path, &bytes);
    THUMB_CACHE
        .write()
        .insert(cache_key, thumb_path.to_string_lossy().to_string());

    let mut resp = Response::new(Bytes::from(bytes).into());
    resp.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    Ok(resp)
}

#[derive(serde::Deserialize)]
pub struct ThumbnailQuery {
    path: String,
    full: Option<String>,
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
