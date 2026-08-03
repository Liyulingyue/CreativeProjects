mod config;
mod monitor;
mod rtmpose;
mod camera;

use std::sync::Arc;
use std::time::Duration;

use std::sync::Arc;
use actix_cors::Cors;
use actix_web::{web, App, HttpResponse, HttpServer};

use crate::camera::CameraCapture;
use crate::monitor::MonitorState;

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({ "status": "ok" }))
}

async fn get_status(data: web::Data<std::sync::Arc<MonitorState>>) -> HttpResponse {
    match data.get_ref().snapshot() {
        Ok(snap) => HttpResponse::Ok().json(snap),
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({"error": "failed"})),
    }
}

async fn start_monitoring(data: web::Data<std::sync::Arc<MonitorState>>) -> HttpResponse {
    let _ = data.get_ref().start();
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

async fn stop_monitoring(data: web::Data<std::sync::Arc<MonitorState>>) -> HttpResponse {
    data.get_ref().stop();
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

async fn get_config(data: web::Data<std::sync::Arc<MonitorState>>) -> HttpResponse {
    match data.get_ref().get_config() {
        Ok(cfg) => HttpResponse::Ok().json(cfg),
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({"error": "failed"})),
    }
}

async fn save_config(
    cfg: web::Json<config::AppConfig>,
    data: web::Data<std::sync::Arc<MonitorState>>,
) -> HttpResponse {
    match data.get_ref().save_config(cfg.into_inner()) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({ "ok": true })),
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({ "ok": false })),
    }
}

async fn dismiss_break_alert(data: web::Data<std::sync::Arc<MonitorState>>) -> HttpResponse {
    data.get_ref().dismiss_alert();
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

fn start_detection_loop(
    monitor: Arc<MonitorState>,
    camera: Arc<CameraCapture>,
) {
    std::thread::spawn(move || {
        loop {
            if let Some(frame_bytes) = camera.latest_frame() {
                if let Ok(pose) = rtmpose::detect_pose_from_bytes(&frame_bytes) {
                    if let Err(e) = monitor.update_detection(pose) {
                        eprintln!("[Detection] update error: {}", e);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let model_path = std::env::var("RTMPOSE_MODEL")
        .unwrap_or_else(|_| "resource/end2end.onnx".to_string());

    if let Err(e) = rtmpose::init_model(&model_path) {
        eprintln!("Failed to initialize RTMPose model: {}", e);
        std::process::exit(1);
    }

    let monitor_state = Arc::new(MonitorState::new());

    let camera = match CameraCapture::new() {
        Ok(cam) => {
            eprintln!("[Camera] initialized");
            Arc::new(cam)
        }
        Err(e) => {
            eprintln!("[Camera] failed to init: {}", e);
            std::sync::Arc::new(CameraCapture::new().expect("fallback"))
        }
    };

    camera.start();
    start_detection_loop(monitor_state.clone(), camera.clone());

    eprintln!("WorkerMonitor backend running on http://127.0.0.1:8080");

    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(web::Data::new(monitor_state.clone()))
            .route("/health", web::get().to(health))
            .route("/api/status", web::get().to(get_status))
            .route("/api/monitoring/start", web::post().to(start_monitoring))
            .route("/api/monitoring/stop", web::post().to(stop_monitoring))
            .route("/api/config", web::get().to(get_config))
            .route("/api/config", web::post().to(save_config))
            .route("/api/alert/dismiss", web::post().to(dismiss_break_alert))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
