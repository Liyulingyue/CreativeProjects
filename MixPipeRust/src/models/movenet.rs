use std::any::Any;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::node::{Frame, FrameData, FrameMeta, Keypoint, Node, NodeError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveNetVariant {
    SinglePoseLightning,
    SinglePoseThunder,
    MultiPoseLightning,
}

pub struct MoveNet {
    session: Arc<Mutex<ort::session::Session>>,
    input_h: usize,
    input_w: usize,
    variant: MoveNetVariant,
}

impl MoveNet {
    pub fn from_file<P: AsRef<Path>>(path: P, variant: MoveNetVariant) -> Result<Self> {
        let session = ort::session::Session::builder()
            .map_err(|e| NodeError::Model(format!("build session: {}", e)))?
            .commit_from_file(path.as_ref())
            .map_err(|e| NodeError::Model(format!("load model: {}", e)))?;

        let (input_h, input_w) = match variant {
            MoveNetVariant::SinglePoseLightning => (192, 192),
            MoveNetVariant::SinglePoseThunder => (256, 256),
            MoveNetVariant::MultiPoseLightning => (256, 256),
        };

        Ok(Self {
            session: Arc::new(Mutex::new(session)),
            input_h,
            input_w,
            variant,
        })
    }

    pub fn from_onnx_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_lowercase();
        let variant = if path_str.contains("multipose") {
            MoveNetVariant::MultiPoseLightning
        } else if path_str.contains("thunder") {
            MoveNetVariant::SinglePoseThunder
        } else {
            MoveNetVariant::SinglePoseLightning
        };
        Self::from_file(path, variant)
    }

    pub fn infer(&self, pixels: &[u8], width: u32, height: u32) -> Result<Vec<Vec<Keypoint>>> {
        let (blob, scale, pad_h, pad_w) =
            preprocess_movenet(pixels, width, height, self.input_h, self.input_w);

        let input_value = ort::value::Value::from_array(blob.into_dyn())
            .map_err(|e| NodeError::Model(format!("create input: {}", e)))?;

        let mut session = self.session.lock().unwrap();
        let outputs = session
            .run(ort::inputs![input_value])
            .map_err(|e| NodeError::Model(format!("inference: {}", e)))?;

        let output = outputs[0]
            .try_extract_array::<f32>()
            .map_err(|e| NodeError::Model(format!("extract output: {}", e)))?;

        let persons = decode_movenet(
            &output,
            self.variant,
            height as f32,
            width as f32,
            self.input_h as f32,
            self.input_w as f32,
            pad_h,
            pad_w,
            scale,
        );

        Ok(persons)
    }
}

impl Clone for MoveNet {
    fn clone(&self) -> Self {
        Self {
            session: self.session.clone(),
            input_h: self.input_h,
            input_w: self.input_w,
            variant: self.variant,
        }
    }
}

impl Node for MoveNet {
    fn process(&self, frame: Frame) -> Result<Frame> {
        let img = match &frame.data {
            FrameData::Image(img) => img,
            _ => {
                return Err(NodeError::UnsupportedMediaType(
                    frame.meta.media_type.clone(),
                ))
            }
        };

        let persons = self.infer(&img.pixels, img.width, img.height)?;

        let mut out_frame = Frame {
            data: frame.data.clone(),
            meta: FrameMeta {
                timestamp_ms: frame.meta.timestamp_ms,
                source: frame.meta.source.clone(),
                media_type: frame.meta.media_type.clone(),
                custom: frame.meta.custom.clone(),
            },
        };

        let keypoints_json: Vec<Vec<crate::node::Keypoint>> = persons;
        out_frame.meta.custom.insert(
            "movenet_keypoints".to_string(),
            serde_json::to_value(keypoints_json).unwrap(),
        );
        Ok(out_frame)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn preprocess_movenet(
    pixels: &[u8],
    width: u32,
    height: u32,
    input_h: usize,
    input_w: usize,
) -> (ndarray::Array4<f32>, f32, f32, f32) {
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

    let pad_h = input_h as f32 - new_h as f32;
    let pad_w = input_w as f32 - new_w as f32;
    let pad_h_half = pad_h / 2.0;
    let pad_w_half = pad_w / 2.0;

    let mut padded = vec![127u8; (input_h as u32 * input_w as u32 * 3) as usize];
    for y in 0..new_h {
        for x in 0..new_w {
            let dst_idx = (((pad_h_half as u32 + y) * input_w as u32) + pad_w_half as u32 + x) * 3;
            let src_idx = (y * new_w + x) * 3;
            padded[dst_idx as usize] = resized[src_idx as usize];
            padded[(dst_idx + 1) as usize] = resized[(src_idx + 1) as usize];
            padded[(dst_idx + 2) as usize] = resized[(src_idx + 2) as usize];
        }
    }

    let mut blob = ndarray::Array4::<f32>::zeros((1, 3, input_h, input_w));
    for y in 0..input_h {
        for x in 0..input_w {
            let idx = (y * input_w + x) * 3;
            blob[[0, 0, y, x]] = (padded[idx] as f32 - 128.0) / 128.0;
            blob[[0, 1, y, x]] = (padded[idx + 1] as f32 - 128.0) / 128.0;
            blob[[0, 2, y, x]] = (padded[idx + 2] as f32 - 128.0) / 128.0;
        }
    }

    (blob, scale, pad_h_half, pad_w_half)
}

fn decode_movenet(
    output: &ndarray::ArrayView<'_, f32, ndarray::IxDyn>,
    variant: MoveNetVariant,
    orig_h: f32,
    orig_w: f32,
    input_h: f32,
    input_w: f32,
    pad_h: f32,
    pad_w: f32,
    scale: f32,
) -> Vec<Vec<Keypoint>> {
    let shape = output.shape();
    let data = output.as_slice().unwrap();

    match variant {
        MoveNetVariant::SinglePoseLightning | MoveNetVariant::SinglePoseThunder => {
            let num_kpts = 17;
            let mut persons = Vec::new();
            let mut person_kpts = Vec::with_capacity(num_kpts);

            for k in 0..num_kpts {
                let y_val = data[k * 3];
                let x_val = data[k * 3 + 1];
                let score = data[k * 3 + 2];

                let x_padded = x_val * input_w - pad_w;
                let y_padded = y_val * input_h - pad_h;
                let x = x_padded / scale;
                let y = y_padded / scale;

                person_kpts.push(Keypoint {
                    x: x.max(0.0).min(orig_w),
                    y: y.max(0.0).min(orig_h),
                    confidence: score.max(0.0).min(1.0),
                });
            }
            persons.push(person_kpts);
            persons
        }
        MoveNetVariant::MultiPoseLightning => {
            let max_persons = 6;
            let num_kpts = 17;
            let stride = 3;

            let mut persons = Vec::new();
            for p in 0..max_persons {
                let person_offset = p * num_kpts * stride;
                let score = data[person_offset + num_kpts * stride - 1];

                if score < 0.2 {
                    continue;
                }

                let mut kpts = Vec::with_capacity(num_kpts);
                for k in 0..num_kpts {
                    let base_idx = person_offset + k * stride;
                    let y_val = data[base_idx];
                    let x_val = data[base_idx + 1];
                    let kpt_score = data[base_idx + 2];

                    let x_padded = x_val * input_w - pad_w;
                    let y_padded = y_val * input_h - pad_h;
                    let x = x_padded / scale;
                    let y = y_padded / scale;

                    kpts.push(Keypoint {
                        x: x.max(0.0).min(orig_w),
                        y: y.max(0.0).min(orig_h),
                        confidence: kpt_score.max(0.0).min(1.0),
                    });
                }
                persons.push(kpts);
            }
            persons
        }
    }
}
