#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod models;
mod services;
mod handlers;
mod paths;

use axum::{
    body::{to_bytes, Body},
    http::Request,
    routing::{delete, get, post},
    Router,
};
use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[cfg(feature = "embed-frontend")]
use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
#[cfg(feature = "embed-frontend")]
use rust_embed::RustEmbed;

use services::AppState;

#[derive(Parser)]
#[command(name = "photoanalyzer", about = "Photo Analyzer - AI-powered photo analysis tool")]
pub struct CliArgs {
    #[arg(long, default_value = "8001", help = "Port to listen on")]
    pub port: u16,

    #[arg(long, default_value = "0.0.0.0", help = "Host address to bind")]
    pub host: String,

    #[arg(long, help = "Do not auto-open browser")]
    pub no_open: bool,

    #[arg(long, help = "Frontend static files directory (overrides embedded frontend)")]
    pub frontend_dir: Option<String>,
}

impl CliArgs {
    pub fn parse() -> Self {
        Parser::parse()
    }
}

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

    if !file_path.contains('.') {
        return embedded_asset_response("index.html");
    }

    (StatusCode::NOT_FOUND, "Not Found").into_response()
}

pub fn build_app() -> Router {
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .try_init();

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
    let app = app.route("/*path", get(serve_embedded_asset));

    app.layer(cors).with_state(state)
}

pub async fn run_server(host: &str, port: u16, open_browser: bool) -> Result<(), String> {
    let app = build_app();
    let addr: SocketAddr = format!("{}:{}", host, port)
        .parse()
        .map_err(|e| format!("无效地址 {host}:{port}: {e}"))?;
    tracing::info!("Starting PhotoAnalyzer server on http://{}:{}", host, port);

    if open_browser {
        let url = format!("http://localhost:{}", port);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let _ = webbrowser::open(&url);
        });
    }

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("绑定端口失败: {e}"))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("服务运行失败: {e}"))
}

pub async fn run_server_from_env() -> Result<(), String> {
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "8001".to_string())
        .parse()
        .map_err(|_| "PORT must be a valid port".to_string())?;

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

    run_server(&host, port, should_open_browser()).await
}

pub struct InProcessApi {
    runtime: tokio::runtime::Runtime,
    app: Router,
}

impl InProcessApi {
    pub fn new() -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("创建运行时失败: {e}"))?;

        let app = build_app();
        Ok(Self { runtime, app })
    }

    pub fn request(
        &self,
        method: &str,
        uri: &str,
        body: Option<Vec<u8>>,
        content_type: Option<&str>,
    ) -> Result<(u16, Vec<u8>, Option<String>), String> {
        self.runtime.block_on(async {
            let mut req_builder = Request::builder().method(method).uri(uri);
            if let Some(ct) = content_type {
                req_builder = req_builder.header("content-type", ct);
            }
            let req = req_builder
                .body(Body::from(body.unwrap_or_default()))
                .map_err(|e| format!("构造请求失败: {e}"))?;

            let response = self
                .app
                .clone()
                .oneshot(req)
                .await
                .map_err(|e| format!("处理请求失败: {e}"))?;

            let status = response.status().as_u16();
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.to_string());
            let bytes = to_bytes(response.into_body(), usize::MAX)
                .await
                .map_err(|e| format!("读取响应失败: {e}"))?;

            Ok((status, bytes.to_vec(), content_type))
        })
    }
}

pub fn spawn_server(preferred_port: Option<u16>, open_browser: bool) -> std::sync::mpsc::Receiver<Result<u16, String>> {
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = ready_tx.send(Err(format!("创建运行时失败: {error}")));
                return;
            }
        };

        runtime.block_on(async move {
            let app = build_app();
            let bind_target = match preferred_port {
                Some(port) => format!("127.0.0.1:{port}"),
                None => "127.0.0.1:0".to_string(),
            };
            let listener = match tokio::net::TcpListener::bind(&bind_target).await {
                Ok(listener) => listener,
                Err(error) => {
                    let _ = ready_tx.send(Err(format!("绑定端口失败: {error}")));
                    return;
                }
            };
            let port = match listener.local_addr() {
                Ok(addr) => addr.port(),
                Err(error) => {
                    let _ = ready_tx.send(Err(format!("获取监听端口失败: {error}")));
                    return;
                }
            };
            tracing::info!("Starting PhotoAnalyzer server on http://127.0.0.1:{}", port);

            if open_browser {
                let url = format!("http://127.0.0.1:{}", port);
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let _ = webbrowser::open(&url);
                });
            }

            let _ = ready_tx.send(Ok(port));

            if let Err(error) = axum::serve(listener, app).await {
                eprintln!("[photo_analyzer] server error: {error}");
            }
        });
    });

    ready_rx
}