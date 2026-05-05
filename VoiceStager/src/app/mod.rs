use serde::{Deserialize, Serialize};

pub mod ipc;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    pub hotkey: String,
    pub language: String,
    pub always_on_top: bool,
    pub server_url: String,
    pub audio_device: Option<String>,
    #[serde(default)]
    pub asr_mode: String, // "local" | "remote"
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: "F13".to_string(),
            language: "auto".to_string(),
            always_on_top: true,
            server_url: "http://127.0.0.1:18789".to_string(),
            audio_device: None,
            asr_mode: "local".to_string(),
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
    ShowNativeMenu {
        client_x: f64,
        client_y: f64,
        text: String,
        has_text: bool,
        is_recording: bool,
        is_processing: bool,
    },
    ShowMainWindow,
    AudioDevices(Vec<(String, String)>),
    AudioLevel(f32),
}
