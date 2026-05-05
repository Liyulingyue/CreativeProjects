use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use tao::event_loop::EventLoopProxy;
use crate::app::{AppConfig, AppEvent, IpcMessage};
use crate::asr::AsrClient;
use crate::audio::AudioRecorder;
use tokio::sync::Mutex as TokioMutex;

fn normalize_server_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{}", trimmed)
    }
}

pub struct IpcContext {
    pub config: Arc<Mutex<AppConfig>>,
    pub proxy: EventLoopProxy<AppEvent>,
    pub app_data_dir: PathBuf,
    pub is_recording: Arc<Mutex<bool>>,
    pub asr_client: Arc<TokioMutex<AsrClient>>,
    pub selected_audio_device: Arc<Mutex<Option<String>>>,
    pub monitor_recorder: Arc<Mutex<Option<AudioRecorder>>>,
}

pub async fn handle_message(msg_raw: &str, ctx: &IpcContext) {
    eprintln!("[IPC Handler] handle_message: {}", msg_raw);
    if let Ok(msg) = serde_json::from_str::<IpcMessage>(msg_raw) {
        eprintln!("[IPC Handler] msg_type: {}", msg.msg_type);
        match msg.msg_type.as_str() {
            "recording_done" => {
                eprintln!("[IPC Handler] Recording done, calling ASR");
                let audio_path = ctx.app_data_dir.join("temp_recording.wav");
                if !audio_path.exists() {
                    eprintln!("[IPC Handler] Audio file not found: {:?}", audio_path);
                    let _ = ctx.proxy.send_event(AppEvent::AsrError("No audio file".to_string()));
                    return;
                }
                eprintln!("[IPC Handler] Audio file found: {:?}", audio_path);
                let (language, use_local) = {
                    let config = ctx.config.lock().unwrap();
                    (config.language.clone(), config.asr_mode == "local")
                };

                // 提前拿出所需参数，不在 await 跨越点持有 MutexGuard
                let result = ctx.asr_client.lock().await
                    .transcribe(&audio_path, &language, use_local).await;

                match result {
                    Ok(text) => {
                        eprintln!("[IPC Handler] ASR result: {}", text);
                        if text.is_empty() {
                            let _ = ctx.proxy.send_event(AppEvent::AsrError("No speech detected".to_string()));
                        } else {
                            let _ = ctx.proxy.send_event(AppEvent::AsrResult(text));
                        }
                    }
                    Err(e) => {
                        eprintln!("[IPC Handler] ASR error: {}", e);
                        let _ = ctx.proxy.send_event(AppEvent::AsrError(e));
                    }
                }
            }
            "recording_error" => {
                let _ = ctx.proxy.send_event(AppEvent::AsrError(msg.value));
            }
            "open_settings" => {
                let _ = ctx.proxy.send_event(AppEvent::OpenSettings);
            }
            "paste_text" => {
                let _ = ctx.proxy.send_event(AppEvent::PasteText(msg.value));
            }
            "start_drag" => {
                let _ = ctx.proxy.send_event(AppEvent::StartDrag);
            }
            "toggle_main_window" => {
                let _ = ctx.proxy.send_event(AppEvent::ToggleMainWindow);
            }
            "show_native_menu" => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&msg.value) {
                    let client_x = value.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let client_y = value.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let text = value
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let has_text = value.get("hasText").and_then(|v| v.as_bool()).unwrap_or(false);
                    let is_recording = value
                        .get("isRecording")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let is_processing = value
                        .get("isProcessing")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let _ = ctx.proxy.send_event(AppEvent::ShowNativeMenu {
                        client_x,
                        client_y,
                        text,
                        has_text,
                        is_recording,
                        is_processing,
                    });
                }
            }
            "start_audio_monitoring" => {
                let mut guard = ctx.monitor_recorder.lock().unwrap();
                if guard.is_none() {
                    let mut recorder = AudioRecorder::new();
                    let device_id = ctx.selected_audio_device.lock().unwrap().clone();
                    recorder.select_device(device_id);
                    let proxy = ctx.proxy.clone();
                    recorder.set_level_callback(Box::new(move |level| {
                        let _ = proxy.send_event(AppEvent::AudioLevel(level));
                    }));
                    if let Ok(_) = recorder.start_monitoring() {
                        *guard = Some(recorder);
                    }
                }
            }
            "stop_audio_monitoring" => {
                let mut guard = ctx.monitor_recorder.lock().unwrap();
                if let Some(mut recorder) = guard.take() {
                    recorder.stop_monitoring();
                }
            }
            "get_audio_devices" => {
                let devices = AudioRecorder::list_devices();
                eprintln!("[IPC Handler] Found {} audio devices", devices.len());
                let _ = ctx.proxy.send_event(AppEvent::AudioDevices(devices));
            }
            "select_audio_device" => {
                let device_id = if msg.value.is_empty() { None } else { Some(msg.value.clone()) };
                *ctx.selected_audio_device.lock().unwrap() = device_id.clone();
                ctx.config.lock().unwrap().audio_device = device_id;
                eprintln!("[IPC Handler] Selected audio device: {:?}", msg.value);
            }
            "save_config" => {
                if let Ok(new_cfg) = serde_json::from_str::<AppConfig>(&msg.value) {
                    let server_url = new_cfg.server_url.clone();
                    let always_on_top = new_cfg.always_on_top;
                    let local_model = new_cfg.local_model.clone();
                    let audio_device = new_cfg.audio_device.clone();

                    {
                        let mut cfg = ctx.config.lock().unwrap();
                        *cfg = new_cfg;
                        let cfg_path = ctx.app_data_dir.join("config.json");
                        let _ = std::fs::write(cfg_path, serde_json::to_string_pretty(&*cfg).unwrap());
                        if let Some(ref device) = audio_device {
                            *ctx.selected_audio_device.lock().unwrap() = Some(device.clone());
                        }
                    }

                    let mut client = ctx.asr_client.lock().await;
                    client.http_provider = crate::asr::HttpAsrProvider::new(&normalize_server_url(&server_url));
                    client.set_local_model(&local_model);

                    let _ = ctx.proxy.send_event(AppEvent::SetAlwaysOnTop(always_on_top));
                }
            }
            _ => {
                eprintln!("Unknown IPC: {}", msg.msg_type);
            }
        }
    }
}

