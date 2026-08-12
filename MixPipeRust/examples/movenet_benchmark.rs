use std::time::Instant;
use mixpipe::{
    download_model_blocking, model_hub::PretrainedModel, ImageData, MoveNet,
    MoveNetVariant, Node, PixelFormat,
};

fn main() -> anyhow::Result<()> {
    println!("=== MoveNet Benchmark ===\n");

    let img_path = "/path/to/image.png";
    println!("Loading image: {}", img_path);
    let img = image::open(img_path)?;
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let pixels = rgb.as_raw().to_vec();
    println!("Image size: {}x{}\n", w, h);

    println!("Downloading model...");
    let model_path = download_model_blocking(PretrainedModel::MoveNetSinglePoseThunder)?;
    let model = MoveNet::from_file(&model_path, MoveNetVariant::SinglePoseThunder)?;
    println!("Model loaded!\n");

    let frame = mixpipe::Frame {
        data: mixpipe::FrameData::Image(ImageData {
            pixels: pixels.clone(),
            width: w,
            height: h,
            format: PixelFormat::Rgb,
        }),
        meta: mixpipe::FrameMeta::default(),
    };

    println!("Warming up (10 iterations)...");
    for _ in 0..10 {
        let _ = model.process(frame.clone());
    }

    const ITERATIONS: usize = 100;
    println!("Running benchmark ({} iterations)...", ITERATIONS);

    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let _ = model.process(frame.clone());
    }
    let elapsed = start.elapsed();

    let fps = ITERATIONS as f64 / elapsed.as_secs_f64();
    let ms_per_frame = elapsed.as_millis() as f64 / ITERATIONS as f64;

    println!("\n=== MoveNet Results ===");
    println!("Total time: {:.2}s", elapsed.as_secs_f64());
    println!("FPS: {:.2}", fps);
    println!("ms/frame: {:.2}", ms_per_frame);

    Ok(())
}
