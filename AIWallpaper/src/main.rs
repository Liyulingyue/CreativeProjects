#![windows_subsystem = "windows"]

use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
    platform::windows::WindowExtWindows,
};
use wry::{
    http::{header::CONTENT_TYPE, Response, StatusCode},
    webview::WebViewBuilder,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SetParent, SetWindowPos, GetWindowLongPtrA, SetWindowLongPtrA,
    GWL_EXSTYLE, WS_EX_TOOLWINDOW, WS_EX_NOACTIVATE,
    HWND_BOTTOM, SWP_NOMOVE, SWP_NOSIZE, SWP_NOACTIVATE, SWP_SHOWWINDOW,
    SystemParametersInfoW, SPI_GETDESKWALLPAPER,
};
use tokio::sync::mpsc;
use tray_icon::{
    menu::{Menu, MenuItem, PredefinedMenuItem, menu_event_receiver},
    TrayIconBuilder, tray_event_receiver, ClickEvent,
};
use std::borrow::Cow;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use include_dir::{include_dir, Dir};

static PRO_DIST: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/ui-pro/dist");

mod api;
mod platform;
mod wallpaper_engine;

#[derive(Serialize, Deserialize, Clone)]
struct AppConfig {
    api_key: String,
    #[serde(default = "default_enable_cache")]
    enable_cache: bool,
    #[serde(default = "default_cache_limit")]
    cache_limit: u32,
    #[serde(default = "default_auto_refresh_hours")]
    auto_refresh_hours: u32,
    #[serde(default = "default_auto_prompt")]
    auto_prompt: String,
    #[serde(default = "default_gallery_path")]
    gallery_path: String,
}

fn default_enable_cache() -> bool { false }
fn default_cache_limit() -> u32 { 100 }
fn default_auto_refresh_hours() -> u32 { 24 }
fn default_auto_prompt() -> String { "Cyberpunk city, neon lights, 8k resolution".to_string() }
fn default_gallery_path() -> String { String::new() }

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
    SwitchMode(String),
    GalleryLoaded(Vec<serde_json::Value>),
}

fn export_image_dir(app_data_dir: &std::path::Path, config: &AppConfig) -> PathBuf {
    if !config.gallery_path.is_empty() {
        let custom_path = PathBuf::from(&config.gallery_path);
        // 如果自定义路径存在或者可以创建
        if custom_path.exists() || fs::create_dir_all(&custom_path).is_ok() {
            return custom_path;
        }
    }

    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        let pictures_dir = PathBuf::from(&user_profile).join("Pictures").join("AIWallpaper");
        if pictures_dir.parent().is_some_and(|parent| parent.exists()) {
            return pictures_dir;
        }

        let downloads_dir = PathBuf::from(user_profile).join("Downloads").join("AIWallpaper");
        if downloads_dir.parent().is_some_and(|parent| parent.exists()) {
            return downloads_dir;
        }
    }

    app_data_dir.join("exports")
}

fn save_generated_image(app_data_dir: &std::path::Path, config: &AppConfig) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let source_path = app_data_dir.join("cache").join("current_wallpaper.png");
    if !source_path.exists() {
        return Err("当前没有可保存的图片".into());
    }

    let export_dir = export_image_dir(app_data_dir, config);
    fs::create_dir_all(&export_dir)?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let target_path = export_dir.join(format!("aiwallpaper-{}.png", ts));
    fs::copy(source_path, &target_path)?;

    Ok(target_path)
}

/* 
// 之前的外部路径加载方式
fn load_icon(path: &std::path::Path) -> Result<tray_icon::icon::Icon, Box<dyn std::error::Error>> {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::open(path)?.into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    tray_icon::icon::Icon::from_rgba(icon_rgba, icon_width, icon_height).map_err(|e| e.into())
}
*/

fn load_embedded_icon() -> Result<tray_icon::icon::Icon, Box<dyn std::error::Error>> {
    let icon_bytes = include_bytes!("../assets/app_icon.png");
    let image = image::load_from_memory(icon_bytes)?.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.clone().into_raw();
    tray_icon::icon::Icon::from_rgba(rgba, width, height).map_err(|e| e.into())
}

fn load_window_icon() -> Result<tao::window::Icon, Box<dyn std::error::Error>> {
    let icon_bytes = include_bytes!("../assets/app_icon.png");
    let image = image::load_from_memory(icon_bytes)?.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    tao::window::Icon::from_rgba(rgba, width, height).map_err(|e| e.into())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let platform_paths = platform::app_paths();
    // 1. 确保目录存在
    let _ = fs::create_dir_all(&platform_paths.app_data_dir);
    let _ = fs::create_dir_all(&platform_paths.cache_dir);

    // 平台启动配置
    let _ = platform::configure_process(&platform_paths);

    // 定义总配置目录在 AppData
    let app_data_dir = std::env::var("LOCALAPPDATA")
        .map(|ld| std::path::PathBuf::from(ld).join("AIWallpaper"))
        .unwrap_or_else(|_| std::env::temp_dir().join("AIWallpaper"));
    
    if !app_data_dir.exists() {
        let _ = fs::create_dir_all(&app_data_dir);
    }

    // 通过环境变量强制 WebView2 把 User Data Folder 放到 AppData，而非 exe 同级目录
    let webview2_data_dir = app_data_dir.join("WebView2");
    std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &webview2_data_dir);

    let config_path = app_data_dir.join("config.json");
    let initial_config = if let Ok(content) = fs::read_to_string(&config_path) {
        serde_json::from_str::<AppConfig>(&content)
            .unwrap_or(AppConfig { 
                api_key: "".to_string(),
                enable_cache: false,
                cache_limit: 100,
                auto_refresh_hours: 24,
                auto_prompt: "Cyberpunk city, neon lights, 8k resolution".to_string(),
                gallery_path: "".to_string(),
            })
    } else {
        dotenvy::dotenv().ok();
        AppConfig { 
            api_key: std::env::var("ERNIE_API_KEY").unwrap_or_default(),
            enable_cache: false,
            cache_limit: 100,
            auto_refresh_hours: 24,
            auto_prompt: "Cyberpunk city, neon lights, 8k resolution".to_string(),
            gallery_path: "".to_string(),
        }
    };

    let config = Arc::new(Mutex::new(initial_config));
    // 跟踪当前激活的面板模式（"lite" 或 "pro"）
    let current_mode = Arc::new(Mutex::new("lite".to_string()));
    let mut event_loop = EventLoop::<AppEvent>::with_user_event();
    
    // 允许平台修改 EventLoop (macOS 必需)
    platform::configure_event_loop(&event_loop);

    let tray_menu = Menu::new();
    let show_item = MenuItem::new("显示控制面板", true, None);
    let switch_to_pro_item = MenuItem::new("切换到 Pro 模式", true, None);
    let switch_to_lite_item = MenuItem::new("切换到 Lite 模式", true, None);
    let quit_item = MenuItem::new("退出", true, None);
    let show_id = show_item.id();
    let switch_to_pro_id = switch_to_pro_item.id();
    let switch_to_lite_id = switch_to_lite_item.id();
    let quit_id = quit_item.id();
    tray_menu.append_items(&[
        &show_item,
        &PredefinedMenuItem::separator(),
        &switch_to_pro_item,
        &switch_to_lite_item,
        &PredefinedMenuItem::separator(),
        &quit_item,
    ]);

    let mut tray_builder = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("AIWallpaper");

    /*
    // 修改: 使用 image crate 手动加载像素，以规避 Windows GDI+ 路径加载问题
    match load_icon(std::path::Path::new("assets/app_icon.png")) {
        Ok(icon) => {
            println!("成功加载图标像素: assets/app_icon.png");
            tray_builder = tray_builder.with_icon(icon);
        }
        Err(e) => {
            eprintln!("无法加载图标: {:?}. 检查 assets/app_icon.png 是否有效。", e);
        }
    }
    */

    // 改用 include_bytes! 宏将图标直接嵌入二进制文件
    match load_embedded_icon() {
        Ok(icon) => {
            tray_builder = tray_builder.with_icon(icon);
        }
        Err(e) => {
            eprintln!("无法加载嵌入图标: {:?}", e);
        }
    }

    let _tray_icon = tray_builder.build()?;

    // 窗口图标
    let window_icon = load_window_icon().ok();

    // 1. 创建背景窗口 (WebView2 壁纸层)
    let bg_builder = WindowBuilder::new()
        .with_title("AIWallpaper Layer")
        .with_decorations(false)
        .with_visible(false)
        .with_window_icon(window_icon.clone());
    
    // 针对平台定制 Builder (macOS 必需属性)
    let bg_builder = platform::configure_background_window_builder(bg_builder, event_loop.primary_monitor());
    
    let bg_window = bg_builder.build(&event_loop)?;

    // 定义 WebView2 数据文件夹路径
    let data_dir = app_data_dir.join("WebView2");
    if !data_dir.exists() {
        let _ = std::fs::create_dir_all(&data_dir);
    }

    let bg_hwnd = bg_window.hwnd() as isize;
    
    // Windows 下执行传统的劫持逻辑
    #[cfg(target_os = "windows")]
    {
        let workerw = unsafe { wallpaper_engine::get_wallpaper_workerw() };
        if workerw != 0 {
            unsafe {
                // 设置父窗口为 WorkerW (Lively 核心技巧)
                SetParent(bg_hwnd, workerw);
                
                // 复刻 Lively 窗口样式：TOOLWINDOW (0x80) | NOACTIVATE (0x08000000)
                let ex_style = GetWindowLongPtrA(bg_hwnd, GWL_EXSTYLE);
                SetWindowLongPtrA(bg_hwnd, GWL_EXSTYLE, ex_style | WS_EX_TOOLWINDOW as isize | WS_EX_NOACTIVATE as isize);
                
                // 确保同步渲染位置
                SetWindowPos(bg_hwnd, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
            }
            bg_window.set_maximized(true);
            bg_window.set_visible(true);
        } else {
            eprintln!("无法定位壁纸层窗口！");
        }
    }

    // 调用平台特定的附加逻辑 (macOS 在此进行劫持)
    let _ = platform::attach_background_window(&bg_window);

    let bg_preview_image_path = app_data_dir.join("cache").join("current_wallpaper.png");
    let bg_preview_image_path_for_init = bg_preview_image_path.clone();

    let bg_webview = WebViewBuilder::new(bg_window)?
        .with_custom_protocol("aiwallpaper".into(), move |request| {
            let request_path = request.uri().path();
            let request_uri = request.uri().to_string();
            if !request_path.contains("current_wallpaper.png") && !request_uri.contains("current_wallpaper.png") {
                return Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Cow::Owned(Vec::new()))
                    .map_err(Into::into);
            }

            match fs::read(&bg_preview_image_path) {
                Ok(bytes) => Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, "image/png")
                    .header("Cache-Control", "no-cache, no-store, must-revalidate")
                    .body(Cow::Owned(bytes))
                    .map_err(Into::into),
                Err(_) => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Cow::Owned(Vec::new()))
                    .map_err(Into::into),
            }
        })
        .with_transparent(true) // 尝试背景透明以减少合并问题
        .with_html(include_str!("../bg/index.html"))?
        // 读取当前系统壁纸路径并注入，避免注入后出现黑屏
        .with_initialization_script(&{
            let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            let file_url = platform::initial_wallpaper_url(&bg_preview_image_path_for_init).unwrap_or_default();
            if file_url.is_empty() { String::new() }
            else { format!("window.__initialWallpaper = '{}?t={}'; console.log('Init wallpaper:', window.__initialWallpaper);", file_url, ts) }
        })
        .build()?;

    // 判断是否需要弹出配置引导
    let initial_key_missing = config.lock().unwrap().api_key.is_empty();
    let show_modal_script = if initial_key_missing {
        "window.addEventListener('load', function() { setTimeout(showApiModal, 300); });"
    } else {
        ""
    };

    // 2. 创建控制面板窗口 (Lite 版)
    let control_window = WindowBuilder::new()
        .with_title("AIWallpaper 控制中心")
        .with_inner_size(tao::dpi::LogicalSize::new(500.0, 640.0))
        .with_visible(false)
        .with_window_icon(window_icon.clone())
        .build(&event_loop)?;

    // 3. 创建 Pro 版窗口 (默认隐藏)
    let pro_window = WindowBuilder::new()
        .with_title("AIWallpaper Pro")
        .with_inner_size(tao::dpi::LogicalSize::new(1200.0, 800.0))
        .with_visible(false)
        .with_window_icon(window_icon)
        .build(&event_loop)?;

    let preview_image_path = app_data_dir.join("cache").join("current_wallpaper.png");
    let preview_image_path_lite = preview_image_path.clone();
    let preview_image_path_pro = preview_image_path.clone();

    let (tx, mut rx) = mpsc::channel::<String>(32);
    let tx_pro = tx.clone();
    let control_webview = WebViewBuilder::new(control_window)?
        .with_custom_protocol("aiwallpaper".into(), move |request| {
            let request_path = request.uri().path();
            let request_uri = request.uri().to_string();
            if !request_path.contains("current_wallpaper.png") && !request_uri.contains("current_wallpaper.png") {
                return Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Cow::Owned(Vec::new()))
                    .map_err(Into::into);
            }

            match fs::read(&preview_image_path_lite) {
                Ok(bytes) => Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, "image/png")
                    .header("Cache-Control", "no-cache, no-store, must-revalidate")
                    .body(Cow::Owned(bytes))
                    .map_err(Into::into),
                Err(_) => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Cow::Owned(Vec::new()))
                    .map_err(Into::into),
            }
        })
        .with_html(include_str!("../ui/index.html"))?
        .with_initialization_script(show_modal_script)
        .with_ipc_handler(move |_window, request| {
            let _ = tx.try_send(request);
        })
        .build()?;

    let pro_webview = WebViewBuilder::new(pro_window)?
        .with_custom_protocol("pro".into(), move |request| {
            let path = request.uri().path();
            let uri = request.uri().to_string();

            // 拦截图片预览请求
            if path.contains("current_wallpaper.png") || uri.contains("current_wallpaper.png") {
                match fs::read(&preview_image_path_pro) {
                    Ok(bytes) => {
                        return Response::builder()
                            .status(StatusCode::OK)
                            .header(CONTENT_TYPE, "image/png")
                            .header("Cache-Control", "no-cache, no-store, must-revalidate")
                            .body(Cow::Owned(bytes))
                            .map_err(Into::into);
                    }
                    Err(_) => {
                        return Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(Cow::Owned(Vec::new()))
                            .map_err(Into::into);
                    }
                }
            }

            let relative_path = if path == "/" || path == "" {
                "index.html"
            } else {
                path.trim_start_matches('/')
            };

            // 从编译时嵌入的 dist 目录中读取 React 产物
            match PRO_DIST.get_file(relative_path) {
                Some(file) => {
                    let mime = if relative_path.ends_with(".js") {
                        "application/javascript"
                    } else if relative_path.ends_with(".css") {
                        "text/css"
                    } else if relative_path.ends_with(".html") {
                        "text/html"
                    } else {
                        "application/octet-stream"
                    };
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(CONTENT_TYPE, mime)
                        .body(Cow::Owned(file.contents().to_vec()))
                        .map_err(Into::into)
                }
                None => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Cow::Owned(Vec::new()))
                    .map_err(Into::into),
            }
        })
        .with_url("pro://localhost/")?
        .with_ipc_handler(move |_window, request| {
            let _ = tx_pro.try_send(request);
        })
        .build()?;

    // 保存窗口 ID，用于事件循环中区分哪个窗口触发了关闭/最小化
    let control_window_id = control_webview.window().id();
    let pro_window_id = pro_webview.window().id();

    let proxy = event_loop.create_proxy();
    let config_task = config.clone();
    let app_data_dir_task = app_data_dir.clone();

    tokio::spawn(async move {
        while let Some(msg_raw) = rx.recv().await {
            if let Ok(msg) = serde_json::from_str::<IpcMessage>(&msg_raw) {
                match msg.msg_type.as_str() {
                    "ready" => {
                        let _ = proxy.send_event(AppEvent::Ready);
                    }
                    "minimize" => {
                        let _ = proxy.send_event(AppEvent::Minimize);
                    }
                    "close" => {
                        let _ = proxy.send_event(AppEvent::Close);
                    }
                    "switch_mode" => {
                        let _ = proxy.send_event(AppEvent::SwitchMode(msg.value));
                    }
                    "save_config" => {
                        let mut cfg = config_task.lock().unwrap();
                        if let Ok(new_cfg) = serde_json::from_str::<AppConfig>(&msg.value) {
                             *cfg = new_cfg;
                             let app_data_dir = std::env::var("LOCALAPPDATA")
                                .map(|ld| std::path::PathBuf::from(ld).join("AIWallpaper"))
                                .unwrap_or_else(|_| std::env::temp_dir().join("AIWallpaper"));
                            let cfg_path = app_data_dir.join("config.json");
                            let _ = fs::write(cfg_path, serde_json::to_string(&*cfg).unwrap());
                            println!("配置已保存并应用: {:?}", msg.value);
                        }
                    }
                    "save_key" => {
                        let mut cfg = config_task.lock().unwrap();
                        cfg.api_key = msg.value.clone();
                        let app_data_dir = std::env::var("LOCALAPPDATA")
                            .map(|ld| std::path::PathBuf::from(ld).join("AIWallpaper"))
                            .unwrap_or_else(|_| std::env::temp_dir().join("AIWallpaper"));
                        let cfg_path = app_data_dir.join("config.json");
                        let _ = fs::write(cfg_path, serde_json::to_string(&*cfg).unwrap());
                        println!("API Key 已保存");
                    }
                    "save_image" => {
                        let app_data_dir_for_save = app_data_dir_task.clone();
                        let proxy_for_save = proxy.clone();
                        let cfg = config_task.lock().unwrap().clone();
                        tokio::spawn(async move {
                            match save_generated_image(&app_data_dir_for_save, &cfg) {
                                Ok(path_buf) => {
                                    let path_str = path_buf.to_string_lossy().to_string();
                                    let _ = proxy_for_save.send_event(AppEvent::Saved(path_str));
                                }
                                Err(e) => {
                                    let _ = proxy_for_save.send_event(AppEvent::Error(e.to_string()));
                                }
                            }
                        });
                    }
                    "generate" => {
                        let current_key = config_task.lock().unwrap().api_key.clone();
                        let proxy_gen = proxy.clone();
                        let prompt = msg.value.clone();
                        let app_data_dir_gen = app_data_dir_task.clone();
                        let cfg = config_task.lock().unwrap().clone();
                        tokio::spawn(async move {
                            if current_key.is_empty() {
                                let _ = proxy_gen.send_event(AppEvent::Error("API Key 为空".into()));
                                return;
                            }
                            match api::generate_image(&prompt, &current_key).await {
                                Ok(image) => {
                                    // 自动保存到画廊
                                    let _ = save_generated_image(&app_data_dir_gen, &cfg);
                                    let _ = proxy_gen.send_event(AppEvent::Generated(image));
                                }
                                Err(e) => {
                                    let _ = proxy_gen.send_event(AppEvent::Error(e.to_string()));
                                }
                            }
                        });
                    }
                    "get_gallery" => {
                        let cfg = config_task.lock().unwrap().clone();
                        let app_data_dir_gallery = app_data_dir_task.clone();
                        let proxy_gallery = proxy.clone();
                        tokio::spawn(async move {
                            let gallery_dir = export_image_dir(&app_data_dir_gallery, &cfg);
                            let mut images = Vec::new();
                            if let Ok(entries) = fs::read_dir(gallery_dir) {
                                for entry in entries.flatten() {
                                    let path = entry.path();
                                    if path.is_file() && path.extension().is_some_and(|ext| ext == "png") {
                                        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
                                        if let Ok(bytes) = fs::read(&path) {
                                            let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
                                            images.push(serde_json::json!({
                                                "name": file_name,
                                                "data": format!("data:image/png;base64,{}", b64)
                                            }));
                                        }
                                    }
                                }
                            }
                            images.sort_by(|a, b| b["name"].as_str().cmp(&a["name"].as_str())); 
                            let _ = proxy_gallery.send_event(AppEvent::GalleryLoaded(images));
                        });
                    }
                    "set_wallpaper" => {
                        let file_name = msg.value.clone();
                        let cfg = config_task.lock().unwrap().clone();
                        let app_data_dir_set = app_data_dir_task.clone();
                        let proxy_set = proxy.clone();
                        tokio::spawn(async move {
                            let gallery_dir = export_image_dir(&app_data_dir_set, &cfg);
                            let img_path = gallery_dir.join(&file_name);
                            if img_path.exists() {
                                let cache_path = app_data_dir_set.join("cache").join("current_wallpaper.png");
                                match fs::copy(&img_path, &cache_path) {
                                    Ok(_) => {
                                        let _ = proxy_set.send_event(AppEvent::Generated(api::GeneratedImage {
                                            preview_url: "aiwallpaper://localhost/current_wallpaper.png".to_string(),
                                            wallpaper_url: "".to_string(),
                                        }));
                                    }
                                    Err(e) => {
                                        let _ = proxy_set.send_event(AppEvent::Error(format!("应用壁纸失败: {}", e)));
                                    }
                                }
                            } else {
                                let _ = proxy_set.send_event(AppEvent::Error(format!("找不到图片文件: {}", file_name)));
                            }
                        });
                    }
                    "delete_image" => {
                        let file_name = msg.value.clone();
                        let cfg = config_task.lock().unwrap().clone();
                        let app_data_dir_del = app_data_dir_task.clone();
                        let proxy_del = proxy.clone();
                        tokio::spawn(async move {
                            let gallery_dir = export_image_dir(&app_data_dir_del, &cfg);
                            let img_path = gallery_dir.join(&file_name);
                            let _ = fs::remove_file(&img_path);
                            // 删除后重新加载画廊
                            let mut images = Vec::new();
                            if let Ok(entries) = fs::read_dir(&gallery_dir) {
                                for entry in entries.flatten() {
                                    let path = entry.path();
                                    if path.is_file() && path.extension().is_some_and(|ext| ext == "png") {
                                        let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
                                        if let Ok(bytes) = fs::read(&path) {
                                            let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
                                            images.push(serde_json::json!({
                                                "name": fname,
                                                "data": format!("data:image/png;base64,{}", b64)
                                            }));
                                        }
                                    }
                                }
                            }
                            images.sort_by(|a, b| b["name"].as_str().cmp(&a["name"].as_str()));
                            let _ = proxy_del.send_event(AppEvent::GalleryLoaded(images));
                        });
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
                let mode = current_mode.lock().unwrap().clone();
                if mode == "pro" {
                    pro_webview.window().set_visible(true);
                    pro_webview.window().set_focus();
                } else {
                    control_webview.window().set_visible(true);
                    control_webview.window().set_focus();
                }
            } else if menu_event.id == switch_to_pro_id {
                *current_mode.lock().unwrap() = "pro".to_string();
                control_webview.window().set_visible(false);
                pro_webview.window().set_visible(true);
                pro_webview.window().set_focus();
            } else if menu_event.id == switch_to_lite_id {
                *current_mode.lock().unwrap() = "lite".to_string();
                pro_webview.window().set_visible(false);
                control_webview.window().set_visible(true);
                control_webview.window().set_focus();
            } else if menu_event.id == quit_id {
                *control_flow = ControlFlow::Exit;
            }
        }

        while let Ok(tray_event) = tray_event_receiver().try_recv() {
            if tray_event.event == ClickEvent::Left {
                let mode = current_mode.lock().unwrap().clone();
                if mode == "pro" {
                    pro_webview.window().set_visible(true);
                    pro_webview.window().set_focus();
                } else {
                    control_webview.window().set_visible(true);
                    control_webview.window().set_focus();
                }
            }
        }

        match event {
            // 点击关闭按钮或按 Alt+F4 → 隐藏到托盘，不退出
            Event::WindowEvent { window_id, event: WindowEvent::CloseRequested, .. } => {
                if window_id == control_window_id {
                    control_webview.window().set_visible(false);
                } else if window_id == pro_window_id {
                    pro_webview.window().set_visible(false);
                }
            }
            Event::UserEvent(app_event) => {
                match app_event {
                    AppEvent::Ready => {
                        // 只显示当前激活模式的窗口（避免 Pro 初始化时误弹 Lite）
                        let mode = current_mode.lock().unwrap().clone();
                        if mode == "pro" {
                            pro_webview.window().set_visible(true);
                            pro_webview.window().set_focus();
                        } else {
                            control_webview.window().set_visible(true);
                            control_webview.window().set_focus();
                        }

                        // 向所有 UI 发送当前配置
                        let cfg = config.lock().unwrap();
                        let js = format!(
                            "if (window.onConfigLoaded) {{ window.onConfigLoaded({}) }}",
                            serde_json::to_string(&*cfg).unwrap()
                        );
                        let _ = control_webview.evaluate_script(&js);
                        let _ = pro_webview.evaluate_script(&js);
                    }
                    AppEvent::Minimize => {
                        // 最小化 = 隐藏到托盘（两个窗口都隐藏）
                        control_webview.window().set_visible(false);
                        pro_webview.window().set_visible(false);
                    }
                    AppEvent::Close => {
                        *control_flow = ControlFlow::Exit;
                    }
                    AppEvent::SwitchMode(mode) => {
                        if mode == "pro" {
                            *current_mode.lock().unwrap() = "pro".to_string();
                            control_webview.window().set_visible(false);
                            pro_webview.window().set_visible(true);
                            pro_webview.window().set_focus();
                        } else {
                            *current_mode.lock().unwrap() = "lite".to_string();
                            pro_webview.window().set_visible(false);
                            control_webview.window().set_visible(true);
                            control_webview.window().set_focus();
                        }
                    }
                    AppEvent::Generated(image) => {
                        // 将图片编码为 base64 data URL，直接内嵌发送，绕开自定义协议加载问题
                        let preview_path = app_data_dir.join("cache").join("current_wallpaper.png");
                        let data_url = fs::read(&preview_path)
                            .map(|bytes| format!("data:image/png;base64,{}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes)))
                            .unwrap_or_default();

                        let ui_payload = serde_json::json!({
                            "previewUrl": data_url,
                            "viewUrl": image.wallpaper_url,
                        });
                        let js_ui = format!(
                            "window.onGenerationComplete(true, '', {})",
                            ui_payload
                        );
                        let _ = control_webview.evaluate_script(&js_ui);
                        let _ = pro_webview.evaluate_script(&js_ui);

                        // 直接传 base64 data URL 给背景层，避免 WebView2 URL 缓存问题
                        // 两条链路（生成/画廊应用）统一走同一路径
                        let js_bg = format!(
                            "window.setWallpaper({}, 'Prompt')",
                            serde_json::to_string(&data_url).unwrap()
                        );
                        let _ = bg_webview.evaluate_script(&js_bg);
                    }
                    AppEvent::Saved(path) => {
                        let js_ui = format!(
                            "window.onImageSaved({})",
                            serde_json::to_string(&path).unwrap()
                        );
                        let _ = control_webview.evaluate_script(&js_ui);
                        let _ = pro_webview.evaluate_script(&js_ui);
                    }
                    AppEvent::Error(err_msg) => {
                        let js_ui = format!(
                            "window.onGenerationComplete(false, {}, null)",
                            serde_json::to_string(&err_msg).unwrap()
                        );
                        let _ = control_webview.evaluate_script(&js_ui);
                        let _ = pro_webview.evaluate_script(&js_ui);
                    }
                    AppEvent::GalleryLoaded(images) => {
                        let js = format!(
                            "if (window.onGalleryLoaded) {{ window.onGalleryLoaded({}) }}",
                            serde_json::to_string(&images).unwrap()
                        );
                        let _ = pro_webview.evaluate_script(&js);
                    }
                }
            }
            _ => (),
        }
    });
}
