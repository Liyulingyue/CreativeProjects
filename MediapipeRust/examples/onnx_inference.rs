use mediapipe_rust::backend::{InferenceBackend, OnnxRuntimeBackend, Session, SessionBackend, Tensor, TensorType};

fn softmax(values: &[f32]) -> Vec<f32> {
    let max_val = values.iter().cloned().fold(f32::MIN, f32::max);
    let exp_values: Vec<f32> = values.iter().map(|v| (v - max_val).exp()).collect();
    let sum: f32 = exp_values.iter().sum();
    exp_values.iter().map(|v| v / sum).collect()
}

fn run_classification(model_path: &str, image_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Classification Model ===");
    let model_data = std::fs::read(model_path)?;
    let backend = OnnxRuntimeBackend::new();
    let (model, session) = backend.load_model_and_session(&model_data)?;

    println!("Model: {}", model_path);
    println!("Inputs: {:?}", model.inputs);
    println!("Outputs: {:?}", model.outputs);

    let input = &model.inputs[0];
    let (width, height) = (input.shape[1] as u32, input.shape[2] as u32);
    let channels = input.shape[3] as u32;

    let img = image::open(image_path)?;
    let img = img.resize_exact(width, height, image::imageops::FilterType::Lanczos3);
    let rgb = img.to_rgb8();

    let tensor = match input.tensor_type {
        TensorType::F32 => {
            let mut pixels: Vec<f32> = Vec::with_capacity((width * height * channels) as usize);
            for pixel in rgb.pixels() {
                pixels.push(pixel[0] as f32 / 255.0);
                pixels.push(pixel[1] as f32 / 255.0);
                pixels.push(pixel[2] as f32 / 255.0);
            }
            let data: Vec<u8> = pixels.iter().flat_map(|&f| f.to_le_bytes()).collect();
            Tensor::new(TensorType::F32, vec![1, height as usize, width as usize, channels as usize], data)
        }
        TensorType::U8 => {
            let mut pixels: Vec<u8> = Vec::with_capacity((width * height * channels) as usize);
            for pixel in rgb.pixels() {
                pixels.push(pixel[0]);
                pixels.push(pixel[1]);
                pixels.push(pixel[2]);
            }
            Tensor::new(TensorType::U8, vec![1, height as usize, width as usize, channels as usize], pixels)
        }
        _ => return Err(format!("Unsupported input type: {:?}", input.tensor_type).into()),
    };

    match session {
        Session::OnnxRuntime(mut onnx_session) => {
            onnx_session.set_input(0, &tensor)?;
            onnx_session.compute()?;

            let mut output_tensor = Tensor::empty(TensorType::F32, model.outputs[0].shape.clone());
            onnx_session.get_output(0, &mut output_tensor)?;

            let output_data: Vec<f32> = output_tensor.data.chunks(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            let output_name_lower = model.outputs[0].name.to_lowercase();
            let probabilities = if output_name_lower.contains("softmax") {
                output_data.clone()
            } else {
                softmax(&output_data)
            };

            let mut indices: Vec<usize> = (0..probabilities.len()).collect();
            indices.sort_by(|&a, &b| probabilities[b].partial_cmp(&probabilities[a]).unwrap());

            println!("\nTop 5 predictions:");
            for i in 0..5.min(probabilities.len()) {
                println!("  {}. index={}: {:.4}", i + 1, indices[i], probabilities[indices[i]]);
            }
        }
        _ => return Err("Expected OnnxRuntime session".into()),
    }

    Ok(())
}

fn run_object_detection(model_path: &str, image_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Object Detection Model ===");
    let model_data = std::fs::read(model_path)?;
    let backend = OnnxRuntimeBackend::new();
    let (model, session) = backend.load_model_and_session(&model_data)?;

    println!("Model: {}", model_path);
    println!("Inputs: {:?}", model.inputs);
    println!("Outputs: {:?}", model.outputs);

    let input = &model.inputs[0];
    let (width, height) = (input.shape[1] as u32, input.shape[2] as u32);
    let channels = input.shape[3] as u32;

    let img = image::open(image_path)?;
    let img = img.resize_exact(width, height, image::imageops::FilterType::Lanczos3);
    let rgb = img.to_rgb8();

    let tensor = match input.tensor_type {
        TensorType::F32 => {
            let mut pixels: Vec<f32> = Vec::with_capacity((width * height * channels) as usize);
            for pixel in rgb.pixels() {
                pixels.push(pixel[0] as f32 / 255.0);
                pixels.push(pixel[1] as f32 / 255.0);
                pixels.push(pixel[2] as f32 / 255.0);
            }
            let data: Vec<u8> = pixels.iter().flat_map(|&f| f.to_le_bytes()).collect();
            Tensor::new(TensorType::F32, vec![1, height as usize, width as usize, channels as usize], data)
        }
        TensorType::U8 => {
            let mut pixels: Vec<u8> = Vec::with_capacity((width * height * channels) as usize);
            for pixel in rgb.pixels() {
                pixels.push(pixel[0]);
                pixels.push(pixel[1]);
                pixels.push(pixel[2]);
            }
            Tensor::new(TensorType::U8, vec![1, height as usize, width as usize, channels as usize], pixels)
        }
        _ => return Err(format!("Unsupported input type: {:?}", input.tensor_type).into()),
    };

    match session {
        Session::OnnxRuntime(mut onnx_session) => {
            onnx_session.set_input(0, &tensor)?;
            onnx_session.compute()?;

            for (i, output_info) in model.outputs.iter().enumerate() {
                let mut output_tensor = Tensor::empty(TensorType::F32, output_info.shape.clone());
                onnx_session.get_output(i, &mut output_tensor)?;

                let output_data: Vec<f32> = output_tensor.data.chunks(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();

                println!("\nOutput {}: shape={:?}, len={}", i, output_info.shape, output_data.len());
                if output_data.len() <= 20 {
                    println!("  Values: {:?}", output_data);
                } else {
                    println!("  First 10 values: {:?}", &output_data[..10]);
                }
            }
        }
        _ => return Err("Expected OnnxRuntime session".into()),
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    println!("=== ONNX Inference Tool ===");
    println!();

    if args.len() < 3 {
        println!("Usage: onnx_inference <model.onnx> <image_path>");
        println!();
        println!("Available models:");
        println!("  Classification:");
        println!("    models/efficientnet_lite0_fp32.onnx  - 224x224, F32");
        println!("    models/mobilenet_v3_small.onnx      - 224x224, F32");
        println!();
        println!("  Object Detection:");
        println!("    models/ssd_mobilenet_v1.onnx       - 300x300, U8");
        println!("    models/blaze_face_short_range.onnx - 128x128, F32");
        println!();
        println!("  Pose Detection:");
        println!("    models/pose_landmarks_detector.onnx - 256x256, F32");
        println!();
        println!("  Segmentation:");
        println!("    models/deeplab_v3.onnx              - 257x257, F32");
        return Ok(());
    }

    let model_path = &args[1];
    let image_path = &args[2];

    let model_name = std::path::Path::new(model_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let is_detection = model_name.contains("ssd_mobilenet")
        || model_name.contains("blaze_face")
        || model_name.contains("face_detector")
        || model_name.contains("object_detection");

    let is_pose = model_name.contains("pose");

    let is_segmentation = model_name.contains("deeplab");

    let is_classification = model_name.contains("efficientnet")
        || model_name.contains("mobilenet")
        || model_name.contains("simple_classifier");

    if is_classification {
        run_classification(model_path, image_path)?;
    } else if is_detection || is_pose || is_segmentation {
        run_object_detection(model_path, image_path)?;
    } else {
        println!("Unknown model type, running generic inference...");
        run_object_detection(model_path, image_path)?;
    }

    println!("\n=== Done ===");
    Ok(())
}
