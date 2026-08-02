use mediapipe_rust::backend::{InferenceBackend, OnnxRuntimeBackend, Session, SessionBackend};
use image::{ImageBuffer, Rgb, RgbImage};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let model_path = "models/simple_conv_relu.onnx";
    let image_path = if args.len() > 1 { &args[1] } else { "references/cat.png" };

    println!("Model: {}", model_path);
    println!("Image: {}", image_path);

    let model_data = std::fs::read(model_path)?;
    let backend = OnnxRuntimeBackend::new();
    let (model, session) = backend.load_model_and_session(&model_data)?;
    println!("Model inputs: {:?}", model.inputs);
    println!("Model outputs: {:?}", model.outputs);

    let img = image::open(image_path)?;
    let img = img.resize_exact(224, 224, image::imageops::FilterType::Lanczos3);
    let rgb = img.to_rgb8();

    let mut pixels: Vec<f32> = vec![0.0f32; 3 * 224 * 224];
    for (i, pixel) in rgb.pixels().enumerate() {
        let x = i % 224;
        let y = i / 224;
        let idx = y * 224 + x;
        pixels[idx] = pixel[0] as f32 / 255.0;
        pixels[224 * 224 + idx] = pixel[1] as f32 / 255.0;
        pixels[2 * 224 * 224 + idx] = pixel[2] as f32 / 255.0;
    }

    println!("\nRunning inference...");

    let (w, h) = (224u32, 224u32);
    match session {
        Session::OnnxRuntime(mut onnx_session) => {
            let tensor = mediapipe_rust::backend::Tensor::new(
                mediapipe_rust::backend::TensorType::F32,
                vec![1, 3, h as usize, w as usize],
                pixels.iter().flat_map(|&f| f.to_le_bytes()).collect()
            );

            onnx_session.set_input(0, &tensor)?;
            onnx_session.compute()?;

            let mut output_tensor = mediapipe_rust::backend::Tensor::empty(
                mediapipe_rust::backend::TensorType::F32,
                vec![1, 16, 222, 222]
            );
            onnx_session.get_output(0, &mut output_tensor)?;

            let output_data: Vec<f32> = output_tensor.data.chunks(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            println!("Output shape: (1, 16, 222, 222)");
            println!("Output stats: min={:.4}, max={:.4}, mean={:.4}",
                output_data.iter().cloned().fold(f32::INFINITY, f32::min),
                output_data.iter().cloned().fold(f32::NEG_INFINITY, f32::max),
                output_data.iter().sum::<f32>() / output_data.len() as f32
            );

            // Save output visualization as image
            let output_path = format!("{}.output.txt", image_path);
            std::fs::write(&output_path, format!("Output shape: {:?}\nFirst 100 values: {:?}\n", [1, 16, 222, 222], &output_data[..100]))?;
            println!("\nSaved output data to: {}", output_path);

            // Create a visualization of the first 16 channels as a grid
            let channels = 16;
            let channel_h = 222u32;
            let channel_w = 222u32;
            let grid_cols = 4;
            let grid_rows = (channels + grid_cols - 1) / grid_cols;

            let mut grid_img: RgbImage = ImageBuffer::new(channel_w * grid_cols as u32, channel_h * grid_rows as u32);

            for c in 0..channels {
                let col = (c as u32) % grid_cols;
                let row = (c as u32) / grid_cols;
                let offset_x = col * channel_w;
                let offset_y = row * channel_h;

                let channel_start = (c * 222 * 222) as usize;
                let channel_data = &output_data[channel_start..channel_start + 222 * 222];
                let min_val = channel_data.iter().cloned().fold(f32::INFINITY, f32::min);
                let max_val = channel_data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                let range = max_val - min_val;

                for y in 0..channel_h {
                    for x in 0..channel_w {
                        let idx = (y * channel_w + x) as usize;
                        let val = (channel_data[idx] - min_val) / range * 255.0;
                        let val = val.clamp(0.0, 255.0) as u8;
                        grid_img.put_pixel(offset_x + x, offset_y + y, Rgb([val, val, val]));
                    }
                }
            }

            let grid_path = format!("{}.channels.png", image_path);
            grid_img.save(&grid_path)?;
            println!("Saved channel visualization to: {}", grid_path);

            // Save original image with output overlaid
            let combined_path = format!("{}.result.png", image_path);
            img.save(&combined_path)?;
            println!("Saved result image to: {}", combined_path);

        }
        _ => println!("Expected OnnxRuntime session"),
    }

    println!("\n=== Test completed ===");
    Ok(())
}
