use std::any::Any;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::node::{Detection, Frame, FrameData, FrameMeta, Node, NodeError, Result};

pub struct MediaPipeFaceDetection {
    session: Arc<Mutex<ort::session::Session>>,
    input_h: usize,
    input_w: usize,
    short_range: bool,
}

impl MediaPipeFaceDetection {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let session = ort::session::Session::builder()
            .map_err(|e| NodeError::Model(format!("build session: {}", e)))?
            .commit_from_file(path.as_ref())
            .map_err(|e| NodeError::Model(format!("load model: {}", e)))?;

        let path_str = path.as_ref().to_string_lossy().to_lowercase();
        let short_range = path_str.contains("short");

        let (input_h, input_w) = if short_range {
            (128, 128)
        } else {
            (192, 192)
        };

        Ok(Self {
            session: Arc::new(Mutex::new(session)),
            input_h,
            input_w,
            short_range,
        })
    }

    pub fn from_pretrained(model: crate::model_hub::PretrainedModel) -> Result<Self> {
        let path =
            crate::model_hub::get_model_path(model).ok_or_else(|| {
                NodeError::Model("Cannot get model cache path".to_string())
            })?;

        if !path.exists() {
            crate::model_hub::download_model_blocking(model)
                .map_err(|e| NodeError::Model(format!("Download failed: {}", e)))?;
        }

        Self::from_file(&path)
    }

    pub fn infer(&self, pixels: &[u8], width: u32, height: u32) -> Result<Vec<Detection>> {
        let blob = preprocess_face_detect(pixels, width, height, self.input_h, self.input_w);

        let input_value =
            ort::value::Value::from_array(blob.into_dyn())
                .map_err(|e| NodeError::Model(format!("create input: {}", e)))?;

        let mut session = self.session.lock().unwrap();
        let outputs = session
            .run(ort::inputs![input_value])
            .map_err(|e| NodeError::Model(format!("inference: {}", e)))?;

        let regressors = outputs[0]
            .try_extract_array::<f32>()
            .map_err(|e| NodeError::Model(format!("extract regressors: {}", e)))?;
        let classifiers = outputs[1]
            .try_extract_array::<f32>()
            .map_err(|e| NodeError::Model(format!("extract classifiers: {}", e)))?;

        let detections = decode_face_detect(
            &regressors,
            &classifiers,
            height as f32,
            width as f32,
            self.input_h as f32,
            self.input_w as f32,
            self.short_range,
        );

        Ok(detections)
    }
}

impl Clone for MediaPipeFaceDetection {
    fn clone(&self) -> Self {
        Self {
            session: self.session.clone(),
            input_h: self.input_h,
            input_w: self.input_w,
            short_range: self.short_range,
        }
    }
}

impl Node for MediaPipeFaceDetection {
    fn process(&self, frame: Frame) -> Result<Frame> {
        let img = match &frame.data {
            FrameData::Image(img) => img,
            _ => {
                return Err(NodeError::UnsupportedMediaType(
                    frame.meta.media_type.clone(),
                ))
            }
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
        out_frame.meta.custom.insert(
            "face_detections".to_string(),
            serde_json::to_value(detections).unwrap(),
        );
        Ok(out_frame)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn preprocess_face_detect(
    pixels: &[u8],
    width: u32,
    height: u32,
    input_h: usize,
    input_w: usize,
) -> ndarray::Array4<f32> {
    let mut resized = Vec::with_capacity((input_w * input_h * 3) as usize);
    for y in 0..input_h {
        for x in 0..input_w {
            let src_x = (x as f32 / input_w as f32 * width as f32) as u32;
            let src_y = (y as f32 / input_h as f32 * height as f32) as u32;
            let src_idx = (src_y * width + src_x) * 3;
            resized.push(pixels[src_idx as usize]);
            resized.push(pixels[(src_idx + 1) as usize]);
            resized.push(pixels[(src_idx + 2) as usize]);
        }
    }

    let mut blob =
        ndarray::Array4::<f32>::zeros((1, input_h, input_w, 3));
    for y in 0..input_h {
        for x in 0..input_w {
            let idx = (y * input_w + x) * 3;
            blob[[0, y, x, 0]] = resized[idx] as f32 / 255.0;
            blob[[0, y, x, 1]] = resized[idx + 1] as f32 / 255.0;
            blob[[0, y, x, 2]] = resized[idx + 2] as f32 / 255.0;
        }
    }

    blob
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn decode_face_detect(
    regressors: &ndarray::ArrayView<'_, f32, ndarray::IxDyn>,
    classifiers: &ndarray::ArrayView<'_, f32, ndarray::IxDyn>,
    orig_h: f32,
    orig_w: f32,
    input_h: f32,
    input_w: f32,
    short_range: bool,
) -> Vec<Detection> {
    let reg_shape = regressors.shape();
    let num_candidates = reg_shape[1];  // 2304
    let score_thresh = 0.5;

    let (grid_w, stride_x, stride_y) = if short_range {
        (32, 4.0, 128.0 / 28.0)
    } else {
        (48, 4.0, 4.0)
    };

    let reg_data = regressors.as_slice().unwrap();
    let cls_data = classifiers.as_slice().unwrap();

    let mut detections = Vec::new();

    for idx in 0..num_candidates {
        let score = sigmoid(cls_data[idx]);
        if score < score_thresh {
            continue;
        }

        let reg_offset = idx * 16;  // 16 values per candidate
        let x_offset = reg_data[reg_offset];
        let y_offset = reg_data[reg_offset + 1];
        let w = reg_data[reg_offset + 2];
        let h = reg_data[reg_offset + 3];

        let y_cell = idx / grid_w;
        let x_cell = idx % grid_w;

        let x_center = (x_cell as f32 + x_offset) * stride_x;
        let y_center = (y_cell as f32 + y_offset) * stride_y;

        let x1 = x_center - w / 2.0;
        let y1 = y_center - h / 2.0;
        let x2 = x_center + w / 2.0;
        let y2 = y_center + h / 2.0;

        let scale_x = orig_w / input_w;
        let scale_y = orig_h / input_h;

        let x1 = (x1 * scale_x).max(0.0).min(orig_w);
        let y1 = (y1 * scale_y).max(0.0).min(orig_h);
        let x2 = (x2 * scale_x).max(0.0).min(orig_w);
        let y2 = (y2 * scale_y).max(0.0).min(orig_h);

        detections.push(Detection {
            bbox: [x1, y1, x2, y2],
            score,
            label: 0,
        });
    }

    detections
}
