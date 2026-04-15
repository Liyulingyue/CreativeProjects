use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::error::Error;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct GeneratedImage {
    pub preview_url: String,
    pub wallpaper_url: String,
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

pub async fn generate_image(prompt: &str, api_key: &str) -> Result<GeneratedImage, Box<dyn Error + Send + Sync>> {
    let client = Client::new();
    let url = "https://aistudio.baidu.com/llm/lmapi/v3/images/generations";

    let payload = ErnieImageRequest {
        model: "ernie-image-turbo".to_string(),
        prompt: prompt.to_string(),
        n: 1,
        response_format: "url".to_string(),
        size: "1024x1024".to_string(), // 可根据显示器调整，规格支持 1024x1024 等
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
            });
        }
    } else {
        let err_text = response.text().await?;
        return Err(format!("API Error: {}", err_text).into());
    }

    Err("No image data found in response".into())
}
