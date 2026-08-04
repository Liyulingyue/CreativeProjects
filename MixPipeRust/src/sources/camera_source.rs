use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::sources::Source;

pub struct CameraSource {
    device_index: usize,
    running: Arc<AtomicBool>,
}

impl CameraSource {
    pub fn new(device_index: usize) -> Self {
        Self {
            device_index,
            running: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Source for CameraSource {
    fn start(&self) -> std::result::Result<(), crate::sources::SourceError> {
        self.running.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
