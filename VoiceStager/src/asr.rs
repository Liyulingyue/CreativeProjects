use std::path::Path;

pub struct AsrClient {
            url: String,
        }

impl AsrClient {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
        }
    }

    pub async fn transcribe(&self, audio_path: &Path, language: &str) -> Result<String, String> {
        let bytes = std::fs::read(audio_path).map_err(|e| format!("read wav: {}", e))?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| format!("http client: {}", e))?;

        let mut req = client.post(format!("{}/transcribe", self.url));
        req = req.header("Content-Type", "audio/wav");
        if !language.is_empty() && language != "auto" {
            req = req.header("X-Language", language);
        }
        req = req.body(bytes);

        let resp = req.send().await.map_err(|e| format!("request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Server error {}: {}", status, body));
        }

        #[derive(serde::Deserialize)]
        struct Resp {
            text: String,
        }

        let result: Resp = resp.json().await.map_err(|e| format!("parse response: {}", e))?;
        Ok(result.text)
    }

    #[allow(dead_code)]
    pub async fn health_check(&self) -> bool {
        match reqwest::get(format!("{}/health", self.url)).await {
            Ok(r) => r.status().is_success(),
            Err(_) => false,
        }
    }
}
