mod camera;
mod config;
mod monitor;
mod rtmpose;

use std::sync::Arc;
use actix_cors::Cors;
use actix_web::{web, App, HttpResponse, HttpServer};

use crate::camera::CameraState;
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

async fn start_monitoring(
    data: web::Data<std::sync::Arc<MonitorState>>,
    camera: web::Data<Arc<CameraState>>,
) -> HttpResponse {
    if let Err(e) = camera.start() {
        return HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("camera start failed: {}", e)}));
    }
    let _ = data.get_ref().start();
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

async fn stop_monitoring(
    data: web::Data<std::sync::Arc<MonitorState>>,
    camera: web::Data<Arc<CameraState>>,
) -> HttpResponse {
    data.get_ref().stop();
    camera.stop();
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

async fn video_stream(camera: web::Data<Arc<CameraState>>) -> HttpResponse {
    if !camera.is_running() {
        return HttpResponse::ServiceUnavailable().body("Camera not started");
    }

    let camera = camera.get_ref().clone();

    let stream = futures_util::stream::unfold((), move |_| {
        let cam = camera.clone();
        async move {
            tokio::time::sleep(std::time::Duration::from_millis(33)).await;
            let frame = match cam.get_frame() {
                Some(f) => f,
                None => return None,
            };
            let header = format!(
                "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                frame.len()
            );
            let mut data = header.into_bytes();
            data.extend(frame);
            data.extend(b"\r\n");
            Some((Ok::<_, actix_web::Error>(bytes::Bytes::from(data)), ()))
        }
    });

    HttpResponse::Ok()
        .content_type("multipart/x-mixed-replace; boundary=frame")
        .streaming(stream)
}

async fn post_frame(
    data: web::Data<std::sync::Arc<MonitorState>>,
    camera: web::Data<Arc<CameraState>>,
) -> HttpResponse {
    if !camera.is_running() {
        if let Err(e) = camera.start() {
            return HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("camera start failed: {}", e)}));
        }
    }

    let frame = match camera.get_frame() {
        Some(f) => f,
        None => {
            return HttpResponse::Ok().json(serde_json::json!({ "ok": true, "person_detected": false }));
        }
    };

    let base64_frame = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &frame);
    let data_url = format!("data:image/jpeg;base64,{}", base64_frame);

    match rtmpose::detect_pose_from_base64(&data_url) {
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
    eprintln!("Initializing models (downloading if needed)...");
    if let Err(e) = rtmpose::init_model("") {
        eprintln!("Failed to initialize models: {}", e);
        std::process::exit(1);
    }
    eprintln!("Models initialized successfully!");

    let monitor_state = Arc::new(MonitorState::new());
    let camera_state = Arc::new(CameraState::new());

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
            .app_data(web::Data::new(camera_state.clone()))
            .route("/health", web::get().to(health))
            .route("/stream", web::get().to(video_stream))
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
