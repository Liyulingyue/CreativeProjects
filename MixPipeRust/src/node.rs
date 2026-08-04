use std::any::Any;
use std::collections::HashMap;

pub trait Node: Send + Sync {
    fn process(&self, frame: Frame) -> Result<Frame>;
    fn as_any(&self) -> &dyn Any;
}

#[derive(Clone)]
pub struct Frame {
    pub data: FrameData,
    pub meta: FrameMeta,
}

#[derive(Clone)]
pub struct FrameMeta {
    pub timestamp_ms: u64,
    pub source: String,
    pub media_type: MediaType,
    pub custom: HashMap<String, serde_json::Value>,
}

impl Frame {
    pub fn new(data: FrameData, meta: FrameMeta) -> Self {
        Self { data, meta }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaType {
    Image,
    Audio,
    Text,
    Video,
    Unknown,
}

#[derive(Clone)]
pub enum FrameData {
    Image(ImageData),
    Audio(AudioData),
    Text(TextData),
    Video(VideoData),
    Unknown,
}

#[derive(Clone)]
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub pixels: Vec<u8>,
}

#[derive(Clone)]
pub struct AudioData {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

#[derive(Clone)]
pub struct TextData {
    pub content: String,
    pub language: Option<String>,
}

#[derive(Clone)]
pub struct VideoData {
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub frames: Vec<ImageData>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Rgb,
    Rgba,
    Bgr,
    Bgra,
    Gray,
    Yuv420,
    Unknown,
}

impl Default for FrameMeta {
    fn default() -> Self {
        Self {
            timestamp_ms: 0,
            source: String::new(),
            media_type: MediaType::Unknown,
            custom: HashMap::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    #[error("processing error: {0}")]
    Process(String),
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("unsupported media type: {0:?}")]
    UnsupportedMediaType(MediaType),
    #[error("model error: {0}")]
    Model(String),
    #[error("source error: {0}")]
    Source(String),
}

pub type Result<T> = std::result::Result<T, NodeError>;
