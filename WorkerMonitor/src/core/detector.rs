use mixpipe::{MoveNet, MoveNetVariant, PretrainedModel, download_model_blocking, Keypoint as MixpipeKeypoint};
use once_cell::sync::OnceCell;
use std::sync::Mutex;

static DETECTOR: OnceCell<Mutex<MoveNet>> = OnceCell::new();

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Keypoint {
    pub x: f32,
    pub y: f32,
    pub confidence: f32,
}

impl From<&MixpipeKeypoint> for Keypoint {
    fn from(kp: &MixpipeKeypoint) -> Self {
        Self {
            x: kp.x,
            y: kp.y,
            confidence: kp.confidence,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PoseOutput {
    pub keypoints: Vec<Keypoint>,
    pub person_detected: bool,
}

pub struct PoseDetector;

impl PoseDetector {
    pub fn init() -> Result<(), String> {
        if DETECTOR.get().is_some() {
            println!("[Detector] MoveNet already initialized");
            return Ok(());
        }

        let model_dir = dirs::data_local_dir()
            .ok_or("cannot find local data dir")?
            .join("mixpipe")
            .join("models");

        std::fs::create_dir_all(&model_dir)
            .map_err(|e| format!("failed to create model dir: {}", e))?;

        let model_path = model_dir.join("movenet_singlepose_lightning.onnx");

        if !model_path.exists() {
            println!("[Detector] Downloading MoveNet SinglePose Lightning model...");
            let downloaded = download_model_blocking(PretrainedModel::MoveNetSinglePoseLightning)
                .map_err(|e| format!("download failed: {}", e))?;
            std::fs::copy(&downloaded, &model_path)
                .map_err(|e| format!("failed to copy model: {}", e))?;
            println!("[Detector] Model downloaded!");
        }

        println!("[Detector] Loading MoveNet model...");
        let model = MoveNet::from_file(&model_path, MoveNetVariant::SinglePoseLightning)
            .map_err(|e| format!("MoveNet init failed: {}", e))?;

        let _ = DETECTOR.set(Mutex::new(model))
            .map_err(|_| "MoveNet already initialized")?;

        println!("[Detector] MoveNet initialized successfully");
        Ok(())
    }

    pub fn detect(image_bytes: &[u8]) -> Result<PoseOutput, String> {
        let detector = DETECTOR.get().ok_or_else(|| "detector not initialized".to_string())?;
        let detector = match detector.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                eprintln!("[Detector] Mutex poisoned, recovering");
                poisoned.into_inner()
            }
        };

        let img = image::load_from_memory(image_bytes)
            .map_err(|e| format!("image decode failed: {}", e))?;
        let rgb = img.to_rgb8();

        let width = rgb.width();
        let height = rgb.height();
        let pixels = rgb.as_raw();

        let persons = detector.infer(pixels, width, height)
            .map_err(|e| format!("detection failed: {}", e))?;

        if persons.is_empty() {
            return Ok(PoseOutput {
                keypoints: vec![],
                person_detected: false,
            });
        }

        let kps: Vec<Keypoint> = persons[0].iter()
            .map(Keypoint::from)
            .collect();

        Ok(PoseOutput {
            keypoints: kps,
            person_detected: true,
        })
    }
}
