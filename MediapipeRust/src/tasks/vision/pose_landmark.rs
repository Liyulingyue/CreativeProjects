use crate::backend::{InferenceBackend, Landmark, Model, Session, Tensor, TensorType, Error};

#[derive(Clone, Debug, Default)]
pub struct PoseLandmarkerOptions {
    pub num_poses: u32,
    pub min_detection_confidence: f32,
    pub min_tracking_confidence: f32,
    pub output_segmentation: bool,
}

pub const POSE_LANDMARKS: &[&str] = &[
    "NOSE",
    "LEFT_EYE_INNER", "LEFT_EYE", "LEFT_EYE_OUTER",
    "RIGHT_EYE_INNER", "RIGHT_EYE", "RIGHT_EYE_OUTER",
    "LEFT_EAR", "RIGHT_EAR",
    "MOUTH_LEFT", "MOUTH_RIGHT",
    "LEFT_SHOULDER", "RIGHT_SHOULDER",
    "LEFT_ELBOW", "RIGHT_ELBOW",
    "LEFT_WRIST", "RIGHT_WRIST",
    "LEFT_PINKY", "RIGHT_PINKY",
    "LEFT_INDEX", "RIGHT_INDEX",
    "LEFT_THUMB", "RIGHT_THUMB",
    "LEFT_HIP", "RIGHT_HIP",
    "LEFT_KNEE", "RIGHT_KNEE",
    "LEFT_ANKLE", "RIGHT_ANKLE",
    "LEFT_HEEL", "RIGHT_HEEL",
    "LEFT_FOOT_INDEX", "RIGHT_FOOT_INDEX",
];

pub struct PoseLandmarkerBuilder {
    options: PoseLandmarkerOptions,
}

impl PoseLandmarkerBuilder {
    pub fn new() -> Self {
        Self {
            options: PoseLandmarkerOptions::default(),
        }
    }

    pub fn num_poses(mut self, num: u32) -> Self {
        self.options.num_poses = num;
        self
    }

    pub fn min_detection_confidence(mut self, conf: f32) -> Self {
        self.options.min_detection_confidence = conf;
        self
    }

    pub fn min_tracking_confidence(mut self, conf: f32) -> Self {
        self.options.min_tracking_confidence = conf;
        self
    }

    pub fn output_segmentation(mut self, output: bool) -> Self {
        self.options.output_segmentation = output;
        self
    }

    pub fn build_from_file<B: InferenceBackend>(self, backend: &B, path: &str) -> Result<PoseLandmarker, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(backend, data)
    }

    pub fn build_from_buffer<B: InferenceBackend>(self, backend: &B, buffer: Vec<u8>) -> Result<PoseLandmarker, Error> {
        let (model, session) = backend.load_model_and_session(&buffer)?;
        Ok(PoseLandmarker {
            model,
            session,
            options: self.options,
        })
    }
}

pub struct PoseLandmarker {
    model: Model,
    session: Session,
    options: PoseLandmarkerOptions,
}

#[derive(Debug, Clone)]
pub struct PoseLandmarkResult {
    pub landmarks: Vec<Landmark>,
    pub world_landmarks: Vec<Landmark>,
    pub segmentation_mask: Option<crate::backend::SegmentationMask>,
    pub confidence: f32,
}

impl PoseLandmarker {
    pub fn detect(&mut self, image_data: &[u8], _width: u32, _height: u32) -> Result<Vec<PoseLandmarkResult>, Error> {
        let input_tensor = Tensor::new(
            self.model.inputs[0].tensor_type,
            self.model.inputs[0].shape.clone(),
            image_data.to_vec(),
        );
        self.session.set_input(0, &input_tensor)?;
        self.session.compute()?;

        let mut landmarks_tensor = Tensor::empty(TensorType::F32, self.model.outputs[0].shape.clone());
        self.session.get_output(0, &mut landmarks_tensor)?;

        let landmarks_data = landmarks_tensor.as_f32();

        let num_keypoints = 33;
        let values_per_keypoint = 5;
        let mut landmarks = Vec::with_capacity(num_keypoints);
        let total_landmark_values = num_keypoints * values_per_keypoint;

        if landmarks_data.len() >= total_landmark_values {
            for i in 0..num_keypoints {
                let idx = i * values_per_keypoint;
                landmarks.push(Landmark::new(
                    landmarks_data[idx],
                    landmarks_data[idx + 1],
                    landmarks_data[idx + 2],
                ));
            }
        } else if landmarks_data.len() >= num_keypoints * 3 {
            for i in 0..num_keypoints {
                let idx = i * 3;
                landmarks.push(Landmark::new(
                    landmarks_data[idx],
                    landmarks_data[idx + 1],
                    landmarks_data[idx + 2],
                ));
            }
        } else {
            let values_per_landmark = landmarks_data.len() / num_keypoints;
            for i in 0..num_keypoints {
                let base = i * values_per_landmark;
                landmarks.push(Landmark::new(
                    landmarks_data.get(base).copied().unwrap_or(0.0),
                    landmarks_data.get(base + 1).copied().unwrap_or(0.0),
                    landmarks_data.get(base + 2).copied().unwrap_or(0.0),
                ));
            }
        }

        let confidence = if self.model.outputs.len() > 1 {
            let mut conf_tensor = Tensor::empty(TensorType::F32, self.model.outputs[1].shape.clone());
            self.session.get_output(1, &mut conf_tensor)?;
            conf_tensor.as_f32().first().copied().unwrap_or(1.0)
        } else {
            1.0
        };

        let segmentation_mask = if self.options.output_segmentation && self.model.outputs.len() > 2 {
            let mut found_mask = None;
            for (i, output) in self.model.outputs.iter().enumerate() {
                if output.shape.len() == 4 || (output.shape.len() == 3 && output.shape[2] < 10) {
                    let mut mask_tensor = Tensor::empty(TensorType::F32, output.shape.clone());
                    self.session.get_output(i, &mut mask_tensor)?;
                    let mask_data = mask_tensor.as_f32();

                    let (mask_h, mask_w) = if output.shape.len() == 4 {
                        (output.shape[1], output.shape[2])
                    } else if output.shape.len() == 3 {
                        (output.shape[1], output.shape[2])
                    } else {
                        (256, 256)
                    };

                    let category_mask: Vec<u8> = mask_data.iter()
                        .map(|&v| (v.max(0.0).min(1.0) * 255.0) as u8)
                        .collect();

                    found_mask = Some(crate::backend::SegmentationMask {
                        width: mask_w as u32,
                        height: mask_h as u32,
                        category_mask,
                        confidence_mask: None,
                    });
                    break;
                }
            }
            found_mask
        } else {
            None
        };

        Ok(vec![PoseLandmarkResult {
            landmarks,
            world_landmarks: Vec::new(),
            segmentation_mask,
            confidence,
        }])
    }
}
