use anyhow::Context;
use std::path::PathBuf;

const MODELSCOPE_BASE: &str = "https://www.modelscope.cn/models/Liyulingyue/mixpipe-rs-model-hub/resolve/master";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelType {
    RtmDet,
    RtmPose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoseVariant {
    Body,
    Face,
    Hand,
    WholeBody,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PretrainedModel {
    RtmDetTiny,
    RtmPoseBody,
    RtmPoseFace,
    RtmPoseHand,
    RtmPoseWholeBody,
}

impl PretrainedModel {
    pub fn model_type(&self) -> ModelType {
        match self {
            PretrainedModel::RtmDetTiny => ModelType::RtmDet,
            PretrainedModel::RtmPoseBody
            | PretrainedModel::RtmPoseFace
            | PretrainedModel::RtmPoseHand
            | PretrainedModel::RtmPoseWholeBody => ModelType::RtmPose,
        }
    }

    pub fn pose_variant(&self) -> Option<PoseVariant> {
        match self {
            PretrainedModel::RtmPoseBody => Some(PoseVariant::Body),
            PretrainedModel::RtmPoseFace => Some(PoseVariant::Face),
            PretrainedModel::RtmPoseHand => Some(PoseVariant::Hand),
            PretrainedModel::RtmPoseWholeBody => Some(PoseVariant::WholeBody),
            _ => None,
        }
    }

    pub fn filename(&self) -> &'static str {
        match self {
            PretrainedModel::RtmDetTiny => "rtmdet-tiny-mmdeploy.onnx",
            PretrainedModel::RtmPoseBody => "rtmpose-tiny-mmdeploy.onnx",
            PretrainedModel::RtmPoseFace => "rtmpose-face-mmdeploy.onnx",
            PretrainedModel::RtmPoseHand => "rtmpose-hand-mmdeploy.onnx",
            PretrainedModel::RtmPoseWholeBody => "rtmpose-wholebody-mmdeploy.onnx",
        }
    }

    pub fn url(&self) -> String {
        format!("{}/{}", MODELSCOPE_BASE, self.filename())
    }
}

pub fn get_cache_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|p| p.join("mixpiperust").join("models"))
}

pub fn get_model_path(model: PretrainedModel) -> Option<PathBuf> {
    get_cache_dir().map(|p| p.join(model.filename()))
}

pub async fn download_model(model: PretrainedModel) -> anyhow::Result<PathBuf> {
    let cache_dir = get_cache_dir()
        .context("Cannot find local data directory")?;
    
    let model_path = cache_dir.join(model.filename());
    
    if model_path.exists() {
        println!("Using cached model: {:?}", model_path);
        return Ok(model_path);
    }

    std::fs::create_dir_all(&cache_dir)
        .context("Failed to create cache directory")?;

    println!("Downloading {} from ModelScope...", model.filename());
    println!("URL: {}", model.url());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .context("Failed to build HTTP client")?;

    let response = client.get(&model.url())
        .send()
        .await
        .context("Failed to download model")?;

    if !response.status().is_success() {
        anyhow::bail!("Download failed with status: {}", response.status());
    }

    let bytes = response.bytes().await
        .context("Failed to read response body")?;

    println!("Downloaded {:.1} MB, saving to cache...", bytes.len() as f64 / 1_048_576.0);
    
    std::fs::write(&model_path, &bytes)
        .context("Failed to write model to cache")?;

    println!("Model saved to: {:?}", model_path);
    Ok(model_path)
}

pub fn download_model_blocking(model: PretrainedModel) -> anyhow::Result<PathBuf> {
    let cache_dir = get_cache_dir()
        .context("Cannot find local data directory")?;
    
    let model_path = cache_dir.join(model.filename());
    
    if model_path.exists() {
        println!("Using cached model: {:?}", model_path);
        return Ok(model_path);
    }

    std::fs::create_dir_all(&cache_dir)
        .context("Failed to create cache directory")?;

    println!("Downloading {} from ModelScope...", model.filename());
    println!("URL: {}", model.url());

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .context("Failed to build HTTP client")?;

    let mut response = client.get(&model.url())
        .send()
        .context("Failed to download model")?;

    if !response.status().is_success() {
        anyhow::bail!("Download failed with status: {}", response.status());
    }

    let mut file = std::fs::File::create(&model_path)
        .context("Failed to create model file")?;
    
    use std::io::copy;
    let total_bytes = copy(&mut response, &mut file)
        .context("Failed to copy response to file")?;

    println!("Downloaded {:.1} MB, saved to {:?}", 
        total_bytes as f64 / 1_048_576.0, model_path);
    Ok(model_path)
}
