use crate::types::{ChatCompletionRequest, ChatCompletionResponse, OpenAIClient};

pub struct OpenAIClientBuilder {
    api_key: String,
    base_url: String,
}

impl OpenAIClientBuilder {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: "https://api.openai.com".to_string(),
        }
    }

    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn build(self) -> ReqwestOpenAIClient {
        ReqwestOpenAIClient {
            api_key: self.api_key,
            base_url: self.base_url,
        }
    }
}

#[derive(Clone)]
pub struct ReqwestOpenAIClient {
    api_key: String,
    base_url: String,
}

impl OpenAIClient for ReqwestOpenAIClient {
    fn chat_completions_create(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, String> {
        let client = reqwest::blocking::Client::new();
        let url = format!("{}/v1/chat/completions", self.base_url);

        let body = serde_json::to_string(&request)
            .map_err(|e| format!("Failed to serialize request: {}", e))?;

        let req_builder = client.post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .body(body);

        let response = req_builder.send()
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        let status = response.status();
        let body = response.text()
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if !status.is_success() {
            return Err(format!("API error {}: {}", status, body));
        }

        serde_json::from_str(&body)
            .map_err(|e| format!("Failed to parse response: {}", e))
    }
}
