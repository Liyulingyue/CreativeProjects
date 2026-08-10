use std::time::Instant;
use mixpipe::{Pipeline, PretrainedModel};

fn main() -> anyhow::Result<()> {
    println!("=== RTMPose-Body Pipeline Benchmark ===\n");

    let img_path = "/path/to/image.png";
    println!("Loading image: {}", img_path);
    let img = image::open(img_path)?;
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let pixels = rgb.into_raw();
    println!("Image size: {}x{}\n", w, h);

    println!("Building pipeline (downloading models if needed)...");
    let pipeline = Pipeline::builder()
        .detector_model(PretrainedModel::RtmDetTiny)
        .pose_model(PretrainedModel::RtmPoseBody)  // 17 keypoints
        .build()?;
    println!("Pipeline ready!\n");

    println!("Warming up (10 iterations)...");
    for _ in 0..10 {
        let _ = pipeline.run(&pixels, w, h)?;
    }

    const ITERATIONS: usize = 100;
    println!("Running benchmark ({} iterations)...", ITERATIONS);

    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let _ = pipeline.run(&pixels, w, h)?;
    }
    let elapsed = start.elapsed();

    let fps = ITERATIONS as f64 / elapsed.as_secs_f64();
    let ms_per_frame = elapsed.as_millis() as f64 / ITERATIONS as f64;

    println!("\n=== RTMPose-Body Pipeline Results ===");
    println!("Total time: {:.2}s", elapsed.as_secs_f64());
    println!("FPS: {:.2}", fps);
    println!("ms/frame: {:.2}", ms_per_frame);

    Ok(())
}
