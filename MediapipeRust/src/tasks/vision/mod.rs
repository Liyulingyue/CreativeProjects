pub mod image_classification;
pub mod object_detection;
pub mod face_detection;
pub mod face_landmark;
pub mod hand_detection;
pub mod hand_landmark;
pub mod gesture_recognition;
pub mod image_segmentation;
pub mod image_embedding;
pub mod pose_landmark;
pub mod selfie_segmentation;
pub mod iris_tracking;

pub use image_classification::*;
pub use object_detection::*;
pub use face_detection::*;
pub use face_landmark::*;
pub use hand_detection::*;
pub use hand_landmark::*;
pub use gesture_recognition::*;
pub use image_embedding::*;
pub use pose_landmark::*;
pub use selfie_segmentation::{SelfieSegmenter, SelfieSegmenterBuilder, SelfieSegmenterOptions, SegmentationOutputType};
pub use image_segmentation::{ImageSegmenter, ImageSegmenterBuilder, ImageSegmenterOptions, ImageSegmenterOutputType};
pub use iris_tracking::{IrisTracker, IrisTrackerBuilder, IrisResult};

use crate::backend::{BoundingBox, Error};

#[derive(Clone, Debug)]
pub struct ImageProcessingOptions {
    pub region_of_interest: Option<BoundingBox>,
    pub rotation_degrees: i32,
}

impl Default for ImageProcessingOptions {
    fn default() -> Self {
        Self {
            region_of_interest: None,
            rotation_degrees: 0,
        }
    }
}

impl ImageProcessingOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn rotation_degrees(mut self, degrees: i32) -> Result<Self, Error> {
        if degrees % 90 != 0 {
            return Err(Error::InvalidArgument(
                format!("Rotation must be multiple of 90 degrees, got {}", degrees)
            ));
        }
        self.rotation_degrees = degrees % 360;
        Ok(self)
    }

    pub fn region_of_interest(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Result<Self, Error> {
        if left >= right || top >= bottom {
            return Err(Error::InvalidArgument("Invalid ROI bounds".into()));
        }
        self.region_of_interest = Some(BoundingBox::new(left, top, right, bottom));
        Ok(self)
    }
}

pub trait TaskSession {
    type Output;
    fn process(&mut self, image_data: &[u8], width: u32, height: u32) -> Result<Self::Output, Error>;
}
