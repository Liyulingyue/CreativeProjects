use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::platform;

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

pub async fn generate_image(
    prompt: &str,
    api_key: &str,
    cache_dir: &Path,
) -> Result<GeneratedImage, Box<dyn Error + Send + Sync>> {
    log::debug!(
        "[api] start_generate prompt_len={} cache_dir={} key_len={}",
        prompt.len(),
        cache_dir.to_string_lossy(),
        api_key.len()
    );
    let client = Client::new();
    let url = "https://aistudio.baidu.com/llm/lmapi/v3/images/generations";

    let payload = ErnieImageRequest {
        model: "ernie-image-turbo".to_string(),
        prompt: prompt.to_string(),
        n: 1,
        response_format: "url".to_string(),
        #[cfg(target_os = "macos")]
        size: "1264x848".to_string(),
        #[cfg(target_os = "windows")]
        size: "1024x1024".to_string(),
    };

    let response = client
        .post(url)
        .header("Authorization", format!("bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?;
    log::debug!("[api] generation_status={}", response.status());

    if response.status().is_success() {
        let res_body: ErnieImageResponse = response.json().await?;
        log::debug!("[api] generation_data_count={}", res_body.data.len());
        if let Some(data) = res_body.data.first() {
            log::debug!("[api] download_url={}", data.url);
            let img_bytes = client.get(&data.url).send().await?.bytes().await?;
            log::debug!("[api] downloaded_bytes={}", img_bytes.len());

            if !cache_dir.exists() {
                fs::create_dir_all(cache_dir)?;
            }

            let save_path = cache_dir.join("current_wallpaper.png");
            fs::write(&save_path, &img_bytes)?;
            let saved_len = fs::metadata(&save_path).map(|meta| meta.len()).unwrap_or(0);
            log::debug!(
                "[api] saved_file={} saved_bytes={}",
                save_path.to_string_lossy(),
                saved_len
            );

            let wallpaper_url =
                platform::path_to_file_url(&save_path).ok_or("Failed to resolve wallpaper path")?;
            let cache_buster = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let preview_url = platform::preview_image_url("current_wallpaper.png", cache_buster);

            return Ok(GeneratedImage {
                preview_url,
                wallpaper_url,
            });
        }
    } else {
        let err_text = response.text().await?;
        log::error!("[api] generation_error_body={err_text}");
        return Err(format!("API Error: {}", err_text).into());
    }

    Err("No image data found in response".into())
}
