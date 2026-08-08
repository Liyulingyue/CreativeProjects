use mixpipe::{Pipeline, PretrainedModel, Visualizer};

fn main() -> anyhow::Result<()> {
    println!("=== MixPipeRust: Visualization Demo ===\n");

    // 方式一：自动下载（推荐，首次使用时自动从 ModelScope 下载）
    let pipeline = Pipeline::builder()
        .detector_model(PretrainedModel::RtmDetTiny)
        .pose_model(PretrainedModel::RtmPoseBody)
        .build()?;

    // 方式二：使用本地模型（如果用户已自行下载）
    // let pipeline = Pipeline::builder()
    //     .detector("path/to/your/rtmdet.onnx")
    //     .pose("path/to/your/rtmpose.onnx")
    //     .build()?;

    println!("Pipeline built successfully!\n");

    let img = image::open("your/image/path.jpg")?;
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let pixels = rgb.as_raw().to_vec();

    println!("Running pipeline on {}x{} image...\n", w, h);
    let persons = pipeline.run(&pixels, w, h)?;

    let mut output = img.to_rgb8();
    let viz = Visualizer::coco17();

    for person in &persons {
        viz.draw_person(&mut output, person);
    }

    output.save("output.png")?;
    println!("Visualization saved to output.png");

    Ok(())
}
