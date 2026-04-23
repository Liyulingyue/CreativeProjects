use std::sync::{Arc, Mutex};
use std::fs;
use tao::event_loop::EventLoopProxy;
use crate::app::{AppConfig, IpcMessage, AppEvent};
use crate::api;
use crate::{export_image_dir, save_generated_image};

pub struct IpcContext {
    pub config: Arc<Mutex<AppConfig>>,
    pub proxy: EventLoopProxy<AppEvent>,
    pub app_data_dir: std::path::PathBuf,
}

pub async fn handle_message(msg_raw: &str, ctx: &IpcContext) {
    if let Ok(msg) = serde_json::from_str::<IpcMessage>(msg_raw) {
        match msg.msg_type.as_str() {
            "ready" => {
                let _ = ctx.proxy.send_event(AppEvent::Ready);
            }
            "minimize" => {
                let _ = ctx.proxy.send_event(AppEvent::Minimize);
            }
            "close" => {
                let _ = ctx.proxy.send_event(AppEvent::Close);
            }
            "switch_mode" => {
                let _ = ctx.proxy.send_event(AppEvent::SwitchMode(msg.value));
            }
            "save_config" => {
                if let Ok(new_cfg) = serde_json::from_str::<AppConfig>(&msg.value) {
                    println!("收到新配置: {:?}", new_cfg);
                    let mut cfg = ctx.config.lock().unwrap();
                    *cfg = new_cfg;
                    let cfg_path = ctx.app_data_dir.join("config.json");
                    let _ = fs::write(cfg_path, serde_json::to_string(&*cfg).unwrap());
                    println!("配置已保存并应用到内存");
                } else {
                    eprintln!("配置解析失败，原始值: {}", msg.value);
                }
            }
            "save_key" => {
                let mut cfg = ctx.config.lock().unwrap();
                cfg.api_key = msg.value.clone();
                let cfg_path = ctx.app_data_dir.join("config.json");
                let _ = fs::write(cfg_path, serde_json::to_string(&*cfg).unwrap());
            }
            "save_image" => {
                let app_data_dir_for_save = ctx.app_data_dir.clone();
                let proxy_for_save = ctx.proxy.clone();
                let cfg = ctx.config.lock().unwrap().clone();
                tokio::spawn(async move {
                    match save_generated_image(&app_data_dir_for_save, &cfg) {
                        Ok(path_buf) => {
                            let path_str = path_buf.to_string_lossy().to_string();
                            let _ = proxy_for_save.send_event(AppEvent::Saved(path_str));
                        }
                        Err(e) => {
                            let _ = proxy_for_save.send_event(AppEvent::Error(e.to_string()));
                        }
                    }
                });
            }
            "generate" => {
                let current_key = ctx.config.lock().unwrap().api_key.clone();
                // 优先使用指令携带的 size，如果没有则默认使用 "auto" 执行识别逻辑
                let requested_size = msg.size.clone().unwrap_or_else(|| "auto".to_string());
                
                let proxy_gen = ctx.proxy.clone();
                let prompt = msg.value.clone();
                let app_data_dir_gen = ctx.app_data_dir.clone();
                let cfg = ctx.config.lock().unwrap().clone();
                tokio::spawn(async move {
                    if current_key.is_empty() {
                        let _ = proxy_gen.send_event(AppEvent::Error("API Key 为空".into()));
                        return;
                    }
                    match api::generate_image(&prompt, &current_key, Some(requested_size)).await {
                        Ok(image) => {
                            // 自动保存到缓存供预览
                            let _ = save_generated_image(&app_data_dir_gen, &cfg);
                            let _ = proxy_gen.send_event(AppEvent::Generated(image));
                        }
                        Err(e) => {
                            let _ = proxy_gen.send_event(AppEvent::Error(e.to_string()));
                        }
                    }
                });
            }
            "get_gallery" => {
                let cfg = ctx.config.lock().unwrap().clone();
                let app_data_dir_gallery = ctx.app_data_dir.clone();
                let proxy_gallery = ctx.proxy.clone();
                tokio::spawn(async move {
                    let gallery_dir = export_image_dir(&app_data_dir_gallery, &cfg);
                    let mut images = Vec::new();
                    if let Ok(entries) = fs::read_dir(gallery_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.is_file() && path.extension().is_some_and(|ext| ext == "png") {
                                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
                                if let Ok(bytes) = fs::read(&path) {
                                    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
                                    images.push(serde_json::json!({
                                        "name": file_name,
                                        "data": format!("data:image/png;base64,{}", b64)
                                    }));
                                }
                            }
                        }
                    }
                    images.sort_by(|a, b| b["name"].as_str().cmp(&a["name"].as_str())); 
                    let _ = proxy_gallery.send_event(AppEvent::GalleryLoaded(images));
                });
            }
            "set_wallpaper" => {
                let file_name = msg.value.clone();
                let cfg = ctx.config.lock().unwrap().clone();
                let app_data_dir_set = ctx.app_data_dir.clone();
                let proxy_set = ctx.proxy.clone();
                tokio::spawn(async move {
                    let gallery_dir = export_image_dir(&app_data_dir_set, &cfg);
                    let img_path = gallery_dir.join(&file_name);
                    if img_path.exists() {
                        let cache_path = app_data_dir_set.join("cache").join("current_wallpaper.png");
                        match fs::copy(&img_path, &cache_path) {
                            Ok(_) => {
                                let _ = proxy_set.send_event(AppEvent::Generated(api::GeneratedImage {
                                    preview_url: "aiwallpaper://localhost/current_wallpaper.png".to_string(),
                                    wallpaper_url: "".to_string(),
                                    size: "local".to_string(),
                                }));
                            }
                            Err(e) => {
                                let _ = proxy_set.send_event(AppEvent::Error(format!("应用壁纸失败: {}", e)));
                            }
                        }
                    } else {
                        let _ = proxy_set.send_event(AppEvent::Error(format!("找不到图片文件: {}", file_name)));
                    }
                });
            }
            "delete_image" => {
                let file_name = msg.value.clone();
                let cfg = ctx.config.lock().unwrap().clone();
                let app_data_dir_del = ctx.app_data_dir.clone();
                let proxy_del = ctx.proxy.clone();
                tokio::spawn(async move {
                    let gallery_dir = export_image_dir(&app_data_dir_del, &cfg);
                    let img_path = gallery_dir.join(&file_name);
                    let _ = fs::remove_file(&img_path);
                    // 删除后重新加载画廊
                    let mut images = Vec::new();
                    if let Ok(entries) = fs::read_dir(&gallery_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.is_file() && path.extension().is_some_and(|ext| ext == "png") {
                                let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
                                if let Ok(bytes) = fs::read(&path) {
                                    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
                                    images.push(serde_json::json!({
                                        "name": fname,
                                        "data": format!("data:image/png;base64,{}", b64)
                                    }));
                                }
                            }
                        }
                    }
                    images.sort_by(|a, b| b["name"].as_str().cmp(&a["name"].as_str()));
                    let _ = proxy_del.send_event(AppEvent::GalleryLoaded(images));
                });
            }
            _ => {
                eprintln!("收到未知的 IPC 指令: {}", msg.msg_type);
            }
        }
    }
}
