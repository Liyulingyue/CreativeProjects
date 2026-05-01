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
    SWP_FRAMECHANGED, SWP_NOZORDER,
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use include_dir::{include_dir, Dir};

static PRO_DIST: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/ui-pro/dist");

mod api;
mod app;
mod platform;
mod wallpaper_engine;

pub use app::{AppConfig, IpcMessage, AppEvent};

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
    // 首先尝试加载环境配置
    dotenvy::dotenv().ok();
    
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
    let mut initial_config = if let Ok(content) = fs::read_to_string(&config_path) {
        let has_minutes_field = content.contains("\"auto_refresh_minutes\"");
        let mut cfg = serde_json::from_str::<AppConfig>(&content)
            .unwrap_or(AppConfig { 
                api_key: "".to_string(),
                enable_cache: false,
                cache_limit: 100,
                auto_refresh_hours: 24,
                auto_refresh_minutes: 24 * 60,
                auto_prompt: "Cyberpunk city, neon lights, 8k resolution".to_string(),
                gallery_path: "".to_string(),
                image_size: "auto".to_string(),
                pe_url: "".to_string(),
                pe_key: "".to_string(),
                pe_model: "".to_string(),
                ui_mode: "lite".to_string(),
                auto_start: false,
            });

        if !has_minutes_field {
            cfg.auto_refresh_minutes = cfg.auto_refresh_hours.saturating_mul(60);
        }
        cfg
    } else {
        AppConfig { 
            api_key: "".to_string(),
            enable_cache: false,
            cache_limit: 100,
            auto_refresh_hours: 24,
            auto_refresh_minutes: 24 * 60,
            auto_prompt: "Cyberpunk city, neon lights, 8k resolution".to_string(),
            gallery_path: "".to_string(),
            image_size: "auto".to_string(),
            pe_url: "".to_string(),
            pe_key: "".to_string(),
            pe_model: "".to_string(),
            ui_mode: "lite".to_string(),
            auto_start: false,
        }
    };

    if initial_config.auto_refresh_minutes > 0 {
        if initial_config.auto_refresh_minutes >= 15 {
            initial_config.auto_refresh_minutes = std::cmp::max(15, ((initial_config.auto_refresh_minutes + 14) / 15) * 15);
        } else {
            // 保留测试场景下的分钟级刷新(1~14 分钟)
            initial_config.auto_refresh_minutes = std::cmp::max(1, initial_config.auto_refresh_minutes);
        }
    }
    initial_config.auto_refresh_hours = initial_config.auto_refresh_minutes / 60;
    // 用注册表中的实际状态覆盖 config 里的 auto_start（避免手动删注册表项后状态不同步）
    initial_config.auto_start = crate::app::ipc::read_auto_start();

    // 从 config 读取上次使用的模式，非法值回退为 lite
    let default_window = if initial_config.ui_mode == "pro" {
        "pro".to_string()
    } else {
        "lite".to_string()
    };
    println!("默认启动窗口模式: {}", default_window);
    let config = Arc::new(Mutex::new(initial_config));
    let current_mode = Arc::new(Mutex::new(default_window.clone()));
    let ready_handled = Arc::new(AtomicBool::new(false));
    
    let event_loop = EventLoop::<AppEvent>::with_user_event();
    
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

    // 加载环境配置：判断是否为开发模式 (Tauri-like Dev)
    let dev_mode = std::env::var("DEV_MODE").unwrap_or_default() == "1";
    let dev_url = std::env::var("DEV_URL").unwrap_or_else(|_| "http://localhost:5173".into());

    if dev_mode {
        println!("🚀 开发模式已启动: 正在连接前端开发服务器 {}...", dev_url);
    }

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
    let app_data_dir_bg = app_data_dir.clone();

    let bg_webview = WebViewBuilder::new(bg_window)?
        .with_custom_protocol("aiwallpaper".into(), move |request| {
            app::protocol::asset_protocol_handler(request, &app_data_dir_bg, &bg_preview_image_path, &PRO_DIST)
                .map_err(|e| {
                    eprintln!("Asset Protocol Error: {:?}", e);
                    wry::Error::DuplicateCustomProtocol("aiwallpaper".to_string()) // 使用一个存在的错误类型
                })
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
        .with_visible(default_window == "lite")
        .with_window_icon(window_icon.clone())
        .build(&event_loop)?;

    // 3. 创建 Pro 版窗口
    let pro_window = WindowBuilder::new()
        .with_title("AIWallpaper Pro")
        .with_inner_size(tao::dpi::LogicalSize::new(1200.0, 800.0))
        .with_visible(default_window == "pro")
        .with_window_icon(window_icon)
        .build(&event_loop)?;

    let preview_image_path = app_data_dir.join("cache").join("current_wallpaper.png");
    let preview_image_path_lite = preview_image_path.clone();
    let preview_image_path_pro = preview_image_path.clone();
    let app_data_dir_lite = app_data_dir.clone();
    let app_data_dir_pro = app_data_dir.clone();

    let (tx, mut rx) = mpsc::channel::<String>(32);
    let tx_pro = tx.clone();
    let control_webview = WebViewBuilder::new(control_window)?
        .with_custom_protocol("aiwallpaper".into(), move |request| {
            app::protocol::asset_protocol_handler(request, &app_data_dir_lite, &preview_image_path_lite, &PRO_DIST)
                .map_err(|e| {
                    eprintln!("Control Asset Error: {:?}", e);
                    wry::Error::DuplicateCustomProtocol("aiwallpaper".to_string())
                })
        });

    let control_webview = if dev_mode {
        control_webview.with_url(&dev_url)?
    } else {
        control_webview.with_url("aiwallpaper://localhost/")?
    };

    let control_webview = control_webview
        .with_initialization_script(show_modal_script)
        .with_ipc_handler(move |_window, request| {
            let _ = tx.try_send(request);
        })
        .build()?;

    let mut pro_webview_builder = WebViewBuilder::new(pro_window)?
        .with_custom_protocol("aiwallpaper".into(), move |request| {
            app::protocol::asset_protocol_handler(request, &app_data_dir_pro, &preview_image_path_pro, &PRO_DIST)
                .map_err(|e| {
                    eprintln!("Pro Asset Error: {:?}", e);
                    wry::Error::DuplicateCustomProtocol("aiwallpaper".to_string())
                })
        });

    if dev_mode {
        pro_webview_builder = pro_webview_builder.with_url(&dev_url)?;
    } else {
        pro_webview_builder = pro_webview_builder.with_url("aiwallpaper://localhost/")?;
    }

    let pro_webview = pro_webview_builder
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
        let ctx = app::ipc::IpcContext {
            config: config_task,
            proxy,
            app_data_dir: app_data_dir_task,
        };
        while let Some(msg_raw) = rx.recv().await {
            app::ipc::handle_message(&msg_raw, &ctx).await;
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
                    AppEvent::ToggleDrawing(enabled) => {
                        let bg_hwnd = bg_webview.window().hwnd() as isize;
                        #[cfg(target_os = "windows")]
                        unsafe {
                            use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrA, SetWindowLongPtrA, GWL_EXSTYLE, WS_EX_TRANSPARENT, WS_EX_LAYERED};
                            let mut ex_style = GetWindowLongPtrA(bg_hwnd, GWL_EXSTYLE);
                            if enabled {
                                // 开启绘制：移除透明击穿，允许接收鼠标事件
                                ex_style &= !WS_EX_TRANSPARENT as isize;
                            } else {
                                // 关闭绘制：增加透明击穿，鼠标点击桌面图标
                                ex_style |= WS_EX_TRANSPARENT as isize;
                            }
                            SetWindowLongPtrA(bg_hwnd, GWL_EXSTYLE, ex_style);
                        }
                        // 同时通知网页端切换模式
                        let _ = bg_webview.evaluate_script(&format!("if(window.toggleDrawingMode) window.toggleDrawingMode({});", enabled));
                    }
                    AppEvent::Ready => {
                        // 只处理第一次 ready（两个窗口的 React 都会发，只响应一次）
                        if ready_handled.swap(true, Ordering::SeqCst) {
                            // 向新触发 ready 的窗口也补发配置，但不再重复处理窗口可见性
                            let cfg = config.lock().unwrap();
                            let js = format!(
                                "if (window.onConfigLoaded) {{ window.onConfigLoaded({}) }}",
                                serde_json::to_string(&*cfg).unwrap()
                            );
                            let _ = control_webview.evaluate_script(&js);
                            let _ = pro_webview.evaluate_script(&js);
                        } else {
                            let mode = current_mode.lock().unwrap().clone();
                            if mode == "pro" {
                                control_webview.window().set_visible(false);
                                pro_webview.window().set_visible(true);
                                pro_webview.window().set_focus();
                                let _ = pro_webview.evaluate_script("if(window.__syncUiMode) window.__syncUiMode('pro');");
                            } else {
                                pro_webview.window().set_visible(false);
                                control_webview.window().set_visible(true);
                                control_webview.window().set_focus();
                                let _ = control_webview.evaluate_script("if(window.__syncUiMode) window.__syncUiMode('lite');");
                            }

                            let cfg = config.lock().unwrap();
                            let js = format!(
                                "if (window.onConfigLoaded) {{ window.onConfigLoaded({}) }}",
                                serde_json::to_string(&*cfg).unwrap()
                            );
                            let _ = control_webview.evaluate_script(&js);
                            let _ = pro_webview.evaluate_script(&js);
                        }
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
                            let _ = pro_webview.evaluate_script("if(window.__syncUiMode) window.__syncUiMode('pro');");
                        } else {
                            *current_mode.lock().unwrap() = "lite".to_string();
                            pro_webview.window().set_visible(false);
                            control_webview.window().set_visible(true);
                            control_webview.window().set_focus();
                            let _ = control_webview.evaluate_script("if(window.__syncUiMode) window.__syncUiMode('lite');");
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
                    AppEvent::PromptEnhanced(text) => {
                        let js = format!(
                            "if (window.onPromptEnhanced) {{ window.onPromptEnhanced({}) }}",
                            serde_json::to_string(&text).unwrap()
                        );
                        let _ = control_webview.evaluate_script(&js);
                        let _ = pro_webview.evaluate_script(&js);
                    }
                    AppEvent::GalleryLoaded(payload) => {
                        let js = format!(
                            "if (window.onGalleryLoaded) {{ window.onGalleryLoaded({}) }}",
                            serde_json::to_string(&payload).unwrap()
                        );
                        let _ = pro_webview.evaluate_script(&js);
                    }
                }
            }
            _ => (),
        }
    });
}
