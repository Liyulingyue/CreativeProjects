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
    // Prompt Enhance (文生文) 配置
    #[serde(default)]
    pub pe_url: String,
    #[serde(default)]
    pub pe_key: String,
    #[serde(default)]
    pub pe_model: String,
    #[serde(default = "default_image_size")]
    pub image_size: String,
    #[serde(default = "default_ui_mode")]
    pub ui_mode: String,
}

fn default_image_size() -> String {
    "1024x1024".to_string()
}

fn default_ui_mode() -> String {
    "lite".to_string()
}

#[derive(Serialize, Deserialize, Debug)]
pub struct IpcMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub data: Option<String>, // 用于传递 Base64 图片数据
    #[serde(default)]
    pub set_as_wallpaper: Option<bool>, // 是否设为壁纸
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
    ToggleDrawing(bool),
    PromptEnhanced(String),
}