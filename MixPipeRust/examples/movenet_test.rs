use anyhow::Context;
use mixpipe::{
    download_model_blocking, model_hub::PretrainedModel, ImageData, MoveNet,
    MoveNetVariant, Node, PixelFormat,
};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    println!("=== MoveNet Inference Test ===\n");

    let args: Vec<String> = std::env::args().collect();
    let img_path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("path/to/image.png"));

    println!("Loading image: {}", img_path.display());
    let img = image::open(&img_path).with_context(|| format!("Failed to open {}", img_path.display()))?;
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let pixels = rgb.as_raw().to_vec();
    println!("Image size: {}x{}\n", w, h);

    println!("Downloading model from ModelScope...");
    let model_path =
        download_model_blocking(PretrainedModel::MoveNetSinglePoseThunder)?;
    println!("Model loaded from: {:?}\n", model_path);

    let model =
        MoveNet::from_file(&model_path, MoveNetVariant::SinglePoseThunder)?;
    println!("Model initialized successfully!\n");

    let frame = mixpipe::Frame {
        data: mixpipe::FrameData::Image(ImageData {
            pixels,
            width: w,
            height: h,
            format: PixelFormat::Rgb,
        }),
        meta: mixpipe::FrameMeta::default(),
    };

    println!("Running inference...");
    let output = model.process(frame)?;

    let keypoints_json = output
        .meta
        .custom
        .get("movenet_keypoints")
        .ok_or_else(|| anyhow::anyhow!("No keypoints found"))?;

    println!("\nDetected keypoints:");
    println!("{}", serde_json::to_string_pretty(&keypoints_json)?);

    let keypoints: Vec<Vec<mixpipe::Keypoint>> =
        serde_json::from_value(keypoints_json.clone())?;

    let mut output_img = img.to_rgb8();
    model.draw(&mut output_img, &keypoints);

    output_img.save("output.jpg")?;
    println!("\nVisualization saved to: output.jpg");

    Ok(())
}