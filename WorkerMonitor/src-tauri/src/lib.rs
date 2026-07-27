pub mod config;
pub mod monitor;

use monitor::{MonitorState, MonitorStatus};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};

#[tauri::command]
fn start_monitoring(state: tauri::State<'_, MonitorState>, app: AppHandle) -> Result<(), String> {
    state.start(app)?;
    Ok(())
}

#[tauri::command]
fn stop_monitoring(state: tauri::State<'_, MonitorState>) -> Result<(), String> {
    state.stop();
    Ok(())
}

#[tauri::command]
fn update_presence(
    state: tauri::State<'_, MonitorState>,
    app: AppHandle,
    present: bool,
) -> Result<monitor::MonitorSnapshot, String> {
    state.update_presence(present, app)
}

#[tauri::command]
fn get_monitor_status(state: tauri::State<'_, MonitorState>) -> Result<monitor::MonitorSnapshot, String> {
    state.snapshot()
}

#[tauri::command]
fn get_config(state: tauri::State<'_, MonitorState>) -> Result<config::AppConfig, String> {
    state.get_config()
}

#[tauri::command]
fn save_config(
    state: tauri::State<'_, MonitorState>,
    app: AppHandle,
    config: config::AppConfig,
) -> Result<(), String> {
    state.save_config(config, app)
}

#[tauri::command]
fn dismiss_break_alert(state: tauri::State<'_, MonitorState>) -> Result<(), String> {
    state.dismiss_alert();
    Ok(())
}

#[tauri::command]
fn report_posture(
    state: tauri::State<'_, MonitorState>,
    app: AppHandle,
    score: u32,
    head_forward: bool,
    head_tilt: bool,
    shoulder_uneven: bool,
    slouching: bool,
) -> Result<(), String> {
    state.report_posture(app, score, head_forward, head_tilt, shoulder_uneven, slouching)
}

fn update_tray_tooltip(app: &AppHandle, snapshot: &monitor::MonitorSnapshot) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let tooltip = match snapshot.status {
            MonitorStatus::Idle => "WorkerMonitor - 未启动".to_string(),
            MonitorStatus::Present => {
                format!("WorkerMonitor - 工作中 {}", format_duration(snapshot.work_duration_secs))
            }
            MonitorStatus::Away => {
                format!("WorkerMonitor - 离开 {}", format_duration(snapshot.break_duration_secs))
            }
            MonitorStatus::Overworked => {
                format!("WorkerMonitor - ⚠ 超时工作 {}", format_duration(snapshot.work_duration_secs))
            }
        };
        let _ = tray.set_tooltip(Some(&tooltip));
    }
}

fn format_duration(secs: f64) -> String {
    let total_secs = secs as u64;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    if hours > 0 {
        format!("{}h{}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m{}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

pub fn run() {
    let monitor_state = MonitorState::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(monitor_state)
        .invoke_handler(tauri::generate_handler![
            start_monitoring,
            stop_monitoring,
            update_presence,
            get_monitor_status,
            get_config,
            save_config,
            dismiss_break_alert,
            report_posture,
        ])
        .setup(|app| {
            let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &separator, &quit_item])?;

            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("WorkerMonitor")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            let main_window = app.get_webview_window("main").unwrap();
            let w = main_window.clone();
            main_window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = w.hide();
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_, _| {});
}
