#![windows_subsystem = "windows"]

mod core;

use std::sync::{Arc, Mutex};
use std::borrow::Cow;
use serde::Deserialize;
use base64::Engine;
use wry::http::{Response, StatusCode};
use tray_icon::{menu::{Menu, MenuItem}, TrayIconBuilder};
use include_dir::{include_dir, Dir};

static FRONTEND_DIST: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/frontend/dist");

struct AppState {
    monitor: Arc<Mutex<core::Monitor>>,
    camera: Arc<Mutex<core::Camera>>,
}

#[derive(Debug, Deserialize)]
struct IpcRequest {
    id: Option<String>,
    method: String,
    params: Option<serde_json::Value>,
}

fn handle_ipc(state: &AppState, req: &IpcRequest) -> Result<serde_json::Value, String> {
    match req.method.as_str() {
        "get_status" => {
            let monitor = state.monitor.lock().map_err(|e| e.to_string())?;
            let snap = monitor.snapshot()?;
            serde_json::to_value(snap).map_err(|e| e.to_string())
        }
        "start_monitoring" => {
            let camera = state.camera.lock().map_err(|e| e.to_string())?;
            let monitor = state.monitor.lock().map_err(|e| e.to_string())?;
            camera.start()?;
            monitor.start()?;
            Ok(serde_json::json!({ "ok": true }))
        }
        "stop_monitoring" => {
            let camera = state.camera.lock().map_err(|e| e.to_string())?;
            let monitor = state.monitor.lock().map_err(|e| e.to_string())?;
            monitor.stop();
            camera.stop();
            Ok(serde_json::json!({ "ok": true }))
        }
        "dismiss_alert" => {
            let monitor = state.monitor.lock().map_err(|e| e.to_string())?;
            monitor.dismiss_alert();
            Ok(serde_json::json!({ "ok": true }))
        }
        "get_config" => {
            let monitor = state.monitor.lock().map_err(|e| e.to_string())?;
            let cfg = monitor.get_config()?;
            serde_json::to_value(cfg).map_err(|e| e.to_string())
        }
        "save_config" => {
            let params = req.params.as_ref().ok_or("missing params")?;
            let config: core::AppConfig = serde_json::from_value(params.clone())
                .map_err(|e| e.to_string())?;
            core::config::save_config(&config)?;
            Ok(serde_json::json!({ "ok": true }))
        }
        "init_detector" => {
            core::PoseDetector::init()?;
            Ok(serde_json::json!({ "ok": true }))
        }
        "detect_frame" => {
            let camera = state.camera.lock().map_err(|e| e.to_string())?;
            if !camera.is_running() {
                camera.start()?;
            }
            let frame = camera.get_frame().ok_or("no frame available")?;
            let result = core::PoseDetector::detect(&frame)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "update_detection" => {
            let params = req.params.as_ref().ok_or("missing params")?;
            let pose: core::detector::PoseOutput = serde_json::from_value(params.clone())
                .map_err(|e| e.to_string())?;
            let monitor = state.monitor.lock().map_err(|e| e.to_string())?;
            monitor.update_detection(pose)?;
            Ok(serde_json::json!({ "ok": true }))
        }
        "get_frame_base64" => {
            let camera = state.camera.lock().map_err(|e| e.to_string())?;
            if !camera.is_running() {
                camera.start()?;
            }
            let frame = camera.get_frame().ok_or("no frame available")?;
                Ok(serde_json::json!({
                "frame": base64::engine::general_purpose::STANDARD.encode(&frame)
            }))
        }
        "enter_compact_mode" | "enter_expanded_mode" | "hide_to_tray" => {
            Ok(serde_json::json!({ "ok": true }))
        }
        _ => Err(format!("unknown method: {}", req.method)),
    }
}

fn asset_protocol_handler(
    request: &wry::http::Request<Vec<u8>>,
) -> Result<Response<Cow<'static, [u8]>>, Box<dyn std::error::Error>> {
    let path = request.uri().path();
    eprintln!("[Protocol] Request path: {}", path);
    let relative_path = if path == "/" || path == "" || path == "/index.html" {
        "index.html"
    } else {
        path.trim_start_matches('/')
    };
    eprintln!("[Protocol] Looking for: {}", relative_path);

    if let Some(file) = FRONTEND_DIST.get_file(relative_path) {
        eprintln!("[Protocol] Found file, size: {}", file.contents().len());
        let mime = if relative_path.ends_with(".js") || relative_path.contains(".js") {
            "application/javascript"
        } else if relative_path.ends_with(".css") || relative_path.contains(".css") {
            "text/css"
        } else if relative_path.ends_with(".html") {
            "text/html"
        } else if relative_path.ends_with(".png") || relative_path.contains(".png") {
            "image/png"
        } else if relative_path.ends_with(".ico") || relative_path.ends_with(".ico") {
            "image/x-icon"
        } else if relative_path.ends_with(".svg") {
            "image/svg+xml"
        } else {
            "application/octet-stream"
        };
        Ok(Response::builder()
            .header("Content-Type", mime)
            .body(file.contents().to_vec().into())?)
    } else {
        eprintln!("[Protocol] File not found: {}", relative_path);
        Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new().into())?)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", "--remote-debugging-port=9222");

    let app_state = Arc::new(AppState {
        monitor: Arc::new(Mutex::new(core::Monitor::new())),
        camera: Arc::new(Mutex::new(core::Camera::new())),
    });

    if let Err(e) = core::PoseDetector::init() {
        eprintln!("[WorkerMonitor] Detector init failed: {}", e);
    }

    let dev_mode = std::env::var("DEV_MODE").unwrap_or_default() == "1";
    let dev_url = std::env::var("DEV_URL").unwrap_or_else(|_| "http://localhost:5175".into());

    println!("[WorkerMonitor] Starting...");
    println!("[WorkerMonitor] DEV_MODE={}, URL={}", dev_mode, dev_url);
    if dev_mode {
        println!("[WorkerMonitor] DEV_MODE: connecting to {}", dev_url);
    }

    eprintln!("[WorkerMonitor] Frontend dist path: {}", std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend/dist").display());

    let event_loop = tao::event_loop::EventLoop::new();
    let window = tao::window::WindowBuilder::new()
        .with_title("WorkerMonitor")
        .with_inner_size(tao::dpi::LogicalSize::new(1024.0, 720.0))
        .with_resizable(true)
        .with_visible(true)
        .build(&event_loop)?;

    let tray_menu = Menu::new();
    let show_item = MenuItem::new("Show", true, None);
    let quit_item = MenuItem::new("Quit", true, None);
    let show_id = show_item.id();
    let quit_id = quit_item.id();
    tray_menu.append_items(&[&show_item, &quit_item]);

    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .build()?;

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let tx_clone = tx.clone();

    let webview = wry::webview::WebViewBuilder::new(window)?
        .with_custom_protocol("workermonitor".into(), move |request| {
            asset_protocol_handler(&request)
                .map_err(|e| {
                    eprintln!("Asset Protocol Error: {:?}", e);
                    wry::Error::DuplicateCustomProtocol("workermonitor".to_string())
                })
        })
        .with_ipc_handler(move |_window, request| {
            let _ = tx_clone.send(request);
        });

    let webview = if dev_mode {
        println!("[WorkerMonitor] Loading dev URL: {}", dev_url);
        webview.with_url(&dev_url)?
    } else {
        println!("[WorkerMonitor] Loading embedded URL: workermonitor://localhost/");
        webview.with_url("workermonitor://localhost/")?
    };

    let webview = webview.build()?;
    eprintln!("[WorkerMonitor] WebView built successfully");
    let app_state_clone = app_state.clone();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = tao::event_loop::ControlFlow::Poll;
        use tao::event::{Event, WindowEvent};

        while let Ok(msg) = rx.try_recv() {
            if msg.starts_with("{") {
                let req: IpcRequest = match serde_json::from_str(&msg) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("[IPC] Failed to parse request: {}", e);
                        continue;
                    }
                };

                let response = handle_ipc(&app_state_clone, &req);

                let response_json = match &response {
                    Ok(result) => serde_json::json!({
                        "id": req.id,
                        "ok": true,
                        "result": result
                    }),
                    Err(err) => serde_json::json!({
                        "id": req.id,
                        "ok": false,
                        "error": err
                    })
                };

                let js = format!("window.ipc && window.ipc.onResponse && window.ipc.onResponse({})", response_json);
                let _ = webview.evaluate_script(&js);
            }
        }

        while let Ok(menu_event) = tray_icon::menu::menu_event_receiver().try_recv() {
            if menu_event.id == show_id {
                let _ = webview.window().set_focus();
            } else if menu_event.id == quit_id {
                *control_flow = tao::event_loop::ControlFlow::Exit;
            }
        }

        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                webview.window().set_visible(false);
            }
            _ => {}
        }
    });
}