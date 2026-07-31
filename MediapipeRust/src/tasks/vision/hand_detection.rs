use crate::backend::{BoundingBox, Class, Detection, InferenceBackend, Model, Session, Tensor, TensorType, Error};
use std::sync::Arc;

pub struct HandDetectorBuilder<B: InferenceBackend> {
    backend: B,
    options: HandDetectorOptions,
}

#[derive(Clone, Debug, Default)]
pub struct HandDetectorOptions {
    pub num_hands: u32,
    pub min_detection_confidence: f32,
    pub min_suppression_threshold: f32,
}

impl<B: InferenceBackend> HandDetectorBuilder<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            options: HandDetectorOptions::default(),
        }
    }

    pub fn num_hands(mut self, num: u32) -> Self {
        self.options.num_hands = num;
        self
    }

    pub fn min_detection_confidence(mut self, conf: f32) -> Self {
        self.options.min_detection_confidence = conf;
        self
    }

    pub fn build_from_file(self, path: &str) -> Result<HandDetector<B>, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(data)
    }

    pub fn build_from_buffer(self, buffer: Vec<u8>) -> Result<HandDetector<B>, Error> {
        let model = self.backend.load_model(&buffer)?;
        Ok(HandDetector {
            backend: self.backend,
            model: Arc::new(model),
            options: self.options,
        })
    }
}

pub struct HandDetector<B: InferenceBackend> {
    backend: B,
    model: Arc<Model>,
    options: HandDetectorOptions,
}

impl<B: InferenceBackend> HandDetector<B> {
    pub fn new_session(&self) -> Result<HandDetectorSession<'_, B>, Error> {
        let session = self.backend.create_session(&*self.model)?;
        Ok(HandDetectorSession {
            detector: self,
            session,
        })
    }

    pub fn detect(&self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<Detection>, Error> {
        let mut session = self.new_session()?;
        session.detect(image_data, width, height)
    }
}

pub struct HandDetectorSession<'a, B: InferenceBackend> {
    detector: &'a HandDetector<B>,
    session: Session,
}

impl<'a, B: InferenceBackend> HandDetectorSession<'a, B> {
    pub fn detect(&mut self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<Detection>, Error> {
        let input_tensor = Tensor::new(
            self.detector.model.inputs[0].tensor_type,
            self.detector.model.inputs[0].shape.clone(),
            image_data.to_vec(),
        );
        self.session.set_input(0, &input_tensor)?;
        self.session.compute()?;

        let mut boxes_tensor = Tensor::empty(TensorType::F32, self.detector.model.outputs[0].shape.clone());
        let mut scores_tensor = Tensor::empty(TensorType::F32, self.detector.model.outputs[1].shape.clone());

        self.session.get_output(0, &mut boxes_tensor)?;
        self.session.get_output(1, &mut scores_tensor)?;

        let boxes = boxes_tensor.as_f32();
        let scores = scores_tensor.as_f32();

        let mut detections = Vec::new();
        let num_hands = self.detector.options.num_hands as usize;

        for i in 0..num_hands.min(boxes.len() / 4) {
            let score = scores[i];
            if score < self.detector.options.min_detection_confidence {
                continue;
            }

            let j = i * 4;
            detections.push(Detection {
                bounding_box: BoundingBox::new(
                    boxes[j] * width as f32,
                    boxes[j + 1] * height as f32,
                    boxes[j + 2] * width as f32,
                    boxes[j + 3] * height as f32,
                ),
                categories: vec![Class::new(0, score, "hand".to_string())],
            });
        }

        Ok(detections)
    }
}
