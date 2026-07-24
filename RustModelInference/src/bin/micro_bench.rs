use std::time::Instant;
use rust_model_inference::*;

fn main() {
    let n_in = 1024usize;
    let n_out = 3072usize;
    let blocks_per_row = n_in / 32;
    let row_stride = blocks_per_row * 34;

    let weight: Vec<u8> = (0..n_out * row_stride).map(|i| ((i * 7 + 3) % 256) as u8).collect();
    let input: Vec<f32> = (0..n_in).map(|i| (i as f32 * 0.01).sin()).collect();
    let mut output = vec![0.0f32; n_out];
    let mut q8_buf = vec![0u8; n_in];
    let mut scale_buf = vec![0.0f32; n_in / 32];

    for _ in 0..100 {
        matmul_q8_0_via_q8(&weight, &input, &mut output, n_in, n_out, &mut q8_buf, &mut scale_buf);
    }

    let iters = 1000;
    let t0 = Instant::now();
    for _ in 0..iters {
        matmul_q8_0_via_q8(&weight, &input, &mut output, n_in, n_out, &mut q8_buf, &mut scale_buf);
    }
    let elapsed = t0.elapsed();
    let per_iter_us = elapsed.as_micros() as f64 / iters as f64;
    println!("matmul {}x{}: {:.1} us/iter ({:.1} GB/s)", n_in, n_out, per_iter_us,
        (n_out as f64 * row_stride as f64 + n_in as f64 * 4.0 + n_in as f64) / per_iter_us / 1000.0);

    let n_out2 = 151936usize;
    let weight2: Vec<u8> = (0..n_out2 * row_stride).map(|i| ((i * 7 + 3) % 256) as u8).collect();
    let mut output2 = vec![0.0f32; n_out2];

    for _ in 0..5 {
        matmul_q8_0_via_q8(&weight2, &input, &mut output2, n_in, n_out2, &mut q8_buf, &mut scale_buf);
    }

    let iters2 = 50;
    let t1 = Instant::now();
    for _ in 0..iters2 {
        matmul_q8_0_via_q8(&weight2, &input, &mut output2, n_in, n_out2, &mut q8_buf, &mut scale_buf);
    }
    let elapsed2 = t1.elapsed();
    let per_iter_us2 = elapsed2.as_micros() as f64 / iters2 as f64;
    println!("matmul {}x{}: {:.1} us/iter ({:.1} GB/s)", n_in, n_out2, per_iter_us2,
        (n_out2 as f64 * row_stride as f64 + n_in as f64 * 4.0 + n_in as f64) / per_iter_us2 / 1000.0);
}
