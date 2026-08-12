use anyhow::Context;
use mixpipe::{
    download_model_blocking, model_hub::PretrainedModel, MediaPipeFaceLandmark, ImageData, Node, PixelFormat,
};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    println!("=== Face Landmark Test ===\n");

    let args: Vec<String> = std::env::args().collect();
    let img_path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("path/to/image.jpg"));

    println!("Loading image: {}", img_path.display());
    let img = image::open(&img_path).with_context(|| format!("Failed to open {}", img_path.display()))?;
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let pixels = rgb.as_raw().to_vec();
    println!("Image size: {}x{}\n", w, h);

    println!("Downloading model...");
    let model_path = download_model_blocking(PretrainedModel::MediaPipeFaceLandmark)?;

    println!("Loading model...");
    let model = MediaPipeFaceLandmark::from_file(&model_path)?;
    println!("Model loaded!\n");

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

    let landmarks_json = output
        .meta
        .custom
        .get("face_landmarks")
        .ok_or_else(|| anyhow::anyhow!("No landmarks found"))?;

    println!("\nDetected {} keypoints", serde_json::from_value::<Vec<mixpipe::Keypoint>>(landmarks_json.clone())?.len());
    println!("Sample landmarks (first 5):");
    let landmarks: Vec<mixpipe::Keypoint> = serde_json::from_value(landmarks_json.clone())?;
    for (i, kp) in landmarks.iter().take(5).enumerate() {
        println!("  {}: ({:.1}, {:.1}) conf={:.3}", i, kp.x, kp.y, kp.confidence);
    }

    let mut output_img = img.to_rgb8();
    model.draw(&mut output_img, &landmarks);

    output_img.save("output.jpg")?;
    println!("\nVisualization saved to: output.jpg");

    Ok(())
}
