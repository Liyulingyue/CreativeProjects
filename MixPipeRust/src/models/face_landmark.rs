use std::any::Any;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::node::{Frame, FrameData, FrameMeta, Keypoint, Node, NodeError, Result};
use image::RgbImage;

pub struct MediaPipeFaceLandmark {
    session: Arc<Mutex<ort::session::Session>>,
    input_h: usize,
    input_w: usize,
}

impl MediaPipeFaceLandmark {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let session = ort::session::Session::builder()
            .map_err(|e| NodeError::Model(format!("build session: {}", e)))?
            .commit_from_file(path.as_ref())
            .map_err(|e| NodeError::Model(format!("load model: {}", e)))?;

        Ok(Self {
            session: Arc::new(Mutex::new(session)),
            input_h: 192,
            input_w: 192,
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

    pub fn infer(&self, pixels: &[u8], width: u32, height: u32) -> Result<Vec<Keypoint>> {
        let blob = preprocess_face(pixels, width, height, self.input_h, self.input_w);

        let input_value =
            ort::value::Value::from_array(blob.into_dyn())
                .map_err(|e| NodeError::Model(format!("create input: {}", e)))?;

        let mut session = self.session.lock().unwrap();
        let outputs = session
            .run(ort::inputs![input_value])
            .map_err(|e| NodeError::Model(format!("inference: {}", e)))?;

        let output = outputs[0]
            .try_extract_array::<f32>()
            .map_err(|e| NodeError::Model(format!("extract output: {}", e)))?;

        let keypoints = decode_face_landmark(
            &output,
            height as f32,
            width as f32,
            self.input_h as f32,
            self.input_w as f32,
        );

        Ok(keypoints)
    }
}

impl Clone for MediaPipeFaceLandmark {
    fn clone(&self) -> Self {
        Self {
            session: self.session.clone(),
            input_h: self.input_h,
            input_w: self.input_w,
        }
    }
}

impl Node for MediaPipeFaceLandmark {
    fn process(&self, frame: Frame) -> Result<Frame> {
        let img = match &frame.data {
            FrameData::Image(img) => img,
            _ => {
                return Err(NodeError::UnsupportedMediaType(
                    frame.meta.media_type.clone(),
                ))
            }
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
            "face_landmarks".to_string(),
            serde_json::to_value(keypoints).unwrap(),
        );
        Ok(out_frame)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn preprocess_face(
    pixels: &[u8],
    width: u32,
    height: u32,
    input_h: usize,
    input_w: usize,
) -> ndarray::Array4<f32> {
    let scale =
        (input_h as f32 / height as f32).min(input_w as f32 / width as f32);
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
    let pad_h_half = pad_h / 2;
    let pad_w_half = pad_w / 2;

    let mut padded = vec![0u8; (input_h as u32 * input_w as u32 * 3) as usize];
    for y in 0..new_h {
        for x in 0..new_w {
            let dst_idx =
                ((pad_h_half + y) * input_w as u32 + pad_w_half + x) * 3;
            let src_idx = (y * new_w + x) * 3;
            padded[dst_idx as usize] = resized[src_idx as usize];
            padded[(dst_idx + 1) as usize] = resized[(src_idx + 1) as usize];
            padded[(dst_idx + 2) as usize] = resized[(src_idx + 2) as usize];
        }
    }

    let mut blob =
        ndarray::Array4::<f32>::zeros((1, input_h, input_w, 3));
    for y in 0..input_h {
        for x in 0..input_w {
            let idx = (y * input_w + x) * 3;
            blob[[0, y, x, 0]] = padded[idx] as f32 / 255.0;
            blob[[0, y, x, 1]] = padded[idx + 1] as f32 / 255.0;
            blob[[0, y, x, 2]] = padded[idx + 2] as f32 / 255.0;
        }
    }

    blob
}

fn decode_face_landmark(
    output: &ndarray::ArrayView<'_, f32, ndarray::IxDyn>,
    orig_h: f32,
    orig_w: f32,
    input_h: f32,
    input_w: f32,
) -> Vec<Keypoint> {
    let data = output.as_slice().unwrap();
    let num_points = 468;

    let scale_x = orig_w / input_w;
    let scale_y = orig_h / input_h;

    let mut keypoints = Vec::with_capacity(num_points);
    for i in 0..num_points {
        let idx = i * 3;
        let x = data[idx] * scale_x;
        let y = data[idx + 1] * scale_y;
        let z = data[idx + 2];
        let confidence = (1.0 - z.abs() / 100.0).max(0.0).min(1.0);
        keypoints.push(Keypoint {
            x: x.max(0.0).min(orig_w),
            y: y.max(0.0).min(orig_h),
            confidence,
        });
    }

    keypoints
}

impl MediaPipeFaceLandmark {
    pub fn draw(&self, image: &mut RgbImage, keypoints: &[Keypoint]) {
        for kp in keypoints.iter() {
            if kp.confidence < 0.3 {
                continue;
            }
            let x = kp.x as i32;
            let y = kp.y as i32;
            let color = image::Rgb([0, 255, 0]);
            for dy in -2..=2 {
                for dx in -2..=2 {
                    if dx * dx + dy * dy <= 4 {
                        let px = x + dx;
                        let py = y + dy;
                        if px >= 0
                            && (px as u32) < image.width()
                            && py >= 0
                            && (py as u32) < image.height()
                        {
                            image.put_pixel(px as u32, py as u32, color);
                        }
                    }
                }
            }
        }
    }
}
