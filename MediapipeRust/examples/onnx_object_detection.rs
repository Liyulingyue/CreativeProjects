use mediapipe_rust::tasks::vision::ObjectDetectorBuilder;
use mediapipe_rust::backend::OnnxRuntimeBackend;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model_path = "models/ssd_mobilenet_v1.onnx";

    println!("Loading model from: {}", model_path);

    let backend = OnnxRuntimeBackend::new();
    let mut detector = ObjectDetectorBuilder::new()
        .max_results(5)
        .min_score_threshold(0.5)
        .build_from_buffer(&backend, std::fs::read(model_path)?)?;

    println!("Model loaded. Running detection...");

    let input_size = 300 * 300 * 3;
    let image_data: Vec<u8> = (0..input_size)
        .map(|i| ((i / 3) % 256) as u8)
        .collect();

    let result = detector.detect(&image_data, 300, 300)?;

    println!("\nDetection results:");
    for (i, detection) in result.iter().enumerate() {
        println!("  Detection #{}:", i);
        println!("    Bounding box: ({:.2}, {:.2}) - ({:.2}, {:.2})",
            detection.bounding_box.left,
            detection.bounding_box.top,
            detection.bounding_box.right,
            detection.bounding_box.bottom
        );
        for cat in &detection.categories {
            println!("    {}: {:.4}", cat.label, cat.score);
        }
    }

    println!("\nObjectDetector with ONNX backend works!");
    Ok(())
}
