#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use tauri::path::BaseDirectory;
use tauri::{Manager, RunEvent};

struct BackendState(Mutex<Option<Child>>);

fn backend_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("rust")
        .join("Cargo.toml")
}

fn backend_resource_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "photo_analyzer_backend.exe"
    } else {
        "photo_analyzer_backend"
    }
}

fn backend_candidate_paths(app: &tauri::AppHandle) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(p) = app
        .path()
        .resolve(backend_resource_name(), BaseDirectory::Resource)
    {
        paths.push(p);
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Case 1: running from bundled app where backend sits next to exe.
            paths.push(exe_dir.join(backend_resource_name()));
            // Case 2: tauri build output where backend is copied to release/bin.
            paths.push(exe_dir.join("bin").join(backend_resource_name()));
            // Case 3: common Windows bundle resource layout.
            paths.push(exe_dir.join("resources").join(backend_resource_name()));
        }
    }

    paths
}

fn spawn_backend(app: &tauri::AppHandle) -> Result<Child, String> {
    #[cfg(debug_assertions)]
    {
        let manifest = backend_manifest_path();
        let mut cmd = Command::new("cargo");
        cmd.arg("run")
            .arg("--manifest-path")
            .arg(manifest)
            .arg("--features")
            .arg("embed-frontend")
            .env("PHOTO_ANALYZER_OPEN_BROWSER", "false")
            .env("PORT", "8001")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        return cmd.spawn().map_err(|e| format!("启动后端失败: {e}"));
    }

    #[cfg(not(debug_assertions))]
    {
        let candidates = backend_candidate_paths(app);
        let mut last_err: Option<String> = None;

        for backend_exe in candidates {
            if !backend_exe.exists() {
                continue;
            }

            let mut cmd = Command::new(&backend_exe);
            cmd.env("PHOTO_ANALYZER_OPEN_BROWSER", "false")
                .env("PORT", "8001")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());

            match cmd.spawn() {
                Ok(child) => return Ok(child),
                Err(e) => {
                    last_err = Some(format!("{} -> {}", backend_exe.display(), e));
                }
            }
        }

        return Err(last_err.unwrap_or_else(|| {
            "未找到后端可执行文件，请使用安装包产物或执行 build_tauri_release.ps1 重新打包".to_string()
        }));
    }
}

fn main() {
    tauri::Builder::default()
        .manage(BackendState(Mutex::new(None)))
        .setup(|app| {
            let child = match spawn_backend(app.handle()) {
                Ok(c) => Some(c),
                Err(e) => {
                    // Keep UI alive even if backend cannot be spawned.
                    // This avoids "double-click flash exit" and lets user see frontend/errors.
                    eprintln!("[tauri] backend spawn warning: {e}");
                    None
                }
            };
            let state = app.state::<BackendState>();
            let mut guard = state.0.lock().map_err(|_| "后端状态锁失败")?;
            *guard = child;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Tauri app build failed")
        .run(|app_handle, event| {
            if matches!(event, RunEvent::ExitRequested { .. }) {
                let state = app_handle.state::<BackendState>();
                let mut guard = match state.0.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                if let Some(mut child) = guard.take() {
                    let _ = child.kill();
                }
            }
        });
}
