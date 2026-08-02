use crate::backend::{Class, InferenceBackend, Landmark, Model, Session, Tensor, TensorType, Error};
use crate::labels::get_hand_label;
use std::sync::Arc;

pub struct HandLandmarkerBuilder<B: InferenceBackend> {
    backend: B,
    options: HandLandmarkerOptions,
}

#[derive(Clone, Debug, Default)]
pub struct HandLandmarkerOptions {
    pub num_hands: u32,
    pub min_hand_detection_confidence: f32,
    pub min_hand_presence_confidence: f32,
    pub min_tracking_confidence: f32,
}

impl<B: InferenceBackend> HandLandmarkerBuilder<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            options: HandLandmarkerOptions::default(),
        }
    }

    pub fn num_hands(mut self, num: u32) -> Self {
        self.options.num_hands = num;
        self
    }

    pub fn min_hand_detection_confidence(mut self, conf: f32) -> Self {
        self.options.min_hand_detection_confidence = conf;
        self
    }

    pub fn build_from_file(self, path: &str) -> Result<HandLandmarker<B>, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(data)
    }

    pub fn build_from_buffer(self, buffer: Vec<u8>) -> Result<HandLandmarker<B>, Error> {
        let model = self.backend.load_model(&buffer)?;
        Ok(HandLandmarker {
            backend: self.backend,
            model: Arc::new(model),
            options: self.options,
        })
    }
}

pub struct HandLandmarker<B: InferenceBackend> {
    backend: B,
    model: Arc<Model>,
    options: HandLandmarkerOptions,
}

impl<B: InferenceBackend> HandLandmarker<B> {
    pub fn new_session(&self) -> Result<HandLandmarkerSession<'_, B>, Error> {
        let session = self.backend.create_session(&*self.model)?;
        Ok(HandLandmarkerSession {
            landmarker: self,
            session,
        })
    }

    pub fn detect(&self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<HandLandmarkResult>, Error> {
        let mut session = self.new_session()?;
        session.detect(image_data, width, height)
    }
}

pub struct HandLandmarkerSession<'a, B: InferenceBackend> {
    landmarker: &'a HandLandmarker<B>,
    session: Session,
}

#[derive(Debug, Clone)]
pub struct HandLandmarkResult {
    pub landmarks: Vec<Landmark>,
    pub world_landmarks: Vec<Landmark>,
    pub handedness: Class,
}

pub const HAND_LANDMARKS: &[&str] = &[
    "WRIST",
    "THUMB_CMC", "THUMB_MCP", "THUMB_IP", "THUMB_TIP",
    "INDEX_FINGER_MCP", "INDEX_FINGER_PIP", "INDEX_FINGER_DIP", "INDEX_FINGER_TIP",
    "MIDDLE_FINGER_MCP", "MIDDLE_FINGER_PIP", "MIDDLE_FINGER_DIP", "MIDDLE_FINGER_TIP",
    "RING_FINGER_MCP", "RING_FINGER_PIP", "RING_FINGER_DIP", "RING_FINGER_TIP",
    "PINKY_MCP", "PINKY_PIP", "PINKY_DIP", "PINKY_TIP",
];

impl<'a, B: InferenceBackend> HandLandmarkerSession<'a, B> {
    pub fn detect(&mut self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<HandLandmarkResult>, Error> {
        let input_tensor = Tensor::new(
            self.landmarker.model.inputs[0].tensor_type,
            self.landmarker.model.inputs[0].shape.clone(),
            image_data.to_vec(),
        );
        self.session.set_input(0, &input_tensor)?;
        self.session.compute()?;

        let mut landmarks_tensor = Tensor::empty(TensorType::F32, self.landmarker.model.outputs[0].shape.clone());
        self.session.get_output(0, &mut landmarks_tensor)?;

        let landmarks_data = landmarks_tensor.as_f32();
        let num_landmarks = landmarks_data.len() / 3;
        let mut landmarks = Vec::with_capacity(num_landmarks);

        for i in 0..num_landmarks {
            landmarks.push(Landmark::new(
                landmarks_data[i * 3],
                landmarks_data[i * 3 + 1],
                landmarks_data[i * 3 + 2],
            ));
        }

        // Read handedness from output if available (typically output 2)
        let handedness = if self.landmarker.model.outputs.len() > 2 {
            let mut handedness_tensor = Tensor::empty(TensorType::F32, self.landmarker.model.outputs[2].shape.clone());
            self.session.get_output(2, &mut handedness_tensor)?;
            let handedness_data = handedness_tensor.as_f32();
            // handedness_data[0] = Left score, handedness_data[1] = Right score
            let (class_id, score) = if handedness_data[0] > handedness_data[1] {
                (0i32, handedness_data[0])
            } else {
                (1i32, handedness_data[1])
            };
            let label = get_hand_label(class_id as usize).unwrap_or("Unknown");
            Class::new(class_id, score, label.to_string())
        } else {
            Class::new(0, 0.9, "Right") // fallback
        };

        Ok(vec![HandLandmarkResult {
            landmarks,
            world_landmarks: Vec::new(),
            handedness,
        }])
    }
}
