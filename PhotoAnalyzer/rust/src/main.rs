mod models;
mod services;
mod handlers;

use axum::{
    routing::{get, post, delete},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use services::AppState;

async fn health() -> &'static str {
    "OK"
}

async fn serve_index() -> &'static str {
    "Photo Analyzer API Server - Frontend not embedded. Build the frontend first or run with --features embed-frontend"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let state = Arc::new(AppState::new());

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/", get(serve_index))
        .route("/api/dirs", get(handlers::list_dirs))
        .route("/api/dirs", post(handlers::add_dir))
        .route("/api/dirs/:id", delete(handlers::remove_dir))
        .route("/api/files", get(handlers::browse_files))
        .route("/api/files", delete(handlers::delete_file))
        .route("/api/files/siblings", get(handlers::get_siblings))
        .route("/api/files/orphaned-raws", get(handlers::get_orphaned_raws))
        .route("/api/files/orphaned-raws", delete(handlers::delete_orphaned_raws))
        .route("/api/thumbnails", get(handlers::get_thumbnail))
        .route("/api/analysis", post(handlers::start_analysis))
        .route("/api/analysis/folder", post(handlers::start_folder_analysis))
        .route("/api/analysis/:job_id", get(handlers::get_analysis_job))
        .route("/api/results", get(handlers::list_results))
        .route("/api/results/*file_path", get(handlers::get_result))
        .route("/api/dedup", post(handlers::start_dedup))
        .route("/api/dedup/:job_id", get(handlers::get_dedup_job))
        .route("/api/dedup/:job_id/resolve", post(handlers::resolve_dedup))
        .route("/api/dedup/by-dir/:dir_id", get(handlers::get_dedup_by_dir))
        .route("/api/dedup/cache/stats", get(handlers::get_cache_stats_with_state))
        .route("/api/dedup/cache/entries", get(handlers::get_cache_entries))
        .route("/api/dedup/cache/clear", post(handlers::clear_cache))
        .route("/api/dedup/cache/entries/:cache_key", delete(handlers::delete_cache_entry))
        .route("/api/dedup/cache/export-to-folder", post(handlers::export_cache_to_folder))
        .route("/api/dedup/cache/import-from-folder", post(handlers::import_cache_from_folder))
        .route("/api/settings", get(handlers::get_settings))
        .route("/api/settings", axum::routing::put(handlers::update_settings))
        .route("/api/stats", get(handlers::get_stats))
        .layer(cors)
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .expect("PORT must be a valid port");

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Starting PhotoAnalyzer server on http://localhost:{}", port);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
