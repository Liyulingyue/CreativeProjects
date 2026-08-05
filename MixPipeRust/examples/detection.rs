use mixpiperust::{RtmDet, PretrainedModel, RtmPose};

fn main() -> anyhow::Result<()> {
    println!("=== MixPipeRust: Object Detection ===\n");

    // 方式一：自动下载（推荐，首次使用时自动从 ModelScope 下载）
    let model = RtmDet::from_pretrained(PretrainedModel::RtmDetTiny)?;

    // 方式二：使用本地模型（如果用户已自行下载）
    // let model = RtmDet::from_file("path/to/your/rtmdet-tiny-mmdeploy.onnx")?;

    println!("Model loaded successfully!\n");

    let img = image::open("your/image/path.jpg")?;
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let pixels = rgb.into_raw();

    println!("Running detection on {}x{} image...\n", w, h);
    let detections = model.infer(&pixels, w, h)?;

    println!("Detected {} objects:\n", detections.len());
    for (i, det) in detections.iter().enumerate() {
        println!("  [{}] bbox=[{:.0}, {:.0}, {:.0}, {:.0}], score={:.3}, label={}",
            i, det.bbox[0], det.bbox[1], det.bbox[2], det.bbox[3], det.score, det.label);
    }

    Ok(())
}
