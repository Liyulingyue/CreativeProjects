use mediapipe_rust::backend::OnnxRuntimeBackend;
use mediapipe_rust::tasks::vision::ImageClassifierBuilder;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model_path = "models/simple_conv_relu.onnx";
    let image_path = "references/cat.png";

    println!("Loading model from: {}", model_path);
    println!("Loading image from: {}", image_path);

    let backend = OnnxRuntimeBackend::new();

    let mut classifier = ImageClassifierBuilder::new()
        .max_results(5)
        .build_from_file(&backend, model_path)?;

    println!("Model loaded. Running classification...");

    let img = image::open(image_path)?;
    let img = img.resize_exact(224, 224, image::imageops::FilterType::Lanczos3);
    let rgb = img.to_rgb8();

    // Convert HWC (RGB u8) to CHW (f32 normalized to [0, 1])
    // CHW idx = c * (H*W) + y * W + x
    let mut pixels: Vec<f32> = vec![0.0f32; 3 * 224 * 224];
    for (i, pixel) in rgb.pixels().enumerate() {
        let x = i % 224;
        let y = i / 224;
        let idx = y * 224 + x;
        pixels[idx] = pixel[0] as f32 / 255.0;                    // R at channel 0
        pixels[224 * 224 + idx] = pixel[1] as f32 / 255.0;   // G at channel 1
        pixels[2 * 224 * 224 + idx] = pixel[2] as f32 / 255.0; // B at channel 2
    }

    // Convert f32 to u8 bytes for Tensor (since Tensor expects u8 data)
    let pixel_bytes: Vec<u8> = pixels.iter()
        .flat_map(|&f| f.to_le_bytes())
        .collect();

    let result = classifier.classify(&pixel_bytes, 224, 224)?;

    println!("\nClassification results:");
    for (i, class) in result.iter().enumerate() {
        println!("  {}. {} (index {}): {:.4}", i + 1, class.label, class.index, class.score);
    }

    println!("\nImageClassification with ONNX backend works!");
    Ok(())
}
