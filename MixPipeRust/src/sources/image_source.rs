use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use image::GenericImageView;

use crate::node::{Frame, FrameData, FrameMeta, ImageData, MediaType, NodeError};
use crate::sources::Source;

pub struct ImageSource {
    path: String,
    running: Arc<AtomicBool>,
}

impl ImageSource {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn load_frame(&self) -> Result<Frame, NodeError> {
        let img = image::open(&self.path).map_err(|e| NodeError::Source(e.to_string()))?;
        let (w, h) = img.dimensions();
        let rgba = img.to_rgba8();
        let pixels = rgba.into_raw();

        let frame_data = FrameData::Image(ImageData {
            width: w,
            height: h,
            format: crate::node::PixelFormat::Rgba,
            pixels,
        });

        let meta = FrameMeta {
            timestamp_ms: 0,
            source: format!("image://{}", self.path),
            media_type: MediaType::Image,
            custom: Default::default(),
        };

        Ok(Frame::new(frame_data, meta))
    }
}

impl Source for ImageSource {
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
