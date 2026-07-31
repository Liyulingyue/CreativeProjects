use crate::backend::{BoundingBox, CategoriesFilter, Class, Detection, InferenceBackend, Model, Session, Tensor, TensorType, Error};
use std::sync::Arc;

pub struct ObjectDetectorBuilder<B: InferenceBackend> {
    backend: B,
    max_results: i32,
    options: ObjectDetectorOptions,
}

#[derive(Clone, Debug, Default)]
pub struct ObjectDetectorOptions {
    pub min_score_threshold: f32,
    pub min_suppression_threshold: f32,
    pub label_allow_list: Vec<String>,
    pub label_deny_list: Vec<String>,
    pub num_classes: u32,
}

impl<B: InferenceBackend> ObjectDetectorBuilder<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            max_results: 10,
            options: ObjectDetectorOptions::default(),
        }
    }

    pub fn max_results(mut self, max: i32) -> Self {
        self.max_results = max;
        self
    }

    pub fn min_score_threshold(mut self, threshold: f32) -> Self {
        self.options.min_score_threshold = threshold;
        self
    }

    pub fn min_suppression_threshold(mut self, threshold: f32) -> Self {
        self.options.min_suppression_threshold = threshold;
        self
    }

    pub fn build_from_file(self, path: &str) -> Result<ObjectDetector<B>, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(data)
    }

    pub fn build_from_buffer(self, buffer: Vec<u8>) -> Result<ObjectDetector<B>, Error> {
        let model = self.backend.load_model(&buffer)?;
        Ok(ObjectDetector {
            backend: self.backend,
            model: Arc::new(model),
            max_results: self.max_results,
            options: self.options,
        })
    }
}

pub struct ObjectDetector<B: InferenceBackend> {
    backend: B,
    model: Arc<Model>,
    max_results: i32,
    options: ObjectDetectorOptions,
}

impl<B: InferenceBackend> ObjectDetector<B> {
    pub fn new_session(&self) -> Result<ObjectDetectorSession<'_, B>, Error> {
        let session = self.backend.create_session(&*self.model)?;
        Ok(ObjectDetectorSession {
            detector: self,
            session,
        })
    }

    pub fn detect(&self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<Detection>, Error> {
        let mut session = self.new_session()?;
        session.detect(image_data, width, height)
    }
}

pub struct ObjectDetectorSession<'a, B: InferenceBackend> {
    detector: &'a ObjectDetector<B>,
    session: Session,
}

impl<'a, B: InferenceBackend> ObjectDetectorSession<'a, B> {
    pub fn detect(&mut self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<Detection>, Error> {
        let input_tensor = self.prepare_input(image_data, width, height)?;
        self.session.set_input(0, &input_tensor)?;
        self.session.compute()?;

        let boxes_size = self.detector.model.outputs[0].shape.iter().product();
        let scores_size = self.detector.model.outputs[1].shape.iter().product();

        let mut boxes_tensor = Tensor::empty(TensorType::F32, vec![boxes_size]);
        let mut scores_tensor = Tensor::empty(TensorType::F32, vec![scores_size]);

        self.session.get_output(0, &mut boxes_tensor)?;
        self.session.get_output(1, &mut scores_tensor)?;

        let detections = self.postprocess_output(&boxes_tensor, &scores_tensor, width, height)?;
        Ok(detections)
    }

    fn prepare_input(&self, image_data: &[u8], width: u32, height: u32) -> Result<Tensor, Error> {
        let input_shape = &self.detector.model.inputs[0].shape;
        let input_type = self.detector.model.inputs[0].tensor_type;
        Ok(Tensor::new(input_type, input_shape.clone(), image_data.to_vec()))
    }

    fn postprocess_output(
        &self,
        boxes: &Tensor,
        scores: &Tensor,
        img_width: u32,
        img_height: u32,
    ) -> Result<Vec<Detection>, Error> {
        let boxes_data = boxes.as_f32();
        let scores_data = scores.as_f32();

        let mut detections = Vec::new();
        let num_detections = (boxes_data.len() / 4).min(self.detector.max_results as usize);

        for i in 0..num_detections {
            let score = scores_data[i];
            if score < self.detector.options.min_score_threshold {
                continue;
            }

            let j = i * 4;
            let detection = Detection {
                bounding_box: BoundingBox::new(
                    boxes_data[j] * img_width as f32,
                    boxes_data[j + 1] * img_height as f32,
                    boxes_data[j + 2] * img_width as f32,
                    boxes_data[j + 3] * img_height as f32,
                ),
                categories: vec![Class::new(0, score, format!("object_{}", i))],
            };
            detections.push(detection);
        }

        self.apply_nms(&mut detections);
        Ok(detections)
    }

    fn apply_nms(&self, detections: &mut Vec<Detection>) {
        let threshold = self.detector.options.min_suppression_threshold;
        detections.sort_by(|a, b| {
            b.categories[0].score.partial_cmp(&a.categories[0].score).unwrap()
        });

        let mut keep = Vec::new();
        while !detections.is_empty() {
            let best = detections.remove(0);
            keep.push(best);

            detections.retain(|d| {
                let iou = self.compute_iou(&keep.last().unwrap().bounding_box, &d.bounding_box);
                iou < threshold
            });
        }
        *detections = keep;
    }

    fn compute_iou(&self, a: &BoundingBox, b: &BoundingBox) -> f32 {
        let x1 = a.left.max(b.left);
        let y1 = a.top.max(b.top);
        let x2 = a.right.min(b.right);
        let y2 = a.bottom.min(b.bottom);

        let intersection = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
        let area_a = a.width() * a.height();
        let area_b = b.width() * b.height();
        let union = area_a + area_b - intersection;

        if union <= 0.0 { 0.0 } else { intersection / union }
    }
}
