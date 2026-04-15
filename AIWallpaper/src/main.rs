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

mod api;
mod wallpaper_engine;

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

fn export_image_dir(app_data_dir: &std::path::Path) -> PathBuf {
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

fn save_generated_image(app_data_dir: &std::path::Path) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let source_path = app_data_dir.join("cache").join("current_wallpaper.png");
    if !source_path.exists() {
        return Err("当前没有可保存的图片".into());
    }

    let export_dir = export_image_dir(app_data_dir);
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
            .unwrap_or(AppConfig { api_key: "".to_string() })
    } else {
        dotenvy::dotenv().ok();
        AppConfig { api_key: std::env::var("ERNIE_API_KEY").unwrap_or_default() }
    };

    let config = Arc::new(Mutex::new(initial_config));
    let event_loop = EventLoop::<AppEvent>::with_user_event();

    let tray_menu = Menu::new();
    let show_item = MenuItem::new("显示控制面板", true, None);
    let quit_item = MenuItem::new("退出", true, None);
    let show_id = show_item.id();
    let quit_id = quit_item.id();
    tray_menu.append_items(&[&show_item, &PredefinedMenuItem::separator(), &quit_item]);

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
    let bg_window = WindowBuilder::new()
        .with_title("AIWallpaper Layer")
        .with_decorations(false)
        .with_visible(false)
        .with_window_icon(window_icon.clone())
        .build(&event_loop)?;

    // 定义 WebView2 数据文件夹路径
    let data_dir = app_data_dir.join("WebView2");
    if !data_dir.exists() {
        let _ = std::fs::create_dir_all(&data_dir);
    }

    let bg_hwnd = bg_window.hwnd() as isize;
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

    let bg_preview_image_path = app_data_dir.join("cache").join("current_wallpaper.png");

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
            let file_url = unsafe {
                let mut buf = vec![0u16; 260];
                SystemParametersInfoW(SPI_GETDESKWALLPAPER, 260, buf.as_mut_ptr() as *mut _, 0);
                let path = String::from_utf16_lossy(&buf)
                    .trim_end_matches('\0')
                    .to_string();
                if path.is_empty() { String::new() }
                else { format!("file:///{}", path.replace('\\', "/")) }
            };
            if file_url.is_empty() { String::new() }
            else { format!("window.__initialWallpaper = '{}'", file_url) }
        })
        .build()?;

    // 判断是否需要弹出配置引导
    let initial_key_missing = config.lock().unwrap().api_key.is_empty();
    let show_modal_script = if initial_key_missing {
        "window.addEventListener('load', function() { setTimeout(showApiModal, 300); });"
    } else {
        ""
    };

    // 2. 创建控制面板窗口
    let control_window = WindowBuilder::new()
        .with_title("AIWallpaper 控制中心")
        .with_inner_size(tao::dpi::LogicalSize::new(500.0, 640.0))
        .with_visible(false)
        .with_window_icon(window_icon)
        .build(&event_loop)?;

    let preview_image_path = app_data_dir.join("cache").join("current_wallpaper.png");

    let (tx, mut rx) = mpsc::channel::<String>(32);
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

            match fs::read(&preview_image_path) {
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
                        match save_generated_image(&app_data_dir_task) {
                            Ok(path) => {
                                let path_str = path.to_string_lossy().to_string();
                                let _ = proxy.send_event(AppEvent::Saved(path_str));
                            }
                            Err(e) => {
                                let _ = proxy.send_event(AppEvent::Error(e.to_string()));
                            }
                        }
                    }
                    "generate" => {
                        let current_key = config_task.lock().unwrap().api_key.clone();
                        if current_key.is_empty() {
                            let _ = proxy.send_event(AppEvent::Error("API Key 为空".into()));
                            continue;
                        }
                        let gen = api::generate_image(&msg.value, &current_key)
                            .await.map_err(|e| e.to_string());
                        match gen {
                            Ok(image) => {
                                let _ = proxy.send_event(AppEvent::Generated(image));
                            }
                            Err(e) => {
                                let _ = proxy.send_event(AppEvent::Error(e));
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
                control_webview.window().set_visible(true);
                control_webview.window().set_focus();
            } else if menu_event.id == quit_id {
                *control_flow = ControlFlow::Exit;
            }
        }

        while let Ok(tray_event) = tray_event_receiver().try_recv() {
            if tray_event.event == ClickEvent::Left {
                control_webview.window().set_visible(true);
                control_webview.window().set_focus();
            }
        }

        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                control_webview.window().set_visible(false);
            }
            Event::UserEvent(app_event) => {
                match app_event {
                    AppEvent::Ready => {
                    control_webview.window().set_visible(true);
                    control_webview.window().set_focus();
                    }
                    AppEvent::Minimize => {
                        control_webview.window().set_visible(false);
                    }
                    AppEvent::Close => {
                        *control_flow = ControlFlow::Exit;
                    }
                    AppEvent::Generated(image) => {
                        let ui_payload = serde_json::json!({
                            "previewUrl": image.preview_url,
                            "viewUrl": image.wallpaper_url,
                        });
                        let js_ui = format!(
                            "window.onGenerationComplete(true, '', {})",
                            ui_payload
                        );
                        let _ = control_webview.evaluate_script(&js_ui);

                        let js_bg = format!(
                            "window.setWallpaper({}, 'Prompt')",
                            serde_json::to_string(&image.preview_url).unwrap()
                        );
                        let _ = bg_webview.evaluate_script(&js_bg);
                    }
                    AppEvent::Saved(path) => {
                        let js_ui = format!(
                            "window.onImageSaved({})",
                            serde_json::to_string(&path).unwrap()
                        );
                        let _ = control_webview.evaluate_script(&js_ui);
                    }
                    AppEvent::Error(err_msg) => {
                        let js_ui = format!(
                            "window.onGenerationComplete(false, {}, null)",
                            serde_json::to_string(&err_msg).unwrap()
                        );
                        let _ = control_webview.evaluate_script(&js_ui);
                    }
                }
            }
            _ => (),
        }
    });
}
