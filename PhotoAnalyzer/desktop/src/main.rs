#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;
use tauri::Manager;

struct ApiBaseState(Mutex<String>);

#[tauri::command]
fn get_api_base(state: tauri::State<ApiBaseState>) -> Result<String, String> {
    state
        .0
        .lock()
        .map(|api_base| api_base.clone())
        .map_err(|_| "读取 API 基址失败".to_string())
}

fn main() {
    tauri::Builder::default()
        .manage(ApiBaseState(Mutex::new("http://127.0.0.1:8001/api".to_string())))
        .invoke_handler(tauri::generate_handler![get_api_base])
        .setup(|app| {
            let ready = photo_analyzer::spawn_server(None, false);

            match ready.recv_timeout(std::time::Duration::from_secs(10)) {
                Ok(Ok(port)) => {
                    let state = app.state::<ApiBaseState>();
                    let lock_result = state.0.lock();
                    match lock_result {
                        Ok(mut guard) => {
                            *guard = format!("http://127.0.0.1:{port}/api");
                            Ok(())
                        }
                        Err(_) => Err(std::io::Error::other("写入 API 基址失败").into()),
                    }
                }
                Ok(Err(message)) => Err(std::io::Error::other(message).into()),
                Err(_) => Err(std::io::Error::other("后端启动超时").into()),
            }
        })
        .build(tauri::generate_context!())
        .expect("Tauri app build failed")
        .run(|_, _| {});
}