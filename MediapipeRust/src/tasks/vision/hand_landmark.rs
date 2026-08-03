use crate::backend::{Class, InferenceBackend, Landmark, Model, Session, Tensor, TensorType, Error};
use crate::labels::get_hand_label;

#[derive(Clone, Debug, Default)]
pub struct HandLandmarkerOptions {
    pub num_hands: u32,
    pub min_hand_detection_confidence: f32,
    pub min_hand_presence_confidence: f32,
    pub min_tracking_confidence: f32,
}

pub struct HandLandmarkerBuilder {
    options: HandLandmarkerOptions,
}

impl HandLandmarkerBuilder {
    pub fn new() -> Self {
        Self {
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

    pub fn build_from_file<B: InferenceBackend>(self, backend: &B, path: &str) -> Result<HandLandmarker, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(backend, data)
    }

    pub fn build_from_buffer<B: InferenceBackend>(self, backend: &B, buffer: Vec<u8>) -> Result<HandLandmarker, Error> {
        let (model, session) = backend.load_model_and_session(&buffer)?;
        Ok(HandLandmarker {
            model,
            session,
            options: self.options,
        })
    }
}

pub struct HandLandmarker {
    model: Model,
    session: Session,
    #[allow(dead_code)]
    options: HandLandmarkerOptions,
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

impl HandLandmarker {
    pub fn detect(&mut self, image_data: &[u8], _width: u32, _height: u32) -> Result<Vec<HandLandmarkResult>, Error> {
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
        let num_landmarks = landmarks_data.len() / 3;
        let mut landmarks = Vec::with_capacity(num_landmarks);

        for i in 0..num_landmarks {
            landmarks.push(Landmark::new(
                landmarks_data[i * 3],
                landmarks_data[i * 3 + 1],
                landmarks_data[i * 3 + 2],
            ));
        }

        let handedness = if self.model.outputs.len() > 2 {
            let mut handedness_tensor = Tensor::empty(TensorType::F32, self.model.outputs[2].shape.clone());
            self.session.get_output(2, &mut handedness_tensor)?;
            let handedness_data = handedness_tensor.as_f32();
            let (class_id, score) = if handedness_data[0] > handedness_data[1] {
                (0i32, handedness_data[0])
            } else {
                (1i32, handedness_data[1])
            };
            let label = get_hand_label(class_id as usize).unwrap_or("Unknown");
            Class::new(class_id, score, label.to_string())
        } else {
            Class::new(0, 0.9, "Right")
        };

        Ok(vec![HandLandmarkResult {
            landmarks,
            world_landmarks: Vec::new(),
            handedness,
        }])
    }
}
