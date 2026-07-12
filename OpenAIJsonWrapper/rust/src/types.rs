use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentPart {
    Text {
        #[serde(rename = "type")]
        part_type: String,
        text: String,
    },
    ImageUrl {
        #[serde(rename = "type")]
        part_type: String,
        image_url: ImageUrlValue,
    },
    ImagePath {
        #[serde(rename = "type")]
        part_type: String,
        image_path: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ImageUrlValue {
    String(String),
    Object {
        url: String,
        detail: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    String(String),
    Array(Vec<ContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResult {
    pub reasoning: String,
    pub data: Option<Value>,
    pub error: Option<String>,
    pub raw_content: String,
    pub response_id: Option<String>,
}

pub struct OpenAIJsonWrapperOptions {
    pub client: Box<dyn OpenAIClient>,
    pub model: String,
    pub target_structure: Option<Value>,
    pub requirements: Vec<String>,
    pub background: Option<String>,
}

pub trait OpenAIClient: Send + Sync {
    fn chat_completions_create(&self, request: ChatCompletionRequest) -> Result<ChatCompletionResponse, String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub message: ChoiceMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoiceMessage {
    pub content: Option<String>,
}

pub const TOOL_MARKER_START: &str = "```json";
pub const TOOL_MARKER_END: &str = "```";
