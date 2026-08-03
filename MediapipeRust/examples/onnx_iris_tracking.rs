use mediapipe_rust::tasks::vision::IrisTrackerBuilder;
use mediapipe_rust::backend::OnnxRuntimeBackend;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model_path = "models/iris_landmark.onnx";

    println!("Loading model from: {}", model_path);

    let backend = OnnxRuntimeBackend::new();
    let mut tracker = IrisTrackerBuilder::new()
        .build_from_buffer(&backend, std::fs::read(model_path)?)?;

    println!("Model loaded. Running iris tracking...");

    let dummy_image = vec![128u8; 64 * 64 * 3];
    let result = tracker.track(&dummy_image, 64, 64)?;

    println!("\nIris tracking results:");
    println!("  Eyes contours: {} landmarks", result.eyes_contours.len());
    println!("  Iris landmarks: {}", result.iris.len());

    if !result.eyes_contours.is_empty() {
        println!("  First eye contour: ({:.3}, {:.3}, {:.3})",
            result.eyes_contours[0].x,
            result.eyes_contours[0].y,
            result.eyes_contours[0].z);
    }

    if !result.iris.is_empty() {
        println!("  First iris point: ({:.3}, {:.3}, {:.3})",
            result.iris[0].x,
            result.iris[0].y,
            result.iris[0].z);
    }

    println!("\nIrisTracker with ONNX backend works!");
    Ok(())
}
