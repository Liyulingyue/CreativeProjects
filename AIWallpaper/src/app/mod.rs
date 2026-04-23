use serde::{Deserialize, Serialize};

pub mod protocol;
pub mod ipc;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    pub api_key: String,
    pub enable_cache: bool,
    pub cache_limit: u32,
    pub auto_refresh_hours: u32,
    pub auto_prompt: String,
    pub gallery_path: String,
    #[serde(default = "default_image_size")]
    pub image_size: String,
}

fn default_image_size() -> String {
    "1024x1024".to_string()
}

#[derive(Serialize, Deserialize, Debug)]
pub struct IpcMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub size: Option<String>,
}

pub enum AppEvent {
    Ready,
    Minimize,
    Close,
    Generated(crate::api::GeneratedImage),
    Saved(String),
    Error(String),
    SwitchMode(String),
    GalleryLoaded(Vec<serde_json::Value>),
}