use std::path::PathBuf;
use tao::window::WindowBuilder;
use tao::event_loop::EventLoopWindowTarget;

pub struct PlatformPaths {
    pub work_dir: PathBuf,
    pub bg_dir: PathBuf,
    pub cache_dir: PathBuf,
}

pub fn get_platform_paths() -> PlatformPaths {
    let work_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    
    // 统一使用 LocalAppData 目录
    let app_data_base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| work_dir.clone())
        .join("AIWallpaper");

    let bg_dir = app_data_base.join("bg");
    let cache_dir = app_data_base.join("cache");
    
    if !bg_dir.exists() { let _ = std::fs::create_dir_all(&bg_dir); }
    if !cache_dir.exists() { let _ = std::fs::create_dir_all(&cache_dir); }
    
    PlatformPaths { work_dir: app_data_base, bg_dir, cache_dir }
}

pub fn app_data_dir() -> PathBuf {
    get_platform_paths().work_dir
}

pub fn current_system_wallpaper_url() -> String {
    "aiwallpaper://localhost/bg/wallpaper.png".to_string()
}

pub fn configure_process(_paths: &crate::platform::AppPaths) -> crate::platform::PlatformResult<()> {
    Ok(())
}

pub fn configure_event_loop<T>(_event_loop: &EventLoopWindowTarget<T>) {
}

pub fn configure_background_window_builder(builder: WindowBuilder, _monitor: Option<tao::monitor::MonitorHandle>) -> WindowBuilder {
    builder
        .with_decorations(false)
        .with_transparent(true)
}

pub fn attach_background_window(_window: &tao::window::Window) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

pub fn get_ui_platform_info<T>(_event_loop: &EventLoopWindowTarget<T>) -> crate::platform::UiPlatformInfo {
    crate::platform::UiPlatformInfo {
        device_pixel_ratio: 1.0,
    }
}

pub fn open_link(url: &str) {
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", url])
        .spawn();
}