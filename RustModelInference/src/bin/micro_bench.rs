use half::f16;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rust_model_inference::*;
use std::time::Instant;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_sorts_without_mutating_caller() {
        let values = vec![4.0, 1.0, 3.0, 2.0, 5.0];
        assert_eq!(median(&values), 3.0);
        assert_eq!(values, vec![4.0, 1.0, 3.0, 2.0, 5.0]);
    }

    #[test]
    fn generated_q8_weights_have_finite_positive_scales() {
        let weights = valid_q8_weights(64, 3, 7);
        for row in 0..3 {
            for block in 0..2 {
                let off = row * 68 + block * 34;
                let bits = u16::from_le_bytes([weights[off], weights[off + 1]]);
                let scale = half::f16::from_bits(bits).to_f32();
                assert!(scale.is_finite() && scale > 0.0);
            }
        }
    }
}

const WARMUP: usize = 10;
const SAMPLES: usize = 15;
const GATE_N_IN: usize = 1024;
const GATE_N_OUT: usize = 3072;
const GATE_SPEEDUP: f64 = 1.10;

fn valid_q8_weights(n_in: usize, n_out: usize, seed: u64) -> Vec<u8> {
    let blocks_per_row = n_in / 32;
    let mut rng = StdRng::seed_from_u64(seed);
    let mut data = Vec::with_capacity(n_out * blocks_per_row * 34);
    for _ in 0..n_out * blocks_per_row {
        let scale = f16::from_f32(rng.gen_range(0.005f32..0.025f32)).to_bits();
        data.extend_from_slice(&scale.to_le_bytes());
        for _ in 0..32 {
            data.push(rng.gen_range(-127i8..=127i8) as u8);
        }
    }
    data
}

fn valid_q8_input(n_in: usize, seed: u64) -> (Vec<u8>, Vec<f32>) {
    let mut rng = StdRng::seed_from_u64(seed);
    let input: Vec<f32> = (0..n_in).map(|_| rng.gen_range(-1.0f32..1.0f32)).collect();
    let mut q8 = vec![0u8; n_in];
    let mut scales = vec![0.0f32; n_in / 32];
    quantize_q8_0_into(&input, n_in, &mut q8, &mut scales);
    (q8, scales)
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[sorted.len() / 2]
}

fn scalar_q8_matmul(
    weight: &[u8],
    input_q8: &[u8],
    input_scales: &[f32],
    output: &mut [f32],
    n_in: usize,
    n_out: usize,
) {
    let blocks = n_in / 32;
    let stride = blocks * 34;
    for row in 0..n_out {
        let mut sum = 0.0f32;
        for block in 0..blocks {
            let off = row * stride + block * 34;
            let bits = u16::from_le_bytes([weight[off], weight[off + 1]]);
            let scale = f16::from_bits(bits).to_f32() * input_scales[block];
            let mut dot = 0i32;
            for lane in 0..32 {
                dot += (weight[off + 2 + lane] as i8 as i32)
                    * (input_q8[block * 32 + lane] as i8 as i32);
            }
            sum += scale * dot as f32;
        }
        output[row] = sum;
    }
}

fn measure_once(mut run: impl FnMut(), iterations: usize) -> f64 {
    let started = Instant::now();
    for _ in 0..iterations {
        run();
    }
    started.elapsed().as_secs_f64() / iterations as f64
}

fn compare_backends(
    weight: &[u8],
    input_q8: &[u8],
    input_scales: &[f32],
    n_in: usize,
    n_out: usize,
    iterations: usize,
) -> (f64, f64) {
    let mut scalar_output = vec![0.0f32; n_out];
    let mut auto_output = vec![0.0f32; n_out];
    scalar_q8_matmul(weight, input_q8, input_scales, &mut scalar_output, n_in, n_out);
    matmul_q8_0_quantized(weight, input_q8, input_scales, &mut auto_output, n_in, n_out);
    for i in 0..n_out {
        let tolerance = 1e-4 + 1e-4 * scalar_output[i].abs();
        if (auto_output[i] - scalar_output[i]).abs() > tolerance {
            eprintln!("backend mismatch at row {i}: auto={} scalar={}", auto_output[i], scalar_output[i]);
            std::process::exit(3);
        }
    }

    for _ in 0..WARMUP {
        scalar_q8_matmul(weight, input_q8, input_scales, &mut scalar_output, n_in, n_out);
        matmul_q8_0_quantized(weight, input_q8, input_scales, &mut auto_output, n_in, n_out);
    }

    let mut scalar_times = Vec::with_capacity(SAMPLES);
    let mut auto_times = Vec::with_capacity(SAMPLES);
    for sample in 0..SAMPLES {
        if sample % 2 == 0 {
            scalar_times.push(measure_once(
                || scalar_q8_matmul(weight, input_q8, input_scales, &mut scalar_output, n_in, n_out),
                iterations,
            ));
            auto_times.push(measure_once(
                || matmul_q8_0_quantized(weight, input_q8, input_scales, &mut auto_output, n_in, n_out),
                iterations,
            ));
        } else {
            auto_times.push(measure_once(
                || matmul_q8_0_quantized(weight, input_q8, input_scales, &mut auto_output, n_in, n_out),
                iterations,
            ));
            scalar_times.push(measure_once(
                || scalar_q8_matmul(weight, input_q8, input_scales, &mut scalar_output, n_in, n_out),
                iterations,
            ));
        }
        std::hint::black_box((&scalar_output, &auto_output));
    }
    (median(&scalar_times), median(&auto_times))
}

fn bench(n_in: usize, n_out: usize, iterations: usize, seed: u64) {
    let blocks_per_row = n_in / 32;
    let row_stride = blocks_per_row * 34;
    let weight = valid_q8_weights(n_in, n_out, seed);
    let (input_q8, input_scales) = valid_q8_input(n_in, seed + 1);
    let mut output = vec![0.0f32; n_out];

    for _ in 0..3 {
        matmul_q8_0_quantized(&weight, &input_q8, &input_scales, &mut output, n_in, n_out);
    }

    let elapsed = measure_once(
        || matmul_q8_0_quantized(&weight, &input_q8, &input_scales, &mut output, n_in, n_out),
        iterations,
    );
    std::hint::black_box(&output);
    let gflops = (n_in as f64 * n_out as f64 * 2.0) / elapsed / 1e9;
    let bw = (n_out as f64 * row_stride as f64 + n_in as f64 * 4.0 + n_in as f64) / elapsed / 1e9;
    println!("{:7} x {:7} | {:7.2}ms | {:7.2}GF | {:7.2}GB",
        n_in, n_out, elapsed * 1000.0, gflops, bw);
}

fn main() {
    let check = std::env::args().skip(1).any(|arg| arg == "--check");
    if check && !cfg!(all(target_arch = "aarch64", target_os = "macos")) {
        eprintln!("--check requires a fixed aarch64-apple-darwin machine");
        std::process::exit(2);
    }

    let backend = if has_avx2_fma() {
        "AVX2/FMA"
    } else if has_neon() {
        "NEON"
    } else {
        "scalar"
    };
    println!("architecture={} backend={backend}", std::env::consts::ARCH);

    let weight = valid_q8_weights(GATE_N_IN, GATE_N_OUT, 42);
    let (input_q8, input_scales) = valid_q8_input(GATE_N_IN, 43);
    let (scalar_median, auto_median) = compare_backends(
        &weight,
        &input_q8,
        &input_scales,
        GATE_N_IN,
        GATE_N_OUT,
        20,
    );
    let speedup = scalar_median / auto_median;
    let operations = (GATE_N_IN * GATE_N_OUT * 2) as f64;
    let bytes = (GATE_N_OUT * (GATE_N_IN / 32 * 34) + GATE_N_IN * 5) as f64;
    println!(
        "gate={}x{} scalar_median={:.3}ms auto_median={:.3}ms speedup={:.3}x auto={:.2}GFLOPS/{:.2}GB/s threshold={:.2}x",
        GATE_N_IN,
        GATE_N_OUT,
        scalar_median * 1000.0,
        auto_median * 1000.0,
        speedup,
        operations / auto_median / 1e9,
        bytes / auto_median / 1e9,
        GATE_SPEEDUP,
    );
    if check && speedup < GATE_SPEEDUP {
        std::process::exit(1);
    }
    if check {
        return;
    }

    println!("\n=== Q8_0 Matmul Auto-backend Report (deterministic data) ===");
    println!("{:>26} | {:>8} | {:>7} | {:>7}", "n_in x n_out", "time", "GFLOPS", "GB/s");
    println!("{}", "=".repeat(65));

    println!("\n-- Qwen3-0.6B --");
    bench(1024, 2048, 50, 100);
    bench(1024, 3072, 30, 101);
    bench(3072, 1024, 20, 102);
    bench(2048, 151936, 1, 103);

    println!("\n-- MiniCPM5-1B --");
    bench(1536, 2048, 30, 200);
    bench(1536, 256, 100, 201);
    bench(2048, 1536, 20, 202);
    bench(1536, 4608, 10, 203);
    bench(4608, 1536, 10, 204);
    bench(2048, 130560, 1, 205);
}
