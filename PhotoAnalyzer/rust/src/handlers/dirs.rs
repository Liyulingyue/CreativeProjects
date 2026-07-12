use axum::{extract::State, Json};
use std::sync::Arc;

use crate::services::AppState;

pub async fn list_dirs(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<crate::models::DirEntry>> {
    Json(state.list_dirs())
}

pub async fn add_dir(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AddDirRequest>,
) -> Result<Json<crate::models::DirEntry>, (axum::http::StatusCode, &'static str)> {
    let path = body.path.as_deref().unwrap_or("");
    if path.is_empty() {
        return Err((axum::http::StatusCode::BAD_REQUEST, "path required"));
    }

    let entry = state.add_dir(path, body.name.as_deref());
    Ok(Json(entry))
}

#[derive(serde::Deserialize)]
pub struct AddDirRequest {
    path: Option<String>,
    name: Option<String>,
}

pub async fn remove_dir(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, &'static str)> {
    if state.remove_dir(&id) {
        Ok(Json(serde_json::json!({ "deleted": id })))
    } else {
        Err((axum::http::StatusCode::NOT_FOUND, "not found"))
    }
}
