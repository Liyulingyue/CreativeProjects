use mediapipe_rust::tasks::ImageClassifierBuilder;
use mediapipe_rust::backend::NativeBackend;

fn main() {
    let backend = NativeBackend::mock();
    
    let classifier = ImageClassifierBuilder::new(backend)
        .max_results(5)
        .build_from_buffer(vec![0u8; 1024])
        .unwrap();

    let dummy_image_data = vec![0u8; 224 * 224 * 3 * 4];
    let result = classifier.classify(&dummy_image_data, 224, 224).unwrap();

    println!("Classification results:");
    for class in &result {
        println!("  {}: {} (score: {:.4})", class.index, class.label, class.score);
    }
}
