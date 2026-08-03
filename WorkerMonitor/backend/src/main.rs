mod config;
mod monitor;
mod rtmpose;

use std::sync::Arc;
use actix_cors::Cors;
use actix_web::{web, App, HttpResponse, HttpServer};

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
        Err(_) => HttpResponse::Ok().json(serde_json::json!({ "ok": false })),
    }
}

async fn dismiss_break_alert(data: web::Data<std::sync::Arc<MonitorState>>) -> HttpResponse {
    data.get_ref().dismiss_alert();
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

#[derive(serde::Deserialize)]
struct FramePayload {
    frame: String,
}

async fn post_frame(
    payload: web::Json<FramePayload>,
    data: web::Data<std::sync::Arc<MonitorState>>,
) -> HttpResponse {
    match rtmpose::detect_pose_from_base64(&payload.frame) {
        Ok(pose) => {
            if !pose.person_detected {
                let _ = data.get_ref().update_detection(pose);
                return HttpResponse::Ok().json(serde_json::json!({ "ok": true, "person_detected": false }));
            }
            match data.get_ref().update_detection(pose) {
                Ok(()) => HttpResponse::Ok().json(serde_json::json!({ "ok": true })),
                Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e})),
            }
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e})),
    }
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
            .route("/api/frame", web::post().to(post_frame))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
