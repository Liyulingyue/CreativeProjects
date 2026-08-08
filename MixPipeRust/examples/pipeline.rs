use mixpipe::{Pipeline, PretrainedModel};

fn main() -> anyhow::Result<()> {
    println!("=== MixPipeRust: Detection + Pose Pipeline ===\n");

    // 方式一：自动下载（推荐，首次使用时自动从 ModelScope 下载）
    let pipeline = Pipeline::builder()
        .detector_model(PretrainedModel::RtmDetTiny)
        .pose_model(PretrainedModel::RtmPoseWholeBody)
        .build()?;

    // 方式二：使用本地模型（如果用户已自行下载）
    // let pipeline = Pipeline::builder()
    //     .detector("path/to/your/rtmdet-tiny-mmdeploy.onnx")
    //     .pose("path/to/your/rtmpose-wholebody-mmdeploy.onnx")
    //     .build()?;

    println!("Pipeline built successfully!\n");

    let img = image::open("your/image/path.jpg")?;
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let pixels = rgb.into_raw();

    println!("Running pipeline on {}x{} image...\n", w, h);
    let persons = pipeline.run(&pixels, w, h)?;

    println!("Detected {} persons:\n", persons.len());
    for (i, person) in persons.iter().enumerate() {
        println!("  Person {}: bbox=[{:.0}, {:.0}, {:.0}, {:.0}]",
            i, person.bbox[0], person.bbox[1], person.bbox[2], person.bbox[3]);
        println!("    Keypoints ({} total):", person.keypoints.len());
        for (j, kp) in person.keypoints.iter().enumerate() {
            println!("      [{}] x={:.1}, y={:.1}, conf={:.3}", j, kp.x, kp.y, kp.confidence);
        }
    }

    Ok(())
}
