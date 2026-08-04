mod pipeline;
mod node;
mod sources;
mod processors;
mod models;

pub use pipeline::{Pipeline, PipelineBuilder};
pub use processors::{Resize, Normalize, ColorConvert};
pub use node::{Node, Frame, FrameMeta, FrameData, ImageData, AudioData, TextData, VideoData, MediaType, PixelFormat, NodeError, Result};

pub mod prelude {
    pub use crate::node::{Node, Frame, FrameMeta, FrameData, MediaType};
    pub use crate::processors::*;
    pub use crate::models::*;
}
