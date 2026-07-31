use mediapipe_rust::tasks::FaceLandmarkerBuilder;
use mediapipe_rust::backend::NativeBackend;

fn main() {
    let backend = NativeBackend::mock();
    
    let landmarker = FaceLandmarkerBuilder::new(backend)
        .num_faces(1)
        .build_from_buffer(vec![0u8; 1024])
        .unwrap();

    let dummy_image_data = vec![0u8; 256 * 256 * 3 * 4];
    let result = landmarker.detect(&dummy_image_data, 256, 256).unwrap();

    println!("Face landmark results:");
    for (i, face) in result.iter().enumerate() {
        println!("  Face #{}: {} landmarks", i, face.landmarks.len());
        for (j, lm) in face.landmarks.iter().take(5).enumerate() {
            println!("    Landmark {}: ({:.3}, {:.3}, {:.3})", j, lm.x, lm.y, lm.z);
        }
    }
}
