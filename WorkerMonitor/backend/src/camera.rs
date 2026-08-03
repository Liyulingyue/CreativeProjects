use nokhwa::Camera;
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Duration;

pub struct CameraCapture {
    camera: Camera,
    latest_bytes: Arc<RwLock<Option<Vec<u8>>>>,
    running: Arc<RwLock<bool>>,
}

impl CameraCapture {
    pub fn new() -> Result<Self, String> {
        let camera = Camera::new(
            0,
            nokhwa::CameraFormat::new(320, 240, nokhwa::frame_format::FrameFormat::MJPEG, 30),
        )
        .map_err(|e| format!("Failed to open camera: {}", e))?;

        Ok(Self {
            camera,
            latest_bytes: Arc::new(RwLock::new(None)),
            running: Arc::new(RwLock::new(false)),
        })
    }

    pub fn start(&self) {
        {
            let mut r = self.running.write();
            if *r {
                return;
            }
            *r = true;
        }

        let camera = self.camera.clone();
        let latest_bytes = self.latest_bytes.clone();
        let running = self.running.clone();

        std::thread::spawn(move || {
            loop {
                if !*running.read() {
                    break;
                }

                match camera.poll() {
                    Ok(frame) => {
                        *latest_bytes.write() = Some(frame.buffer().to_vec());
                    }
                    Err(e) => {
                        eprintln!("[Camera] poll error: {}", e);
                    }
                }

                std::thread::sleep(Duration::from_millis(100));
            }
        });
    }

    pub fn stop(&self) {
        *self.running.write() = false;
    }

    pub fn latest_frame(&self) -> Option<Vec<u8>> {
        self.latest_bytes.read().clone()
    }
}

impl Drop for CameraCapture {
    fn drop(&mut self) {
        self.stop();
    }
}
