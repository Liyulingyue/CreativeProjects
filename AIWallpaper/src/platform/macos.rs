#![allow(unexpected_cfgs)]

use std::path::Path;
use std::process::Command;

use cocoa::{
    appkit::{NSApp, NSWindowCollectionBehavior},
    base::{id, nil},
    foundation::NSUInteger,
};
use objc::{msg_send, sel, sel_impl};
use tao::{
    event_loop::EventLoop,
    monitor::MonitorHandle,
    platform::macos::{ActivationPolicy, EventLoopExtMacOS, WindowBuilderExtMacOS, WindowExtMacOS},
    window::{Window, WindowBuilder},
};

use crate::platform::{AppPaths, PlatformResult, UiPlatformInfo};

const KCG_DESKTOP_WINDOW_LEVEL_KEY: i32 = 2;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowLevelForKey(key: i32) -> i32;
}

pub fn app_data_dir() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(|value| {
            std::path::PathBuf::from(value)
                .join("Library")
                .join("Application Support")
                .join("AIWallpaper")
        })
        .unwrap_or_else(|_| std::env::temp_dir().join("AIWallpaper"))
}

pub fn export_image_dir(app_data_dir: &Path) -> std::path::PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let pictures_dir = std::path::PathBuf::from(&home)
            .join("Pictures")
            .join("AIWallpaper");
        if pictures_dir.parent().is_some_and(|parent| parent.exists()) {
            return pictures_dir;
        }

        let downloads_dir = std::path::PathBuf::from(home)
            .join("Downloads")
            .join("AIWallpaper");
        if downloads_dir.parent().is_some_and(|parent| parent.exists()) {
            return downloads_dir;
        }
    }

    app_data_dir.join("exports")
}

pub fn configure_process(_paths: &AppPaths) -> PlatformResult<()> {
    Ok(())
}

pub fn configure_event_loop<T>(event_loop: &mut EventLoop<T>) {
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    event_loop.enable_default_menu_creation(false);
    event_loop.set_activate_ignoring_other_apps(true);
}

pub fn configure_background_window_builder(
    mut builder: WindowBuilder,
    primary_monitor: Option<MonitorHandle>,
) -> WindowBuilder {
    if let Some(monitor) = primary_monitor {
        builder = builder
            .with_position(monitor.position())
            .with_inner_size(monitor.size());
    }

    builder
        .with_decorations(false)
        .with_resizable(false)
        .with_transparent(true)
        .with_visible_on_all_workspaces(true)
        .with_has_shadow(false)
}

pub fn attach_background_window(window: &Window) -> PlatformResult<()> {
    window.set_visible_on_all_workspaces(true);
    window.set_ignore_cursor_events(true)?;

    unsafe {
        let ns_window = window.ns_window() as id;
        let desktop_level = CGWindowLevelForKey(KCG_DESKTOP_WINDOW_LEVEL_KEY);
        let _: () = msg_send![ns_window, setLevel: desktop_level];

        let collection_behavior: NSUInteger = msg_send![ns_window, collectionBehavior];
        let stationary_behavior =
            (NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
                | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
                | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary)
                .bits();
        let _: () = msg_send![
            ns_window,
            setCollectionBehavior: collection_behavior | stationary_behavior
        ];
    }

    window.set_visible(true);

    unsafe {
        let ns_window = window.ns_window() as id;
        let _: () = msg_send![ns_window, orderBack: nil];
    }

    Ok(())
}

pub fn reveal_control_window(window: &Window) {
    window.set_visible(true);

    unsafe {
        let app = NSApp();
        let _: () = msg_send![app, activateIgnoringOtherApps: true];
        let ns_window = window.ns_window() as id;
        let _: () = msg_send![ns_window, makeKeyAndOrderFront: nil];
    }

    window.set_focus();
}

pub fn hide_control_window(window: &Window) {
    window.set_visible(false);
}

pub fn open_external_url(url: &str) -> PlatformResult<()> {
    Command::new("open").arg(url).spawn()?;
    Ok(())
}

pub fn current_system_wallpaper_url() -> Option<String> {
    None
}

pub fn ui_platform_info() -> UiPlatformInfo {
    UiPlatformInfo {
        version_label: "AIWallpaper v1.1.0-STABLE · macOS".to_string(),
        architecture_lines: vec![
            "1. 基于 Rust 高性能异步后端".to_string(),
            "2. AppKit + WKWebView 桌面层渲染".to_string(),
            "3. 状态栏常驻 + 主屏桌面层 (macOS)".to_string(),
        ],
    }
}
