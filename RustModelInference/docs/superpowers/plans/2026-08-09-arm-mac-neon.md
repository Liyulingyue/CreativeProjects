# ARM Mac Native and NEON Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every crate target build and test natively on `aarch64-apple-darwin`, run Qwen3 Q8_0 inference through stable NEON hot paths, preserve scalar fallbacks and the existing x86 SIMD path, and add a reproducible Apple Silicon performance gate.

**Architecture:** Keep the existing public operator functions as the only dispatch boundary. Compile AVX2/FMA/F16C only on x86_64, compile stable `std::arch::aarch64` NEON kernels only on aarch64, and fall through to shared scalar helpers everywhere else; do not introduce a backend trait or dependency. Compare every new NEON kernel against its scalar reference before timing it.

**Tech Stack:** Rust 2021, stable `std::arch::{x86_64,aarch64}`, existing `half`, `rand`, Rayon, Cargo tests, existing GGUF Qwen3-0.6B model.

## Global Constraints

- Target Apple Silicon as `aarch64-apple-darwin` without Rosetta.
- Do not add dependencies, Metal, Accelerate, BLAS, C/C++ FFI, nightly Rust, or inline assembly.
- Keep existing public inference entry points and Q8_0 byte layout unchanged.
- Keep x86_64 AVX2/FMA/F16C kernel bodies and their dispatch order unchanged.
- Q4_K, Q5_K, and Q6_K must compile and remain correct on ARM through scalar fallbacks; they do not receive NEON kernels in this change.
- Use only stable NEON intrinsics; implement i8 dot products with `vmull_s8`, `vmull_high_s8`, and `vpaddlq_s16` because `vdotq_s32` is unstable in Rust 1.97.
- Floating reductions must satisfy `abs(neon - scalar) <= 1e-4 + 1e-4 * abs(scalar)`.
- The explicit fixed-machine performance gate is `median_scalar / median_neon >= 1.10` for Qwen3 FFN shape `1024 x 3072`.
- Ordinary `cargo test` must never enforce a wall-clock threshold.

## File Map

- Modify `src/ops.rs`: architecture guards, shared scalar range, NEON vector/F16/Q8 kernels, dispatch, and kernel parity tests.
- Modify `src/quant.rs`: isolate every AVX2-only Q4_K/Q5_K/Q6_K symbol and route ARM to existing scalar implementations.
- Modify `src/qwen35.rs`: remove architecture-invalid nested AVX2 wrappers while preserving x86 fast calls.
- Modify `src/vision.rs`: use NEON normalization helpers and shared NEON vector operators in Attention while preserving x86 AVX2 bodies.
- Modify `src/bin/micro_bench.rs`: deterministic valid Q8 data, scalar-versus-NEON timing, median reporting, and `--check`.
- Modify `README.md`: Apple Silicon build, test, inference, and benchmark commands.
- Modify `OPTIMIZATION.md`: exact measured ARM environment and results from this execution.

---

### Task 1: Restore architecture-safe compilation and scalar fallback

**Files:**
- Modify: `src/ops.rs:1-123,628-1050`
- Modify: `src/quant.rs:39-193,276-288,683-950`
- Modify: `src/qwen35.rs:1210-1244`
- Modify: `src/vision.rs:508-551,1178-1288`

**Interfaces:**
- Consumes: existing public functions `has_avx2_fma()`, `has_f16c()`, `matmul_q8_0_quantized_range(...)`, and quantized dot-product entry points.
- Produces: the same public signatures on x86_64 and ARM; private `matmul_q8_0_quantized_scalar_range(...)` as the single Q8 scalar reference used by later tests.

- [ ] **Step 1: Re-run the failing ARM compile regression**

Run:

```bash
cargo check --all-targets
```

Expected: exit 101 with the current 23 errors, beginning with `is_x86_feature_detected!` in `src/ops.rs` and invalid AVX2 target features in `src/quant.rs` and `src/qwen35.rs`.

- [ ] **Step 2: Guard x86 feature detection and one-value F16 intrinsics**

Replace the unguarded feature state in `src/ops.rs` with target-specific definitions. Keep the existing x86 implementation body intact:

```rust
#[cfg(target_arch = "x86_64")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_arch = "x86_64")]
static HAS_AVX2_FMA: AtomicBool = AtomicBool::new(false);
#[cfg(target_arch = "x86_64")]
static HAS_F16C: AtomicBool = AtomicBool::new(false);
#[cfg(target_arch = "x86_64")]
static INIT_DONE: AtomicBool = AtomicBool::new(false);

#[cfg(target_arch = "x86_64")]
fn init_cpu_features() {
    if INIT_DONE.load(Ordering::Relaxed) { return; }
    let avx2_fma = is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma");
    let f16c = is_x86_feature_detected!("f16c");
    HAS_AVX2_FMA.store(avx2_fma, Ordering::Relaxed);
    HAS_F16C.store(f16c, Ordering::Relaxed);
    INIT_DONE.store(true, Ordering::Relaxed);
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
pub fn has_avx2_fma() -> bool {
    if !INIT_DONE.load(Ordering::Relaxed) { init_cpu_features(); }
    HAS_AVX2_FMA.load(Ordering::Relaxed)
}

#[cfg(not(target_arch = "x86_64"))]
#[inline(always)]
pub const fn has_avx2_fma() -> bool { false }

#[cfg(target_arch = "x86_64")]
#[inline(always)]
pub fn has_f16c() -> bool {
    if !INIT_DONE.load(Ordering::Relaxed) { init_cpu_features(); }
    HAS_F16C.load(Ordering::Relaxed)
}

#[cfg(not(target_arch = "x86_64"))]
#[inline(always)]
pub const fn has_f16c() -> bool { false }
```

Change both single-value conversion branches from `cfg(target_feature = "f16c")` to `cfg(all(target_arch = "x86_64", target_feature = "f16c"))`, and use the exact negation for their scalar branches:

```rust
#[cfg(not(all(target_arch = "x86_64", target_feature = "f16c")))]
```

- [ ] **Step 3: Isolate every AVX2-only definition and centralize the Q8 scalar range**

Add `#[cfg(target_arch = "x86_64")]` to `matmul_q8_0_vs_q8_0_avx2`, `matmul_q8_0_avx2_range`, and `hsum_ps`. Extract the repeated range loop into this private helper and call it after target-specific early returns:

```rust
fn matmul_q8_0_quantized_scalar_range(
    weight: &[u8],
    input_q8: &[u8],
    input_scales: &[f32],
    output: &mut [f32],
    n_in: usize,
    row_start: usize,
    row_end: usize,
) {
    let blocks_per_row = n_in / 32;
    let row_stride = blocks_per_row * 34;
    for (out_idx, row) in (row_start..row_end).enumerate() {
        let row_off = row * row_stride;
        let mut sum = 0.0f32;
        for block in 0..blocks_per_row {
            let off = row_off + block * 34;
            let wd = f16_to_f32(u16::from_le_bytes([weight[off], weight[off + 1]]));
            let qx = &weight[off + 2..off + 34];
            let qy = &input_q8[block * 32..(block + 1) * 32];
            let mut dot = 0i32;
            for lane in 0..32 {
                dot += (qx[lane] as i8 as i32) * (qy[lane] as i8 as i32);
            }
            sum += wd * input_scales[block] * dot as f32;
        }
        output[out_idx] = sum;
    }
}
```

Every architecture-neutral function must reference the AVX2 symbol only inside a `cfg(target_arch = "x86_64")` block and then use the scalar helper:

```rust
pub fn matmul_q8_0_quantized_range(
    weight: &[u8], input_q8: &[u8], input_scales: &[f32], output: &mut [f32],
    n_in: usize, row_start: usize, row_end: usize,
) {
    debug_assert_eq!(output.len(), row_end - row_start);
    #[cfg(target_arch = "x86_64")]
    if has_avx2_fma() {
        unsafe { matmul_q8_0_vs_q8_0_avx2(weight, input_q8, input_scales, output, n_in, row_start, row_end) };
        return;
    }
    matmul_q8_0_quantized_scalar_range(weight, input_q8, input_scales, output, n_in, row_start, row_end);
}
```

Apply the same target-guarded early-return pattern to `matmul_q8_0_via_q8`, `matmul_q8_0_quantized`, `q8_0_dot_row`, `parallel_range`, `matmul_q8_0`, and `matmul_q8_0_batch`; do not leave an unguarded reference to an AVX2 symbol.

- [ ] **Step 4: Remove architecture-invalid nested wrappers in quantized model code**

In `src/quant.rs`, put `#[cfg(target_arch = "x86_64")]` on every AVX2 definition and direct AVX2 export. Replace nested `#[target_feature]` functions in the public dispatchers with a guarded early return:

```rust
pub fn quantize_row_q8_k_into(x: &[f32], buf: &mut [BlockQ8K]) {
    #[cfg(target_arch = "x86_64")]
    if crate::ops::has_avx2_fma() {
        unsafe { quantize_row_q8_k_avx2_into(x, buf) };
        return;
    }
    quantize_row_q8_k_scalar_into(x, buf);
}
```

Use explicit guarded early returns for the allocating quantizer and all three K-quant dot products:

```rust
pub fn quantize_row_q8_k(x: &[f32]) -> Vec<BlockQ8K> {
    #[cfg(target_arch = "x86_64")]
    if crate::ops::has_avx2_fma() {
        return unsafe { quantize_row_q8_k_avx2(x) };
    }
    quantize_row_q8_k_scalar(x)
}

pub fn vec_dot_q4k_q8k(q4k_data: &[u8], q8k: &[BlockQ8K]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    if crate::ops::has_avx2_fma() {
        return unsafe { vec_dot_q4k_q8k_avx2(q4k_data, q8k) };
    }
    vec_dot_q4k_q8k_scalar(q4k_data, q8k)
}

pub fn vec_dot_q5k_q8k(q5k_data: &[u8], q8k: &[BlockQ8K]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    if crate::ops::has_avx2_fma() {
        return unsafe { vec_dot_q5k_q8k_avx2(q5k_data, q8k) };
    }
    vec_dot_q5k_q8k_scalar(q5k_data, q8k)
}

pub fn vec_dot_q6k_q8k(q6k_data: &[u8], q8k: &[BlockQ8K]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    if crate::ops::has_avx2_fma() {
        return unsafe { vec_dot_q6k_q8k_avx2(q6k_data, q8k) };
    }
    vec_dot_q6k_q8k_scalar(q6k_data, q8k)
}
```

In `src/qwen35.rs`, preserve each x86 direct call but guard it at compile time:

```rust
fn vec_dot_q4k_q8k_fast(q4k_data: &[u8], q8k: &[BlockQ8K]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    if crate::ops::has_avx2_fma() {
        return unsafe { quant::vec_dot_q4k_q8k_avx2_direct(q4k_data, q8k) };
    }
    quant::vec_dot_q4k_q8k_scalar(q4k_data, q8k)
}

fn vec_dot_q5k_q8k_fast(q5k_data: &[u8], q8k: &[BlockQ8K]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    if crate::ops::has_avx2_fma() {
        return unsafe { quant::vec_dot_q5k_q8k_avx2_direct(q5k_data, q8k) };
    }
    quant::vec_dot_q5k_q8k_scalar(q5k_data, q8k)
}

fn vec_dot_q6k_q8k_fast(q6k_data: &[u8], q8k: &[BlockQ8K]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    if crate::ops::has_avx2_fma() {
        return unsafe { quant::vec_dot_q6k_q8k_avx2_direct(q6k_data, q8k) };
    }
    quant::vec_dot_q6k_q8k_scalar(q6k_data, q8k)
}
```

- [ ] **Step 5: Make Vision's non-x86 path refer only to scalar/shared symbols**

Keep the current x86 attention kernels unchanged. Guard calls to `attention_qk_avx2` and `attn_scaled_add_avx2` with `#[cfg(target_arch = "x86_64")]`; for all other targets execute the existing scalar loops. Delete the misleading non-x86 functions named `attention_qk_avx2` and `attention_dot_avx2` because no caller should need an AVX-named stub.

- [ ] **Step 6: Verify ARM compilation and existing tests**

Run:

```bash
cargo fmt --all
cargo check --all-targets
cargo test --all-targets
```

Expected: all commands exit 0; the 23 architecture errors are absent. Record warnings separately instead of treating them as test failures.

- [ ] **Step 7: Commit the portability fix**

```bash
git add src/ops.rs src/quant.rs src/qwen35.rs src/vision.rs
git commit -m "fix: compile inference engine on arm64 macOS"
```

---

### Task 2: Add stable NEON F32 and F16 vector primitives

**Files:**
- Modify: `src/ops.rs:94-484,1145-1325`
- Test: `src/ops.rs` inline `#[cfg(test)]` module

**Interfaces:**
- Consumes: existing public vector APIs and scalar loops.
- Produces: `has_neon() -> bool` plus private `*_neon` kernels selected by the existing public APIs; later Vision code consumes `dot_f32` and `vec_mad_f32` without knowing the backend.

- [ ] **Step 1: Add failing ARM parity tests**

Append an inline test module that requires the not-yet-defined NEON functions:

```rust
#[cfg(test)]
mod neon_tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        let tolerance = 1e-4 + 1e-4 * expected.abs();
        assert!((actual - expected).abs() <= tolerance, "actual={actual} expected={expected}");
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn neon_f32_ops_match_scalar_with_tail() {
        let a: Vec<f32> = (0..19).map(|i| i as f32 * 0.125 - 1.0).collect();
        let b: Vec<f32> = (0..19).map(|i| 0.75 - i as f32 * 0.0625).collect();
        let expected_dot: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        assert_close(unsafe { dot_f32_neon(&a, &b, a.len()) }, expected_dot);

        let mut scaled = a.clone();
        unsafe { vec_scale_f32_neon(&mut scaled, -0.25) };
        for (actual, source) in scaled.iter().zip(&a) { assert_close(*actual, source * -0.25); }

        let mut mad = a.clone();
        unsafe { vec_mad_f32_neon(&mut mad, &b, 0.5) };
        for i in 0..mad.len() { assert_close(mad[i], a[i] + 0.5 * b[i]); }
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn neon_f16_ops_match_scalar_with_tail() {
        let src: Vec<f32> = (0..13).map(|i| i as f32 * 0.2 - 1.1).collect();
        let mut bits = vec![0u16; src.len()];
        unsafe { f32_slice_to_f16_neon(&src, &mut bits) };
        let expected: Vec<u16> = src.iter().map(|&v| f32_to_f16(v)).collect();
        assert_eq!(bits, expected);

        let expected_dot: f32 = src.iter().zip(&bits).map(|(x, h)| x * f16_to_f32(*h)).sum();
        assert_close(unsafe { dot_f16_f32_neon(&src, &bits, src.len()) }, expected_dot);
    }
}
```

- [ ] **Step 2: Run the focused tests to verify the red state**

Run:

```bash
cargo test neon_ --lib
```

Expected: compilation fails because `dot_f32_neon`, `vec_scale_f32_neon`, `vec_mad_f32_neon`, `f32_slice_to_f16_neon`, and `dot_f16_f32_neon` do not exist.

- [ ] **Step 3: Add compile-time NEON availability and F32 kernels**

Add the following stable AArch64 implementations and select them after the x86 early return but before each scalar loop:

```rust
#[cfg(target_arch = "aarch64")]
#[inline(always)]
pub const fn has_neon() -> bool { true }

#[cfg(not(target_arch = "aarch64"))]
#[inline(always)]
pub const fn has_neon() -> bool { false }

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn dot_f32_neon(a: &[f32], b: &[f32], n: usize) -> f32 {
    use std::arch::aarch64::*;
    let mut acc = vdupq_n_f32(0.0);
    let mut i = 0;
    while i + 4 <= n {
        acc = vfmaq_f32(acc, vld1q_f32(a.as_ptr().add(i)), vld1q_f32(b.as_ptr().add(i)));
        i += 4;
    }
    let mut sum = vaddvq_f32(acc);
    while i < n { sum += a[i] * b[i]; i += 1; }
    sum
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn vec_scale_f32_neon(y: &mut [f32], scale: f32) {
    use std::arch::aarch64::*;
    let scale = vdupq_n_f32(scale);
    let mut i = 0;
    while i + 4 <= y.len() {
        vst1q_f32(y.as_mut_ptr().add(i), vmulq_f32(vld1q_f32(y.as_ptr().add(i)), scale));
        i += 4;
    }
    while i < y.len() { y[i] *= vgetq_lane_f32::<0>(scale); i += 1; }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn vec_mad_f32_neon(y: &mut [f32], x: &[f32], scale: f32) {
    use std::arch::aarch64::*;
    let scale_v = vdupq_n_f32(scale);
    let mut i = 0;
    while i + 4 <= y.len() {
        let result = vfmaq_f32(vld1q_f32(y.as_ptr().add(i)), vld1q_f32(x.as_ptr().add(i)), scale_v);
        vst1q_f32(y.as_mut_ptr().add(i), result);
        i += 4;
    }
    while i < y.len() { y[i] += x[i] * scale; i += 1; }
}
```

Implement the remaining F32 kernels with the same four-lane/tail boundary:

```rust
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn sum_sq_f32_neon(x: &[f32]) -> f32 {
    use std::arch::aarch64::*;
    let mut acc = vdupq_n_f32(0.0);
    let mut i = 0;
    while i + 4 <= x.len() {
        let value = vld1q_f32(x.as_ptr().add(i));
        acc = vfmaq_f32(acc, value, value);
        i += 4;
    }
    let mut sum = vaddvq_f32(acc);
    while i < x.len() { sum += x[i] * x[i]; i += 1; }
    sum
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn scale_mul_neon(scale: f32, weight: &[f32], x: &mut [f32]) {
    use std::arch::aarch64::*;
    let scale_v = vdupq_n_f32(scale);
    let mut i = 0;
    while i + 4 <= x.len() {
        let value = vmulq_f32(vmulq_f32(vld1q_f32(x.as_ptr().add(i)), scale_v), vld1q_f32(weight.as_ptr().add(i)));
        vst1q_f32(x.as_mut_ptr().add(i), value);
        i += 4;
    }
    while i < x.len() { x[i] = x[i] * scale * weight[i]; i += 1; }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn vec_mul_neon(a: &[f32], b: &mut [f32]) {
    use std::arch::aarch64::*;
    let mut i = 0;
    while i + 4 <= b.len() {
        let value = vmulq_f32(vld1q_f32(a.as_ptr().add(i)), vld1q_f32(b.as_ptr().add(i)));
        vst1q_f32(b.as_mut_ptr().add(i), value);
        i += 4;
    }
    while i < b.len() { b[i] *= a[i]; i += 1; }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn vec_add_neon(a: &[f32], b: &mut [f32]) {
    use std::arch::aarch64::*;
    let mut i = 0;
    while i + 4 <= b.len() {
        let value = vaddq_f32(vld1q_f32(a.as_ptr().add(i)), vld1q_f32(b.as_ptr().add(i)));
        vst1q_f32(b.as_mut_ptr().add(i), value);
        i += 4;
    }
    while i < b.len() { b[i] += a[i]; i += 1; }
}
```

- [ ] **Step 4: Add stable F16 conversion and F16/F32 vector kernels**

First replace the handwritten scalar conversions with the already-installed `half` crate so scalar, F16C, and NEON all use IEEE round-to-nearest-even behavior:

```rust
#[cfg(not(all(target_arch = "x86_64", target_feature = "f16c")))]
{
    half::f16::from_bits(bits).to_f32()
}

#[cfg(not(all(target_arch = "x86_64", target_feature = "f16c")))]
{
    half::f16::from_f32(v).to_bits()
}
```

Then use `u16` loads plus reinterpret intrinsics so the vector code does not depend on a Rust source-level `f16` type:

```rust
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn f32_slice_to_f16_neon(src: &[f32], dst: &mut [u16]) {
    use std::arch::aarch64::*;
    let mut i = 0;
    while i + 4 <= src.len() {
        let halves = vreinterpret_u16_f16(vcvt_f16_f32(vld1q_f32(src.as_ptr().add(i))));
        vst1_u16(dst.as_mut_ptr().add(i), halves);
        i += 4;
    }
    while i < src.len() { dst[i] = f32_to_f16(src[i]); i += 1; }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn dot_f16_f32_neon(a: &[f32], b: &[u16], n: usize) -> f32 {
    use std::arch::aarch64::*;
    let mut acc = vdupq_n_f32(0.0);
    let mut i = 0;
    while i + 4 <= n {
        let halves = vreinterpret_f16_u16(vld1_u16(b.as_ptr().add(i)));
        acc = vfmaq_f32(acc, vld1q_f32(a.as_ptr().add(i)), vcvt_f32_f16(halves));
        i += 4;
    }
    let mut sum = vaddvq_f32(acc);
    while i < n { sum += a[i] * f16_to_f32(b[i]); i += 1; }
    sum
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn vec_mad_f16_f32_neon(y: &mut [f32], x: &[u16], scale: f32) {
    use std::arch::aarch64::*;
    let scale_v = vdupq_n_f32(scale);
    let mut i = 0;
    while i + 4 <= y.len() {
        let halves = vreinterpret_f16_u16(vld1_u16(x.as_ptr().add(i)));
        let result = vfmaq_f32(vld1q_f32(y.as_ptr().add(i)), vcvt_f32_f16(halves), scale_v);
        vst1q_f32(y.as_mut_ptr().add(i), result);
        i += 4;
    }
    while i < y.len() { y[i] += scale * f16_to_f32(x[i]); i += 1; }
}
```

Wire these into `f32_slice_to_f16`, `dot_f16_f32`, and `vec_mad_f16_f32` after the existing x86 branches.

- [ ] **Step 5: Verify red-green for vector kernels and full tests**

Run:

```bash
cargo fmt --all
cargo test neon_ --lib
cargo test --all-targets
```

Expected: both focused NEON tests pass and the full test suite exits 0.

- [ ] **Step 6: Commit the vector kernels**

```bash
git add src/ops.rs
git commit -m "feat: add stable NEON vector primitives"
```

---

### Task 3: Accelerate Q8_0 quantization and matrix multiplication with NEON

**Files:**
- Modify: `src/ops.rs:499-1050`
- Test: `src/ops.rs` inline `neon_tests` module

**Interfaces:**
- Consumes: `has_neon()`, `f16_to_f32(...)`, and `matmul_q8_0_quantized_scalar_range(...)` from Tasks 1-2.
- Produces: private `quantize_q8_0_into_neon_range(...)`, `dot_i8x32_neon(...)`, and `matmul_q8_0_vs_q8_0_neon(...)`; existing public Q8 APIs dispatch to them on aarch64.

- [ ] **Step 1: Add failing Q8 NEON parity tests**

Add deterministic valid Q8 weights and tests inside `neon_tests`:

```rust
fn valid_q8_weights(n_in: usize, n_out: usize) -> Vec<u8> {
    let blocks = n_in / 32;
    let mut data = Vec::with_capacity(n_out * blocks * 34);
    for row in 0..n_out {
        for block in 0..blocks {
            let scale = half::f16::from_f32(0.01 + (row + block) as f32 * 0.0001).to_bits();
            data.extend_from_slice(&scale.to_le_bytes());
            for lane in 0..32 {
                data.push((((row * 17 + block * 13 + lane * 7) % 255) as i16 - 127) as i8 as u8);
            }
        }
    }
    data
}

#[cfg(target_arch = "aarch64")]
#[test]
fn neon_q8_quantization_matches_scalar() {
    let input: Vec<f32> = (0..64).map(|i| ((i as i32 % 17) - 8) as f32 * 0.125).collect();
    let mut scalar_q = vec![0u8; 64];
    let mut scalar_s = vec![0.0f32; 2];
    let mut neon_q = vec![0u8; 64];
    let mut neon_s = vec![0.0f32; 2];
    quantize_q8_0_into_scalar_range(&input, &mut scalar_q, &mut scalar_s, 0, 2);
    unsafe { quantize_q8_0_into_neon_range(&input, &mut neon_q, &mut neon_s, 0, 2) };
    assert_eq!(neon_q, scalar_q);
    assert_eq!(neon_s, scalar_s);
}

#[cfg(target_arch = "aarch64")]
#[test]
fn neon_q8_matmul_matches_scalar_for_partial_row_range() {
    let n_in = 64;
    let weights = valid_q8_weights(n_in, 7);
    let input: Vec<f32> = (0..n_in).map(|i| (i as f32 * 0.03).sin()).collect();
    let mut q8 = vec![0u8; n_in];
    let mut scales = vec![0.0f32; n_in / 32];
    quantize_q8_0_into(&input, n_in, &mut q8, &mut scales);
    let mut scalar = vec![0.0f32; 5];
    let mut neon = vec![0.0f32; 5];
    matmul_q8_0_quantized_scalar_range(&weights, &q8, &scales, &mut scalar, n_in, 1, 6);
    unsafe { matmul_q8_0_vs_q8_0_neon(&weights, &q8, &scales, &mut neon, n_in, 1, 6) };
    for i in 0..5 { assert_close(neon[i], scalar[i]); }
}
```

Extract the current scalar quantization loop as `quantize_q8_0_into_scalar_range(...)` so the test and generic fallback use one reference implementation.

- [ ] **Step 2: Run focused tests to verify the red state**

Run:

```bash
cargo test neon_q8 --lib
```

Expected: compilation fails because the new scalar-range and NEON Q8 functions do not exist.

- [ ] **Step 3: Implement exact-rounding NEON Q8 activation quantization**

First extract the scalar block loop used by both serial and parallel fallbacks:

```rust
fn quantize_q8_0_into_scalar_range(
    input: &[f32], q8: &mut [u8], scales: &mut [f32], block_start: usize, block_end: usize,
) {
    for block in block_start..block_end {
        let values = &input[block * 32..(block + 1) * 32];
        let amax = values.iter().fold(0.0f32, |current, value| current.max(value.abs()));
        let scale = if amax == 0.0 { 0.0 } else { amax / 127.0 };
        let inverse = if scale == 0.0 { 0.0 } else { 1.0 / scale };
        scales[block] = scale;
        for lane in 0..32 {
            q8[block * 32 + lane] = (values[lane] * inverse).round().clamp(-128.0, 127.0) as i8 as u8;
        }
    }
}
```

Then implement one 32-value NEON block at a time. `vrndaq_f32` matches Rust `round()` ties-away semantics; narrowing intrinsics saturate to i8:

```rust
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn quantize_q8_0_into_neon_range(
    input: &[f32], q8: &mut [u8], scales: &mut [f32], block_start: usize, block_end: usize,
) {
    use std::arch::aarch64::*;
    for block in block_start..block_end {
        let src = input.as_ptr().add(block * 32);
        let v0 = vld1q_f32(src);
        let v1 = vld1q_f32(src.add(4));
        let v2 = vld1q_f32(src.add(8));
        let v3 = vld1q_f32(src.add(12));
        let v4 = vld1q_f32(src.add(16));
        let v5 = vld1q_f32(src.add(20));
        let v6 = vld1q_f32(src.add(24));
        let v7 = vld1q_f32(src.add(28));
        let m0 = vmaxq_f32(vmaxq_f32(vabsq_f32(v0), vabsq_f32(v1)), vmaxq_f32(vabsq_f32(v2), vabsq_f32(v3)));
        let m1 = vmaxq_f32(vmaxq_f32(vabsq_f32(v4), vabsq_f32(v5)), vmaxq_f32(vabsq_f32(v6), vabsq_f32(v7)));
        let amax = vmaxvq_f32(vmaxq_f32(m0, m1));
        let scale = if amax == 0.0 { 0.0 } else { amax / 127.0 };
        scales[block] = scale;
        let inverse = vdupq_n_f32(if scale == 0.0 { 0.0 } else { 1.0 / scale });
        let q0 = vcvtq_s32_f32(vrndaq_f32(vmulq_f32(v0, inverse)));
        let q1 = vcvtq_s32_f32(vrndaq_f32(vmulq_f32(v1, inverse)));
        let q2 = vcvtq_s32_f32(vrndaq_f32(vmulq_f32(v2, inverse)));
        let q3 = vcvtq_s32_f32(vrndaq_f32(vmulq_f32(v3, inverse)));
        let q4 = vcvtq_s32_f32(vrndaq_f32(vmulq_f32(v4, inverse)));
        let q5 = vcvtq_s32_f32(vrndaq_f32(vmulq_f32(v5, inverse)));
        let q6 = vcvtq_s32_f32(vrndaq_f32(vmulq_f32(v6, inverse)));
        let q7 = vcvtq_s32_f32(vrndaq_f32(vmulq_f32(v7, inverse)));
        let lo = vcombine_s8(
            vqmovn_s16(vcombine_s16(vqmovn_s32(q0), vqmovn_s32(q1))),
            vqmovn_s16(vcombine_s16(vqmovn_s32(q2), vqmovn_s32(q3))),
        );
        let hi = vcombine_s8(
            vqmovn_s16(vcombine_s16(vqmovn_s32(q4), vqmovn_s32(q5))),
            vqmovn_s16(vcombine_s16(vqmovn_s32(q6), vqmovn_s32(q7))),
        );
        let dst = q8.as_mut_ptr().add(block * 32) as *mut i8;
        vst1q_s8(dst, lo);
        vst1q_s8(dst.add(16), hi);
    }
}
```

Wire both serial and parallel public quantizers to this function on aarch64.

- [ ] **Step 4: Implement baseline NEON i8 dot and Q8 matrix range**

Use stable widening multiply and pairwise add:

```rust
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn dot_i8x32_neon(a: *const u8, b: *const u8) -> i32 {
    use std::arch::aarch64::*;
    let a0 = vld1q_s8(a as *const i8);
    let b0 = vld1q_s8(b as *const i8);
    let a1 = vld1q_s8(a.add(16) as *const i8);
    let b1 = vld1q_s8(b.add(16) as *const i8);
    let p0 = vaddq_s32(vpaddlq_s16(vmull_s8(vget_low_s8(a0), vget_low_s8(b0))), vpaddlq_s16(vmull_high_s8(a0, b0)));
    let p1 = vaddq_s32(vpaddlq_s16(vmull_s8(vget_low_s8(a1), vget_low_s8(b1))), vpaddlq_s16(vmull_high_s8(a1, b1)));
    vaddvq_s32(vaddq_s32(p0, p1))
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn matmul_q8_0_vs_q8_0_neon(
    weight: &[u8], input_q8: &[u8], input_scales: &[f32], output: &mut [f32],
    n_in: usize, row_start: usize, row_end: usize,
) {
    let blocks = n_in / 32;
    let stride = blocks * 34;
    for (out_idx, row) in (row_start..row_end).enumerate() {
        let mut sum = 0.0f32;
        for block in 0..blocks {
            let off = row * stride + block * 34;
            let wd = f16_to_f32(u16::from_le_bytes([weight[off], weight[off + 1]]));
            let dot = dot_i8x32_neon(weight.as_ptr().add(off + 2), input_q8.as_ptr().add(block * 32));
            sum += wd * input_scales[block] * dot as f32;
        }
        output[out_idx] = sum;
    }
}
```

Add aarch64 early returns to every public/range Q8 dispatcher at the same points where x86 currently returns. Keep the scalar helper last.

- [ ] **Step 5: Verify Q8 parity, inference operators, and release compilation**

Run:

```bash
cargo fmt --all
cargo test neon_q8 --lib
cargo test --all-targets
cargo build --release --all-targets
```

Expected: exact quantization parity, matrix results within the documented tolerance, all tests pass, and the release build exits 0.

- [ ] **Step 6: Commit the Q8 kernels**

```bash
git add src/ops.rs
git commit -m "feat: accelerate Q8 inference with NEON"
```

---

### Task 4: Route Vision normalization and Attention through NEON

**Files:**
- Modify: `src/vision.rs:985-1123,1178-1288`
- Test: `src/vision.rs` inline `#[cfg(test)]` module

**Interfaces:**
- Consumes: architecture-neutral `dot_f32(...)`, `vec_mad_f32(...)`, and `has_neon()` from Task 2.
- Produces: ARM NEON normalization kernels; Vision Attention uses shared NEON operations without changing `VisionEncoder` interfaces.

- [ ] **Step 1: Add failing normalization and Attention parity tests**

Append tests with a non-multiple-of-four dimension:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) {
        assert!((a - b).abs() <= 1e-4 + 1e-4 * b.abs(), "a={a} b={b}");
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn neon_layer_norm_helpers_match_scalar() {
        let input: Vec<f32> = (0..13).map(|i| i as f32 * 0.1 - 0.7).collect();
        let weight: Vec<f32> = (0..13).map(|i| 0.9 + i as f32 * 0.01).collect();
        let bias: Vec<f32> = (0..13).map(|i| i as f32 * -0.005).collect();
        let mean = input.iter().sum::<f32>() / input.len() as f32;
        let inv = 1.0 / (input.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / input.len() as f32 + 1e-6).sqrt();
        let mut expected = input.clone();
        for i in 0..expected.len() { expected[i] = (expected[i] - mean) * inv * weight[i] + bias[i]; }
        let mut actual = input.clone();
        unsafe { layer_norm_scale_bias_neon(&mut actual, &weight, &bias, mean, inv) };
        for i in 0..actual.len() { close(actual[i], expected[i]); }
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn shared_neon_attention_ops_match_scalar() {
        let q: Vec<f32> = (0..13).map(|i| i as f32 * 0.07 - 0.3).collect();
        let k: Vec<f32> = (0..13).map(|i| 0.4 - i as f32 * 0.02).collect();
        let expected: f32 = q.iter().zip(&k).map(|(x, y)| x * y).sum();
        close(dot_f32(&q, &k, q.len()), expected);
        let mut out = vec![0.25f32; 13];
        vec_mad_f32(&mut out, &k, 0.5);
        for i in 0..13 { close(out[i], 0.25 + 0.5 * k[i]); }
    }
}
```

- [ ] **Step 2: Run the focused tests to verify the red state**

Run:

```bash
cargo test vision::tests --lib
```

Expected: compilation fails because `layer_norm_scale_bias_neon` is missing.

- [ ] **Step 3: Implement NEON normalization helpers and dispatch**

Add these four normalization kernels and call each one after the corresponding existing x86 early return:

```rust
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn sum_f32_neon(x: &[f32]) -> f32 {
    use std::arch::aarch64::*;
    let mut acc = vdupq_n_f32(0.0);
    let mut i = 0;
    while i + 4 <= x.len() {
        acc = vaddq_f32(acc, vld1q_f32(x.as_ptr().add(i)));
        i += 4;
    }
    let mut sum = vaddvq_f32(acc);
    while i < x.len() { sum += x[i]; i += 1; }
    sum
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn sum_sq_centered_f32_neon(x: &[f32], mean: f32) -> f32 {
    use std::arch::aarch64::*;
    let mean_v = vdupq_n_f32(mean);
    let mut acc = vdupq_n_f32(0.0);
    let mut i = 0;
    while i + 4 <= x.len() {
        let delta = vsubq_f32(vld1q_f32(x.as_ptr().add(i)), mean_v);
        acc = vfmaq_f32(acc, delta, delta);
        i += 4;
    }
    let mut sum = vaddvq_f32(acc);
    while i < x.len() { let delta = x[i] - mean; sum += delta * delta; i += 1; }
    sum
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn layer_norm_scale_bias_neon(x: &mut [f32], weight: &[f32], bias: &[f32], mean: f32, inv: f32) {
    use std::arch::aarch64::*;
    let mean_v = vdupq_n_f32(mean);
    let inv_v = vdupq_n_f32(inv);
    let mut i = 0;
    while i + 4 <= x.len() {
        let centered = vsubq_f32(vld1q_f32(x.as_ptr().add(i)), mean_v);
        let normalized = vmulq_f32(vmulq_f32(centered, inv_v), vld1q_f32(weight.as_ptr().add(i)));
        let value = vaddq_f32(normalized, vld1q_f32(bias.as_ptr().add(i)));
        vst1q_f32(x.as_mut_ptr().add(i), value);
        i += 4;
    }
    while i < x.len() { x[i] = (x[i] - mean) * inv * weight[i] + bias[i]; i += 1; }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn layer_norm_scale_neon(x: &mut [f32], weight: &[f32], mean: f32, inv: f32) {
    use std::arch::aarch64::*;
    let mean_v = vdupq_n_f32(mean);
    let inv_v = vdupq_n_f32(inv);
    let mut i = 0;
    while i + 4 <= x.len() {
        let centered = vsubq_f32(vld1q_f32(x.as_ptr().add(i)), mean_v);
        let value = vmulq_f32(vmulq_f32(centered, inv_v), vld1q_f32(weight.as_ptr().add(i)));
        vst1q_f32(x.as_mut_ptr().add(i), value);
        i += 4;
    }
    while i < x.len() { x[i] = (x[i] - mean) * inv * weight[i]; i += 1; }
}
```

- [ ] **Step 4: Use shared NEON vector entry points in Vision Attention**

Leave both AVX2 function bodies byte-for-byte unchanged. In the non-x86 branch, replace raw scalar inner loops with slices passed to the shared operators:

```rust
let q_slice = std::slice::from_raw_parts(q_ptr, d_head);
for s in 0..n_tokens {
    let k_ptr = attn_buf.as_ptr().add(k_base + s * d_head);
    let k_slice = std::slice::from_raw_parts(k_ptr, d_head);
    score_slice[t * n_tokens + s] = dot_f32(q_slice, k_slice, d_head) * scale;
}

for s in 0..n_tokens {
    let value_ptr = attn_buf.as_ptr().add(v_base + s * d_head);
    let value = std::slice::from_raw_parts(value_ptr, d_head);
    vec_mad_f32(&mut out_slice[out_base..out_base + d_head], value, score_slice[t * n_tokens + s]);
}
```

On aarch64 these calls enter Task 2's NEON kernels; other non-x86 targets retain their scalar fallbacks.

- [ ] **Step 5: Verify Vision parity and the full suite**

Run:

```bash
cargo fmt --all
cargo test vision::tests --lib
cargo test --all-targets
```

Expected: both Vision tests pass and the full suite exits 0.

- [ ] **Step 6: Commit the Vision kernels**

```bash
git add src/vision.rs
git commit -m "feat: accelerate vision kernels with NEON"
```

---

### Task 5: Add deterministic Apple Silicon performance benchmark and gate

**Files:**
- Modify: `src/bin/micro_bench.rs:1-49`
- Test: `src/bin/micro_bench.rs` inline `#[cfg(test)]` module

**Interfaces:**
- Consumes: public `matmul_q8_0_quantized(...)` auto-dispatch and `half::f16` already present in dependencies.
- Produces: `micro-bench` report mode and explicit `micro-bench --check` fixed-machine gate; no production API changes.

- [ ] **Step 1: Add failing benchmark-helper tests**

Define tests before the helpers:

```rust
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
```

- [ ] **Step 2: Run the benchmark tests to verify the red state**

Run:

```bash
cargo test --bin micro-bench
```

Expected: compilation fails because `median` and `valid_q8_weights` do not exist.

- [ ] **Step 3: Replace arbitrary random bytes with deterministic valid Q8 data**

Use the already-installed `rand` and `half` crates:

```rust
use half::f16;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

fn valid_q8_weights(n_in: usize, n_out: usize, seed: u64) -> Vec<u8> {
    let blocks = n_in / 32;
    let mut rng = StdRng::seed_from_u64(seed);
    let mut data = Vec::with_capacity(n_out * blocks * 34);
    for _ in 0..n_out * blocks {
        let scale = f16::from_f32(rng.gen_range(0.005f32..0.025f32)).to_bits();
        data.extend_from_slice(&scale.to_le_bytes());
        for _ in 0..32 { data.push(rng.gen_range(-127i8..=127i8) as u8); }
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
    weight: &[u8], input_q8: &[u8], input_scales: &[f32], output: &mut [f32],
    n_in: usize, n_out: usize,
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
```

- [ ] **Step 4: Implement alternating median measurement and result parity**

Use these constants and helpers:

```rust
const WARMUP: usize = 10;
const SAMPLES: usize = 15;
const GATE_N_IN: usize = 1024;
const GATE_N_OUT: usize = 3072;
const GATE_SPEEDUP: f64 = 1.10;

fn measure_once(mut run: impl FnMut(), iterations: usize) -> f64 {
    let started = Instant::now();
    for _ in 0..iterations { run(); }
    started.elapsed().as_secs_f64() / iterations as f64
}

fn compare_backends(
    weight: &[u8], input_q8: &[u8], input_scales: &[f32],
    n_in: usize, n_out: usize, iterations: usize,
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
            scalar_times.push(measure_once(|| scalar_q8_matmul(weight, input_q8, input_scales, &mut scalar_output, n_in, n_out), iterations));
            auto_times.push(measure_once(|| matmul_q8_0_quantized(weight, input_q8, input_scales, &mut auto_output, n_in, n_out), iterations));
        } else {
            auto_times.push(measure_once(|| matmul_q8_0_quantized(weight, input_q8, input_scales, &mut auto_output, n_in, n_out), iterations));
            scalar_times.push(measure_once(|| scalar_q8_matmul(weight, input_q8, input_scales, &mut scalar_output, n_in, n_out), iterations));
        }
        std::hint::black_box((&scalar_output, &auto_output));
    }
    (median(&scalar_times), median(&auto_times))
}
```

Keep the existing Qwen3/MiniCPM shape table as auto-backend report-only runs, but feed it deterministic valid buffers. Run `compare_backends` only for the canonical `GATE_N_IN x GATE_N_OUT` case with 20 iterations per sample; compute GFLOPS and GB/s from the two returned medians and report `scalar_median / auto_median` as the speedup.

- [ ] **Step 5: Implement explicit `--check` behavior**

Parse only the existing no-argument report mode and `--check`:

```rust
let check = std::env::args().skip(1).any(|arg| arg == "--check");
if check && !cfg!(all(target_arch = "aarch64", target_os = "macos")) {
    eprintln!("--check requires a fixed aarch64-apple-darwin machine");
    std::process::exit(2);
}
```

After measuring `1024 x 3072`, exit 1 when `check && speedup < GATE_SPEEDUP`; otherwise exit normally. Print the detected architecture and selected production backend at the top so a scalar-only run cannot be mistaken for NEON.

- [ ] **Step 6: Verify helper tests and run the release benchmark**

Run:

```bash
cargo fmt --all
cargo test --bin micro-bench
cargo run --release --bin micro-bench
cargo run --release --bin micro-bench -- --check
```

Expected: helper tests pass; report mode prints finite metrics; `--check` runs only on ARM macOS and exits 0 only when the canonical median speedup is at least 1.10. If the gate fails, retain the measurements and optimize the NEON Q8 range in Task 3 rather than weakening the threshold.

- [ ] **Step 7: Commit the benchmark**

```bash
git add src/bin/micro_bench.rs
git commit -m "bench: add Apple Silicon SIMD performance gate"
```

---

### Task 6: Verify native inference, cross-compile x86, and document measured results

**Files:**
- Modify: `README.md:1-120`
- Modify: `OPTIMIZATION.md:1-151`

**Interfaces:**
- Consumes: all build, test, inference, and benchmark commands delivered by Tasks 1-5.
- Produces: user-facing Apple Silicon instructions and an evidence-backed local benchmark record.

- [ ] **Step 1: Capture the exact ARM environment**

Run and retain the literal output for the optimization record:

```bash
uname -m
sysctl -n machdep.cpu.brand_string
sw_vers
rustc -vV
```

Expected: `uname -m` is `arm64`, Rust host is `aarch64-apple-darwin`, and the other commands identify the exact chip, macOS build, Rust, and LLVM versions.

- [ ] **Step 2: Run the complete fresh verification matrix**

Run:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo build --release --all-targets
cargo run --release --
```

Expected: every command exits 0. The self-test prints `MemoryArena: ptr stable [OK]`.

- [ ] **Step 3: Cross-check the x86_64 macOS build without changing its kernels**

Run:

```bash
rustup target add x86_64-apple-darwin
cargo check --target x86_64-apple-darwin --all-targets
git diff HEAD~5 -- src/ops.rs src/quant.rs src/vision.rs | rg '^[-+]\s*(acc|cv|_mm|let .*_mm|unsafe fn .*avx2)' || true
```

Expected: the target installs, cross-check exits 0, and inspection shows architecture guards/dispatch additions but no algorithmic edits inside existing AVX2/FMA/F16C kernel bodies. Do not report x86 hardware performance because it was not measured on x86 hardware.

- [ ] **Step 4: Run deterministic Qwen3 ARM inference smoke**

Run:

```bash
cargo run --release -- \
  --model models/Qwen3-0.6B-Q8_0.gguf \
  --prompt "2 + 3 =" \
  --max-tokens 1 \
  --temp 0 \
  --bench
```

Expected: exit 0 and the first decoded generated token is ` 5`.

- [ ] **Step 5: Run and capture the ARM micro and full-model benchmarks**

Run:

```bash
cargo run --release --bin micro-bench -- --check
cargo run --release -- \
  --model models/Qwen3-0.6B-Q8_0.gguf \
  --prompt "2 + 3 =" \
  --max-tokens 32 \
  --temp 0 \
  --threads 4 \
  --bench \
  --profile
```

Expected: the micro gate exits 0 with at least 1.10x median speedup; full inference exits 0 and prints timing/tok/s data.

- [ ] **Step 6: Add exact Apple Silicon usage documentation**

Add this section to `README.md` after Quick Start, retaining the existing general commands:

````markdown
### Apple Silicon (ARM64)

Apple Silicon uses native NEON kernels automatically; Rosetta and external C/C++ libraries are not required.

```bash
cargo check --all-targets
cargo test --all-targets
cargo build --release
cargo run --release -- --model models/Qwen3-0.6B-Q8_0.gguf --prompt "2 + 3 =" --max-tokens 1 --temp 0 --bench
cargo run --release --bin micro-bench
```

On a fixed Apple Silicon performance machine, enforce the Q8_0 NEON gate explicitly:

```bash
cargo run --release --bin micro-bench -- --check
```
````

- [ ] **Step 7: Record only measurements actually produced in this execution**

Append `## Apple Silicon NEON（2026-08-09）` to `OPTIMIZATION.md`. Under it, paste the literal environment output from Step 1, the complete `micro-bench --check` summary, and the full-model `--profile` summary from Step 5 in fenced text blocks. Add these two factual sentences verbatim:

```markdown
数值正确性由 NEON/标量单元测试和 Qwen3-0.6B Q8_0 确定性推理冒烟验证。
x86_64 本次仅完成交叉编译与 AVX2/FMA/F16C 路径静态核对，未执行 x86 硬件性能测试。
```

Do not invent, round up, or copy older Intel measurements into this ARM section.

- [ ] **Step 8: Final diff and regression verification**

Run:

```bash
git diff --check
git status --short
cargo test --all-targets
cargo run --release --bin micro-bench -- --check
```

Expected: no whitespace errors; status contains only the intended README/optimization edits at this task; tests pass; the fixed-machine performance gate exits 0.

- [ ] **Step 9: Commit documentation and measured evidence**

```bash
git add README.md OPTIMIZATION.md
git commit -m "docs: document Apple Silicon support and benchmarks"
```

---

## Final Acceptance Checklist

- [ ] `cargo check --all-targets` passes on Apple Silicon.
- [ ] `cargo test --all-targets` passes, including NEON/scalar parity tests.
- [ ] `cargo build --release --all-targets` passes natively.
- [ ] Qwen3-0.6B Q8_0 deterministic inference returns first token ` 5`.
- [ ] `cargo check --target x86_64-apple-darwin --all-targets` passes.
- [ ] Existing x86 AVX2/FMA/F16C kernel bodies remain algorithmically unchanged.
- [ ] `micro-bench --check` reports at least 1.10x median speedup on the fixed Apple Silicon machine.
- [ ] README and OPTIMIZATION contain only commands and measurements actually verified in this execution.
