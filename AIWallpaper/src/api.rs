use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::error::Error;

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

pub async fn generate_image(prompt: &str, api_key: &str) -> Result<String, Box<dyn Error>> {
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
            return Ok(data.url.clone());
        }
    } else {
        let err_text = response.text().await?;
        return Err(format!("API Error: {}", err_text).into());
    }

    Err("No image data found in response".into())
}
