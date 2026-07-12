use axum::{extract::State, Json};
use std::sync::Arc;

use crate::models::{AnalysisData, AnalysisResult};
use crate::services::AppState;

pub async fn analyze(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AnalyzeRequest>,
) -> Result<Json<AnalysisResult>, (axum::http::StatusCode, &'static str)> {
    let path = body.path.as_deref().unwrap_or("");
    if path.is_empty() {
        return Err((axum::http::StatusCode::BAD_REQUEST, "path required"));
    }

    let path_buf = std::path::Path::new(path);
    if !path_buf.exists() {
        let result = AnalysisResult {
            file_path: path.to_string(),
            success: false,
            data: None,
        };
        return Ok(Json(result));
    }

    let result = AnalysisResult {
        file_path: path.to_string(),
        success: true,
        data: Some(AnalysisData {
            score: 75,
            blurry: "清晰".to_string(),
            style: "城市街景".to_string(),
        }),
    };

    state.add_result(result.clone());
    Ok(Json(result))
}

#[derive(serde::Deserialize)]
pub struct AnalyzeRequest {
    path: Option<String>,
}

pub async fn analyze_batch(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Vec<String>>,
) -> Result<Json<Vec<AnalysisResult>>, (axum::http::StatusCode, &'static str)> {
    let mut results = Vec::new();

    for path in body {
        let path_buf = std::path::Path::new(&path);
        if !path_buf.exists() {
            results.push(AnalysisResult {
                file_path: path.clone(),
                success: false,
                data: None,
            });
            continue;
        }

        results.push(AnalysisResult {
            file_path: path.clone(),
            success: true,
            data: Some(AnalysisData {
                score: 75,
                blurry: "清晰".to_string(),
                style: "城市街景".to_string(),
            }),
        });
    }

    Ok(Json(results))
}

pub async fn list_results(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<AnalysisResult>> {
    Json(state.list_results())
}
