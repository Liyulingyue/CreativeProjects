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

    pub fn from_rgb(pixels: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            data: FrameData::Image(ImageData {
                width,
                height,
                format: PixelFormat::Rgb,
                pixels,
            }),
            meta: FrameMeta::default(),
        }
    }

    pub fn detections(&self) -> Option<Vec<Detection>> {
        self.meta.custom.get("detections")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn keypoints(&self) -> Option<Vec<Keypoint>> {
        self.meta.custom.get("keypoints")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn set_detections(&mut self, detections: Vec<Detection>) {
        self.meta.custom.insert(
            "detections".to_string(),
            serde_json::to_value(detections).unwrap(),
        );
    }

    pub fn set_keypoints(&mut self, keypoints: Vec<Keypoint>) {
        self.meta.custom.insert(
            "keypoints".to_string(),
            serde_json::to_value(keypoints).unwrap(),
        );
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Detection {
    pub bbox: [f32; 4],
    pub score: f32,
    pub label: i32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Keypoint {
    pub x: f32,
    pub y: f32,
    pub confidence: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Person {
    pub bbox: [f32; 4],
    pub keypoints: Vec<Keypoint>,
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

pub fn crop_frame(frame: &Frame, bbox: &[f32; 4]) -> Result<Frame> {
    let img = match &frame.data {
        FrameData::Image(img) => img,
        _ => return Err(NodeError::UnsupportedMediaType(frame.meta.media_type.clone())),
    };

    let [x1, y1, x2, y2] = *bbox;
    let x1 = x1.max(0.0) as u32;
    let y1 = y1.max(0.0) as u32;
    let x2 = x2.min(img.width as f32) as u32;
    let y2 = y2.min(img.height as f32) as u32;

    let crop_w = x2 - x1;
    let crop_h = y2 - y1;

    if crop_w == 0 || crop_h == 0 {
        return Err(NodeError::Process("Invalid crop size".to_string()));
    }

    let mut crop_pixels = Vec::with_capacity((crop_w * crop_h * 3) as usize);
    for y in y1..y2 {
        for x in x1..x2 {
            let idx = ((y * img.width + x) * 3) as usize;
            crop_pixels.push(img.pixels[idx]);
            crop_pixels.push(img.pixels[idx + 1]);
            crop_pixels.push(img.pixels[idx + 2]);
        }
    }

    Ok(Frame::from_rgb(crop_pixels, crop_w, crop_h))
}
