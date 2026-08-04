use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use image::GenericImageView;

use crate::node::{Frame, FrameData, FrameMeta, ImageData, MediaType, Node, NodeError, PixelFormat, Result};
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
}

impl Node for ImageSource {
    fn process(&self, _frame: Frame) -> Result<Frame> {
        let img = image::open(&self.path).map_err(|e| NodeError::Source(e.to_string()))?;
        let (w, h) = img.dimensions();
        let rgba = img.to_rgba8();
        let pixels = rgba.into_raw();

        let frame_data = FrameData::Image(ImageData {
            width: w,
            height: h,
            format: PixelFormat::Rgba,
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
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
