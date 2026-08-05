use mixpipe::{Pipeline, PretrainedModel};
use once_cell::sync::OnceCell;
use std::sync::Mutex;

static PIPELINE: OnceCell<Mutex<Pipeline>> = OnceCell::new();

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Keypoint {
    pub x: f32,
    pub y: f32,
    pub confidence: f32,
}

#[derive(Debug, Clone)]
pub struct PoseOutput {
    pub keypoints: Vec<Keypoint>,
    pub person_detected: bool,
}

pub fn init_model(_model_path: &str) -> Result<(), String> {
    if PIPELINE.get().is_some() {
        return Ok(());
    }

    let model_dir = dirs::data_local_dir()
        .ok_or("cannot find local data dir")?
        .join("mixpipe")
        .join("models");

    let det_path = model_dir.join("rtmdet-tiny-mmdeploy.onnx");
    let pose_path = model_dir.join("rtmpose-tiny-mmdeploy.onnx");

    if !det_path.exists() || !pose_path.exists() {
        std::fs::create_dir_all(&model_dir)
            .map_err(|e| format!("failed to create model dir: {}", e))?;

        eprintln!("Downloading models...");
        download_file("https://www.modelscope.cn/models/Liyulingyue/mixpipe-rs-model-hub/resolve/master/rtmdet-tiny-mmdeploy.onnx", &det_path)?;
        download_file("https://www.modelscope.cn/models/Liyulingyue/mixpipe-rs-model-hub/resolve/master/rtmpose-tiny-mmdeploy.onnx", &pose_path)?;
        eprintln!("Models downloaded!");
    }

    let pipeline = Pipeline::from_files(&det_path, &pose_path)
        .map_err(|e| format!("pipeline build failed: {}", e))?;

    let _ = PIPELINE.set(Mutex::new(pipeline))
        .map_err(|_| "pipeline already initialized")?;

    Ok(())
}

fn download_file(url: &str, path: &std::path::Path) -> Result<(), String> {
    let result = std::thread::scope(|s| {
        s.spawn(|| {
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .user_agent("Mozilla/5.0")
                .build()
                .expect("failed to build client");

            let mut response = client.get(url)
                .send()
                .expect("download failed");

            if !response.status().is_success() {
                panic!("download failed: {}", response.status());
            }

            let mut file = std::fs::File::create(path).expect("file create error");
            std::io::copy(&mut response, &mut file).expect("file write error");
        }).join()
    });

    result.map_err(|e| format!("thread panicked: {:?}", e))
}

fn get_pipeline() -> Result<&'static Mutex<Pipeline>, String> {
    PIPELINE.get().ok_or_else(|| "pipeline not initialized".to_string())
}

pub fn detect_pose_from_bytes(img_bytes: &[u8]) -> Result<PoseOutput, String> {
    detect_pose_from_base64(&base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        img_bytes,
    ))
}

pub fn detect_pose_from_base64(data: &str) -> Result<PoseOutput, String> {
    let pipeline = get_pipeline()?;
    let pipeline = pipeline.lock().map_err(|e| format!("lock failed: {}", e))?;

    let data = data
        .strip_prefix("data:image/jpeg;base64,")
        .unwrap_or(data);
    let data = data
        .strip_prefix("data:image/png;base64,")
        .unwrap_or(data);
    let data = data
        .strip_prefix("data:image/webp;base64,")
        .unwrap_or(data);

    let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data)
        .map_err(|e| format!("base64 decode failed: {}", e))?;

    let img = image::load_from_memory(&decoded)
        .map_err(|e| format!("image decode failed: {}", e))?;
    let rgb = img.to_rgb8();

    let width = rgb.width();
    let height = rgb.height();
    let pixels = rgb.as_raw().clone();

    let persons = pipeline.run(&pixels, width, height)
        .map_err(|e| format!("pipeline run failed: {}", e))?;

    if persons.is_empty() {
        return Ok(PoseOutput {
            keypoints: vec![],
            person_detected: false,
        });
    }

    let person = &persons[0];
    let keypoints: Vec<Keypoint> = person.keypoints.iter()
        .map(|k| Keypoint {
            x: k.x,
            y: k.y,
            confidence: k.confidence,
        })
        .collect();

    let person_detected = !keypoints.is_empty();

    Ok(PoseOutput {
        keypoints,
        person_detected,
    })
}
