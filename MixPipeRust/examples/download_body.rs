use mixpiperust::{PretrainedModel, download_model_blocking};

fn main() -> anyhow::Result<()> {
    println!("Downloading RtmPoseBody model...");
    let path = download_model_blocking(PretrainedModel::RtmPoseBody)?;
    println!("Downloaded to: {:?}", path);
    Ok(())
}
