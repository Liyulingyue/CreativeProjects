pub mod prelude {
    pub use super::Source;
}

pub trait Source: Send + Sync {
    fn start(&self) -> std::result::Result<(), SourceError>;
    fn stop(&self);
    fn is_running(&self) -> bool;
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("camera error: {0}")]
    Camera(String),
    #[error("not supported: {0}")]
    NotSupported(String),
}

pub mod image_source;
pub use image_source::ImageSource;

#[cfg(feature = "camera")]
pub mod camera_source;
#[cfg(feature = "camera")]
pub use camera_source::CameraSource;
