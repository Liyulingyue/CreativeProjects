use base64::{engine::general_purpose::STANDARD, Engine};
use ndarray::s;
use ort::session::Session;
use ort::value::Value;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

const INPUT_W: usize = 192;
const INPUT_H: usize = 256;
const NUM_KEYPOINTS: usize = 17;

const MEAN: [f32; 3] = [123.675, 116.28, 103.53];
const STD: [f32; 3] = [58.395, 57.12, 57.375];

static SESSION: once_cell::sync::OnceCell<Arc<RwLock<Session>>> =
    once_cell::sync::OnceCell::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keypoint {
    pub x: f32,
    pub y: f32,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoseOutput {
    pub keypoints: Vec<Keypoint>,
    pub person_detected: bool,
}

pub fn init_model(model_path: &str) -> Result<(), String> {
    eprintln!("Loading RTMPose model from: {}", model_path);
    let session = Session::builder()
        .map_err(|e| format!("Failed to build session: {}", e))?
        .commit_from_file(std::path::Path::new(model_path))
        .map_err(|e| format!("Failed to load ONNX model: {}", e))?;
    SESSION
        .set(Arc::new(RwLock::new(session)))
        .map_err(|_| "Model already initialized".to_string())?;
    eprintln!("RTMPose model loaded successfully");
    Ok(())
}

fn preprocess(img_bytes: &[u8]) -> Result<ndarray::Array4<f32>, String> {
    let img =
        image::load_from_memory(img_bytes).map_err(|e| format!("Failed to decode image: {}", e))?;
    let resized = image::imageops::resize(
        &img,
        INPUT_W as u32,
        INPUT_H as u32,
        image::imageops::FilterType::Triangle,
    );
    let mut tensor = ndarray::Array4::<f32>::zeros((1, 3, INPUT_H, INPUT_W));
    for y in 0..INPUT_H {
        for x in 0..INPUT_W {
            let pixel = resized.get_pixel(x as u32, y as u32);
            tensor[[0, 0, y, x]] = (pixel[2] as f32 - MEAN[2]) / STD[2];
            tensor[[0, 1, y, x]] = (pixel[1] as f32 - MEAN[1]) / STD[1];
            tensor[[0, 2, y, x]] = (pixel[0] as f32 - MEAN[0]) / STD[0];
        }
    }
    Ok(tensor)
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

fn decode_simcc(
    sx: &ndarray::ArrayView<'_, f32, ndarray::IxDyn>,
    sy: &ndarray::ArrayView<'_, f32, ndarray::IxDyn>,
) -> Vec<Keypoint> {
    let hx = sx.shape()[2];
    let hy = sy.shape()[2];
    let mut keypoints = Vec::with_capacity(NUM_KEYPOINTS);
    for k in 0..NUM_KEYPOINTS {
        let xv = sx.slice(s![0, k, ..]);
        let yv = sy.slice(s![0, k, ..]);
        let (xi, xs) = peak(&xv.to_vec());
        let (yi, ys) = peak(&yv.to_vec());
        let x = xi as f32 / (hx as f32 / 2.0) * INPUT_W as f32;
        let y = yi as f32 / (hy as f32 / 2.0) * INPUT_H as f32;
        let conf = ((xs * ys).sqrt()).max(0.0).min(1.0);
        keypoints.push(Keypoint { x, y, confidence: conf });
    }
    keypoints
}

pub fn detect_pose_from_bytes(img_bytes: &[u8]) -> Result<PoseOutput, String> {
    let mut session = SESSION
        .get()
        .ok_or("Model not initialized")?
        .write()
        .map_err(|e| e.to_string())?;
    let tensor = preprocess(img_bytes)?;
    let input_value =
        Value::from_array(tensor.into_dyn()).map_err(|e| format!("Failed to create input: {}", e))?;
    let outputs = session
        .run(ort::inputs![input_value])
        .map_err(|e| format!("Inference failed: {}", e))?;
    let sx = outputs[0]
        .try_extract_array::<f32>()
        .map_err(|e| format!("Failed to extract simcc_x: {}", e))?;
    let sy = outputs[1]
        .try_extract_array::<f32>()
        .map_err(|e| format!("Failed to extract simcc_y: {}", e))?;
    let keypoints =
        decode_simcc(&ndarray::ArrayView::from(&sx), &ndarray::ArrayView::from(&sy));
    let avg_conf: f32 =
        keypoints.iter().map(|k| k.confidence).sum::<f32>() / NUM_KEYPOINTS as f32;
    let person_detected = avg_conf > 0.3;
    Ok(PoseOutput {
        keypoints,
        person_detected,
    })
}

pub fn detect_pose_from_base64(data: &str) -> Result<PoseOutput, String> {
    let data = data
        .strip_prefix("data:image/jpeg;base64,")
        .unwrap_or(data);
    let data = data
        .strip_prefix("data:image/png;base64,")
        .unwrap_or(data);
    let data = data
        .strip_prefix("data:image/webp;base64,")
        .unwrap_or(data);
    let decoded = STANDARD
        .decode(data)
        .map_err(|e| format!("Base64 decode failed: {}", e))?;
    detect_pose_from_bytes(&decoded)
}
