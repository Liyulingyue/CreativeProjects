#![windows_subsystem = "windows"]

use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use std::thread;
use std::sync::mpsc as sync_mpsc;
use tokio::sync::mpsc as tokio_mpsc;
use include_dir::Dir;
use include_dir::include_dir as embed_dir;
use enigo::{Enigo, Key, Keyboard, Settings};
use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_NCLBUTTONDOWN};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;

use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
    dpi::LogicalSize,
    platform::windows::{WindowBuilderExtWindows, WindowExtWindows},
};
use wry::webview::WebViewBuilder;

use crate::app::{AppConfig, AppEvent};
use crate::audio::AudioRecorder;
use crate::asr::AsrClient;
use crate::app::ipc::IpcContext;

mod app;
mod audio;
mod asr;

static PRO_DIST: Dir<'static> = embed_dir!("$CARGO_MANIFEST_DIR/ui/dist");

fn load_window_icon() -> Result<tao::window::Icon, Box<dyn std::error::Error>> {
    let icon_bytes = include_bytes!("../assets/app_icon.png");
    let image = image::load_from_memory(icon_bytes)?.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    tao::window::Icon::from_rgba(rgba, width, height).map_err(|e| e.into())
}

fn serve_asset(path: &str) -> wry::http::Response<Cow<'static, [u8]>> {
    use wry::http::{header::CONTENT_TYPE, Response, StatusCode};
    let path = path.trim_start_matches('/');
    let (mime, data) = if path.is_empty() || path == "index.html" {
        PRO_DIST.get_file("index.html")
    } else {
        PRO_DIST.get_file(path)
    }
    .map(|file| {
        let mime = if path.ends_with(".js") {
            "application/javascript"
        } else if path.ends_with(".css") {
            "text/css"
        } else if path.ends_with(".html") {
            "text/html"
        } else if path.ends_with(".png") {
            "image/png"
        } else {
            "application/octet-stream"
        };
        (mime, file.contents())
    })
    .unwrap_or_else(|| ("text/html", b"<html><body><h1>404</h1></body></html>" as &[u8]));

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, mime)
        .body(Cow::Owned(data.to_vec()))
        .unwrap()
}

fn setup_hotkey(hotkey_str: &str, recording: Arc<Mutex<bool>>, tx: tokio_mpsc::Sender<String>) {
    use hotkey::Listener;

    let mut listener = Listener::new();

    let (mods, key_code) = match hotkey_str.to_uppercase().as_str() {
        "CTRL+SPACE" => (hotkey::modifiers::CONTROL, 0x20),
        "F13" => (0, 0x7C),
        "F14" => (0, 0x7D),
        "F15" => (0, 0x7E),
        "F16" => (0, 0x7F),
        "F17" => (0, 0x80),
        "F18" => (0, 0x81),
        "F19" => (0, 0x82),
        "F20" => (0, 0x83),
        "F21" => (0, 0x84),
        "F22" => (0, 0x85),
        "F23" => (0, 0x86),
        "F24" => (0, 0x87),
        _ => {
            eprintln!("Unsupported hotkey: {}. Using F13.", hotkey_str);
            (0, 0x7C)
        }
    };

    let rec = recording.clone();
    match listener.register_hotkey(mods, key_code, move || {
        let state = *rec.lock().unwrap();
        let msg = serde_json::json!({"type": if state { "stop_recording" } else { "start_recording" }}).to_string();
        let _ = tx.blocking_send(msg);
    }) {
        Ok(id) => println!("Hotkey registered (id={}, key={:#x})", id, key_code),
        Err(e) => eprintln!("Failed to register hotkey: {}", e),
    }

    listener.listen();
}

fn build_webview(
    window: tao::window::Window,
    dev_mode: bool,
    dev_url: &str,
    prod_url: &str,
    tx: tokio_mpsc::Sender<String>,
) -> Result<wry::webview::WebView, Box<dyn std::error::Error>> {
    let webview = if dev_mode {
        println!("DEV mode: connecting to {}", dev_url);
        WebViewBuilder::new(window)?
            .with_url(dev_url)?
            .with_ipc_handler(move |_window, request| {
                let _ = tx.try_send(request);
            })
            .with_initialization_script(
                r#"window.ipc = { postMessage: function(m) { window._ipc_post && window._ipc_post(m); } };"#
            )
            .build()?
    } else {
        WebViewBuilder::new(window)?
            .with_custom_protocol("vstage".into(), move |request| {
                Ok(serve_asset(request.uri().path()))
            })
            .with_url(prod_url)?
            .with_ipc_handler(move |_window, request| {
                let _ = tx.try_send(request);
            })
            .with_initialization_script(
                r#"window.ipc = { postMessage: function(m) { window._ipc_post && window._ipc_post(m); } };"#
            )
            .build()?
    };
    Ok(webview)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let app_data_dir = std::env::var("LOCALAPPDATA")
        .map(|ld| std::path::PathBuf::from(ld).join("VStage"))
        .unwrap_or_else(|_| std::env::temp_dir().join("VStage"));

    if !app_data_dir.exists() {
        std::fs::create_dir_all(&app_data_dir)?;
    }

    let config_path = app_data_dir.join("config.json");
    let initial_config = if let Ok(content) = std::fs::read_to_string(&config_path) {
        serde_json::from_str::<AppConfig>(&content).unwrap_or_default()
    } else {
        AppConfig::default()
    };

    let config = Arc::new(Mutex::new(initial_config.clone()));
    let is_recording = Arc::new(Mutex::new(false));
    let asr_client = Arc::new(Mutex::new(AsrClient::new(&format!(
        "http://127.0.0.1:{}",
        initial_config.server_port
    ))));
    let selected_audio_device = Arc::new(Mutex::new(initial_config.audio_device.clone()));

    let (audio_cmd_tx, audio_cmd_rx) = sync_mpsc::channel::<AudioCommand>();

    let dev_mode = std::env::var("DEV_MODE").unwrap_or_default() == "1";
    let dev_url = std::env::var("DEV_URL").unwrap_or_else(|_| "http://localhost:5173".into());

    let event_loop = EventLoop::<AppEvent>::with_user_event();
    let window_icon = load_window_icon().ok();

    let main_window = WindowBuilder::new()
        .with_title("V-Stage")
        .with_inner_size(LogicalSize::new(400.0, 56.0))
        .with_min_inner_size(LogicalSize::new(360.0, 56.0))
        .with_max_inner_size(LogicalSize::new(600.0, 56.0))
        .with_decorations(false)
        .with_always_on_top(initial_config.always_on_top)
        .with_window_icon(window_icon.clone())
        .with_skip_taskbar(true)
        .build(&event_loop)?;

    let settings_window = WindowBuilder::new()
        .with_title("V-Stage Settings")
        .with_inner_size(LogicalSize::new(440.0, 420.0))
        .with_resizable(false)
        .with_visible(false)
        .with_window_icon(window_icon)
        .build(&event_loop)?;

    let tray_icon = tray_item::IconSource::Resource("#1");
    let proxy = event_loop.create_proxy();
    let proxy2 = proxy.clone();

    let mut tray = tray_item::TrayItem::new("V-Stage", tray_icon)?;
    let proxy_settings = proxy.clone();
    let proxy_toggle = proxy.clone();
    tray.add_menu_item("Show/Hide", move || {
        let _ = proxy_toggle.send_event(AppEvent::ToggleMainWindow);
    })?;
    tray.add_menu_item("Settings", move || {
        let _ = proxy_settings.send_event(AppEvent::OpenSettings);
    })?;
    tray.add_menu_item("Exit", move || {
        let _ = proxy2.send_event(AppEvent::QuitApp);
    })?;

    let (tx, mut rx) = tokio_mpsc::channel::<String>(32);

    let hotkey_config = config.clone();
    let hotkey_recording = is_recording.clone();
    let tx_for_hotkey = tx.clone();
    std::thread::spawn(move || {
        let hotkey_str = hotkey_config.lock().unwrap().hotkey.clone();
        setup_hotkey(&hotkey_str, hotkey_recording, tx_for_hotkey);
    });

    let audio_dir = app_data_dir.clone();
    let tx_for_audio = tx.clone();
    let selected_device_for_thread = selected_audio_device.clone();
    thread::spawn(move || {
        let mut recorder = AudioRecorder::new();
        loop {
            match audio_cmd_rx.recv() {
                Ok(AudioCommand::Start) => {
                    eprintln!("[Audio] Starting recording");
                    let device = selected_device_for_thread.lock().unwrap().clone();
                    recorder.select_device(device);
                    if let Err(e) = recorder.start_recording() {
                        eprintln!("[Audio] Start error: {}", e);
                        let _ = tx_for_audio.blocking_send(
                            serde_json::json!({"type": "recording_error", "value": e}).to_string()
                        );
                    }
                }
                Ok(AudioCommand::Stop) => {
                    eprintln!("[Audio] Stopping recording");
                    let path = audio_dir.join("temp_recording.wav");
                    match recorder.stop_recording(&path) {
                        Ok(_) => {
                            eprintln!("[Audio] Saved to {:?}", path);
                            let _ = tx_for_audio.blocking_send(
                                serde_json::json!({"type": "recording_done"}).to_string()
                            );
                        }
                        Err(e) => {
                            eprintln!("[Audio] Stop error: {}", e);
                            let _ = tx_for_audio.blocking_send(
                                serde_json::json!({"type": "recording_error", "value": e}).to_string()
                            );
                        }
                    }
                }
                Ok(AudioCommand::Quit) | Err(_) => break,
            }
        }
    });

    let proxy = event_loop.create_proxy();
    let config_task = config.clone();
    let app_data_dir_task = app_data_dir.clone();
    let is_recording_task = is_recording.clone();
    let asr_task = asr_client.clone();
    let audio_cmd_tx2 = audio_cmd_tx.clone();
    let selected_device_task = selected_audio_device.clone();

    tokio::spawn(async move {
        let ctx = IpcContext {
            config: config_task,
            proxy,
            app_data_dir: app_data_dir_task,
            is_recording: is_recording_task,
            asr_client: asr_task,
            selected_audio_device: selected_device_task,
            monitor_recorder: Arc::new(Mutex::new(None)),
        };
        while let Some(msg_raw) = rx.recv().await {
            eprintln!("[IPC] Received: {}", msg_raw);
            let msg_type = if let Ok(obj) = serde_json::from_str::<serde_json::Value>(&msg_raw) {
                obj.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string()
            } else {
                msg_raw.clone()
            };
            match msg_type.as_str() {
                "start_recording" => {
                    eprintln!("[IPC] Starting recording");
                    let _ = audio_cmd_tx2.send(AudioCommand::Start);
                    *ctx.is_recording.lock().unwrap() = true;
                    let _ = ctx.proxy.send_event(AppEvent::RecordingStarted);
                }
                "stop_recording" => {
                    eprintln!("[IPC] Stopping recording");
                    let _ = audio_cmd_tx2.send(AudioCommand::Stop);
                    *ctx.is_recording.lock().unwrap() = false;
                    let _ = ctx.proxy.send_event(AppEvent::RecordingStopped);
                }
                _ => {
                    app::ipc::handle_message(&msg_raw, &ctx).await;
                }
            }
        }
    });

    let main_webview = build_webview(main_window, dev_mode, &dev_url, "vstage://localhost/?window=main", tx.clone())?;
    let settings_webview = build_webview(
        settings_window,
        dev_mode,
        &format!("{}?window=settings", dev_url),
        "vstage://localhost/?window=settings",
        tx.clone(),
    )?;

    let main_window_id = main_webview.window().id();
    let settings_window_id = settings_webview.window().id();
    let audio_cmd_tx3 = audio_cmd_tx;
    let proxy_for_paste = event_loop.create_proxy();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::WindowEvent { window_id: wid, event: WindowEvent::CloseRequested, .. } => {
                if wid == main_window_id {
                    main_webview.window().set_visible(false);
                } else if wid == settings_window_id {
                    settings_webview.window().set_visible(false);
                }
            }
            Event::UserEvent(app_event) => {
                match app_event {
                    AppEvent::RecordingStarted => {
                        let _ = main_webview.evaluate_script("window.onRecordingStarted && window.onRecordingStarted()");
                    }
                    AppEvent::RecordingStopped => {
                        let _ = main_webview.evaluate_script("window.onRecordingStopped && window.onRecordingStopped()");
                    }
                    AppEvent::AsrResult(text) => {
                        let js = format!(
                            "window.onAsrResult && window.onAsrResult({})",
                            serde_json::to_string(&text).unwrap()
                        );
                        let _ = main_webview.evaluate_script(&js);
                        let _ = settings_webview.evaluate_script(&js);
                    }
                    AppEvent::AsrError(err) => {
                        let js = format!(
                            "window.onAsrError && window.onAsrError({})",
                            serde_json::to_string(&err).unwrap()
                        );
                        let _ = main_webview.evaluate_script(&js);
                        let _ = settings_webview.evaluate_script(&js);
                        
                        // 发送系统通知
                        let _ = tray_item::TrayItem::new("V-Stage Error", tray_item::IconSource::Resource("#1"))
                            .and_then(|mut t| t.display_notification("ASR Error", &err));
                    }
                    AppEvent::OpenSettings => {
                        let sw = settings_webview.window();
                        sw.set_visible(true);
                        sw.set_focus();
                    }
                    AppEvent::SetAlwaysOnTop(on_top) => {
                        let mw = main_webview.window();
                        mw.set_always_on_top(on_top);
                    }
                    AppEvent::QuitApp => {
                        let _ = audio_cmd_tx3.send(AudioCommand::Quit);
                        *control_flow = ControlFlow::Exit;
                    }
                    AppEvent::PasteText(text) => {
                        let _ = main_webview.evaluate_script("window.onPasteDone && window.onPasteDone()");
                        let _ = clipboard_win::set_clipboard(clipboard_win::formats::Unicode, &text);
                        main_webview.window().set_visible(false);
                        let p = proxy_for_paste.clone();
                        thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                            if let Ok(mut enigo) = Enigo::new(&Settings::default()) {
                                let _ = enigo.key(Key::Control, enigo::Direction::Press);
                                let _ = enigo.key(Key::Unicode('v'), enigo::Direction::Click);
                                let _ = enigo.key(Key::Control, enigo::Direction::Release);
                            }
                            std::thread::sleep(std::time::Duration::from_millis(100));
                            let _ = p.send_event(AppEvent::ShowMainWindow);
                        });
                    }
                    AppEvent::ShowMainWindow => {
                        let mw = main_webview.window();
                        mw.set_visible(true);
                        mw.set_focus();
                    }
                    AppEvent::ToggleMainWindow => {
                        let mw = main_webview.window();
                        if mw.is_visible() {
                            mw.set_visible(false);
                        } else {
                            mw.set_visible(true);
                            mw.set_focus();
                        }
                    }
                    AppEvent::StartDrag => {
                        let hwnd = main_webview.window().hwnd() as _;
                        unsafe {
                            ReleaseCapture();
                            SendMessageW(hwnd, WM_NCLBUTTONDOWN, 2usize, 0);
                        }
                    }
                    AppEvent::AudioDevices(devices) => {
                        let devices_json = serde_json::to_string(&devices).unwrap();
                        let js = format!("window.onAudioDevices && window.onAudioDevices({})", devices_json);
                        let _ = settings_webview.evaluate_script(&js);
                    }
                    AppEvent::AudioLevel(level) => {
                        let js = format!("window.onAudioLevel && window.onAudioLevel({})", level);
                        let _ = settings_webview.evaluate_script(&js);
                    }
                }
            }
            _ => {}
        }
    });
    #[allow(unreachable_code)]
    Ok(())
}

enum AudioCommand {
    Start,
    Stop,
    Quit,
}
