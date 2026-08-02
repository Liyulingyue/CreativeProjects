use mediapipe_rust::backend::{InferenceBackend, OnnxRuntimeBackend, Session, SessionBackend};
use image::RgbImage;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model_path = "models/efficientnet_lite0_fp32.onnx";
    let image_path = "references/cat.png";

    println!("Model: {}", model_path);
    println!("Image: {}", image_path);

    let model_data = std::fs::read(model_path)?;
    let backend = OnnxRuntimeBackend::new();
    let (model, session) = backend.load_model_and_session(&model_data)?;
    println!("Model loaded");
    println!("  inputs: {:?}", model.inputs);
    println!("  outputs: {:?}", model.outputs);

    // EfficientNet expects NHWC [1, 224, 224, 3]
    let img = image::open(image_path)?;
    let img = img.resize_exact(224, 224, image::imageops::FilterType::Lanczos3);
    let rgb = img.to_rgb8();

    // Convert to NHWC [1, 224, 224, 3] format (HWC)
    let mut pixels: Vec<f32> = Vec::with_capacity(224 * 224 * 3);
    for pixel in rgb.pixels() {
        pixels.push(pixel[0] as f32 / 255.0); // R
        pixels.push(pixel[1] as f32 / 255.0); // G
        pixels.push(pixel[2] as f32 / 255.0); // B
    }

    println!("\nRunning inference...");

    match session {
        Session::OnnxRuntime(mut onnx_session) => {
            let tensor = mediapipe_rust::backend::Tensor::new(
                mediapipe_rust::backend::TensorType::F32,
                vec![1, 224, 224, 3], // NHWC
                pixels.iter().flat_map(|&f| f.to_le_bytes()).collect()
            );

            onnx_session.set_input(0, &tensor)?;
            onnx_session.compute()?;

            let mut output_tensor = mediapipe_rust::backend::Tensor::empty(
                mediapipe_rust::backend::TensorType::F32,
                vec![1, 1000]
            );
            onnx_session.get_output(0, &mut output_tensor)?;

            let output_data: Vec<f32> = output_tensor.data.chunks(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            // Find top 5
            let mut indices: Vec<usize> = (0..1000).collect();
            indices.sort_by(|&a, &b| output_data[b].partial_cmp(&output_data[a]).unwrap());

            println!("\nTop 5 predictions:");
            for i in 0..5 {
                println!("  {}. index={}: {:.4}", i + 1, indices[i], output_data[indices[i]]);
            }

        }
        _ => println!("Expected OnnxRuntime session"),
    }

    println!("\n=== EfficientNet-Lite0 via ONNX works! ===");
    Ok(())
}
