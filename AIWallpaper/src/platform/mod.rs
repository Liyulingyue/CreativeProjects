use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tao::{
    event_loop::EventLoop,
    monitor::MonitorHandle,
    window::{Window, WindowBuilder},
};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(target_os = "windows")]
use windows as imp;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
compile_error!("AIWallpaper currently supports Windows and macOS only.");

pub type PlatformResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone)]
pub struct AppPaths {
    pub app_data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub config_path: PathBuf,
    pub current_wallpaper_path: PathBuf,
    pub webview_data_dir: PathBuf,
}

#[derive(Serialize)]
pub struct UiPlatformInfo {
    pub version_label: String,
    pub architecture_lines: Vec<String>,
}

pub fn app_paths() -> AppPaths {
    let app_data_dir = imp::app_data_dir();
    let cache_dir = app_data_dir.join("cache");

    AppPaths {
        config_path: app_data_dir.join("config.json"),
        current_wallpaper_path: cache_dir.join("current_wallpaper.png"),
        webview_data_dir: app_data_dir.join("WebView2"),
        app_data_dir,
        cache_dir,
    }
}

pub fn export_image_dir(app_data_dir: &Path) -> PathBuf {
    imp::export_image_dir(app_data_dir)
}

pub fn configure_process(paths: &AppPaths) -> PlatformResult<()> {
    imp::configure_process(paths)
}

pub fn configure_event_loop<T>(event_loop: &mut EventLoop<T>) {
    imp::configure_event_loop(event_loop)
}

pub fn configure_background_window_builder(
    builder: WindowBuilder,
    primary_monitor: Option<MonitorHandle>,
) -> WindowBuilder {
    imp::configure_background_window_builder(builder, primary_monitor)
}

pub fn attach_background_window(window: &Window) -> PlatformResult<()> {
    imp::attach_background_window(window)
}

pub fn reveal_control_window(window: &Window) {
    imp::reveal_control_window(window)
}

pub fn hide_control_window(window: &Window) {
    imp::hide_control_window(window)
}

pub fn open_external_url(url: &str) -> PlatformResult<()> {
    imp::open_external_url(url)
}

pub fn initial_wallpaper_url(current_wallpaper_path: &Path) -> Option<String> {
    if current_wallpaper_path.exists() {
        Some(preview_image_url(
            "current_wallpaper.png",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_millis(),
        ))
    } else {
        imp::current_system_wallpaper_url()
    }
}

pub fn preview_image_url(file_name: &str, cache_buster: u128) -> String {
    format!("aiwallpaper://localhost/{file_name}?ts={cache_buster}")
}

pub fn path_to_file_url(path: &Path) -> Option<String> {
    let absolute = fs::canonicalize(path).ok()?;
    let normalized = absolute.to_string_lossy().replace('\\', "/");
    let encoded = percent_encode_file_path(&normalized);

    if encoded.starts_with('/') {
        Some(format!("file://{encoded}"))
    } else {
        Some(format!("file:///{encoded}"))
    }
}

pub fn ui_platform_info() -> UiPlatformInfo {
    imp::ui_platform_info()
}

fn percent_encode_file_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());

    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }

    encoded
}
