use mediapipe_rust::tasks::ObjectDetectorBuilder;
use mediapipe_rust::backend::NativeBackend;

fn main() {
    let backend = NativeBackend::mock_detection();
    
    let detector = ObjectDetectorBuilder::new(backend)
        .max_results(5)
        .min_score_threshold(0.5)
        .build_from_buffer(vec![0u8; 1024])
        .unwrap();

    let dummy_image_data = vec![0u8; 256 * 256 * 3 * 4];
    let result = detector.detect(&dummy_image_data, 256, 256).unwrap();

    println!("Detection results:");
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
}
