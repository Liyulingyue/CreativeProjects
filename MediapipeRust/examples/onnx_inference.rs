use mediapipe_rust::backend::{Backend, InferenceBackend, OnnxRuntimeBackend, Session};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <path_to_onnx_model>", args[0]);
        eprintln!("Example: {} models/simple_conv_relu.onnx", args[0]);
        return Ok(());
    }

    let model_path = &args[1];
    println!("Loading ONNX model from: {}", model_path);

    let model_data = fs::read(model_path)?;
    println!("Model file size: {} bytes", model_data.len());

    let backend = OnnxRuntimeBackend::new();
    println!("Backend name: {}", <OnnxRuntimeBackend as Backend>::name(&backend));

    let (model, session) = backend.load_model_and_session(&model_data)?;
    println!("Model loaded successfully!");
    println!("  Inputs: {:?}", model.inputs);
    println!("  Outputs: {:?}", model.outputs);

    let input_data: Vec<f32> = vec![1.0f32; 1 * 3 * 224 * 224];
    let input_shape = vec![1, 3, 224, 224];
    println!("\nRunning inference...");
    println!("  Input shape: {:?}", input_shape);
    println!("  Input[0..10]: {:?}", &input_data[..10]);

    match session {
        Session::OnnxRuntime(mut onnx_session) => {
            let output = onnx_session.run(&input_data, &input_shape)?;
            println!("\nInference succeeded!");
            println!("  Output length: {} floats", output.len());
            println!("  Output[0..10]: {:?}", &output[..10]);
        }
        _ => {
            println!("Expected OnnxRuntime session");
        }
    }

    println!("\n=== ONNX Runtime backend is working! ===");
    Ok(())
}
