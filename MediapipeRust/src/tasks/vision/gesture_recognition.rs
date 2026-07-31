use crate::backend::{Class, InferenceBackend, Landmark, Model, Session, Tensor, TensorType, Error};
use std::sync::Arc;

pub struct GestureRecognizerBuilder<B: InferenceBackend> {
    backend: B,
    options: GestureRecognizerOptions,
}

#[derive(Clone, Debug, Default)]
pub struct GestureRecognizerOptions {
    pub num_hands: u32,
    pub min_hand_detection_confidence: f32,
    pub min_hand_presence_confidence: f32,
    pub min_tracking_confidence: f32,
}

impl<B: InferenceBackend> GestureRecognizerBuilder<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            options: GestureRecognizerOptions::default(),
        }
    }

    pub fn num_hands(mut self, num: u32) -> Self {
        self.options.num_hands = num;
        self
    }

    pub fn build_from_file(self, path: &str) -> Result<GestureRecognizer<B>, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(data)
    }

    pub fn build_from_buffer(self, buffer: Vec<u8>) -> Result<GestureRecognizer<B>, Error> {
        let model = self.backend.load_model(&buffer)?;
        Ok(GestureRecognizer {
            backend: self.backend,
            model: Arc::new(model),
            options: self.options,
        })
    }
}

pub struct GestureRecognizer<B: InferenceBackend> {
    backend: B,
    model: Arc<Model>,
    options: GestureRecognizerOptions,
}

impl<B: InferenceBackend> GestureRecognizer<B> {
    pub fn new_session(&self) -> Result<GestureRecognizerSession<'_, B>, Error> {
        let session = self.backend.create_session(&*self.model)?;
        Ok(GestureRecognizerSession {
            recognizer: self,
            session,
        })
    }

    pub fn recognize(&self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<GestureRecognizerResult>, Error> {
        let mut session = self.new_session()?;
        session.recognize(image_data, width, height)
    }
}

pub struct GestureRecognizerSession<'a, B: InferenceBackend> {
    recognizer: &'a GestureRecognizer<B>,
    session: Session,
}

#[derive(Debug, Clone)]
pub struct GestureRecognizerResult {
    pub hand_landmarks: Vec<Landmark>,
    pub handedness: Class,
    pub gestures: Vec<Class>,
}

impl<'a, B: InferenceBackend> GestureRecognizerSession<'a, B> {
    pub fn recognize(&mut self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<GestureRecognizerResult>, Error> {
        let input_tensor = Tensor::new(
            self.recognizer.model.inputs[0].tensor_type,
            self.recognizer.model.inputs[0].shape.clone(),
            image_data.to_vec(),
        );
        self.session.set_input(0, &input_tensor)?;
        self.session.compute()?;

        let mut gestures_tensor = Tensor::empty(TensorType::F32, self.recognizer.model.outputs[0].shape.clone());
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
