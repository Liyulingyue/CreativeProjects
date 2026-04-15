use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use tao::platform::windows::WindowExtWindows;
use wry::webview::WebViewBuilder;
use windows_sys::Win32::UI::WindowsAndMessaging::{SetParent, SetWindowPos, SWP_SHOWWINDOW};
use tokio::sync::mpsc;
use tray_icon::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    TrayIconBuilder,
};
use std::fs;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};

mod wallpaper_engine;
mod api;

#[derive(Serialize, Deserialize, Clone)]
struct AppConfig {
    api_key: String,
}

#[derive(Serialize, Deserialize)]
struct IpcMessage {
    #[serde(rename = "type")]
    msg_type: String,
    value: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 尝试加载配置
    let config_path = "config.json";
    let initial_config = if let Ok(content) = fs::read_to_string(config_path) {
        serde_json::from_str::<AppConfig>(&content).unwrap_or(AppConfig { api_key: "".to_string() })
    } else {
        // 如果没有 JSON，尝试读 .env
        dotenvy::dotenv().ok();
        let env_key = std::env::var("ERNIE_API_KEY").unwrap_or_default();
        AppConfig { api_key: env_key }
    };

    let config = Arc::new(Mutex::new(initial_config));
    let event_loop = EventLoop::<(String, String)>::with_user_event();

    // --- 托盘设置 ---
    let tray_menu = Menu::new();
    let show_item = MenuItem::new("显示控制面板", true, None);
    let quit_item = MenuItem::new("退出", true, None);
    tray_menu.append_items(&[&show_item, &PredefinedMenuItem::separator(), &quit_item]);

    let _tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("AIWallpaper")
        .build()?;

    // 1. 创建控制面板窗口 (UI Window)
    let control_window = WindowBuilder::new()
        .with_title("AIWallpaper 控制中心")
        .with_inner_size(tao::dpi::LogicalSize::new(400.0, 600.0))
        .build(&event_loop)?;

    // 2. 创建壁纸背景窗 (Background Window)
    let bg_window = WindowBuilder::new()
        .with_title("AIWallpaper Background")
        .with_decorations(false)
        .build(&event_loop)?;

    unsafe {
        let workerw = wallpaper_engine::get_wallpaper_workerw();
        if workerw != 0 {
            let bg_hwnd = bg_window.hwnd() as isize;
            SetParent(bg_hwnd, workerw);
            SetWindowPos(bg_hwnd, 0, 0, 0, 1920, 1080, SWP_SHOWWINDOW); 
        }
    }

    let (tx, mut rx) = mpsc::channel::<String>(32);

    let _control_webview = WebViewBuilder::new(control_window)?
        .with_html(include_str!("../ui/index.html"))?
        .with_ipc_handler(move |_window, request| {
            let _ = tx.try_send(request);
        })
        .build()?;

    let bg_webview = WebViewBuilder::new(bg_window)?
        .with_html(include_str!("../bg/index.html"))?
        .build()?;

    let proxy = event_loop.create_proxy();
    let config_task = config.clone();

    tokio::spawn(async move {
        while let Some(msg_raw) = rx.recv().await {
            if let Ok(msg) = serde_json::from_str::<IpcMessage>(&msg_raw) {
                match msg.msg_type.as_str() {
                    "save_key" => {
                        let mut cfg = config_task.lock().unwrap();
                        cfg.api_key = msg.value.clone();
                        let _ = fs::write(config_path, serde_json::to_string(&*cfg).unwrap());
                        println!("API Key 已保存并持久化。");
                    }
                    "generate" => {
                        let current_key = config_task.lock().unwrap().api_key.clone();
                        if current_key.is_empty() {
                            eprintln!("错误：API Key 为空，请先配置。");
                            continue;
                        }
                        match api::generate_image(&msg.value, &current_key).await {
                            Ok(img_url) => {
                                let _ = proxy.send_event((img_url, msg.value));
                            }
                            Err(e) => eprintln!("生成失败: {:?}", e),
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *control_flow = ControlFlow::Exit,
            Event::UserEvent((url, prompt)) => {
                let js = format!("window.setWallpaper('{}', '{}')", url, prompt);
                let _ = bg_webview.evaluate_script(&js);
            }
            _ => (),
        }
    });
}
