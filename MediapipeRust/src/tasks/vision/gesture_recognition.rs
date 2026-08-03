use crate::backend::{Class, InferenceBackend, Landmark, Model, Session, Tensor, TensorType, Error};

#[derive(Clone, Debug, Default)]
pub struct GestureRecognizerOptions {
    pub num_hands: u32,
    pub min_hand_detection_confidence: f32,
    pub min_hand_presence_confidence: f32,
    pub min_tracking_confidence: f32,
}

pub struct GestureRecognizerBuilder {
    options: GestureRecognizerOptions,
}

impl GestureRecognizerBuilder {
    pub fn new() -> Self {
        Self {
            options: GestureRecognizerOptions::default(),
        }
    }

    pub fn num_hands(mut self, num: u32) -> Self {
        self.options.num_hands = num;
        self
    }

    pub fn build_from_file<B: InferenceBackend>(self, backend: &B, path: &str) -> Result<GestureRecognizer, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(backend, data)
    }

    pub fn build_from_buffer<B: InferenceBackend>(self, backend: &B, buffer: Vec<u8>) -> Result<GestureRecognizer, Error> {
        let (model, session) = backend.load_model_and_session(&buffer)?;
        Ok(GestureRecognizer {
            model,
            session,
            options: self.options,
        })
    }
}

pub struct GestureRecognizer {
    model: Model,
    session: Session,
    #[allow(dead_code)]
    options: GestureRecognizerOptions,
}

#[derive(Debug, Clone)]
pub struct GestureRecognizerResult {
    pub hand_landmarks: Vec<Landmark>,
    pub handedness: Class,
    pub gestures: Vec<Class>,
}

impl GestureRecognizer {
    pub fn recognize(&mut self, image_data: &[u8], _width: u32, _height: u32) -> Result<Vec<GestureRecognizerResult>, Error> {
        let input_tensor = Tensor::new(
            self.model.inputs[0].tensor_type,
            self.model.inputs[0].shape.clone(),
            image_data.to_vec(),
        );
        self.session.set_input(0, &input_tensor)?;
        self.session.compute()?;

        let mut gestures_tensor = Tensor::empty(TensorType::F32, self.model.outputs[0].shape.clone());
        self.session.get_output(0, &mut gestures_tensor)?;

        let gestures_data = gestures_tensor.as_f32();
        let mut gestures = Vec::new();
        for i in 0..gestures_data.len() {
            if gestures_data[i] > 0.5 {
                gestures.push(Class::new(i as i32, gestures_data[i], format!("gesture_{}", i)));
            }
        }

        Ok(vec![GestureRecognizerResult {
            hand_landmarks: Vec::new(),
            handedness: Class::new(0, 0.9, "Right"),
            gestures,
        }])
    }
}
