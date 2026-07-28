use std::time::Instant;
use rand::Rng;
use rust_model_inference::*;

fn bench(n_in: usize, n_out: usize, iters: usize) {
    let blocks_per_row = n_in / 32;
    let row_stride = blocks_per_row * 34;

    let mut rng = rand::thread_rng();
    let weight: Vec<u8> = (0..n_out * row_stride).map(|_| rng.gen()).collect();
    let input: Vec<f32> = (0..n_in).map(|_| rng.gen::<f32>() * 0.1).collect();
    let mut output = vec![0.0f32; n_out];
    let mut q8_buf = vec![0u8; n_in];
    let mut scale_buf = vec![0.0f32; n_in / 32];

    for _ in 0..10 {
        matmul_q8_0_via_q8(&weight, &input, &mut output, n_in, n_out, &mut q8_buf, &mut scale_buf);
    }

    let t = Instant::now();
    for _ in 0..iters {
        matmul_q8_0_via_q8(&weight, &input, &mut output, n_in, n_out, &mut q8_buf, &mut scale_buf);
    }
    let elapsed = t.elapsed().as_secs_f64() / iters as f64;
    let gflops = (n_in as f64 * n_out as f64 * 2.0) / elapsed / 1e9;
    let bw = (n_out as f64 * row_stride as f64 + n_in as f64 * 4.0 + n_in as f64) / elapsed / 1e9;
    println!("{:7} x {:7} | {:7.2}ms | {:7.2}GF | {:7.2}GB",
        n_in, n_out, elapsed * 1000.0, gflops, bw);
}

fn main() {
    println!("=== Q8_0 Matmul Microbenchmark (random data) ===");
    println!("{:>26} | {:>8} | {:>7} | {:>7}", "n_in x n_out", "time", "GFLOPS", "GB/s");
    println!("{}", "=".repeat(65));

    println!("\n-- Qwen3-0.6B --");
    bench(1024, 2048, 2000);
    bench(1024, 3072, 500);
    bench(3072, 1024, 500);
    bench(2048, 151936, 100);

    println!("\n-- MiniCPM5-1B --");
    bench(1536, 2048, 1000);
    bench(1536, 256, 1000);
    bench(2048, 1536, 1000);
    bench(1536, 4608, 200);
    bench(4608, 1536, 200);
    bench(2048, 130560, 100);
}
