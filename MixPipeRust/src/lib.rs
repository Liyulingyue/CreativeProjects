mod pipeline;
mod node;
mod sources;
mod processors;
pub mod models;
pub mod model_hub;
pub mod visualizer;

pub use pipeline::{Pipeline, PipelineBuilder, PoseModel};
pub use processors::{Resize, Normalize, ColorConvert};
pub use models::{MoveNet, MoveNetVariant, RtmDet, RtmPose, Detection, Keypoint};
pub use node::{Node, Frame, FrameMeta, FrameData, ImageData, AudioData, TextData, VideoData, MediaType, PixelFormat, NodeError, Result, Person};
pub use model_hub::{PretrainedModel, download_model, download_model_blocking, get_model_path, get_cache_dir};
pub use visualizer::Visualizer;

pub mod prelude {
    pub use crate::node::{Node, Frame, FrameMeta, FrameData, MediaType};
    pub use crate::processors::*;
    pub use crate::models::*;
    pub use crate::model_hub::PretrainedModel;
    pub use crate::Person;
}
