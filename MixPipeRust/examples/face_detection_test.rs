use anyhow::Context;
use mixpipe::{download_model_blocking, model_hub::PretrainedModel, MediaPipeFaceDetection, ImageData, Node, PixelFormat};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    println!("=== Face Detection Test ===\n");

    let args: Vec<String> = std::env::args().collect();
    let img_path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("path/to/image.jpg"));

    println!("Loading image: {}", img_path.display());
    let img = image::open(&img_path).with_context(|| format!("Failed to open {}", img_path.display()))?;
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let pixels = rgb.as_raw().to_vec();
    println!("Image size: {}x{}\n", w, h);

    println!("Downloading model...");
    let model_path = download_model_blocking(PretrainedModel::MediaPipeFaceDetectionFullRange)?;

    println!("Loading model...");
    let model = MediaPipeFaceDetection::from_file(&model_path)?;
    println!("Model loaded!\n");

    let frame = mixpipe::Frame {
        data: mixpipe::FrameData::Image(ImageData {
            pixels,
            width: w,
            height: h,
            format: PixelFormat::Rgb,
        }),
        meta: mixpipe::FrameMeta::default(),
    };

    println!("Running inference...");
    let output = model.process(frame)?;

    let detections_json = output
        .meta
        .custom
        .get("face_detections")
        .ok_or_else(|| anyhow::anyhow!("No detections found"))?;

    let detections: Vec<mixpipe::Detection> = serde_json::from_value(detections_json.clone())?;
    println!("\nDetected {} faces", detections.len());

    for (i, det) in detections.iter().enumerate() {
        println!("  Face {}: bbox=[{:.0}, {:.0}, {:.0}, {:.0}] score={:.3}",
            i, det.bbox[0], det.bbox[1], det.bbox[2], det.bbox[3], det.score);
    }

    let mut output_img = img.to_rgb8();
    let color = image::Rgb([0, 255, 0]);
    for det in &detections {
        let [x1, y1, x2, y2] = det.bbox;
        let x1 = x1 as i32;
        let y1 = y1 as i32;
        let x2 = x2 as i32;
        let y2 = y2 as i32;
        for x in x1..=x2 {
            if x >= 0 && (x as u32) < output_img.width() {
                if y1 >= 0 && (y1 as u32) < output_img.height() {
                    output_img.put_pixel(x as u32, y1 as u32, color);
                }
                if y2 >= 0 && (y2 as u32) < output_img.height() {
                    output_img.put_pixel(x as u32, y2 as u32, color);
                }
            }
        }
        for y in y1..=y2 {
            if y >= 0 && (y as u32) < output_img.height() {
                if x1 >= 0 && (x1 as u32) < output_img.width() {
                    output_img.put_pixel(x1 as u32, y as u32, color);
                }
                if x2 >= 0 && (x2 as u32) < output_img.width() {
                    output_img.put_pixel(x2 as u32, y as u32, color);
                }
            }
        }
    }

    output_img.save("output.jpg")?;
    println!("\nVisualization saved to: output.jpg");

    Ok(())
}
