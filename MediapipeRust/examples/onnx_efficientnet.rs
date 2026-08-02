use mediapipe_rust::backend::OnnxRuntimeBackend;
use mediapipe_rust::tasks::vision::ImageClassifierBuilder;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model_path = "models/efficientnet_lite0_fp32.onnx";
    let image_path = "references/cat.png";

    println!("Model: {}", model_path);
    println!("Image: {}", image_path);

    let backend = OnnxRuntimeBackend::new();

    let mut classifier = ImageClassifierBuilder::new()
        .max_results(5)
        .build_from_file(&backend, model_path)?;

    println!("Model loaded. Running classification...");

    // EfficientNet expects NHWC [1, 224, 224, 3]
    let img = image::open(image_path)?;
    let img = img.resize_exact(224, 224, image::imageops::FilterType::Lanczos3);
    let rgb = img.to_rgb8();

    // Convert to NHWC format (HWC = [224, 224, 3])
    let mut pixels: Vec<u8> = Vec::with_capacity(224 * 224 * 3);
    for pixel in rgb.pixels() {
        pixels.push(pixel[0]); // R
        pixels.push(pixel[1]); // G
        pixels.push(pixel[2]); // B
    }

    // Convert f32 to bytes
    let pixels_f32: Vec<f32> = pixels.iter().map(|&p| p as f32 / 255.0).collect();
    let pixel_bytes: Vec<u8> = pixels_f32.iter()
        .flat_map(|&f| f.to_le_bytes())
        .collect();

    let result = classifier.classify(&pixel_bytes, 224, 224)?;

    println!("\nClassification results:");
    for (i, class) in result.iter().enumerate() {
        println!("  {}. {} (index {}): {:.4}", i + 1, class.label, class.index, class.score);
    }

    println!("\n=== ImageClassification with EfficientNet-Lite0 works! ===");
    Ok(())
}
