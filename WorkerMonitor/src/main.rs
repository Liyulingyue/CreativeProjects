#![windows_subsystem = "windows"]

mod core;

use std::sync::{Arc, Mutex};
use std::borrow::Cow;
use std::io::{Read, Write};
use serde::Deserialize;
use base64::Engine;
use tao::platform::windows::WindowExtWindows;
use wry::http::{Response, StatusCode};
use tray_icon::{menu::{Menu, MenuItem}, TrayIconBuilder};
use include_dir::{include_dir, Dir};
use once_cell::sync::OnceCell;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

static FRONTEND_DIST: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/frontend/dist");
static LOG_GUARD: OnceCell<WorkerGuard> = OnceCell::new();
static LOCAL_REQ_ID_SEQ: AtomicU64 = AtomicU64::new(1);

struct RuntimeStats {
    ipc_fast_total: AtomicU64,
    ipc_fast_fail: AtomicU64,
    ipc_fast_latency_ms_sum: AtomicU64,
    ipc_heavy_total: AtomicU64,
    ipc_heavy_fail: AtomicU64,
    ipc_heavy_latency_ms_sum: AtomicU64,
    detect_total: AtomicU64,
    detect_fail: AtomicU64,
    detect_latency_ms_sum: AtomicU64,
    frame_fetch_total: AtomicU64,
    frame_fetch_miss: AtomicU64,
}

impl RuntimeStats {
    fn new() -> Self {
        Self {
            ipc_fast_total: AtomicU64::new(0),
            ipc_fast_fail: AtomicU64::new(0),
            ipc_fast_latency_ms_sum: AtomicU64::new(0),
            ipc_heavy_total: AtomicU64::new(0),
            ipc_heavy_fail: AtomicU64::new(0),
            ipc_heavy_latency_ms_sum: AtomicU64::new(0),
            detect_total: AtomicU64::new(0),
            detect_fail: AtomicU64::new(0),
            detect_latency_ms_sum: AtomicU64::new(0),
            frame_fetch_total: AtomicU64::new(0),
            frame_fetch_miss: AtomicU64::new(0),
        }
    }

    fn record_ipc(&self, queue: IpcQueue, elapsed_ms: u64, ok: bool) {
        match queue {
            IpcQueue::Fast => {
                self.ipc_fast_total.fetch_add(1, Ordering::Relaxed);
                self.ipc_fast_latency_ms_sum.fetch_add(elapsed_ms, Ordering::Relaxed);
                if !ok {
                    self.ipc_fast_fail.fetch_add(1, Ordering::Relaxed);
                }
            }
            IpcQueue::Heavy => {
                self.ipc_heavy_total.fetch_add(1, Ordering::Relaxed);
                self.ipc_heavy_latency_ms_sum.fetch_add(elapsed_ms, Ordering::Relaxed);
                if !ok {
                    self.ipc_heavy_fail.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    fn record_detect(&self, elapsed_ms: u64, ok: bool) {
        self.detect_total.fetch_add(1, Ordering::Relaxed);
        self.detect_latency_ms_sum.fetch_add(elapsed_ms, Ordering::Relaxed);
        if !ok {
            self.detect_fail.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn record_frame_fetch(&self, hit: bool) {
        self.frame_fetch_total.fetch_add(1, Ordering::Relaxed);
        if !hit {
            self.frame_fetch_miss.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[derive(Clone, Copy)]
enum IpcQueue {
    Fast,
    Heavy,
}

fn avg_ms(sum: u64, count: u64) -> u64 {
    if count == 0 { 0 } else { sum / count }
}

fn init_logging() -> bool {
    let enabled = matches!(
        std::env::var("WORKER_MONITOR_ENABLE_LOG")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    );

    if !enabled {
        return false;
    }

    let log_root = dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("WorkerMonitor")
        .join("logs");

    if let Err(err) = std::fs::create_dir_all(&log_root) {
        eprintln!("[WorkerMonitor] Failed to create log dir {}: {}", log_root.display(), err);
        return false;
    }

    let file_appender = tracing_appender::rolling::daily(&log_root, "worker-monitor.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let _ = LOG_GUARD.set(guard);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if let Err(err) = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(non_blocking)
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .try_init()
    {
        eprintln!("[WorkerMonitor] Failed to init logging subscriber: {}", err);
        return false;
    }

    info!("logging initialized at {}", log_root.display());
    true
}

struct AppState {
    monitor: Arc<Mutex<core::Monitor>>,
    camera: Arc<Mutex<core::Camera>>,
}

const CAMERA_STREAM_ADDR: &str = "127.0.0.1:18181";

fn spawn_camera_stream_server(frame_reader: core::camera::FrameReader) {
    thread::spawn(move || {
        let listener = match TcpListener::bind(CAMERA_STREAM_ADDR) {
            Ok(listener) => listener,
            Err(err) => {
                eprintln!("[CameraStream] Failed to bind {}: {}", CAMERA_STREAM_ADDR, err);
                return;
            }
        };

        info!("camera stream server listening on {}", CAMERA_STREAM_ADDR);

        for incoming in listener.incoming() {
            match incoming {
                Ok(stream) => {
                    let frame_reader = frame_reader.clone();
                    thread::spawn(move || handle_camera_stream_connection(stream, frame_reader));
                }
                Err(err) => {
                    warn!("[CameraStream] incoming connection error: {}", err);
                }
            }
        }
    });
}

fn handle_camera_stream_connection(mut stream: TcpStream, frame_reader: core::camera::FrameReader) {
    let mut request_buf = [0_u8; 1024];
    let read_len = match stream.read(&mut request_buf) {
        Ok(len) => len,
        Err(err) => {
            warn!("[CameraStream] failed to read request: {}", err);
            return;
        }
    };

    let request_text = String::from_utf8_lossy(&request_buf[..read_len]);
    let request_line = request_text.lines().next().unwrap_or_default();
    let raw_path = request_line.split_whitespace().nth(1).unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or("/");

    if path != "/camera.mjpg" && path != "/" {
        let _ = write!(
            stream,
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        return;
    }

    let header = concat!(
        "HTTP/1.1 200 OK\r\n",
        "Connection: close\r\n",
        "Cache-Control: no-cache, no-store, must-revalidate\r\n",
        "Pragma: no-cache\r\n",
        "Expires: 0\r\n",
        "Content-Type: multipart/x-mixed-replace; boundary=frame\r\n\r\n"
    );

    if stream.write_all(header.as_bytes()).is_err() {
        return;
    }

    let _ = stream.flush();

    loop {
        let frame = frame_reader.get_frame();

        let Some(frame) = frame else {
            thread::sleep(Duration::from_millis(20));
            continue;
        };

        let part_header = format!(
            "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
            frame.len()
        );

        if stream.write_all(part_header.as_bytes()).is_err() {
            break;
        }
        if stream.write_all(&frame).is_err() {
            break;
        }
        if stream.write_all(b"\r\n").is_err() {
            break;
        }
        if stream.flush().is_err() {
            break;
        }

        thread::sleep(Duration::from_millis(16));
    }
}

#[derive(Debug, Deserialize, Clone)]
struct IpcRequest {
    id: Option<String>,
    method: String,
    params: Option<serde_json::Value>,
}

fn handle_ipc(state: &AppState, req: &IpcRequest, stats: &RuntimeStats) -> Result<serde_json::Value, String> {
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
            let frame = match camera.get_frame() {
                Some(data) => {
                    stats.record_frame_fetch(true);
                    data
                }
                None => {
                    stats.record_frame_fetch(false);
                    return Err("no frame available".to_string());
                }
            };
                Ok(serde_json::json!({
                "frame": base64::engine::general_purpose::STANDARD.encode(&frame)
            }))
        }
        "enter_compact_mode" | "enter_expanded_mode" | "hide_to_tray" | "start_window_drag" | "quit_app" => {
            Ok(serde_json::json!({ "ok": true }))
        }
        _ => Err(format!("unknown method: {}", req.method)),
    }
}

fn is_heavy_ipc_method(method: &str) -> bool {
    matches!(method, "init_detector" | "detect_frame")
}

fn request_id_for_log(req: &IpcRequest) -> String {
    req.id
        .clone()
        .unwrap_or_else(|| format!("local-{}", LOCAL_REQ_ID_SEQ.fetch_add(1, Ordering::Relaxed)))
}

fn spawn_background_detection_worker(app_state: Arc<AppState>, stats: Arc<RuntimeStats>) {
    thread::spawn(move || {
        loop {
            let (is_monitoring, check_interval_secs) = {
                let monitor = match app_state.monitor.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        warn!("[DetectLoop] monitor lock poisoned, recovering");
                        poisoned.into_inner()
                    }
                };

                let snapshot = match monitor.snapshot() {
                    Ok(snap) => snap,
                    Err(err) => {
                        warn!("[DetectLoop] snapshot error: {}", err);
                        thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                };

                let cfg = monitor.get_config().ok();
                let interval = cfg.map(|c| c.check_interval_seconds.max(1)).unwrap_or(2);
                (snapshot.is_monitoring, interval)
            };

            if !is_monitoring {
                thread::sleep(Duration::from_millis(250));
                continue;
            }

            let frame = {
                let camera = match app_state.camera.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        warn!("[DetectLoop] camera lock poisoned, recovering");
                        poisoned.into_inner()
                    }
                };

                if !camera.is_running() {
                    if let Err(err) = camera.start() {
                        warn!("[DetectLoop] camera start failed: {}", err);
                        thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                }

                camera.get_frame()
            };

            if let Some(frame) = frame {
                let t0 = Instant::now();
                let mut ok = true;
                match core::PoseDetector::detect(&frame) {
                    Ok(pose) => {
                        let monitor = match app_state.monitor.lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => {
                                warn!("[DetectLoop] monitor lock poisoned on update, recovering");
                                poisoned.into_inner()
                            }
                        };
                        if let Err(err) = monitor.update_detection(pose) {
                            ok = false;
                            warn!("[DetectLoop] update_detection failed: {}", err);
                        }
                    }
                    Err(err) => {
                        ok = false;
                        warn!("[DetectLoop] detect failed: {}", err);
                    }
                }
                let elapsed_ms = t0.elapsed().as_millis() as u64;
                stats.record_detect(elapsed_ms, ok);
            }

            thread::sleep(Duration::from_secs(u64::from(check_interval_secs)));
        }
    });
}

fn spawn_metrics_reporter(stats: Arc<RuntimeStats>) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(60));

        let ipc_fast_total = stats.ipc_fast_total.swap(0, Ordering::Relaxed);
        let ipc_fast_fail = stats.ipc_fast_fail.swap(0, Ordering::Relaxed);
        let ipc_fast_latency_sum = stats.ipc_fast_latency_ms_sum.swap(0, Ordering::Relaxed);

        let ipc_heavy_total = stats.ipc_heavy_total.swap(0, Ordering::Relaxed);
        let ipc_heavy_fail = stats.ipc_heavy_fail.swap(0, Ordering::Relaxed);
        let ipc_heavy_latency_sum = stats.ipc_heavy_latency_ms_sum.swap(0, Ordering::Relaxed);

        let detect_total = stats.detect_total.swap(0, Ordering::Relaxed);
        let detect_fail = stats.detect_fail.swap(0, Ordering::Relaxed);
        let detect_latency_sum = stats.detect_latency_ms_sum.swap(0, Ordering::Relaxed);

        let frame_fetch_total = stats.frame_fetch_total.swap(0, Ordering::Relaxed);
        let frame_fetch_miss = stats.frame_fetch_miss.swap(0, Ordering::Relaxed);

        info!(
            "[Metrics][1m] ipc_fast={{total:{},fail:{},avg_ms:{}}} ipc_heavy={{total:{},fail:{},avg_ms:{}}} detect={{total:{},fail:{},avg_ms:{}}} frame_fetch={{total:{},miss:{}}}",
            ipc_fast_total,
            ipc_fast_fail,
            avg_ms(ipc_fast_latency_sum, ipc_fast_total),
            ipc_heavy_total,
            ipc_heavy_fail,
            avg_ms(ipc_heavy_latency_sum, ipc_heavy_total),
            detect_total,
            detect_fail,
            avg_ms(detect_latency_sum, detect_total),
            frame_fetch_total,
            frame_fetch_miss
        );
    });
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

fn load_tray_icon() -> Result<tray_icon::icon::Icon, Box<dyn std::error::Error>> {
    let icon_bytes = include_bytes!("../icons/icon.png");
    let image = image::load_from_memory(icon_bytes)?.into_rgba8();
    let (width, height) = image.dimensions();
    tray_icon::icon::Icon::from_rgba(image.into_raw(), width, height).map_err(|e| e.into())
}

fn load_window_icon() -> Result<tao::window::Icon, Box<dyn std::error::Error>> {
    let icon_bytes = include_bytes!("../icons/icon.png");
    let image = image::load_from_memory(icon_bytes)?.into_rgba8();
    let (width, height) = image.dimensions();
    tao::window::Icon::from_rgba(image.into_raw(), width, height).map_err(|e| e.into())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let logging_enabled = init_logging();
    std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", "--remote-debugging-port=9222");

    let stats = Arc::new(RuntimeStats::new());
    if logging_enabled {
        spawn_metrics_reporter(stats.clone());
    }

    let app_state = Arc::new(AppState {
        monitor: Arc::new(Mutex::new(core::Monitor::new())),
        camera: Arc::new(Mutex::new(core::Camera::new())),
    });

    spawn_background_detection_worker(app_state.clone(), stats.clone());
    let frame_reader = {
        let camera = match app_state.camera.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        camera.frame_reader()
    };
    spawn_camera_stream_server(frame_reader);

    if let Err(e) = core::PoseDetector::init() {
        warn!("[WorkerMonitor] Detector init failed: {}", e);
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
    let window_icon = load_window_icon().ok();
    let window = tao::window::WindowBuilder::new()
        .with_title("WorkerMonitor")
        .with_inner_size(tao::dpi::LogicalSize::new(1024.0, 720.0))
        .with_window_icon(window_icon)
        .with_decorations(false)
        .with_resizable(true)
        .with_visible(true)
        .build(&event_loop)?;

    let tray_menu = Menu::new();
    let show_item = MenuItem::new("Show", true, None);
    let quit_item = MenuItem::new("Quit", true, None);
    let show_id = show_item.id();
    let quit_id = quit_item.id();
    tray_menu.append_items(&[&show_item, &quit_item]);

    let mut tray_builder = TrayIconBuilder::new().with_menu(Box::new(tray_menu));
    if let Ok(icon) = load_tray_icon() {
        tray_builder = tray_builder.with_icon(icon);
    }
    let _tray = tray_builder.build()?;

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let tx_clone = tx.clone();

    let (ipc_req_tx_fast, ipc_req_rx_fast) = std::sync::mpsc::channel::<IpcRequest>();
    let (ipc_req_tx_heavy, ipc_req_rx_heavy) = std::sync::mpsc::channel::<IpcRequest>();
    let (ipc_res_tx, ipc_res_rx) = std::sync::mpsc::channel::<(Option<String>, Result<serde_json::Value, String>)>();
    let app_state_fast = app_state.clone();
    let app_state_heavy = app_state.clone();
    let stats_fast = stats.clone();
    let stats_heavy = stats.clone();
    let ipc_res_tx_fast = ipc_res_tx.clone();
    let ipc_res_tx_heavy = ipc_res_tx.clone();

    thread::spawn(move || {
        while let Ok(req) = ipc_req_rx_fast.recv() {
            let request_id = request_id_for_log(&req);
            let method = req.method.clone();
            let req_id = req.id.clone();
            let t0 = Instant::now();
            let result = handle_ipc(app_state_fast.as_ref(), &req, stats_fast.as_ref());
            let elapsed_ms = t0.elapsed().as_millis() as u64;
            stats_fast.record_ipc(IpcQueue::Fast, elapsed_ms, result.is_ok());
            info!(
                event = "ipc_request",
                request_id = %request_id,
                method = %method,
                queue = "fast",
                elapsed_ms,
                ok = result.is_ok(),
                "IPC handled"
            );
            if ipc_res_tx_fast.send((req_id, result)).is_err() {
                break;
            }
        }
    });

    thread::spawn(move || {
        while let Ok(req) = ipc_req_rx_heavy.recv() {
            let request_id = request_id_for_log(&req);
            let method = req.method.clone();
            let req_id = req.id.clone();
            let t0 = Instant::now();
            let result = handle_ipc(app_state_heavy.as_ref(), &req, stats_heavy.as_ref());
            let elapsed_ms = t0.elapsed().as_millis() as u64;
            stats_heavy.record_ipc(IpcQueue::Heavy, elapsed_ms, result.is_ok());
            info!(
                event = "ipc_request",
                request_id = %request_id,
                method = %method,
                queue = "heavy",
                elapsed_ms,
                ok = result.is_ok(),
                "IPC handled"
            );
            if ipc_res_tx_heavy.send((req_id, result)).is_err() {
                break;
            }
        }
    });

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

                let mut handled_in_ui = true;
                let ui_result: Result<serde_json::Value, String> = match req.method.as_str() {
                    "enter_compact_mode" => {
                        let win = webview.window();
                        let compact_size = tao::dpi::LogicalSize::new(220.0, 148.0);
                        let _ = win.set_decorations(false);
                        let _ = win.set_always_on_top(true);
                        let _ = win.set_skip_taskbar(true);
                        let _ = win.set_resizable(false);
                        win.set_min_inner_size(Some(compact_size));
                        win.set_max_inner_size(Some(compact_size));
                        win.set_inner_size(compact_size);
                        win.set_visible(true);
                        let _ = win.set_focus();
                        Ok(serde_json::json!({ "ok": true }))
                    }
                    "enter_expanded_mode" => {
                        let win = webview.window();
                        let expanded_size = tao::dpi::LogicalSize::new(1024.0, 720.0);
                        let _ = win.set_decorations(false);
                        let _ = win.set_always_on_top(false);
                        let _ = win.set_skip_taskbar(false);
                        let _ = win.set_resizable(true);
                        win.set_min_inner_size(None::<tao::dpi::LogicalSize<f64>>);
                        win.set_max_inner_size(None::<tao::dpi::LogicalSize<f64>>);
                        win.set_inner_size(expanded_size);
                        win.set_visible(true);
                        let _ = win.set_focus();
                        Ok(serde_json::json!({ "ok": true }))
                    }
                    "hide_to_tray" => {
                        webview.window().set_visible(false);
                        Ok(serde_json::json!({ "ok": true }))
                    }
                    "start_window_drag" => {
                        match webview.window().drag_window() {
                            Ok(_) => Ok(serde_json::json!({ "ok": true })),
                            Err(err) => Err(err.to_string()),
                        }
                    }
                    "quit_app" => {
                        *control_flow = tao::event_loop::ControlFlow::Exit;
                        Ok(serde_json::json!({ "ok": true }))
                    }
                    _ => {
                        handled_in_ui = false;
                        Ok(serde_json::Value::Null)
                    }
                };

                if handled_in_ui {
                    let response_json = match ui_result {
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

                    let js = format!(
                        "(window.__workerMonitorOnResponse && window.__workerMonitorOnResponse({0})) || (window.ipc && window.ipc.onResponse && window.ipc.onResponse({0}))",
                        response_json
                    );
                    let _ = webview.evaluate_script(&js);
                    continue;
                }

                let method = req.method.clone();
                let send_result = if is_heavy_ipc_method(&method) {
                    ipc_req_tx_heavy.send(req)
                } else {
                    ipc_req_tx_fast.send(req)
                };

                if send_result.is_err() {
                    error!("[IPC] Worker thread not available for method: {}", method);
                }
            }
        }

        while let Ok((req_id, response)) = ipc_res_rx.try_recv() {
            let response_json = match &response {
                Ok(result) => serde_json::json!({
                    "id": req_id,
                    "ok": true,
                    "result": result
                }),
                Err(err) => serde_json::json!({
                    "id": req_id,
                    "ok": false,
                    "error": err
                })
            };

            let js = format!(
                "(window.__workerMonitorOnResponse && window.__workerMonitorOnResponse({0})) || (window.ipc && window.ipc.onResponse && window.ipc.onResponse({0}))",
                response_json
            );
            let _ = webview.evaluate_script(&js);
        }

        while let Ok(menu_event) = tray_icon::menu::menu_event_receiver().try_recv() {
            if menu_event.id == show_id {
                webview.window().set_visible(true);
                let _ = webview.window().set_minimized(false);
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