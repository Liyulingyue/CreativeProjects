#![allow(unexpected_cfgs)]
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::{
    borrow::Cow,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use tokio::sync::mpsc;
use tray_icon::{
    menu::{menu_event_receiver, Menu, MenuItem, PredefinedMenuItem},
    tray_event_receiver, ClickEvent, TrayIconBuilder,
};
use wry::{
    http::{header::CONTENT_TYPE, Request, Response, StatusCode},
    webview::WebViewBuilder,
};

mod api;
mod platform;

type AppResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Serialize, Deserialize, Clone)]
struct AppConfig {
    api_key: String,
}

#[derive(Serialize, Deserialize)]
struct IpcMessage {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    value: String,
}

enum AppEvent {
    Ready,
    Minimize,
    Close,
    Generated(api::GeneratedImage),
    Saved(String),
    Error(String),
}

fn debug_file_state(path: &Path) -> String {
    match fs::metadata(path) {
        Ok(metadata) => format!(
            "exists size={}B path={}",
            metadata.len(),
            path.to_string_lossy()
        ),
        Err(err) => format!("missing path={} err={err}", path.to_string_lossy()),
    }
}

fn save_generated_image(paths: &platform::AppPaths) -> AppResult<PathBuf> {
    if !paths.current_wallpaper_path.exists() {
        return Err("当前没有可保存的图片".into());
    }

    let export_dir = platform::export_image_dir(&paths.app_data_dir);
    fs::create_dir_all(&export_dir)?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let target_path = export_dir.join(format!("aiwallpaper-{}.png", ts));
    fs::copy(&paths.current_wallpaper_path, &target_path)?;
    log::debug!(
        "[save_image] source={} target={}",
        debug_file_state(&paths.current_wallpaper_path),
        debug_file_state(&target_path)
    );

    Ok(target_path)
}

fn load_embedded_icon() -> AppResult<tray_icon::icon::Icon> {
    let icon_bytes = include_bytes!("../assets/app_icon.png");
    let image = image::load_from_memory(icon_bytes)?.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    tray_icon::icon::Icon::from_rgba(rgba, width, height).map_err(Into::into)
}

fn load_window_icon() -> AppResult<tao::window::Icon> {
    let icon_bytes = include_bytes!("../assets/app_icon.png");
    let image = image::load_from_memory(icon_bytes)?.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    tao::window::Icon::from_rgba(rgba, width, height).map_err(Into::into)
}

fn preview_protocol_response(
    request: &Request<Vec<u8>>,
    image_path: &Path,
    source: &str,
) -> wry::Result<Response<Cow<'static, [u8]>>> {
    let request_path = request.uri().path();
    let request_uri = request.uri().to_string();
    log::debug!(
        "[protocol:{source}] request_uri={} file={}",
        request_uri,
        debug_file_state(image_path)
    );
    if !request_path.contains("current_wallpaper.png")
        && !request_uri.contains("current_wallpaper.png")
    {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Cow::Owned(Vec::new()))
            .map_err(Into::into);
    }

    match fs::read(image_path) {
        Ok(bytes) => {
            log::debug!(
                "[protocol:{source}] served_bytes={} file={}",
                bytes.len(),
                image_path.to_string_lossy()
            );
            Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, "image/png")
                .header("Cache-Control", "no-cache, no-store, must-revalidate")
                .body(Cow::Owned(bytes))
                .map_err(Into::into)
        }
        Err(err) => {
            log::error!(
                "[protocol:{source}] read_failed file={} err={err}",
                image_path.to_string_lossy()
            );
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Cow::Owned(Vec::new()))
                .map_err(Into::into)
        }
    }
}

fn background_init_script(paths: &platform::AppPaths) -> String {
    let mut lines = Vec::new();

    let initial_url = platform::initial_wallpaper_url(&paths.current_wallpaper_path);
    log::debug!(
        "[bg_init] current_wallpaper={} initial_url={initial_url:?}",
        debug_file_state(&paths.current_wallpaper_path)
    );

    if let Some(url) = initial_url {
        lines.push(format!(
            "window.__initialWallpaper = {};",
            serde_json::to_string(&url).unwrap()
        ));
    }

    lines.join("\n")
}

fn control_init_script(initial_key_missing: bool) -> String {
    let mut lines = vec![format!(
        "window.__platformInfo = {};",
        serde_json::to_string(&platform::ui_platform_info()).unwrap()
    )];

    if initial_key_missing {
        lines.push(
            "window.addEventListener('load', function() { setTimeout(showApiModal, 300); });"
                .to_string(),
        );
    }

    lines.join("\n")
}

fn init_logging() {
    let env = env_logger::Env::default().filter_or("RUST_LOG", "debug");
    let _ = env_logger::Builder::from_env(env)
        .format_timestamp_millis()
        .try_init();
}

#[tokio::main]
async fn main() -> AppResult<()> {
    init_logging();

    let paths = platform::app_paths();
    fs::create_dir_all(&paths.app_data_dir)?;
    fs::create_dir_all(&paths.cache_dir)?;
    fs::create_dir_all(&paths.webview_data_dir)?;
    platform::configure_process(&paths)?;
    log::debug!(
        "[startup] app_data_dir={} cache_dir={} current_wallpaper={} webview_data_dir={}",
        paths.app_data_dir.to_string_lossy(),
        paths.cache_dir.to_string_lossy(),
        debug_file_state(&paths.current_wallpaper_path),
        paths.webview_data_dir.to_string_lossy()
    );

    let initial_config = if let Ok(content) = fs::read_to_string(&paths.config_path) {
        serde_json::from_str::<AppConfig>(&content).unwrap_or(AppConfig {
            api_key: String::new(),
        })
    } else {
        dotenvy::dotenv().ok();
        AppConfig {
            api_key: std::env::var("ERNIE_API_KEY").unwrap_or_default(),
        }
    };

    let config = Arc::new(Mutex::new(initial_config));
    let mut event_loop = EventLoop::<AppEvent>::with_user_event();
    platform::configure_event_loop(&mut event_loop);

    let tray_menu = Menu::new();
    let show_item = MenuItem::new("显示控制面板", true, None);
    let quit_item = MenuItem::new("退出", true, None);
    let show_id = show_item.id();
    let quit_id = quit_item.id();
    let _ = tray_menu.append_items(&[&show_item, &PredefinedMenuItem::separator(), &quit_item]);

    let mut tray_builder = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("AIWallpaper");

    if let Ok(icon) = load_embedded_icon() {
        tray_builder = tray_builder.with_icon(icon);
    }

    let _tray_icon = tray_builder.build()?;

    let window_icon = load_window_icon().ok();

    let bg_builder = WindowBuilder::new()
        .with_title("AIWallpaper Layer")
        .with_decorations(false)
        .with_visible(false)
        .with_transparent(true)
        .with_window_icon(window_icon.clone());
    let bg_builder =
        platform::configure_background_window_builder(bg_builder, event_loop.primary_monitor());
    let bg_window = bg_builder.build(&event_loop)?;
    if let Err(err) = platform::attach_background_window(&bg_window) {
        log::error!("壁纸层初始化失败: {err}");
    } else {
        log::debug!("[bg_window] background window attached");
    }

    let bg_preview_image_path = paths.current_wallpaper_path.clone();
    let bg_preview_image_path_for_protocol = bg_preview_image_path.clone();
    let bg_init_script = background_init_script(&paths);
    let bg_webview = WebViewBuilder::new(bg_window)?
        .with_custom_protocol("aiwallpaper".into(), move |request| {
            preview_protocol_response(&request, &bg_preview_image_path_for_protocol, "bg")
        })
        .with_transparent(true)
        .with_html(include_str!("../bg/index.html"))?
        .with_initialization_script(&bg_init_script)
        .with_ipc_handler(move |_window, request| {
            log::debug!("[bg_ipc] {request}");
        })
        .build()?;

    let initial_key_missing = config.lock().unwrap().api_key.is_empty();
    let control_window = WindowBuilder::new()
        .with_title("AIWallpaper 控制中心")
        .with_inner_size(tao::dpi::LogicalSize::new(500.0, 640.0))
        .with_visible(false)
        .with_window_icon(window_icon)
        .build(&event_loop)?;

    let preview_image_path = paths.current_wallpaper_path.clone();
    let (tx, mut rx) = mpsc::channel::<String>(32);
    let control_webview = WebViewBuilder::new(control_window)?
        .with_custom_protocol("aiwallpaper".into(), move |request| {
            preview_protocol_response(&request, &preview_image_path, "ui")
        })
        .with_html(include_str!("../ui/index.html"))?
        .with_initialization_script(&control_init_script(initial_key_missing))
        .with_ipc_handler(move |_window, request| {
            let _ = tx.try_send(request);
        })
        .build()?;

    let proxy = event_loop.create_proxy();
    let config_task = config.clone();
    let paths_task = paths.clone();

    tokio::spawn(async move {
        while let Some(msg_raw) = rx.recv().await {
            if let Ok(msg) = serde_json::from_str::<IpcMessage>(&msg_raw) {
                match msg.msg_type.as_str() {
                    "ready" => {
                        log::debug!("[ipc] ready");
                        let _ = proxy.send_event(AppEvent::Ready);
                    }
                    "minimize" => {
                        log::debug!("[ipc] minimize");
                        let _ = proxy.send_event(AppEvent::Minimize);
                    }
                    "close" => {
                        log::debug!("[ipc] close");
                        let _ = proxy.send_event(AppEvent::Close);
                    }
                    "open_external" => {
                        log::debug!("[ipc] open_external {}", msg.value);
                        if let Err(err) = platform::open_external_url(&msg.value) {
                            log::error!("[open_external] url={} err={err}", msg.value);
                            let _ =
                                proxy.send_event(AppEvent::Error(format!("打开链接失败: {err}")));
                        }
                    }
                    "save_key" => {
                        let mut cfg = config_task.lock().unwrap();
                        cfg.api_key = msg.value.clone();
                        let _ = fs::write(
                            &paths_task.config_path,
                            serde_json::to_string(&*cfg).unwrap_or_else(|_| "{}".to_string()),
                        );
                        log::debug!(
                            "[save_key] config_path={} key_len={}",
                            paths_task.config_path.to_string_lossy(),
                            cfg.api_key.len()
                        );
                    }
                    "save_image" => match save_generated_image(&paths_task) {
                        Ok(path) => {
                            let _ =
                                proxy.send_event(AppEvent::Saved(path.to_string_lossy().into()));
                        }
                        Err(e) => {
                            let _ = proxy.send_event(AppEvent::Error(e.to_string()));
                        }
                    },
                    "generate" => {
                        let current_key = config_task.lock().unwrap().api_key.clone();
                        log::debug!(
                            "[generate] prompt_len={} cache_target={} key_len={}",
                            msg.value.len(),
                            paths_task.current_wallpaper_path.to_string_lossy(),
                            current_key.len()
                        );
                        if current_key.is_empty() {
                            let _ = proxy.send_event(AppEvent::Error("API Key 为空".into()));
                            continue;
                        }

                        match api::generate_image(&msg.value, &current_key, &paths_task.cache_dir)
                            .await
                        {
                            Ok(image) => {
                                let _ = proxy.send_event(AppEvent::Generated(image));
                            }
                            Err(e) => {
                                let _ = proxy.send_event(AppEvent::Error(e.to_string()));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        while let Ok(menu_event) = menu_event_receiver().try_recv() {
            if menu_event.id == show_id {
                platform::reveal_control_window(control_webview.window());
            } else if menu_event.id == quit_id {
                *control_flow = ControlFlow::Exit;
            }
        }

        while let Ok(tray_event) = tray_event_receiver().try_recv() {
            if tray_event.event == ClickEvent::Left {
                platform::reveal_control_window(control_webview.window());
            }
        }

        match event {
            Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if window_id == control_webview.window().id() {
                    platform::hide_control_window(control_webview.window());
                } else if window_id == bg_webview.window().id() {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(app_event) => match app_event {
                AppEvent::Ready => {
                    log::debug!("[event] ready");
                    platform::reveal_control_window(control_webview.window());
                }
                AppEvent::Minimize => {
                    log::debug!("[event] minimize");
                    platform::hide_control_window(control_webview.window());
                }
                AppEvent::Close => {
                    log::debug!("[event] close");
                    *control_flow = ControlFlow::Exit;
                }
                AppEvent::Generated(image) => {
                    log::debug!(
                        "[event][generated] preview_url={} wallpaper_url={} current_file={}",
                        image.preview_url,
                        image.wallpaper_url,
                        debug_file_state(&paths.current_wallpaper_path)
                    );
                    let ui_payload = serde_json::json!({
                        "previewUrl": image.preview_url,
                        "viewUrl": image.wallpaper_url,
                    });
                    let js_ui = format!("window.onGenerationComplete(true, '', {})", ui_payload);
                    match control_webview.evaluate_script(&js_ui) {
                        Ok(_) => log::debug!("[eval][ui] onGenerationComplete ok"),
                        Err(err) => log::error!("[eval][ui] onGenerationComplete err={err}"),
                    }

                    let js_bg = format!(
                        "window.setWallpaper({}, 'Prompt')",
                        serde_json::to_string(&image.preview_url).unwrap()
                    );
                    match bg_webview.evaluate_script(&js_bg) {
                        Ok(_) => log::debug!("[eval][bg] setWallpaper ok"),
                        Err(err) => log::error!("[eval][bg] setWallpaper err={err}"),
                    }
                }
                AppEvent::Saved(path) => {
                    log::debug!("[event][saved] {path}");
                    let js_ui = format!(
                        "window.onImageSaved({})",
                        serde_json::to_string(&path).unwrap()
                    );
                    match control_webview.evaluate_script(&js_ui) {
                        Ok(_) => log::debug!("[eval][ui] onImageSaved ok"),
                        Err(err) => log::error!("[eval][ui] onImageSaved err={err}"),
                    }
                }
                AppEvent::Error(err_msg) => {
                    log::error!("[event][error] {err_msg}");
                    let js_ui = format!(
                        "window.onGenerationComplete(false, {}, null)",
                        serde_json::to_string(&err_msg).unwrap()
                    );
                    match control_webview.evaluate_script(&js_ui) {
                        Ok(_) => log::debug!("[eval][ui] onGenerationComplete(error) ok"),
                        Err(err) => log::error!("[eval][ui] onGenerationComplete(error) err={err}"),
                    }
                }
            },
            _ => {}
        }
    });
}
