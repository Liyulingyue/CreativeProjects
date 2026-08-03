use crate::backend::{BoundingBox, Class, Detection, InferenceBackend, Model, Session, Tensor, TensorType, Error};
use crate::labels::get_coco_label;

#[derive(Clone, Debug)]
pub enum LabelSource {
    Coco,
    Custom(Vec<String>),
    None,
}

#[derive(Clone, Debug)]
pub struct ObjectDetectorOptions {
    pub min_score_threshold: f32,
    pub min_suppression_threshold: f32,
    pub label_allow_list: Vec<String>,
    pub label_deny_list: Vec<String>,
    pub num_classes: u32,
}

impl Default for ObjectDetectorOptions {
    fn default() -> Self {
        Self {
            min_score_threshold: 0.5,
            min_suppression_threshold: 0.5,
            label_allow_list: Vec::new(),
            label_deny_list: Vec::new(),
            num_classes: 90,
        }
    }
}

pub struct ObjectDetectorBuilder {
    max_results: i32,
    options: ObjectDetectorOptions,
    label_source: LabelSource,
}

impl ObjectDetectorBuilder {
    pub fn new() -> Self {
        Self {
            max_results: 10,
            options: ObjectDetectorOptions::default(),
            label_source: LabelSource::Coco,
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

    pub fn label_source(mut self, source: LabelSource) -> Self {
        self.label_source = source;
        self
    }

    pub fn build_from_file<B: InferenceBackend>(self, backend: &B, path: &str) -> Result<ObjectDetector, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(backend, data)
    }

    pub fn build_from_buffer<B: InferenceBackend>(self, backend: &B, buffer: Vec<u8>) -> Result<ObjectDetector, Error> {
        let (model, session) = backend.load_model_and_session(&buffer)?;
        Ok(ObjectDetector {
            model,
            session,
            max_results: self.max_results,
            options: self.options,
            label_source: self.label_source,
        })
    }
}

pub struct ObjectDetector {
    model: Model,
    session: Session,
    max_results: i32,
    options: ObjectDetectorOptions,
    label_source: LabelSource,
}

impl ObjectDetector {
    pub fn detect(&mut self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<Detection>, Error> {
        let input_tensor = self.prepare_input(image_data, width, height)?;
        self.session.set_input(0, &input_tensor)?;
        self.session.compute()?;

        let is_tflite_format = self.model.outputs.len() >= 4
            && self.model.outputs[3].shape == vec![1];

        if is_tflite_format {
            let num_boxes = self.model.outputs[0].shape[1];

            let mut boxes_tensor = Tensor::empty(TensorType::F32, self.model.outputs[0].shape.clone());
            let mut scores_tensor = Tensor::empty(TensorType::F32, self.model.outputs[1].shape.clone());
            let mut classes_tensor = Tensor::empty(TensorType::F32, self.model.outputs[2].shape.clone());
            let mut _num_det_tensor = Tensor::empty(TensorType::F32, self.model.outputs[3].shape.clone());

            self.session.get_output(0, &mut boxes_tensor)?;
            self.session.get_output(1, &mut scores_tensor)?;
            self.session.get_output(2, &mut classes_tensor)?;
            self.session.get_output(3, &mut _num_det_tensor)?;

            self.postprocess_tflite_format(&boxes_tensor, &scores_tensor, &classes_tensor, num_boxes, width, height)
        } else {
            let boxes_size = self.model.outputs[0].shape.iter().product();
            let scores_size = self.model.outputs[1].shape.iter().product();

            let mut boxes_tensor = Tensor::empty(TensorType::F32, vec![boxes_size]);
            let mut scores_tensor = Tensor::empty(TensorType::F32, vec![scores_size]);

            self.session.get_output(0, &mut boxes_tensor)?;
            self.session.get_output(1, &mut scores_tensor)?;

            self.postprocess_output(&boxes_tensor, &scores_tensor, width, height)
        }
    }

    fn prepare_input(&self, image_data: &[u8], _width: u32, _height: u32) -> Result<Tensor, Error> {
        let input_shape = &self.model.inputs[0].shape;
        let input_type = self.model.inputs[0].tensor_type;
        let tensor = Tensor::new(input_type, input_shape.clone(), image_data.to_vec());
        Ok(tensor)
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
        let num_detections = (boxes_data.len() / 4).min(self.max_results as usize);

        for i in 0..num_detections {
            let score = scores_data[i];
            if score < self.options.min_score_threshold {
                continue;
            }

            let j = i * 4;
            let label = self.get_label(0, i);

            let ymin = boxes_data[j] * img_height as f32;
            let xmin = boxes_data[j + 1] * img_width as f32;
            let ymax = boxes_data[j + 2] * img_height as f32;
            let xmax = boxes_data[j + 3] * img_width as f32;

            let detection = Detection {
                bounding_box: BoundingBox::new(xmin, ymin, xmax, ymax),
                categories: vec![Class::new(0, score, label)],
            };
            detections.push(detection);
        }

        self.apply_nms(&mut detections);
        Ok(detections)
    }

    fn postprocess_tflite_format(
        &self,
        boxes: &Tensor,
        scores: &Tensor,
        classes: &Tensor,
        num_boxes: usize,
        img_width: u32,
        img_height: u32,
    ) -> Result<Vec<Detection>, Error> {
        let boxes_data = boxes.as_f32();
        let mut scores_data = scores.as_f32().to_vec();
        let classes_data = classes.as_f32();

        crate::postprocess::Sigmoid::apply(&mut scores_data);

        let mut detections = Vec::new();
        let num_detections = num_boxes.min(self.max_results as usize);

        for i in 0..num_detections {
            let score = scores_data[i];
            if score < self.options.min_score_threshold {
                continue;
            }

            let class_id = classes_data[i] as i32;
            let j = i * 4;
            let label = self.get_label(class_id, i);

            let xmin = boxes_data[j] * img_width as f32;
            let ymin = boxes_data[j + 1] * img_height as f32;
            let xmax = boxes_data[j + 2] * img_width as f32;
            let ymax = boxes_data[j + 3] * img_height as f32;

            let detection = Detection {
                bounding_box: BoundingBox::new(xmin, ymin, xmax, ymax),
                categories: vec![Class::new(class_id, score, label)],
            };
            detections.push(detection);
        }

        self.apply_nms(&mut detections);
        Ok(detections)
    }

    fn get_label(&self, class_id: i32, _index: usize) -> String {
        match &self.label_source {
            LabelSource::Coco => {
                get_coco_label(class_id as usize).unwrap_or("unknown").to_string()
            }
            LabelSource::Custom(labels) => {
                labels.get(class_id as usize).cloned().unwrap_or_else(|| format!("class_{}", class_id))
            }
            LabelSource::None => {
                format!("object_{}", class_id)
            }
        }
    }

    fn apply_nms(&self, detections: &mut Vec<Detection>) {
        let threshold = self.options.min_suppression_threshold;
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
