use std::path::PathBuf;
use std::fs;
use std::borrow::Cow;
use wry::http::{header::CONTENT_TYPE, Response, StatusCode};
use include_dir::Dir;

pub fn asset_protocol_handler(
    request: &wry::http::Request<Vec<u8>>,
    _app_data_dir: &PathBuf,
    preview_path: &PathBuf,
    is_pro: bool,
    pro_dist: &Dir<'static>,
) -> Result<Response<Cow<'static, [u8]>>, Box<dyn std::error::Error>> {
    let uri = request.uri().to_string();
    let path = request.uri().path();

    // 1. 拦截预览图请求 (通用于所有窗口)
    if uri.contains("current_wallpaper.png") {
        return match fs::read(preview_path) {
            Ok(bytes) => Ok(Response::builder()
                .header(CONTENT_TYPE, "image/png")
                .header("Cache-Control", "no-cache, no-store, must-revalidate")
                .body(bytes.into())?),
            Err(_) => Ok(Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new().into())?),
        };
    }

    // 2. 拦截本地绝对路径请求 (格式: asset://local/C:/path/to/img.png)
    if uri.starts_with("asset://local/") {
        let local_path_encoded = uri.replace("asset://local/", "");
        let local_path = percent_encoding::percent_decode_str(&local_path_encoded).decode_utf8_lossy();
        return match fs::read(PathBuf::from(local_path.as_ref())) {
            Ok(bytes) => {
                let mime = if local_path.ends_with(".png") { "image/png" } 
                          else if local_path.ends_with(".jpg") || local_path.ends_with(".jpeg") { "image/jpeg" }
                          else { "application/octet-stream" };
                Ok(Response::builder().header(CONTENT_TYPE, mime).body(bytes.into())?)
            },
            Err(_) => Ok(Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new().into())?),
        };
    }

    // 3. 处理 Pro 版的前端资源加载 (仅限 Pro 窗口)
    if is_pro {
        let relative_path = if path == "/" || path == "" { "index.html" } else { path.trim_start_matches('/') };
        return match pro_dist.get_file(relative_path) {
            Some(file) => {
                let mime = if relative_path.ends_with(".js") { "application/javascript" }
                          else if relative_path.ends_with(".css") { "text/css" }
                          else if relative_path.ends_with(".html") { "text/html" }
                          else { "application/octet-stream" };
                Ok(Response::builder().header(CONTENT_TYPE, mime).body(file.contents().to_vec().into())?)
            }
            None => Ok(Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new().into())?),
        };
    }

    Ok(Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new().into())?)
}
