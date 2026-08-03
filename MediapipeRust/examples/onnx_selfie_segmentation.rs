use mediapipe_rust::tasks::vision::SelfieSegmenterBuilder;
use mediapipe_rust::backend::OnnxRuntimeBackend;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model_path = "models/deeplab_v3.onnx";

    println!("Loading model from: {}", model_path);

    let backend = OnnxRuntimeBackend::new();
    let mut segmenter = SelfieSegmenterBuilder::new()
        .build_from_buffer(&backend, std::fs::read(model_path)?)?;

    println!("Model loaded. Running segmentation...");

    let input_size = 257 * 257 * 3;
    let image_data: Vec<u8> = (0..input_size)
        .map(|i| ((i / 3) % 256) as u8)
        .collect();

    let result = segmenter.segment(&image_data, 257, 257)?;

    println!("\nSegmentation results:");
    println!("  Mask size: {}x{}", result.width, result.height);

    if let Some(ref conf) = result.confidence_mask {
        let person_pixels = conf.iter().filter(|&&c| c > 0.5).count();
        let total_pixels = conf.len();
        let ratio = person_pixels as f32 / total_pixels as f32;
        println!("  Person pixels: {} / {} ({:.1}%)", person_pixels, total_pixels, ratio * 100.0);
    }

    println!("\nSelfieSegmenter with ONNX backend works!");
    Ok(())
}
