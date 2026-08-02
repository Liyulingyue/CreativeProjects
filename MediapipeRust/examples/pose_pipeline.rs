use image::{GenericImageView, ImageBuffer, Rgb, RgbImage};
use mediapipe_rust::backend::{InferenceBackend, OnnxRuntimeBackend, Session, SessionBackend, Tensor, TensorType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        println!("Usage: pose_pipeline <detection_model.onnx> <pose_model.onnx> <image_path>");
        println!();
        println!("Example:");
        println!("  pose_pipeline models/ssd_mobilenet_v1.onnx models/pose_landmarks_detector.onnx image.png");
        return Ok(());
    }

    let detection_model_path = &args[1];
    let pose_model_path = &args[2];
    let image_path = &args[3];

    println!("=== Pose Estimation Pipeline ===");
    println!("Detection model: {}", detection_model_path);
    println!("Pose model: {}", pose_model_path);
    println!("Image: {}", image_path);

    let img = image::open(image_path)?;
    let (orig_width, orig_height) = img.dimensions();
    println!("\nOriginal image: {}x{}", orig_width, orig_height);

    let detection_model_data = std::fs::read(detection_model_path)?;
    let pose_model_data = std::fs::read(pose_model_path)?;

    let backend = OnnxRuntimeBackend::new();

    println!("\n--- Loading detection model ---");
    let (detection_model, detection_session) = backend.load_model_and_session(&detection_model_data)?;
    println!("Detection inputs: {:?}", detection_model.inputs);
    println!("Detection outputs: {:?}", detection_model.outputs);

    println!("\n--- Loading pose model ---");
    let (pose_model, pose_session) = backend.load_model_and_session(&pose_model_data)?;
    println!("Pose inputs: {:?}", pose_model.inputs);
    println!("Pose outputs: {:?}", pose_model.outputs);

    let detection_input = &detection_model.inputs[0];
    let (det_w, det_h) = (detection_input.shape[1] as u32, detection_input.shape[2] as u32);

    let pose_input = &pose_model.inputs[0];
    let (pose_w, pose_h) = (pose_input.shape[1] as u32, pose_input.shape[2] as u32);

    println!("\n--- Running detection ---");
    let img_resized = img.resize_exact(det_w, det_h, image::imageops::FilterType::Lanczos3);
    let rgb = img_resized.to_rgb8();

    let det_tensor = if detection_input.tensor_type == TensorType::U8 {
        let mut pixels: Vec<u8> = Vec::with_capacity((det_w * det_h * 3) as usize);
        for pixel in rgb.pixels() {
            pixels.push(pixel[0]);
            pixels.push(pixel[1]);
            pixels.push(pixel[2]);
        }
        Tensor::new(TensorType::U8, vec![1, det_h as usize, det_w as usize, 3], pixels)
    } else {
        let mut pixels: Vec<f32> = Vec::with_capacity((det_w * det_h * 3) as usize);
        for pixel in rgb.pixels() {
            pixels.push(pixel[0] as f32 / 255.0);
            pixels.push(pixel[1] as f32 / 255.0);
            pixels.push(pixel[2] as f32 / 255.0);
        }
        let data: Vec<u8> = pixels.iter().flat_map(|&f| f.to_le_bytes()).collect();
        Tensor::new(TensorType::F32, vec![1, det_h as usize, det_w as usize, 3], data)
    };

    match detection_session {
        Session::OnnxRuntime(mut sess) => {
            sess.set_input(0, &det_tensor)?;
            sess.compute()?;

            if detection_model.outputs.len() >= 4 && detection_model.outputs[3].shape == vec![1] {
                let mut boxes_tensor = Tensor::empty(TensorType::F32, detection_model.outputs[0].shape.clone());
                sess.get_output(0, &mut boxes_tensor)?;
                let boxes: Vec<f32> = boxes_tensor.data.chunks(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();

                let mut scores_tensor = Tensor::empty(TensorType::F32, detection_model.outputs[1].shape.clone());
                sess.get_output(1, &mut scores_tensor)?;
                let scores: Vec<f32> = scores_tensor.data.chunks(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();

                let mut classes_tensor = Tensor::empty(TensorType::F32, detection_model.outputs[2].shape.clone());
                sess.get_output(2, &mut classes_tensor)?;
                let classes: Vec<f32> = classes_tensor.data.chunks(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();

                let num_boxes = detection_model.outputs[0].shape[1];

                let mut best_person_idx: Option<usize> = None;
                let mut best_score = 0.0f32;

                for i in 0..num_boxes.min(10) {
                    let class_id = classes[i] as i32;
                    let score = scores[i];
                    if class_id == 0 && score > best_score {
                        best_score = score;
                        best_person_idx = Some(i);
                    }
                }

                if let Some(idx) = best_person_idx {
                    let ymin = boxes[idx * 4];
                    let xmin = boxes[idx * 4 + 1];
                    let ymax = boxes[idx * 4 + 2];
                    let xmax = boxes[idx * 4 + 3];

                    println!("\nPerson detected! Score: {:.3}", best_score);
                    println!("Bounding box (normalized): ({:.3}, {:.3}) - ({:.3}, {:.3})", xmin, ymin, xmax, ymax);

                    let xmin_px = (xmin * orig_width as f32) as i32;
                    let ymin_px = (ymin * orig_height as f32) as i32;
                    let xmax_px = (xmax * orig_width as f32) as i32;
                    let ymax_px = (ymax * orig_height as f32) as i32;

                    println!("Bounding box (pixels): ({}, {}) - ({}, {})", xmin_px, ymin_px, xmax_px, ymax_px);

                    let xmin_crop = xmin_px.max(0) as u32;
                    let ymin_crop = ymin_px.max(0) as u32;
                    let xmax_crop = xmax_px.min(orig_width as i32).max(0) as u32;
                    let ymax_crop = ymax_px.min(orig_height as i32).max(0) as u32;

                    let roi_width = xmax_crop - xmin_crop;
                    let roi_height = ymax_crop - ymin_crop;

                    if roi_width > 10 && roi_height > 10 {
                        println!("\n--- Cropping person region: {}x{} ---", roi_width, roi_height);

                        let cropped = img.crop_imm(xmin_crop, ymin_crop, roi_width, roi_height);
                        let resized = cropped.resize_exact(pose_w, pose_h, image::imageops::FilterType::Lanczos3);
                        let rgb_cropped = resized.to_rgb8();

                        let mut pose_pixels: Vec<f32> = Vec::with_capacity((pose_w * pose_h * 3) as usize);
                        for pixel in rgb_cropped.pixels() {
                            pose_pixels.push(pixel[0] as f32 / 255.0);
                            pose_pixels.push(pixel[1] as f32 / 255.0);
                            pose_pixels.push(pixel[2] as f32 / 255.0);
                        }
                        let pose_data: Vec<u8> = pose_pixels.iter().flat_map(|&f| f.to_le_bytes()).collect();
                        let pose_tensor = Tensor::new(TensorType::F32, vec![1, pose_h as usize, pose_w as usize, 3], pose_data);

                        println!("\n--- Running pose detection ---");

                        match pose_session {
                            Session::OnnxRuntime(mut pose_sess) => {
                                pose_sess.set_input(0, &pose_tensor)?;
                                pose_sess.compute()?;

                                let mut landmarks_tensor = Tensor::empty(TensorType::F32, pose_model.outputs[0].shape.clone());
                                pose_sess.get_output(0, &mut landmarks_tensor)?;
                                let landmarks: Vec<f32> = landmarks_tensor.data.chunks(4)
                                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                                    .collect();

                                let num_keypoints = 33;
                                let values_per_keypoint = 5;

                                println!("\nPose landmarks (33 keypoints):");
                                for i in 0..num_keypoints.min(10) {
                                    let idx = i * values_per_keypoint;
                                    if idx + 2 < landmarks.len() {
                                        println!("  [{}]: x={:.2}, y={:.2}, z={:.2}", i, landmarks[idx], landmarks[idx+1], landmarks[idx+2]);
                                    }
                                }
                                if num_keypoints > 10 {
                                    println!("  ... and {} more keypoints", num_keypoints - 10);
                                }

                                let mut conf_tensor = Tensor::empty(TensorType::F32, pose_model.outputs[1].shape.clone());
                                pose_sess.get_output(1, &mut conf_tensor)?;
                                let confidence: f32 = f32::from_le_bytes([
                                    conf_tensor.data[0], conf_tensor.data[1],
                                    conf_tensor.data[2], conf_tensor.data[3]
                                ]);
                                println!("\nPose confidence: {:.3}", confidence);

                                let result_img = draw_pose_keypoints(&img, &landmarks, xmin_crop, ymin_crop, roi_width, roi_height, pose_w, pose_h);
                                result_img.save("pose_result.png")?;
                                println!("\nResult saved to pose_result.png");
                            }
                            _ => return Err("Expected OnnxRuntime session for pose".into()),
                        }
                    } else {
                        println!("Warning: Person bounding box too small");
                    }
                } else {
                    println!("\nNo person detected in image");
                }
            } else {
                return Err("Unexpected detection output format".into());
            }
        }
        _ => return Err("Expected OnnxRuntime session".into()),
    }

    Ok(())
}

fn draw_pose_keypoints(
    orig_img: &image::DynamicImage,
    landmarks: &[f32],
    roi_x: u32,
    roi_y: u32,
    roi_w: u32,
    roi_h: u32,
    pose_w: u32,
    pose_h: u32,
) -> RgbImage {
    let mut img = orig_img.to_rgb8();

    let num_keypoints = 33;
    let values_per_keypoint = 5;

    for i in 0..num_keypoints {
        let idx = i * values_per_keypoint;
        if idx + 1 < landmarks.len() {
            let lx = landmarks[idx];
            let ly = landmarks[idx + 1];

            let img_x = roi_x as i32 + (lx / pose_w as f32 * roi_w as f32) as i32;
            let img_y = roi_y as i32 + (ly / pose_h as f32 * roi_h as f32) as i32;

            if img_x >= 0 && img_y >= 0 && (img_x as u32) < img.width() && (img_y as u32) < img.height() {
                for dx in -3i32..=3 {
                    for dy in -3i32..=3 {
                        let px = (img_x + dx) as u32;
                        let py = (img_y + dy) as u32;
                        if px < img.width() && py < img.height() {
                            img.put_pixel(px, py, Rgb([255, 0, 0]));
                        }
                    }
                }
            }
        }
    }

    img
}
