use mediapipe_rust::backend::{OnnxRuntimeBackend, Tensor, TensorType};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        println!("Usage: onnx_run <model.onnx> <input_image>");
        println!("\nAvailable models:");
        println!("  models/efficientnet_lite0_fp32.onnx  - Image Classification (224x224)");
        println!("  models/deeplab_v3.onnx              - Image Segmentation (257x257)");
        println!("  models/mobilenet_v3_small.onnx      - Image Embedding (224x224, 1024d)");
        println!("  models/mobilenet_v3_large.onnx      - Image Embedding (224x224, 1280d)");
        println!("  models/blaze_face_short_range.onnx   - Face Detection (128x128)");
        println!("  models/pose_landmarks_detector.onnx  - Pose Landmarks (256x256)");
        return Ok(());
    }

    let model_path = &args[1];
    let image_path = &args[2];

    println!("Model: {}", model_path);
    println!("Image: {}", image_path);

    let model_data = fs::read(model_path)?;
    let backend = OnnxRuntimeBackend::new();
    let (model, _session) = backend.load_model_and_session(&model_data)?;

    println!("\nModel info:");
    println!("  Inputs: {:?}", model.inputs);
    println!("  Outputs: {:?}", model.outputs);

    let input = &model.inputs[0];
    let (width, height) = (input.shape[1] as u32, input.shape[2] as u32);
    let channels = input.shape[3] as u32;

    println!("\nInput: {}x{}, {} channels", width, height, channels);

    let img = image::open(image_path)?;
    let img = img.resize_exact(width, height, image::imageops::FilterType::Lanczos3);
    let rgb = img.to_rgb8();

    let mut pixels: Vec<u8> = Vec::with_capacity((width * height * channels) as usize);
    for pixel in rgb.pixels() {
        pixels.push(pixel[0]);
        pixels.push(pixel[1]);
        pixels.push(pixel[2]);
    }

    let pixels_f32: Vec<f32> = pixels.iter().map(|&p| p as f32 / 255.0).collect();
    let pixel_bytes: Vec<u8> = pixels_f32.iter()
        .flat_map(|&f| f.to_le_bytes())
        .collect();

    let tensor = Tensor {
        data: pixel_bytes,
        shape: vec![1, height as usize, width as usize, channels as usize],
        tensor_type: TensorType::F32,
    };

    let result = _session.run(&tensor)?;

    println!("\nOutput shapes:");
    for (i, out) in result.iter().enumerate() {
        println!("  Output {}: {:?}", i, out.shape);
    }

    println!("\n=== ONNX inference completed! ===");
    Ok(())
}
