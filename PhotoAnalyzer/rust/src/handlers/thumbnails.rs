use axum::{extract::Query, response::IntoResponse, body::Bytes};
use image::imageops::FilterType;
use std::io::Cursor;

const THUMBNAIL_SIZE: u32 = 200;

pub async fn get_thumbnail(
    Query(params): Query<ThumbnailQuery>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    let path = std::path::Path::new(&params.path);

    if !path.exists() || !path.is_file() {
        return Err((axum::http::StatusCode::NOT_FOUND, "File not found"));
    }

    let img = match image::open(path) {
        Ok(img) => img,
        Err(_) => return Err((axum::http::StatusCode::BAD_REQUEST, "Cannot open image")),
    };

    let thumbnail = img.resize(THUMBNAIL_SIZE, THUMBNAIL_SIZE, FilterType::Triangle);

    let mut bytes = Vec::new();
    let mut cursor = Cursor::new(&mut bytes);
    if let Err(_) = thumbnail.write_to(&mut cursor, image::ImageFormat::Jpeg) {
        return Err((axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Failed to encode thumbnail"));
    }

    Ok(Bytes::from(bytes))
}

#[derive(serde::Deserialize)]
pub struct ThumbnailQuery {
    path: String,
}
