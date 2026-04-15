use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
    platform::windows::WindowExtWindows,
};
use wry::webview::WebViewBuilder;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SetParent, SetWindowPos, GetWindowLongPtrA, SetWindowLongPtrA,
    GWL_EXSTYLE, WS_EX_TOOLWINDOW, WS_EX_NOACTIVATE,
    HWND_BOTTOM, SWP_NOMOVE, SWP_NOSIZE, SWP_NOACTIVATE, SWP_SHOWWINDOW,
};
use tokio::sync::mpsc;
use tray_icon::{
    menu::{Menu, MenuItem, PredefinedMenuItem, menu_event_receiver},
    TrayIconBuilder, tray_event_receiver, ClickEvent,
};
use std::fs;
use std::sync::{Arc, Mutex};
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
    value: String,
}

// (img_url, success, error_msg)
type AppEvent = (String, bool, String);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = "config.json";
    let initial_config = if let Ok(content) = fs::read_to_string(config_path) {
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

    let _tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("AIWallpaper")
        .build()?;

    // 1. 创建背景窗口 (WebView2 壁纸层)
    let bg_window = WindowBuilder::new()
        .with_title("AIWallpaper Layer")
        .with_decorations(false)
        .with_visible(false)
        .build(&event_loop)?;

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

    let bg_webview = WebViewBuilder::new(bg_window)?
        .with_transparent(true) // 尝试背景透明以减少合并问题
        .with_html(include_str!("../bg/index.html"))?
        .build()?;

    // 2. 创建控制面板窗口
    let control_window = WindowBuilder::new()
        .with_title("AIWallpaper 控制中心")
        .with_inner_size(tao::dpi::LogicalSize::new(400.0, 620.0))
        .build(&event_loop)?;

    let (tx, mut rx) = mpsc::channel::<String>(32);
    let control_webview = WebViewBuilder::new(control_window)?
        .with_html(include_str!("../ui/index.html"))?
        .with_ipc_handler(move |_window, request| {
            let _ = tx.try_send(request);
        })
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
                        println!("API Key 已保存");
                    }
                    "generate" => {
                        let current_key = config_task.lock().unwrap().api_key.clone();
                        if current_key.is_empty() {
                            let _ = proxy.send_event(("".into(), false, "API Key 为空".into()));
                            continue;
                        }
                        let gen = api::generate_image(&msg.value, &current_key)
                            .await.map_err(|e| e.to_string());
                        match gen {
                            Ok(img_url) => {
                                let _ = proxy.send_event((img_url, true, "".into()));
                            }
                            Err(e) => {
                                let _ = proxy.send_event(("".into(), false, e));
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
            Event::UserEvent((img_url, success, err_msg)) => {
                if success {
                    let js_ui = format!("window.onGenerationComplete(true, '', '{}')", img_url.replace('\'', "\\'"));
                    let _ = control_webview.evaluate_script(&js_ui);
                    
                    let js_bg = format!("window.setWallpaper('{}', 'Prompt')", img_url.replace('\'', "\\'"));
                    let _ = bg_webview.evaluate_script(&js_bg);
                } else {
                    let js_ui = format!("window.onGenerationComplete(false, '{}', '')", err_msg.replace('\'', "\\'"));
                    let _ = control_webview.evaluate_script(&js_ui);
                }
            }
            _ => (),
        }
    });
}
