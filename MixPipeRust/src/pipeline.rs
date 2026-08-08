use crate::node::{ImageData, Keypoint, NodeError, Person, Result};
use crate::models::{RtmDet, RtmPose};
use crate::model_hub::PretrainedModel;
use std::path::{Path, PathBuf};

pub enum PoseModel {
    Body,
    WholeBody,
    Face,
    Hand,
}

impl PoseModel {
    fn input_size(&self) -> (usize, usize) {
        match self {
            PoseModel::Body | PoseModel::WholeBody => (256, 192),
            PoseModel::Face | PoseModel::Hand => (256, 256),
        }
    }
}

pub struct Pipeline {
    detector: RtmDet,
    pose_estimator: RtmPose,
}

impl Pipeline {
    pub fn builder() -> PipelineBuilder {
        PipelineBuilder::new()
    }

    pub fn from_files(detector_path: &Path, pose_path: &Path) -> Result<Self> {
        let detector = RtmDet::from_file(detector_path)?;
        let pose_estimator = RtmPose::from_file(pose_path)?;
        Ok(Self { detector, pose_estimator })
    }

    pub fn run(&self, pixels: &[u8], width: u32, height: u32) -> Result<Vec<Person>> {
        let detections = self.detector.infer(pixels, width, height)?;

        let mut persons = Vec::new();
        for det in &detections {
            let crop_result = self.crop_and_resize(pixels, width, height, &det.bbox)?;
            let keypoints = self.pose_estimator.infer(&crop_result.pixels, crop_result.width, crop_result.height)?;

            let adjusted_kpts = self.adjust_keypoints(&keypoints, &det.bbox, &crop_result);

            persons.push(Person {
                bbox: det.bbox,
                keypoints: adjusted_kpts,
            });
        }

        Ok(persons)
    }

    fn crop_and_resize(&self, pixels: &[u8], img_w: u32, _img_h: u32, bbox: &[f32; 4]) -> Result<ImageData> {
        let [x1, y1, x2, y2] = *bbox;
        let crop_w = (x2 - x1) as u32;
        let crop_h = (y2 - y1) as u32;

        if crop_w == 0 || crop_h == 0 {
            return Err(NodeError::Process("Invalid crop size".to_string()));
        }

        let mut crop_pixels = Vec::with_capacity((crop_w * crop_h * 3) as usize);
        for y in y1 as u32..y2 as u32 {
            for x in x1 as u32..x2 as u32 {
                let idx = ((y * img_w + x) * 3) as usize;
                crop_pixels.push(pixels[idx]);
                crop_pixels.push(pixels[idx + 1]);
                crop_pixels.push(pixels[idx + 2]);
            }
        }

        Ok(ImageData {
            width: crop_w,
            height: crop_h,
            format: crate::node::PixelFormat::Rgb,
            pixels: crop_pixels,
        })
    }

    fn adjust_keypoints(&self, keypoints: &[Keypoint], bbox: &[f32; 4], _crop: &ImageData) -> Vec<Keypoint> {
        keypoints.iter().map(|kp| {
            Keypoint {
                x: kp.x + bbox[0],
                y: kp.y + bbox[1],
                confidence: kp.confidence,
            }
        }).collect()
    }
}

pub struct PipelineBuilder {
    detector_path: Option<PathBuf>,
    pose_path: Option<PathBuf>,
}

impl PipelineBuilder {
    pub fn new() -> Self {
        Self {
            detector_path: None,
            pose_path: None,
        }
    }

    pub fn detector<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.detector_path = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn pose<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.pose_path = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn body_pose<P: AsRef<Path>>(self, pose_path: P) -> Self {
        self.pose(pose_path)
    }

    pub fn wholebody_pose<P: AsRef<Path>>(self, pose_path: P) -> Self {
        self.pose(pose_path)
    }

    pub fn face_pose<P: AsRef<Path>>(self, pose_path: P) -> Self {
        self.pose(pose_path)
    }

    pub fn hand_pose<P: AsRef<Path>>(self, pose_path: P) -> Self {
        self.pose(pose_path)
    }

    pub fn detector_model(mut self, model: PretrainedModel) -> Self {
        self.detector_path = crate::model_hub::get_model_path(model);
        self
    }

    pub fn pose_model(mut self, model: PretrainedModel) -> Self {
        self.pose_path = crate::model_hub::get_model_path(model);
        self
    }

    pub fn build(self) -> Result<Pipeline> {
        let detector_path = self.detector_path
            .ok_or_else(|| NodeError::Process("detector path not set".to_string()))?;
        let pose_path = self.pose_path
            .ok_or_else(|| NodeError::Process("pose path not set".to_string()))?;

        if !detector_path.exists() {
            crate::model_hub::download_model_blocking(PretrainedModel::RtmDetTiny)
                .map_err(|e| NodeError::Model(format!("download detector: {}", e)))?;
        }
        if !pose_path.exists() {
            let pose_model = match pose_path.file_name().and_then(|n| n.to_str()) {
                Some("rtmpose-tiny-mmdeploy.onnx") => PretrainedModel::RtmPoseBody,
                Some("rtmpose-wholebody-mmdeploy.onnx") => PretrainedModel::RtmPoseWholeBody,
                Some("rtmpose-face-mmdeploy.onnx") => PretrainedModel::RtmPoseFace,
                Some("rtmpose-hand-mmdeploy.onnx") => PretrainedModel::RtmPoseHand,
                _ => return Err(NodeError::Model("unknown pose model".to_string())),
            };
            crate::model_hub::download_model_blocking(pose_model)
                .map_err(|e| NodeError::Model(format!("download pose: {}", e)))?;
        }

        Pipeline::from_files(detector_path.as_path(), pose_path.as_path())
    }
}

impl Default for PipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}
