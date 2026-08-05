use mixpiperust::{RtmPose, PretrainedModel};

fn main() -> anyhow::Result<()> {
    println!("=== MixPipeRust: Pose Estimation ===\n");

    // 方式一：自动下载（推荐，首次使用时自动从 ModelScope 下载）
    let model = RtmPose::from_pretrained(PretrainedModel::RtmPoseWholeBody)?;

    // 方式二：使用本地模型（如果用户已自行下载）
    // let model = RtmPose::from_file("path/to/your/rtmpose-wholebody-mmdeploy.onnx")?;

    println!("Model loaded successfully!\n");

    let img = image::open("your/image/path.jpg")?;
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let pixels = rgb.into_raw();

    println!("Running pose estimation on {}x{} image...\n", w, h);
    let keypoints = model.infer(&pixels, w, h)?;

    println!("Detected {} keypoints:\n", keypoints.len());
    for (i, kp) in keypoints.iter().take(17).enumerate() {
        println!("  [{}] ({:.1}, {:.1}) conf={:.3}", i, kp.x, kp.y, kp.confidence);
    }
    if keypoints.len() > 17 {
        println!("  ... and {} more keypoints", keypoints.len() - 17);
    }

    Ok(())
}
