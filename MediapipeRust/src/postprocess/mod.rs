use crate::backend::BoundingBox;

pub struct NonMaxSuppression {
    pub max_output_size: usize,
    pub iou_threshold: f32,
    pub score_threshold: f32,
}

impl Default for NonMaxSuppression {
    fn default() -> Self {
        Self {
            max_output_size: 100,
            iou_threshold: 0.5,
            score_threshold: 0.0,
        }
    }
}

impl NonMaxSuppression {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn max_output_size(mut self, size: usize) -> Self {
        self.max_output_size = size;
        self
    }

    pub fn iou_threshold(mut self, threshold: f32) -> Self {
        self.iou_threshold = threshold;
        self
    }

    pub fn score_threshold(mut self, threshold: f32) -> Self {
        self.score_threshold = threshold;
        self
    }

    pub fn apply(&self, detections: &mut Vec<crate::backend::Detection>) -> Vec<crate::backend::Detection> {
        detections.retain(|d| {
            d.categories.first()
                .map(|c| c.score >= self.score_threshold)
                .unwrap_or(false)
        });

        detections.sort_by(|a, b| {
            b.categories.first()
                .map(|c| c.score)
                .unwrap_or(0.0)
                .partial_cmp(&a.categories.first().map(|c| c.score).unwrap_or(0.0))
                .unwrap()
        });

        let mut results = Vec::new();
        let mut used = vec![false; detections.len()];

        for i in 0..detections.len() {
            if results.len() >= self.max_output_size {
                break;
            }
            if used[i] {
                continue;
            }

            results.push(detections[i].clone());
            used[i] = true;

            for j in (i + 1)..detections.len() {
                if used[j] {
                    continue;
                }
                let iou = self.iou(&detections[i].bounding_box, &detections[j].bounding_box);
                if iou >= self.iou_threshold {
                    used[j] = true;
                }
            }
        }

        results
    }

    fn iou(&self, a: &BoundingBox, b: &BoundingBox) -> f32 {
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

pub struct Softmax;

impl Softmax {
    pub fn apply(data: &mut [f32]) {
        if data.is_empty() {
            return;
        }

        let max_val = data.iter().fold(f32::MIN, |a, &b| a.max(b));

        let sum: f32 = data.iter()
            .map(|&x| (x - max_val).exp())
            .sum();

        for x in data.iter_mut() {
            *x = ((*x - max_val).exp() / sum).min(1.0).max(0.0);
        }
    }
}

pub struct Sigmoid;

impl Sigmoid {
    pub fn apply(data: &mut [f32]) {
        for x in data.iter_mut() {
            *x = 1.0 / (1.0 + (-*x).exp());
        }
    }
}

pub fn decode_boxes(
    encoded: &[f32],
    priors: &[(f32, f32, f32, f32)],
    variances: &[f32; 4],
) -> Vec<BoundingBox> {
    let mut boxes = Vec::new();

    for (i, enc) in encoded.chunks(4).enumerate() {
        if enc.len() < 4 {
            break;
        }
        if i >= priors.len() {
            break;
        }

        let prior = priors[i];
        let v = variances;

        let cx = prior.0 + enc[0] * v[0] * prior.2;
        let cy = prior.1 + enc[1] * v[1] * prior.3;
        let w = prior.2 * (enc[2] * v[2]).exp();
        let h = prior.3 * (enc[3] * v[3]).exp();

        boxes.push(BoundingBox::new(
            cx - w / 2.0,
            cy - h / 2.0,
            cx + w / 2.0,
            cy + h / 2.0,
        ));
    }

    boxes
}

pub fn landmark_normalize(
    landmarks: &mut [(f32, f32, f32)],
    image_width: u32,
    image_height: u32,
) {
    for lm in landmarks.iter_mut() {
        lm.0 *= image_width as f32;
        lm.1 *= image_height as f32;
    }
}

pub fn clip_boxes(boxes: &mut [BoundingBox], image_width: u32, image_height: u32) {
    for b in boxes.iter_mut() {
        b.left = b.left.max(0.0).min(image_width as f32);
        b.right = b.right.max(0.0).min(image_width as f32);
        b.top = b.top.max(0.0).min(image_height as f32);
        b.bottom = b.bottom.max(0.0).min(image_height as f32);
    }
}
