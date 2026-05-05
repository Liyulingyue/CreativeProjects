use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use tao::event_loop::EventLoopProxy;
use crate::app::{AppConfig, AppEvent, IpcMessage};
use crate::asr::AsrClient;
use crate::audio::AudioRecorder;

pub struct IpcContext {
    pub config: Arc<Mutex<AppConfig>>,
    pub proxy: EventLoopProxy<AppEvent>,
    pub app_data_dir: PathBuf,
    pub is_recording: Arc<Mutex<bool>>,
    pub asr_client: Arc<Mutex<AsrClient>>,
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
                let language = ctx.config.lock().unwrap().language.clone();
                let url = format!("http://{}", ctx.config.lock().unwrap().server_url);
                eprintln!("[IPC Handler] Calling ASR at {}", url);

                match AsrClient::new(&url).transcribe(&audio_path, &language).await {
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
                    let mut cfg = ctx.config.lock().unwrap();
                    *cfg = new_cfg.clone();
                    let cfg_path = ctx.app_data_dir.join("config.json");
                    let _ = std::fs::write(cfg_path, serde_json::to_string_pretty(&*cfg).unwrap());

                    let port = new_cfg.server_url;
                    let always_on_top = new_cfg.always_on_top;
                    if let Some(ref device) = new_cfg.audio_device {
                        *ctx.selected_audio_device.lock().unwrap() = Some(device.clone());
                    }
                    drop(cfg);
                    *ctx.asr_client.lock().unwrap() = AsrClient::new(&format!("http://{}", port));

                    let _ = ctx.proxy.send_event(AppEvent::SetAlwaysOnTop(always_on_top));
                }
            }
            _ => {
                eprintln!("Unknown IPC: {}", msg.msg_type);
            }
        }
    }
}

