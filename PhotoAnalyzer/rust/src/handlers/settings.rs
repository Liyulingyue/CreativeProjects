use axum::{extract::State, Json};
use std::sync::Arc;

use crate::models::{AppSettings, Stats};
use crate::services::AppState;

pub async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Json<AppSettings> {
    Json(state.get_settings())
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(settings): Json<AppSettings>,
) -> Json<AppSettings> {
    state.update_settings(settings.clone());
    Json(settings)
}

pub async fn get_stats(
    State(state): State<Arc<AppState>>,
) -> Json<Stats> {
    Json(state.get_stats())
}
