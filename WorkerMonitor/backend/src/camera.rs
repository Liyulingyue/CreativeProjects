use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub struct CameraState {
    sender: Mutex<Option<mpsc::Sender<CameraCommand>>>,
    is_running: std::sync::atomic::AtomicBool,
    latest_frame: Arc<Mutex<Option<Vec<u8>>>>,
}

#[derive(Debug)]
enum CameraCommand {
    Stop,
}

impl CameraState {
    pub fn new() -> Self {
        Self {
            sender: Mutex::new(None),
            is_running: std::sync::atomic::AtomicBool::new(false),
            latest_frame: Arc::new(Mutex::new(None)),
        }
    }

    pub fn start(&self) -> Result<(), String> {
        if self.is_running.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(());
        }

        let (tx, rx) = mpsc::channel();
        *self.sender.lock().unwrap() = Some(tx);
        let latest_frame = self.latest_frame.clone();

        thread::spawn(move || {
            camera_thread_fn(rx, latest_frame);
        });

        self.is_running.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    pub fn stop(&self) {
        self.is_running.store(false, std::sync::atomic::Ordering::SeqCst);
        if let Some(sender) = self.sender.lock().unwrap().take() {
            let _ = sender.send(CameraCommand::Stop);
        }
    }

    pub fn get_frame(&self) -> Option<Vec<u8>> {
        self.latest_frame.lock().unwrap().clone()
    }

    pub fn is_running(&self) -> bool {
        self.is_running.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Drop for CameraState {
    fn drop(&mut self) {
        self.stop();
    }
}

fn camera_thread_fn(rx: mpsc::Receiver<CameraCommand>, latest_frame: Arc<Mutex<Option<Vec<u8>>>>) {
    use nokhwa::pixel_format::RgbFormat;
    let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);

    let mut camera = match Camera::new(CameraIndex::Index(0), requested) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to open camera: {}", e);
            return;
        }
    };

    if let Err(e) = camera.open_stream() {
        eprintln!("Failed to open stream: {}", e);
        return;
    }

    eprintln!("Camera opened successfully!");

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
                    if let Ok(()) = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85)
                        .encode(rgb_img, width, height, image::ExtendedColorType::Rgb8) {
                        *latest_frame.lock().unwrap() = Some(buf);
                    }
                }
            }
            Err(e) => {
                eprintln!("Frame error: {}", e);
            }
        }

        thread::sleep(Duration::from_millis(66));
    }
}
