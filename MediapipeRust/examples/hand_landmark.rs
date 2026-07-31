use mediapipe_rust::tasks::HandLandmarkerBuilder;
use mediapipe_rust::backend::NativeBackend;

fn main() {
    let backend = NativeBackend::mock_landmark();
    
    let landmarker = HandLandmarkerBuilder::new(backend)
        .num_hands(2)
        .build_from_buffer(vec![0u8; 1024])
        .unwrap();

    let dummy_image_data = vec![0u8; 256 * 256 * 3 * 4];
    let result = landmarker.detect(&dummy_image_data, 256, 256).unwrap();

    println!("Hand landmark results:");
    for (i, hand) in result.iter().enumerate() {
        println!("  Hand #{} ({}):", i, hand.handedness.label);
        println!("    {} landmarks", hand.landmarks.len());
        for (j, lm) in hand.landmarks.iter().take(5).enumerate() {
            println!("    Landmark {}: ({:.3}, {:.3}, {:.3})", j, lm.x, lm.y, lm.z);
        }
    }
}
