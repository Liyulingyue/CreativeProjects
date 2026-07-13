#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod models;
mod services;
mod handlers;
mod paths;

use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, delete},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[cfg(feature = "embed-frontend")]
use rust_embed::RustEmbed;

use services::AppState;

fn should_open_browser() -> bool {
    std::env::var("PHOTO_ANALYZER_OPEN_BROWSER")
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(true)
}

#[cfg(feature = "embed-frontend")]
#[derive(RustEmbed)]
#[folder = "../web/frontend/dist/"]
struct FrontendAssets;

async fn health() -> &'static str {
    "OK"
}

#[cfg(feature = "embed-frontend")]
fn embedded_asset_response(path: &str) -> Response {
    match FrontendAssets::get(path) {
        Some(asset) => {
            let content_type = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();
            let mut resp = asset.data.into_owned().into_response();
            if let Ok(v) = HeaderValue::from_str(&content_type) {
                resp.headers_mut().insert(header::CONTENT_TYPE, v);
            }
            resp
        }
        None => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    }
}

#[cfg(feature = "embed-frontend")]
async fn serve_index() -> impl IntoResponse {
    embedded_asset_response("index.html")
}

#[cfg(not(feature = "embed-frontend"))]
async fn serve_index() -> &'static str {
    "Photo Analyzer API Server - Frontend not embedded. Build the frontend first or run with --features embed-frontend"
}

#[cfg(feature = "embed-frontend")]
async fn serve_embedded_asset(axum::extract::Path(path): axum::extract::Path<String>) -> impl IntoResponse {
    let req_path = path.trim_start_matches('/');
    let file_path = if req_path.is_empty() { "index.html" } else { req_path };

    if FrontendAssets::get(file_path).is_some() {
        return embedded_asset_response(file_path);
    }

    // SPA routes should fallback to index.html.
    if !file_path.contains('.') {
        return embedded_asset_response("index.html");
    }

    (StatusCode::NOT_FOUND, "Not Found").into_response()
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

    let mut app = Router::new()
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
        .route("/api/fs/browse", get(handlers::browse_fs))
        .route("/api/fs/suggest", get(handlers::suggest_path))
        .route("/api/thumbnails", get(handlers::get_thumbnail))
        .route("/api/analysis", post(handlers::start_analysis))
        .route("/api/analysis/folder", post(handlers::start_folder_analysis))
        .route("/api/analysis/:job_id", get(handlers::get_analysis_job))
        .route("/api/analysis/:job_id/cancel", post(handlers::cancel_analysis_job))
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
        .route("/api/stats", get(handlers::get_stats));

    #[cfg(feature = "embed-frontend")]
    {
        app = app.route("/*path", get(serve_embedded_asset));
    }

    let app = app.layer(cors).with_state(state);

    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "8001".to_string())
        .parse()
        .expect("PORT must be a valid port");

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Starting PhotoAnalyzer server on http://localhost:{}", port);

    if should_open_browser() {
        let url = format!("http://localhost:{}", port);
        tokio::spawn(async move {
            // Delay briefly so browser doesn't race server startup.
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let _ = webbrowser::open(&url);
        });
    }

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
