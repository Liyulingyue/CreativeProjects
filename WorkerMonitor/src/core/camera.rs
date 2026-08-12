use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera as NokhwaCamera;
use nokhwa::pixel_format::RgbFormat;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use image::ImageEncoder;

fn lock_or_recover<'a, T>(mutex: &'a Mutex<T>, name: &str) -> std::sync::MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!("[Camera] Mutex poisoned, recovering: {}", name);
            poisoned.into_inner()
        }
    }
}

#[derive(Clone)]
pub struct FrameReader {
    latest_frame: Arc<Mutex<Option<Vec<u8>>>>,
}

impl FrameReader {
    pub fn get_frame(&self) -> Option<Vec<u8>> {
        lock_or_recover(&self.latest_frame, "latest_frame").clone()
    }
}

pub struct Camera {
    sender: Mutex<Option<mpsc::Sender<CameraCommand>>>,
    is_running: std::sync::atomic::AtomicBool,
    frame_reader: FrameReader,
}

#[derive(Debug)]
enum CameraCommand {
    Stop,
}

impl Camera {
    pub fn new() -> Self {
        let latest_frame = Arc::new(Mutex::new(None));
        Self {
            sender: Mutex::new(None),
            is_running: std::sync::atomic::AtomicBool::new(false),
            frame_reader: FrameReader { latest_frame: latest_frame.clone() },
        }
    }

    pub fn start(&self) -> Result<(), String> {
        if self.is_running.load(std::sync::atomic::Ordering::SeqCst) {
            println!("[Camera] Already running");
            return Ok(());
        }

        let (tx, rx) = mpsc::channel();
        *lock_or_recover(&self.sender, "sender") = Some(tx);

        let frame_reader = self.frame_reader.clone();

        println!("[Camera] Starting camera thread...");
        thread::spawn(move || {
            camera_thread_fn(rx, frame_reader);
        });

        self.is_running.store(true, std::sync::atomic::Ordering::SeqCst);
        println!("[Camera] Camera started");
        Ok(())
    }

    pub fn stop(&self) {
        println!("[Camera] Stopping...");
        self.is_running.store(false, std::sync::atomic::Ordering::SeqCst);
        if let Some(sender) = lock_or_recover(&self.sender, "sender").take() {
            let _ = sender.send(CameraCommand::Stop);
        }
        println!("[Camera] Stopped");
    }

    pub fn get_frame(&self) -> Option<Vec<u8>> {
        self.frame_reader.get_frame()
    }

    #[allow(dead_code)]
    pub fn frame_reader(&self) -> FrameReader {
        self.frame_reader.clone()
    }

    pub fn is_running(&self) -> bool {
        self.is_running.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        self.stop();
    }
}

fn camera_thread_fn(rx: mpsc::Receiver<CameraCommand>, frame_reader: FrameReader) {
    let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);

    println!("[Camera] Opening camera...");
    let mut camera = match NokhwaCamera::new(CameraIndex::Index(0), requested) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[Camera] Failed to open camera: {}", e);
            return;
        }
    };

    if let Err(e) = camera.open_stream() {
        eprintln!("[Camera] Failed to open stream: {}", e);
        return;
    }

    println!("[Camera] Camera stream opened successfully!");

    loop {
        match rx.try_recv() {
            Ok(CameraCommand::Stop) => break,
            _ => {}
        }

        match camera.frame() {
            Ok(frame) => {
                if let Ok(rgb) = frame.decode_image::<RgbFormat>() {
                    let rgb_img = rgb.as_raw();
                    let width = rgb.width();
                    let height = rgb.height();

                    let mut buf = Vec::new();
                    let mut cursor = std::io::Cursor::new(&mut buf);
                    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 85);
                    if encoder.write_image(rgb_img, width, height, image::ColorType::Rgb8).is_ok() {
                        *lock_or_recover(&frame_reader.latest_frame, "latest_frame") = Some(buf);
                    }
                }
            }
            Err(e) => {
                eprintln!("[Camera] Frame error: {}", e);
            }
        }

        thread::sleep(Duration::from_millis(33));
    }
}