use serde::{Deserialize, Serialize};

pub mod ipc;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    pub hotkey: String,
    pub asr_model: String,
    pub language: String,
    pub always_on_top: bool,
    pub server_port: u16,
    pub audio_device: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: "F13".to_string(),
            asr_model: "base".to_string(),
            language: "auto".to_string(),
            always_on_top: true,
            server_port: 18789,
            audio_device: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct IpcMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Clone)]
pub enum AppEvent {
    RecordingStarted,
    RecordingStopped,
    AsrResult(String),
    AsrError(String),
    OpenSettings,
    SetAlwaysOnTop(bool),
    QuitApp,
    PasteText(String),
    ToggleMainWindow,
    StartDrag,
    ShowMainWindow,
    AudioDevices(Vec<(String, String)>),
    AudioLevel(f32),
}
