use std::any::Any;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::node::{Detection, Frame, FrameData, FrameMeta, Node, NodeError, Result};

pub struct RtmDet {
    session: Arc<Mutex<ort::session::Session>>,
}

impl RtmDet {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let session = ort::session::Session::builder()
            .map_err(|e| NodeError::Model(format!("build session: {}", e)))?
            .commit_from_file(path.as_ref())
            .map_err(|e| NodeError::Model(format!("load model: {}", e)))?;
        Ok(Self { session: Arc::new(Mutex::new(session)) })
    }

    pub fn from_pretrained(model: crate::model_hub::PretrainedModel) -> Result<Self> {
        let path = crate::model_hub::get_model_path(model)
            .ok_or_else(|| NodeError::Model("Cannot get model cache path".to_string()))?;
        
        if !path.exists() {
            crate::model_hub::download_model_blocking(model)
                .map_err(|e| NodeError::Model(format!("Download failed: {}", e)))?;
        }
        
        Self::from_file(&path)
    }

    pub fn infer(&self, pixels: &[u8], width: u32, height: u32) -> Result<Vec<Detection>> {
        let input_size = 640u32;
        let (blob, _scale, left, top, new_w, new_h) = preprocess_rtmdet(pixels, width, height, input_size);

        let input_value = ort::value::Value::from_array(blob.into_dyn())
            .map_err(|e| NodeError::Model(format!("create input: {}", e)))?;

        let mut session = self.session.lock().unwrap();
        let outputs = session
            .run(ort::inputs![input_value])
            .map_err(|e| NodeError::Model(format!("inference: {}", e)))?;

        let dets = outputs[0]
            .try_extract_array::<f32>()
            .map_err(|e| NodeError::Model(format!("extract dets: {}", e)))?;
        let labels = outputs[1]
            .try_extract_array::<i64>()
            .map_err(|e| NodeError::Model(format!("extract labels: {}", e)))?;

        let mut detections = Vec::new();
        let score_thresh = 0.3f32;

        for i in 0..dets.shape()[1] {
            let x1 = dets[[0, i, 0]];
            let y1 = dets[[0, i, 1]];
            let x2 = dets[[0, i, 2]];
            let y2 = dets[[0, i, 3]];
            let score = dets[[0, i, 4]];
            let label = labels[[0, i]] as i32;

            if score > score_thresh && label == 0 {
                let x1_orig = (x1 - left as f32) / new_w as f32 * width as f32;
                let y1_orig = (y1 - top as f32) / new_h as f32 * height as f32;
                let x2_orig = (x2 - left as f32) / new_w as f32 * width as f32;
                let y2_orig = (y2 - top as f32) / new_h as f32 * height as f32;
                detections.push(Detection {
                    bbox: [x1_orig, y1_orig, x2_orig, y2_orig],
                    score,
                    label,
                });
            }
        }

        detections = nms(detections, 0.65);
        Ok(detections)
    }
}

impl Clone for RtmDet {
    fn clone(&self) -> Self {
        Self { session: self.session.clone() }
    }
}

impl Node for RtmDet {
    fn process(&self, frame: Frame) -> Result<Frame> {
        let img = match &frame.data {
            FrameData::Image(img) => img,
            _ => return Err(NodeError::UnsupportedMediaType(frame.meta.media_type.clone())),
        };

        let detections = self.infer(&img.pixels, img.width, img.height)?;

        let mut out_frame = Frame {
            data: frame.data.clone(),
            meta: FrameMeta {
                timestamp_ms: frame.meta.timestamp_ms,
                source: frame.meta.source.clone(),
                media_type: frame.meta.media_type.clone(),
                custom: frame.meta.custom.clone(),
            },
        };
        out_frame.set_detections(detections);
        Ok(out_frame)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn preprocess_rtmdet(pixels: &[u8], width: u32, height: u32, input_size: u32) -> (ndarray::Array4<f32>, f32, i32, i32, u32, u32) {
    let scale = (input_size as f32 / width as f32).min(input_size as f32 / height as f32);
    let new_w = (width as f32 * scale) as u32;
    let new_h = (height as f32 * scale) as u32;

    let mut resized = Vec::with_capacity((new_w * new_h * 3) as usize);
    for y in 0..new_h {
        for x in 0..new_w {
            let src_x = (x as f32 / new_w as f32 * width as f32) as u32;
            let src_y = (y as f32 / new_h as f32 * height as f32) as u32;
            let src_idx = (src_y * width + src_x) * 3;
            resized.push(pixels[src_idx as usize]);
            resized.push(pixels[(src_idx + 1) as usize]);
            resized.push(pixels[(src_idx + 2) as usize]);
        }
    }

    let pad_h = input_size - new_h;
    let pad_w = input_size - new_w;
    let top = pad_h / 2;
    let left = pad_w / 2;

    let mut padded = vec![0u8; (input_size * input_size * 3) as usize];
    for y in 0..new_h {
        for x in 0..new_w {
            let dst_idx = ((top + y) * input_size + left + x) * 3;
            let src_idx = (y * new_w + x) * 3;
            padded[dst_idx as usize] = resized[src_idx as usize];
            padded[(dst_idx + 1) as usize] = resized[(src_idx + 1) as usize];
            padded[(dst_idx + 2) as usize] = resized[(src_idx + 2) as usize];
        }
    }

    let mean: [f32; 3] = [123.675, 116.28, 103.53];
    let std: [f32; 3] = [58.395, 57.12, 57.375];

    let mut blob = ndarray::Array4::<f32>::zeros((1, 3, input_size as usize, input_size as usize));
    for y in 0..input_size {
        for x in 0..input_size {
            let idx = (y * input_size + x) * 3;
            blob[[0, 0, y as usize, x as usize]] = (padded[(idx + 2) as usize] as f32 - mean[0]) / std[0];
            blob[[0, 1, y as usize, x as usize]] = (padded[(idx + 1) as usize] as f32 - mean[1]) / std[1];
            blob[[0, 2, y as usize, x as usize]] = (padded[idx as usize] as f32 - mean[2]) / std[2];
        }
    }

    (blob, scale, left as i32, top as i32, new_w, new_h)
}

fn nms(mut dets: Vec<Detection>, iou_thresh: f32) -> Vec<Detection> {
    if dets.is_empty() {
        return dets;
    }

    dets.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

    let mut keep = Vec::new();
    while !dets.is_empty() {
        let best = dets.remove(0);
        keep.push(best);

        dets.retain(|d| {
            let iou = box_iou(&keep[keep.len() - 1].bbox, &d.bbox);
            iou <= iou_thresh
        });
    }

    keep
}

fn box_iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let x1 = a[0].max(b[0]);
    let y1 = a[1].max(b[1]);
    let x2 = a[2].min(b[2]);
    let y2 = a[3].min(b[3]);

    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let area_a = (a[2] - a[0]) * (a[3] - a[1]);
    let area_b = (b[2] - b[0]) * (b[3] - b[1]);
    let union = area_a + area_b - inter;

    inter / union
}
