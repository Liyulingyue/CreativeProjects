use std::sync::{Arc, Mutex};
use std::fs;
use tao::event_loop::EventLoopProxy;
use crate::app::{AppConfig, IpcMessage, AppEvent};
use crate::api;
use crate::{export_image_dir, save_generated_image};

#[cfg(target_os = "windows")]
fn apply_auto_start(enabled: bool) {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const APP_NAME: &str = "AIWallpaper";
    if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE) {
        if enabled {
            if let Ok(exe) = std::env::current_exe() {
                let _ = key.set_value(APP_NAME, &exe.to_string_lossy().as_ref());
            }
        } else {
            let _ = key.delete_value(APP_NAME);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn apply_auto_start(_enabled: bool) {}

/// 读取当前注册表中是否有开机启动项
#[cfg(target_os = "windows")]
pub fn read_auto_start() -> bool {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const APP_NAME: &str = "AIWallpaper";
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(RUN_KEY, KEY_READ)
        .and_then(|key| key.get_value::<String, _>(APP_NAME))
        .is_ok()
}

#[cfg(not(target_os = "windows"))]
pub fn read_auto_start() -> bool { false }

/// 读取画廊目录第 page 页（从0开始），每页 page_size 张，返回分页 JSON
fn load_gallery_page(gallery_dir: &std::path::Path, page: usize, page_size: usize) -> serde_json::Value {
    let mut all_names: Vec<(String, std::path::PathBuf)> = Vec::new();
    if let Ok(entries) = fs::read_dir(gallery_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase();
                if ["png", "jpg", "jpeg", "webp"].contains(&ext.as_str()) {
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
                    all_names.push((file_name, path));
                }
            }
        }
    }
    all_names.sort_by(|a, b| b.0.cmp(&a.0));
    let total = all_names.len();
    let start = page * page_size;
    let mut images = Vec::new();
    for (file_name, path) in all_names.into_iter().skip(start).take(page_size) {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase();
        if let Ok(bytes) = fs::read(&path) {
            let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
            let mime = match ext.as_str() {
                "jpg" | "jpeg" => "image/jpeg",
                "webp" => "image/webp",
                _ => "image/png",
            };
            images.push(serde_json::json!({
                "name": file_name,
                "data": format!("data:{};base64,{}", mime, b64)
            }));
        }
    }
    serde_json::json!({
        "items": images,
        "total": total,
        "page": page,
        "page_size": page_size
    })
}

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
            "toggle_drawing" => {
                let enabled = msg.value == "true";
                let _ = ctx.proxy.send_event(AppEvent::ToggleDrawing(enabled));
            }
            "clear_drawing" => {
                // 这个指令通常直接透传给 bg_webview，
                // 但为了统一，我们也可以通过 AppEvent 路由
                let _ = ctx.proxy.send_event(AppEvent::ToggleDrawing(true)); // 暂时复用，或者加新事件
            }
            "switch_mode" => {
                // 持久化 ui_mode 到 config.json
                {
                    let mut cfg = ctx.config.lock().unwrap();
                    cfg.ui_mode = msg.value.clone();
                    let cfg_path = ctx.app_data_dir.join("config.json");
                    let _ = fs::write(cfg_path, serde_json::to_string(&*cfg).unwrap());
                }
                let _ = ctx.proxy.send_event(AppEvent::SwitchMode(msg.value));
            }
            "save_config" => {
                if let Ok(new_cfg) = serde_json::from_str::<AppConfig>(&msg.value) {
                    println!("收到新配置: {:?}", new_cfg);
                    apply_auto_start(new_cfg.auto_start);
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
            "import_image" => {
                let proxy = ctx.proxy.clone();
                let app_data_dir = ctx.app_data_dir.clone();
                let cfg = ctx.config.lock().unwrap().clone();
                tokio::spawn(async move {
                    if let Some(path) = rfd::AsyncFileDialog::new()
                        .add_filter("图片文件", &["png", "jpg", "jpeg", "webp"])
                        .set_title("选择要导入的壁纸")
                        .pick_file()
                        .await 
                    {
                        let src_path = path.path();
                        let export_dir = export_image_dir(&app_data_dir, &cfg);
                        let _ = fs::create_dir_all(&export_dir);

                        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                        let ext = src_path.extension().and_then(|e| e.to_str()).unwrap_or("png");
                        let target_path = export_dir.join(format!("imported-{}.{}", ts, ext));
                        
                        match fs::copy(src_path, &target_path) {
                            Ok(_) => {
                                // 1. 将导入的图片拷贝到预览缓存，使其立即生效作为当前壁纸
                                let cache_path = app_data_dir.join("cache").join("current_wallpaper.png");
                                let _ = fs::copy(&target_path, &cache_path);
                                
                                // 2. 通知所有 UI：图片已保存，且壁纸已更新
                                let _ = proxy.send_event(AppEvent::Saved(target_path.to_string_lossy().to_string()));
                                let _ = proxy.send_event(AppEvent::Generated(api::GeneratedImage {
                                    preview_url: "aiwallpaper://localhost/current_wallpaper.png".to_string(), 
                                    wallpaper_url: "".to_string(),
                                    size: "local".to_string(),
                                }));
                                let _ = proxy.send_event(AppEvent::Ready); 
                            }
                            Err(e) => {
                                let _ = proxy.send_event(AppEvent::Error(format!("导入失败: {}", e)));
                            }
                        }
                    }
                });
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
            "save_edited_image" => {
                let proxy = ctx.proxy.clone();
                let app_data_dir = ctx.app_data_dir.clone();
                let cfg = ctx.config.lock().unwrap().clone();
                
                // 打印调试信息，确认收到的消息内容
                println!("收到 save_edited_image 指令");
                
                let (base64_data, set_as_wallpaper) = if let Some(d) = msg.data.clone() {
                    (d, msg.set_as_wallpaper.unwrap_or(true))
                } else if !msg.value.is_empty() {
                    // 兼容前端 sendIpc: 对象参数被 JSON.stringify 后放入 value
                    // value 可能是纯 base64 字符串，也可能是 {"data":"...","set_as_wallpaper":true}
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&msg.value) {
                        let data = v
                            .get("data")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
                        let as_wallpaper = v
                            .get("set_as_wallpaper")
                            .and_then(|x| x.as_bool())
                            .or(msg.set_as_wallpaper)
                            .unwrap_or(true);
                        (data, as_wallpaper)
                    } else {
                        (msg.value.clone(), msg.set_as_wallpaper.unwrap_or(true))
                    }
                } else {
                    ("".to_string(), true)
                };
                
                tokio::spawn(async move {
                    if base64_data.is_empty() {
                        let _ = proxy.send_event(AppEvent::Error("保存失败：没有接收到图片数据".into()));
                        return;
                    }

                    // 1. 解码 Base64
                    let data = base64_data.split(',').last().unwrap_or(&base64_data);
                    let bytes = match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data) {
                        Ok(b) => b,
                        Err(e) => {
                            let _ = proxy.send_event(AppEvent::Error(format!("解码失败: {}", e)));
                            return;
                        }
                    };

                    // 2. 准备保存目录
                    let export_dir = export_image_dir(&app_data_dir, &cfg);
                    let _ = fs::create_dir_all(&export_dir);

                    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                    let target_path = export_dir.join(format!("edited-{}.png", ts));
                    
                    // 3. 写入文件
                    match fs::write(&target_path, &bytes) {
                        Ok(_) => {
                            if set_as_wallpaper {
                                // 4. 将编辑后的图片拷贝到预览缓存，并立即生效
                                let cache_path = app_data_dir.join("cache").join("current_wallpaper.png");
                                let _ = fs::write(&cache_path, &bytes);
                                
                                let _ = proxy.send_event(AppEvent::Generated(api::GeneratedImage {
                                    preview_url: "aiwallpaper://localhost/current_wallpaper.png".to_string(),
                                    wallpaper_url: "".to_string(),
                                    size: "local".to_string(),
                                }));
                            }

                            let _ = proxy.send_event(AppEvent::Saved(target_path.to_string_lossy().to_string()));
                            let _ = proxy.send_event(AppEvent::Ready); 
                        }
                        Err(e) => {
                            let _ = proxy.send_event(AppEvent::Error(format!("保存失败: {}", e)));
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
            "prompt_enhance" => {
                let proxy_pe = ctx.proxy.clone();
                let cfg = ctx.config.lock().unwrap().clone();
                let prompt = msg.value.clone();
                tokio::spawn(async move {
                    if cfg.pe_url.is_empty() || cfg.pe_model.is_empty() {
                        let _ = proxy_pe.send_event(AppEvent::Error("Prompt Enhance 未配置 URL 或 MODEL".to_string()));
                        return;
                    }
                    match api::enhance_prompt(&cfg.pe_url, &cfg.pe_key, &cfg.pe_model, &prompt).await {
                        Ok(enhanced) => {
                            let _ = proxy_pe.send_event(AppEvent::PromptEnhanced(enhanced));
                        }
                        Err(e) => {
                            let _ = proxy_pe.send_event(AppEvent::Error(format!("PE 失败: {}", e)));
                        }
                    }
                });
            }
            "get_gallery" => {
                let cfg = ctx.config.lock().unwrap().clone();
                let app_data_dir_gallery = ctx.app_data_dir.clone();
                let proxy_gallery = ctx.proxy.clone();
                // 解析分页参数，默认 page=0, page_size=20
                let (page, page_size) = if let Ok(v) = serde_json::from_str::<serde_json::Value>(&msg.value) {
                    let p = v["page"].as_u64().unwrap_or(0) as usize;
                    let ps = v["page_size"].as_u64().unwrap_or(20) as usize;
                    (p, ps.max(1).min(100))
                } else {
                    (0usize, 20usize)
                };
                tokio::spawn(async move {
                    let gallery_dir = export_image_dir(&app_data_dir_gallery, &cfg);
                    let payload = load_gallery_page(&gallery_dir, page, page_size);
                    let _ = proxy_gallery.send_event(AppEvent::GalleryLoaded(payload));
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
                    let payload = load_gallery_page(&gallery_dir, 0, 20);
                    let _ = proxy_del.send_event(AppEvent::GalleryLoaded(payload));
                });
            }
            "delete_images" => {
                // msg.value expected to be a JSON array of filenames
                if let Ok(file_list) = serde_json::from_str::<Vec<String>>(&msg.value) {
                    let cfg = ctx.config.lock().unwrap().clone();
                    let app_data_dir_del = ctx.app_data_dir.clone();
                    let proxy_del = ctx.proxy.clone();
                    tokio::spawn(async move {
                        let gallery_dir = export_image_dir(&app_data_dir_del, &cfg);
                        for fname in file_list.iter() {
                            let _ = fs::remove_file(gallery_dir.join(fname));
                        }
                        let payload = load_gallery_page(&gallery_dir, 0, 20);
                        let _ = proxy_del.send_event(AppEvent::GalleryLoaded(payload));
                    });
                } else {
                    eprintln!("delete_images payload parse error: {}", msg.value);
                }
            }
            _ => {
                eprintln!("收到未知的 IPC 指令: {}", msg.msg_type);
            }
        }
    }
}
