use std::{path::Path, process::Command, ptr};

use tao::{
    event_loop::EventLoop,
    monitor::MonitorHandle,
    platform::windows::WindowExtWindows,
    window::{Window, WindowBuilder},
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrA, SetParent, SetWindowLongPtrA, SetWindowPos, SystemParametersInfoW,
    GWL_EXSTYLE, HWND_BOTTOM, SPI_GETDESKWALLPAPER, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

use crate::platform::{AppPaths, PlatformResult, UiPlatformInfo};

mod wallpaper_engine;

pub fn app_data_dir() -> std::path::PathBuf {
    std::env::var("LOCALAPPDATA")
        .map(|value| std::path::PathBuf::from(value).join("AIWallpaper"))
        .unwrap_or_else(|_| std::env::temp_dir().join("AIWallpaper"))
}

pub fn export_image_dir(app_data_dir: &Path) -> std::path::PathBuf {
    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        let pictures_dir = std::path::PathBuf::from(&user_profile)
            .join("Pictures")
            .join("AIWallpaper");
        if pictures_dir.parent().is_some_and(|parent| parent.exists()) {
            return pictures_dir;
        }

        let downloads_dir = std::path::PathBuf::from(user_profile)
            .join("Downloads")
            .join("AIWallpaper");
        if downloads_dir.parent().is_some_and(|parent| parent.exists()) {
            return downloads_dir;
        }
    }

    app_data_dir.join("exports")
}

pub fn configure_process(paths: &AppPaths) -> PlatformResult<()> {
    std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &paths.webview_data_dir);
    Ok(())
}

pub fn configure_event_loop<T>(_event_loop: &mut EventLoop<T>) {}

pub fn configure_background_window_builder(
    builder: WindowBuilder,
    _primary_monitor: Option<MonitorHandle>,
) -> WindowBuilder {
    builder
}

pub fn attach_background_window(window: &Window) -> PlatformResult<()> {
    let bg_hwnd = window.hwnd() as isize;
    let workerw = unsafe { wallpaper_engine::get_wallpaper_workerw() };
    if workerw == 0 {
        return Err("无法定位壁纸层窗口".into());
    }

    unsafe {
        SetParent(bg_hwnd, workerw);

        let ex_style = GetWindowLongPtrA(bg_hwnd, GWL_EXSTYLE);
        SetWindowLongPtrA(
            bg_hwnd,
            GWL_EXSTYLE,
            ex_style | WS_EX_TOOLWINDOW as isize | WS_EX_NOACTIVATE as isize,
        );

        SetWindowPos(
            bg_hwnd,
            HWND_BOTTOM,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }

    window.set_maximized(true);
    window.set_visible(true);
    Ok(())
}

pub fn reveal_control_window(window: &Window) {
    window.set_visible(true);
    window.set_focus();
}

pub fn hide_control_window(window: &Window) {
    window.set_visible(false);
}

pub fn open_external_url(url: &str) -> PlatformResult<()> {
    Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn()?;
    Ok(())
}

pub fn current_system_wallpaper_url() -> Option<String> {
    unsafe {
        let mut buffer = vec![0u16; 260];
        let ok = SystemParametersInfoW(
            SPI_GETDESKWALLPAPER,
            buffer.len() as u32,
            buffer.as_mut_ptr() as *mut _,
            0,
        );
        if ok == 0 {
            return None;
        }

        let path = String::from_utf16_lossy(&buffer)
            .trim_end_matches('\0')
            .to_string();
        if path.is_empty() {
            None
        } else {
            Some(format!("file:///{}", path.replace('\\', "/")))
        }
    }
}

pub fn ui_platform_info() -> UiPlatformInfo {
    UiPlatformInfo {
        version_label: "AIWallpaper v1.1.0-STABLE · Windows".to_string(),
        architecture_lines: vec![
            "1. 基于 Rust 高性能异步后端".to_string(),
            "2. Win32 消息钩子 (0x052C) 桌面注入".to_string(),
            "3. WebView2 GPU 加速渲染层 (Windows)".to_string(),
        ],
    }
}
