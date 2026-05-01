use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::error::Error;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct GeneratedImage {
    pub preview_url: String,
    pub wallpaper_url: String,
    pub size: String,
}

#[derive(Serialize)]
struct ErnieImageRequest {
    model: String,
    prompt: String,
    n: u32,
    response_format: String,
    size: String,
}

#[derive(Deserialize)]
struct ErnieImageData {
    url: String,
}

#[derive(Deserialize)]
struct ErnieImageResponse {
    data: Vec<ErnieImageData>,
}

/// 根据宽高比选择最合适的文心一言图片规格
fn select_best_size(width: u32, height: u32) -> &'static str {
    let ratio = width as f32 / height as f32;
    
    // 常见的屏幕比例判断
    if ratio > 1.7 { // 16:9 左右
        "1376x768"
    } else if ratio > 1.4 { // 3:2 左右
        "1264x848"
    } else if ratio > 1.3 { // 4:3 左右
        "1200x896"
    } else if ratio < 0.6 { // 9:16 左右 (竖屏)
        "768x1376"
    } else if ratio < 0.75 { // 2:3 左右 (竖屏)
        "848x1264"
    } else if ratio < 0.8 { // 3:4 左右 (竖屏)
        "896x1200"
    } else {
        "1024x1024" // 默认正方形
    }
}

pub async fn generate_image(prompt: &str, api_key: &str, size_override: Option<String>) -> Result<GeneratedImage, Box<dyn Error + Send + Sync>> {
    // 动态识别屏幕尺寸 (Windows 平台)
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    
    let best_size = if let Some(size) = size_override {
        if size == "auto" || size.is_empty() {
             // 只有当明确配置为 auto 时才执行自动探测
            let (screen_w, screen_h) = unsafe {
                (
                    GetSystemMetrics(SM_CXSCREEN) as u32,
                    GetSystemMetrics(SM_CYSCREEN) as u32
                )
            };
            let detected = select_best_size(screen_w, screen_h);
            println!("配置为自动模式，检测到屏幕 {}x{} -> 规格 {}", screen_w, screen_h, detected);
            detected.to_string()
        } else {
            // 否则尊重配置中的值（即使它已经被保存为 1024x1024）
            println!("使用配置中的图片规格: {}", size);
            size
        }
    } else {
        "1024x1024".to_string()
    };

    let client = Client::new();
    let url = "https://aistudio.baidu.com/llm/lmapi/v3/images/generations";

    let payload = ErnieImageRequest {
        model: "ernie-image-turbo".to_string(),
        prompt: prompt.to_string(),
        n: 1,
        response_format: "url".to_string(),
        size: best_size.clone(),
    };

    let response = client
        .post(url)
        .header("Authorization", format!("bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        let res_body: ErnieImageResponse = response.json().await?;
        if let Some(data) = res_body.data.first() {
            // 下载图片并保存为固定文件名，以实现覆盖更新
            let img_bytes = client.get(&data.url).send().await?.bytes().await?;
            
            // 使用绝对路径，始终保存到 AppData/AIWallpaper/cache/
            let cache_dir = std::env::var("LOCALAPPDATA")
                .map(|ld| std::path::PathBuf::from(ld).join("AIWallpaper").join("cache"))
                .unwrap_or_else(|_| std::env::temp_dir().join("AIWallpaper").join("cache"));
            if !cache_dir.exists() {
                fs::create_dir_all(&cache_dir)?;
            }
            
            let save_path = cache_dir.join("current_wallpaper.png");
            fs::write(&save_path, &img_bytes)?;

            let abs_path = fs::canonicalize(&save_path)?;
            let path_str = abs_path.to_string_lossy().replace('\\', "/");
            let wallpaper_url = format!("file:///{}", path_str);
            let cache_buster = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let preview_url = format!("https://aiwallpaper.preview/current_wallpaper.png?ts={}", cache_buster);

            return Ok(GeneratedImage {
                preview_url,
                wallpaper_url,
                size: best_size,
            });
        }
    } else {
        let err_text = response.text().await?;
        return Err(format!("API Error: {}", err_text).into());
    }

    Err("No image data found in response".into())
}

#[derive(Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

/// 使用 OpenAI 格式的接口对 prompt 进行增强（Prompt Enhance）
pub async fn enhance_prompt(url: &str, api_key: &str, model: &str, prompt: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    let client = Client::new();

    // 构造 messages，要求模型只返回增强后的提示词文本
    let messages = vec![
        serde_json::json!({"role":"system","content":"You are a prompt enhancement assistant. Given a short user prompt, rewrite and expand it into a detailed, descriptive prompt for image generation. Output only the improved prompt text without any extra commentary."}),
        serde_json::json!({"role":"user","content": prompt}),
    ];

    let body = serde_json::json!({
        "model": model,
        "messages": messages,
        "max_tokens": 512,
    });

    // 如果用户只填写到 /v1（或不包含 chat/completions），自动补全为 /chat/completions
    let base = url.trim_end_matches('/');
    let full_url = if base.contains("/chat") || base.contains("/completions") {
        base.to_string()
    } else {
        format!("{}/chat/completions", base)
    };

    // 标准 OpenAI 鉴权。注意：绝大多数兼容厂商要求 "Bearer " 前缀。
    // 如果遇到少数不带前缀的特殊厂商，可能需要将下行改为 .header("Authorization", api_key)
    let resp = client.post(&full_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let err_text = resp.text().await.unwrap_or_default();
        return Err(format!("PE API Error: {} (url: {})", err_text, full_url).into());
    }

    let json: ChatCompletionResponse = resp.json().await?;
    if let Some(choice) = json.choices.into_iter().next() {
        return Ok(choice.message.content);
    }

    Err("No completion returned".into())
}
