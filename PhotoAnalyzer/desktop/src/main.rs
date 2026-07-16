#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use base64::Engine;
use std::sync::Mutex;
use tauri::Manager;

struct ApiBaseState(Mutex<String>);
struct InProcessApiState(Mutex<Option<photo_analyzer::InProcessApi>>);

#[tauri::command]
fn get_api_base(state: tauri::State<ApiBaseState>) -> Result<String, String> {
    state
        .0
        .lock()
        .map(|api_base| api_base.clone())
        .map_err(|_| "读取 API 基址失败".to_string())
}

#[tauri::command]
fn api_request(
    state: tauri::State<InProcessApiState>,
    method: String,
    path: String,
    query: Option<String>,
    body: Option<String>,
) -> Result<(u16, String, Option<String>), String> {
    let guard = state
        .0
        .lock()
        .map_err(|_| "读取 API 运行态失败".to_string())?;
    let api = guard
        .as_ref()
        .ok_or_else(|| "当前运行模式不支持 in-process 请求".to_string())?;

    let uri = match query {
        Some(q) if !q.is_empty() => format!("{}?{}", path, q),
        _ => path,
    };
    let (status, bytes, content_type) = api.request(
        &method,
        &uri,
        body.map(|s| s.into_bytes()),
        Some("application/json"),
    )?;
    let body_text = String::from_utf8(bytes).unwrap_or_default();

    Ok((status, body_text, content_type))
}

#[tauri::command]
fn get_thumbnail_data_url(
    state: tauri::State<InProcessApiState>,
    path: String,
    full: bool,
) -> Result<String, String> {
    let guard = state
        .0
        .lock()
        .map_err(|_| "读取 API 运行态失败".to_string())?;
    let api = guard
        .as_ref()
        .ok_or_else(|| "当前运行模式不支持 in-process 缩略图".to_string())?;

    let mut query = format!("path={}", encode_uri_component(&path));
    if full {
        query.push_str("&full=1");
    }
    let (status, bytes, content_type) = api.request(
        "GET",
        &format!("/api/thumbnails?{}", query),
        None,
        None,
    )?;
    if !(200..300).contains(&status) {
        return Err(format!("读取缩略图失败: HTTP {status}"));
    }

    let mime = content_type.unwrap_or_else(|| "image/jpeg".to_string());
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{};base64,{}", mime, data))
}

fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

fn encode_uri_component(input: &str) -> String {
    let mut result = String::new();
    for c in input.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

fn main() {
    if std::env::args().any(|a| a == "--serve") {
        let args = photo_analyzer::CliArgs::parse();
        let rt = tokio::runtime::Runtime::new().expect("创建运行时失败");
        if let Err(error) = rt.block_on(photo_analyzer::run_server(&args.host, args.port, !args.no_open)) {
            eprintln!("[photo_analyzer] server error: {error}");
            std::process::exit(1);
        }
        return;
    }

    tauri::Builder::default()
        .manage(ApiBaseState(Mutex::new("inproc".to_string())))
        .manage(InProcessApiState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![get_api_base, api_request, get_thumbnail_data_url])
        .setup(|app| {
            if env_truthy("PHOTO_ANALYZER_EXPOSE_HTTP") {
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
            } else {
                let inprocess_state = app.state::<InProcessApiState>();
                let mut guard = inprocess_state
                    .0
                    .lock()
                    .map_err(|_| std::io::Error::other("初始化 in-process 状态失败"))?;
                *guard = Some(
                    photo_analyzer::InProcessApi::new()
                        .map_err(std::io::Error::other)?,
                );
                Ok(())
            }
        })
        .build(tauri::generate_context!())
        .expect("Tauri app build failed")
        .run(|_, _| {});
}