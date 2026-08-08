use std::any::Any;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::node::{Frame, FrameData, FrameMeta, Keypoint, Node, NodeError, Result};

pub struct RtmPose {
    session: Arc<Mutex<ort::session::Session>>,
    input_h: usize,
    input_w: usize,
}

impl RtmPose {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let session = ort::session::Session::builder()
            .map_err(|e| NodeError::Model(format!("build session: {}", e)))?
            .commit_from_file(path.as_ref())
            .map_err(|e| NodeError::Model(format!("load model: {}", e)))?;

        let path_str = path.as_ref().to_string_lossy().to_lowercase();
        let (input_h, input_w) = if path_str.contains("face") || path_str.contains("hand") {
            (256, 256)
        } else {
            (256, 192)
        };

        Ok(Self { session: Arc::new(Mutex::new(session)), input_h, input_w })
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

    pub fn infer(&self, pixels: &[u8], width: u32, height: u32) -> Result<Vec<Keypoint>> {
        let (blob, scale, left, top, new_w, new_h) = preprocess_rtmpose(pixels, width, height, self.input_h, self.input_w);

        let input_value = ort::value::Value::from_array(blob.into_dyn())
            .map_err(|e| NodeError::Model(format!("create input: {}", e)))?;

        let mut session = self.session.lock().unwrap();
        let outputs = session
            .run(ort::inputs![input_value])
            .map_err(|e| NodeError::Model(format!("inference: {}", e)))?;

        let simcc_x = outputs[0]
            .try_extract_array::<f32>()
            .map_err(|e| NodeError::Model(format!("extract simcc_x: {}", e)))?;
        let simcc_y = outputs[1]
            .try_extract_array::<f32>()
            .map_err(|e| NodeError::Model(format!("extract simcc_y: {}", e)))?;

        let keypoints = decode_simcc(
            &simcc_x,
            &simcc_y,
            height as f32,
            width as f32,
            self.input_h as f32,
            self.input_w as f32,
            left,
            top,
            new_w,
            new_h,
            scale,
        );

        Ok(keypoints)
    }
}

impl Clone for RtmPose {
    fn clone(&self) -> Self {
        Self {
            session: self.session.clone(),
            input_h: self.input_h,
            input_w: self.input_w,
        }
    }
}

impl Node for RtmPose {
    fn process(&self, frame: Frame) -> Result<Frame> {
        let img = match &frame.data {
            FrameData::Image(img) => img,
            _ => return Err(NodeError::UnsupportedMediaType(frame.meta.media_type.clone())),
        };

        let keypoints = self.infer(&img.pixels, img.width, img.height)?;

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
            "keypoints".to_string(),
            serde_json::to_value(keypoints).unwrap(),
        );
        Ok(out_frame)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn preprocess_rtmpose(
    pixels: &[u8],
    width: u32,
    height: u32,
    input_h: usize,
    input_w: usize,
) -> (ndarray::Array4<f32>, f32, i32, i32, u32, u32) {
    let scale = (input_h as f32 / height as f32).min(input_w as f32 / width as f32);
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

    let pad_h = input_h as u32 - new_h;
    let pad_w = input_w as u32 - new_w;
    let top = pad_h / 2;
    let left = pad_w / 2;

    let mut padded = vec![114u8; (input_h as u32 * input_w as u32 * 3) as usize];
    for y in 0..new_h {
        for x in 0..new_w {
            let dst_idx = ((top + y) * input_w as u32 + left + x) * 3;
            let src_idx = (y * new_w + x) * 3;
            padded[dst_idx as usize] = resized[src_idx as usize];
            padded[(dst_idx + 1) as usize] = resized[(src_idx + 1) as usize];
            padded[(dst_idx + 2) as usize] = resized[(src_idx + 2) as usize];
        }
    }

    let mean: [f32; 3] = [123.675, 116.28, 103.53];
    let std: [f32; 3] = [58.395, 57.12, 57.375];

    let mut blob = ndarray::Array4::<f32>::zeros((1, 3, input_h, input_w));
    for y in 0..input_h {
        for x in 0..input_w {
            let idx = (y * input_w + x) * 3;
            blob[[0, 0, y, x]] = (padded[idx] as f32 - mean[2]) / std[2];
            blob[[0, 1, y, x]] = (padded[idx + 1] as f32 - mean[1]) / std[1];
            blob[[0, 2, y, x]] = (padded[idx + 2] as f32 - mean[0]) / std[0];
        }
    }

    (blob, scale, left as i32, top as i32, new_w, new_h)
}

fn decode_simcc(
    simcc_x: &ndarray::ArrayView<'_, f32, ndarray::IxDyn>,
    simcc_y: &ndarray::ArrayView<'_, f32, ndarray::IxDyn>,
    _orig_h: f32,
    _orig_w: f32,
    input_h: f32,
    input_w: f32,
    left: i32,
    top: i32,
    _new_w: u32,
    _new_h: u32,
    scale: f32,
) -> Vec<Keypoint> {
    let num_kpts = simcc_x.shape()[1];
    let mut keypoints = Vec::with_capacity(num_kpts);

    let simcc_x_slice = simcc_x.as_slice().unwrap();
    let simcc_y_slice = simcc_y.as_slice().unwrap();

    for k in 0..num_kpts {
        let xv = &simcc_x_slice[k * simcc_x.shape()[2]..(k + 1) * simcc_x.shape()[2]];
        let yv = &simcc_y_slice[k * simcc_y.shape()[2]..(k + 1) * simcc_y.shape()[2]];

        let (xi, xs) = peak(xv);
        let (yi, ys) = peak(yv);

        let x_padded = xi as f32 / simcc_x.shape()[2] as f32 * input_w;
        let y_padded = yi as f32 / simcc_y.shape()[2] as f32 * input_h;

        let x_resized = x_padded - left as f32;
        let y_resized = y_padded - top as f32;

        let x = x_resized / scale;
        let y = y_resized / scale;

        let conf = (xs * ys).sqrt().max(0.0).min(1.0);

        keypoints.push(Keypoint { x, y, confidence: conf });
    }

    keypoints
}

fn peak(h: &[f32]) -> (usize, f32) {
    let mut mi = 0usize;
    let mut mv = f32::MIN;
    for (i, &v) in h.iter().enumerate() {
        if v > mv {
            mv = v;
            mi = i;
        }
    }
    (mi, mv)
}
