use mediapipe_rust::backend::{InferenceBackend, OnnxRuntimeBackend, Session, SessionBackend};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let model_path = "models/simple_conv_relu.onnx";
    let image_path = if args.len() > 1 { &args[1] } else { "references/cat.png" };

    println!("Loading model from: {}", model_path);
    println!("Loading image from: {}", image_path);

    let model_data = fs::read(model_path)?;
    let backend = OnnxRuntimeBackend::new();
    let (model, session) = backend.load_model_and_session(&model_data)?;
    println!("Model loaded: inputs={:?}, outputs={:?}", model.inputs, model.outputs);

    // Load and preprocess image
    let img = image::open(image_path)?;
    let img = img.resize_exact(224, 224, image::imageops::FilterType::Lanczos3);
    let rgb = img.to_rgb8();

    // Convert HWC -> CHW format
    let mut pixels: Vec<f32> = vec![0.0f32; 3 * 224 * 224];
    for (i, pixel) in rgb.pixels().enumerate() {
        let x = i % 224;
        let y = i / 224;
        let idx = y * 224 + x;
        pixels[idx] = pixel[0] as f32 / 255.0;
        pixels[224 * 224 + idx] = pixel[1] as f32 / 255.0;
        pixels[2 * 224 * 224 + idx] = pixel[2] as f32 / 255.0;
    }

    println!("Input shape: [1, 3, 224, 224]");
    println!("Input[0..10]: {:?}", &pixels[..10]);

    match session {
        Session::OnnxRuntime(mut onnx_session) => {
            // Use SessionBackend trait methods
            let tensor = mediapipe_rust::backend::Tensor::new(
                mediapipe_rust::backend::TensorType::F32,
                vec![1, 3, 224, 224],
                pixels.iter().flat_map(|&f| f.to_le_bytes()).collect()
            );

            onnx_session.set_input(0, &tensor).unwrap();
            onnx_session.compute().unwrap();

            let mut output_tensor = mediapipe_rust::backend::Tensor::empty(
                mediapipe_rust::backend::TensorType::F32,
                vec![1, 16, 222, 222]
            );
            onnx_session.get_output(0, &mut output_tensor).unwrap();

            let output_data: Vec<f32> = output_tensor.data.chunks(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            println!("Output shape: (1, 16, 222, 222)");
            println!("Output[0..10]: {:?}", &output_data[..10]);
        }
        _ => println!("Expected OnnxRuntime session"),
    }

    println!("\n=== ONNX Runtime backend works! ===");
    Ok(())
}
