#[cfg(target_arch = "x86_64")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(all(target_arch = "aarch64", target_endian = "little"))]
use std::arch::asm;

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

#[cfg(target_arch = "aarch64")]
#[inline(always)]
pub const fn has_neon() -> bool { true }

#[cfg(not(target_arch = "aarch64"))]
#[inline(always)]
pub const fn has_neon() -> bool { false }

#[inline]
pub fn f16_to_f32(bits: u16) -> f32 {
    #[cfg(all(target_arch = "x86_64", target_feature = "f16c"))]
    {
        unsafe {
            use std::arch::x86_64::*;
            let v = _mm_set1_epi16(bits as i16);
            _mm_cvtss_f32(_mm_cvtph_ps(v))
        }
    }
    #[cfg(not(all(target_arch = "x86_64", target_feature = "f16c")))]
    {
        half::f16::from_bits(bits).to_f32()
    }
}

#[inline]
pub fn f32_to_f16(v: f32) -> u16 {
    #[cfg(all(target_arch = "x86_64", target_feature = "f16c"))]
    {
        unsafe {
            use std::arch::x86_64::*;
            let fv = _mm_set_ss(v);
            let hv = _mm_cvtps_ph(fv, 0);
            _mm_extract_epi16(hv, 0) as u16
        }
    }
    #[cfg(not(all(target_arch = "x86_64", target_feature = "f16c")))]
    {
        half::f16::from_f32(v).to_bits()
    }
}

pub fn f32_slice_to_f16(src: &[f32], dst: &mut [u16]) {
    debug_assert_eq!(src.len(), dst.len());
    #[cfg(target_arch = "x86_64")]
    {
        if has_f16c() {
            unsafe { f32_slice_to_f16_avx2(src, dst); }
            return;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if has_neon() {
            unsafe { f32_slice_to_f16_neon(src, dst); }
            return;
        }
    }
    for i in 0..src.len() {
        dst[i] = f32_to_f16(src[i]);
    }
}

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
    while i < src.len() {
        dst[i] = f32_to_f16(src[i]);
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
unsafe fn f32_slice_to_f16_avx2(src: &[f32], dst: &mut [u16]) {
    use std::arch::x86_64::*;
    let n = src.len();
    let mut i = 0;
    while i + 8 <= n {
        let v = _mm256_loadu_ps(src.as_ptr().add(i));
        let h = _mm256_cvtps_ph(v, 0);
        _mm_storeu_si128(dst.as_mut_ptr().add(i) as *mut __m128i, h);
        i += 8;
    }
    while i < n {
        dst[i] = f32_to_f16(src[i]);
        i += 1;
    }
}

pub fn rms_norm(input: &[f32], weight: &[f32], output: &mut [f32], eps: f32) {
    let n = input.len().min(weight.len()).min(output.len());
    let sum_sq: f64 = input[..n].iter().map(|&x| f64::from(x * x)).sum();
    let mean_sq = (sum_sq / n as f64) as f32;
    let scale = 1.0f32 / (mean_sq + eps).sqrt();
    for i in 0..n {
        output[i] = input[i] * scale * weight[i];
    }
}

pub fn rms_norm_inplace(x: &mut [f32], weight: &[f32], eps: f32) {
    let n = x.len().min(weight.len());
    let sum_sq: f64 = x[..n].iter().map(|&value| f64::from(value * value)).sum();
    let mean_sq = (sum_sq / n as f64) as f32;
    let scale = 1.0f32 / (mean_sq + eps).sqrt();
    scale_mul_inplace(scale, &weight[..n], &mut x[..n]);
}

fn scale_mul_inplace(scale: f32, weight: &[f32], x: &mut [f32]) {
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2_fma() {
            unsafe { scale_mul_avx2(scale, weight, x) };
            return;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if has_neon() {
            unsafe { scale_mul_neon(scale, weight, x); }
            return;
        }
    }
    for i in 0..weight.len() { x[i] = x[i] * scale * weight[i]; }
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
    while i < x.len() {
        x[i] = x[i] * scale * weight[i];
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn scale_mul_avx2(scale: f32, weight: &[f32], x: &mut [f32]) {
    use std::arch::x86_64::*;
    let n = weight.len();
    let n8 = n / 8 * 8;
    let vscale = _mm256_set1_ps(scale);
    let mut i = 0;
    while i < n8 {
        let vx = _mm256_loadu_ps(x.as_ptr().add(i));
        let vw = _mm256_loadu_ps(weight.as_ptr().add(i));
        _mm256_storeu_ps(x.as_mut_ptr().add(i), _mm256_mul_ps(_mm256_mul_ps(vx, vscale), vw));
        i += 8;
    }
    while i < n { x[i] = x[i] * scale * weight[i]; i += 1; }
}

pub fn rope_neox(x: &mut [f32], pos: usize, head_dim: usize, freq_base: f32) {
    let half = head_dim / 2;
    let n_heads = x.len() / head_dim;
    let theta_scale = freq_base.powf(-2.0f32 / head_dim as f32);
    for h in 0..n_heads {
        let base = h * head_dim;
        let mut theta = pos as f32;
        for i in 0..half {
            let cos_a = theta.cos();
            let sin_a = theta.sin();
            let x0 = x[base + i];
            let x1 = x[base + i + half];
            x[base + i] = x0.mul_add(cos_a, x1 * -sin_a);
            x[base + i + half] = x0.mul_add(sin_a, x1 * cos_a);
            theta *= theta_scale;
        }
    }
}

pub fn rope_mrope(x: &mut [f32], positions: [usize; 4], sections: [i32; 4], head_dim: usize, freq_base: f32) {
    let n_heads = x.len() / head_dim;
    let half = head_dim / 2;
    let total_sections: i32 = sections.iter().sum();
    if total_sections == 0 {
        rope_neox(x, positions[0], head_dim, freq_base);
        return;
    }
    let total_sections = total_sections as usize;
    let theta_scale = freq_base.powf(-2.0f32 / head_dim as f32);
    let section_h = sections[0] as usize;
    let section_w = section_h + sections[1] as usize;
    let section_e = section_w + sections[2] as usize;
    for h in 0..n_heads {
        let base = h * head_dim;
        let mut theta = positions.map(|position| position as f32);
        for i in 0..half {
            let sector = i % total_sections;
            let axis = if sector < section_h {
                0
            } else if sector < section_w {
                1
            } else if sector < section_e {
                2
            } else {
                3
            };
            let cos_a = theta[axis].cos();
            let sin_a = theta[axis].sin();
            let idx0 = base + i;
            let idx1 = idx0 + half;
            let x0 = x[idx0];
            let x1 = x[idx1];
            x[idx0] = x0.mul_add(cos_a, -(x1 * sin_a));
            x[idx1] = x0.mul_add(sin_a, x1 * cos_a);
            for value in &mut theta {
                *value *= theta_scale;
            }
        }
    }
}

pub fn rope_mrope_interleaved(
    x: &mut [f32],
    positions: [usize; 4],
    sections: [i32; 4],
    head_dim: usize,
    freq_base: f32,
    n_rope_dims: usize,
) {
    assert!(n_rope_dims <= head_dim && n_rope_dims % 2 == 0);
    let pair_count = n_rope_dims / 2;
    let section_pairs: usize = sections.iter().map(|&value| value as usize).sum();
    let theta_scale = freq_base.powf(-2.0 / n_rope_dims as f32);
    for head in x.chunks_exact_mut(head_dim) {
        let mut theta = positions.map(|value| value as f32);
        for pair in 0..pair_count {
            let sector = pair % section_pairs;
            let axis = if sector % 3 == 1 && sector < 3 * sections[1] as usize {
                1
            } else if sector % 3 == 2 && sector < 3 * sections[2] as usize {
                2
            } else if sector % 3 == 0 && sector < 3 * sections[0] as usize {
                0
            } else {
                3
            };
            let (sin, cos) = theta[axis].sin_cos();
            let x0 = head[pair];
            let x1 = head[pair + pair_count];
            head[pair] = x0.mul_add(cos, -(x1 * sin));
            head[pair + pair_count] = x0.mul_add(sin, x1 * cos);
            for value in &mut theta {
                *value *= theta_scale;
            }
        }
    }
}

#[inline]
pub fn silu(x: f32) -> f32 {
    x / (1.0f32 + (-x).exp())
}

#[inline(always)]
#[cfg(target_arch = "x86_64")]
unsafe fn dot_f32_avx2(a: &[f32], b: &[f32], n: usize) -> f32 {
    use std::arch::x86_64::*;
    let mut acc = _mm256_setzero_ps();
    let mut i = 0;
    while i + 8 <= n {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));
        acc = _mm256_fmadd_ps(va, vb, acc);
        i += 8;
    }
    let mut sum = hsum_ps(acc);
    while i < n {
        sum += a[i] * b[i];
        i += 1;
    }
    sum
}

#[inline(always)]
fn dot_f32_scalar(a: &[f32], b: &[f32], n: usize) -> f32 {
    let mut s = 0.0f32;
    for i in 0..n { s += a[i] * b[i]; }
    s
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn dot_f32_neon(a: &[f32], b: &[f32], n: usize) -> f32 {
    use std::arch::aarch64::*;
    let mut acc0 = vdupq_n_f32(0.0);
    let mut acc1 = vdupq_n_f32(0.0);
    let mut acc2 = vdupq_n_f32(0.0);
    let mut acc3 = vdupq_n_f32(0.0);
    let mut i = 0;
    while i + 16 <= n {
        acc0 = vfmaq_f32(acc0, vld1q_f32(a.as_ptr().add(i)), vld1q_f32(b.as_ptr().add(i)));
        acc1 = vfmaq_f32(acc1, vld1q_f32(a.as_ptr().add(i + 4)), vld1q_f32(b.as_ptr().add(i + 4)));
        acc2 = vfmaq_f32(acc2, vld1q_f32(a.as_ptr().add(i + 8)), vld1q_f32(b.as_ptr().add(i + 8)));
        acc3 = vfmaq_f32(acc3, vld1q_f32(a.as_ptr().add(i + 12)), vld1q_f32(b.as_ptr().add(i + 12)));
        i += 16;
    }
    acc0 = vaddq_f32(acc0, acc2);
    acc1 = vaddq_f32(acc1, acc3);
    let mut sum = vaddvq_f32(vaddq_f32(acc0, acc1));
    while i < n {
        sum += a[i] * b[i];
        i += 1;
    }
    sum
}

pub fn dot_f16_f32(a: &[f32], b_f16: &[u16], n: usize) -> f32 {
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2_fma() && has_f16c() {
            return unsafe { dot_f16_f32_avx2(a, b_f16, n) };
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if has_neon() {
            return unsafe { dot_f16_f32_neon(a, b_f16, n) };
        }
    }
    let mut s = 0.0f32;
    for i in 0..n { s += a[i] * f16_to_f32(b_f16[i]); }
    s
}

pub fn dot_f16_f16_bytes(a: &[u16], b: &[u8], n: usize) -> f32 {
    debug_assert!(a.len() >= n && b.len() >= n * 2);
    #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
    let (mut sum, tail_start) = {
        let prefix = n & !31;
        if prefix > 0 && std::arch::is_aarch64_feature_detected!("fp16") {
            (
                f64::from(unsafe { dot_f16_f16_fp16_neon(a.as_ptr(), b.as_ptr(), prefix) }),
                prefix,
            )
        } else {
            (0.0, 0)
        }
    };
    #[cfg(not(all(target_arch = "aarch64", target_endian = "little")))]
    let (mut sum, tail_start) = (0.0f64, 0usize);
    for index in tail_start..n {
        let weight = u16::from_le_bytes(b[index * 2..index * 2 + 2].try_into().unwrap());
        sum += f64::from(f16_to_f32(a[index]) * f16_to_f32(weight));
    }
    sum as f32
}

#[cfg(all(target_arch = "aarch64", target_endian = "little"))]
unsafe fn dot_f16_f16_fp16_neon(x: *const u16, y: *const u8, n: usize) -> f32 {
    debug_assert_eq!(n % 32, 0);
    let bits: u32;
    asm!(
        "movi v0.8h, #0", "movi v1.8h, #0",
        "movi v2.8h, #0", "movi v3.8h, #0",
        "cbz {n}, 2f",
        "1:",
        "ld1 {{v4.8h-v7.8h}}, [{x}], #64",
        "ld1 {{v16.8h-v19.8h}}, [{y}], #64",
        "fmla v0.8h, v4.8h, v16.8h",
        "fmla v1.8h, v5.8h, v17.8h",
        "fmla v2.8h, v6.8h, v18.8h",
        "fmla v3.8h, v7.8h, v19.8h",
        "subs {n}, {n}, #32",
        "b.ne 1b",
        "2:",
        "fadd v0.8h, v0.8h, v2.8h",
        "fadd v1.8h, v1.8h, v3.8h",
        "fadd v0.8h, v0.8h, v1.8h",
        "fcvtl v4.4s, v0.4h",
        "fcvtl2 v5.4s, v0.8h",
        "fadd v4.4s, v4.4s, v5.4s",
        "faddp v4.4s, v4.4s, v4.4s",
        "faddp s4, v4.2s",
        "fmov {bits:w}, s4",
        x = inout(reg) x => _,
        y = inout(reg) y => _,
        n = inout(reg) n => _,
        bits = lateout(reg) bits,
        out("v0") _, out("v1") _, out("v2") _, out("v3") _,
        out("v4") _, out("v5") _, out("v6") _, out("v7") _,
        out("v16") _, out("v17") _, out("v18") _, out("v19") _,
        options(nostack, readonly),
    );
    f32::from_bits(bits)
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
    while i < n {
        sum += a[i] * f16_to_f32(b[i]);
        i += 1;
    }
    sum
}

#[cfg(target_arch = "x86_64")]
unsafe fn dot_f16_f32_avx2(a: &[f32], b_f16: &[u16], n: usize) -> f32 {
    use std::arch::x86_64::*;
    let mut acc = _mm256_setzero_ps();
    let mut i = 0;
    while i + 8 <= n {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let hb = _mm_loadu_si128(b_f16.as_ptr().add(i) as *const __m128i);
        let vb = _mm256_cvtph_ps(hb);
        acc = _mm256_fmadd_ps(va, vb, acc);
        i += 8;
    }
    let mut sum = hsum_ps(acc);
    while i < n {
        sum += a[i] * f16_to_f32(b_f16[i]);
        i += 1;
    }
    sum
}

pub fn vec_mad_f16_f32(y: &mut [f32], x_f16: &[u16], v: f32) {
    debug_assert_eq!(y.len(), x_f16.len());
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2_fma() && has_f16c() {
            unsafe { vec_mad_f16_f32_avx2(y, x_f16, v); }
            return;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if has_neon() {
            unsafe { vec_mad_f16_f32_neon(y, x_f16, v); }
            return;
        }
    }
    for i in 0..y.len() { y[i] += v * f16_to_f32(x_f16[i]); }
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
    while i < y.len() {
        y[i] += scale * f16_to_f32(x[i]);
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
unsafe fn vec_mad_f16_f32_avx2(y: &mut [f32], x_f16: &[u16], v: f32) {
    use std::arch::x86_64::*;
    let vv = _mm256_set1_ps(v);
    let n = y.len();
    let mut i = 0;
    while i + 8 <= n {
        let yi = _mm256_loadu_ps(y.as_ptr().add(i));
        let hx = _mm_loadu_si128(x_f16.as_ptr().add(i) as *const __m128i);
        let xf = _mm256_cvtph_ps(hx);
        _mm256_storeu_ps(y.as_mut_ptr().add(i), _mm256_fmadd_ps(vv, xf, yi));
        i += 8;
    }
    if i + 4 <= n {
        let vv128 = _mm256_castps256_ps128(vv);
        let yi = _mm_loadu_ps(y.as_ptr().add(i));
        let hx = _mm_loadl_epi64(x_f16.as_ptr().add(i) as *const __m128i);
        let xf = _mm_cvtph_ps(hx);
        _mm_storeu_ps(y.as_mut_ptr().add(i), _mm_fmadd_ps(vv128, xf, yi));
        i += 4;
    }
    while i < n {
        y[i] += v * f16_to_f32(x_f16[i]);
        i += 1;
    }
}

#[inline(always)]
pub fn dot_f32(a: &[f32], b: &[f32], n: usize) -> f32 {
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2_fma() {
            return unsafe { dot_f32_avx2(a, b, n) };
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if has_neon() {
            return unsafe { dot_f32_neon(a, b, n) };
        }
    }
    dot_f32_scalar(a, b, n)
}

#[inline(always)]
pub fn vec_scale_f32(y: &mut [f32], v: f32) {
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2_fma() {
            unsafe { vec_scale_f32_avx2(y, v); }
            return;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if has_neon() {
            unsafe { vec_scale_f32_neon(y, v); }
            return;
        }
    }
    for y_i in y.iter_mut() { *y_i *= v; }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn vec_scale_f32_neon(y: &mut [f32], scale: f32) {
    use std::arch::aarch64::*;
    let scale_v = vdupq_n_f32(scale);
    let mut i = 0;
    while i + 4 <= y.len() {
        vst1q_f32(y.as_mut_ptr().add(i), vmulq_f32(vld1q_f32(y.as_ptr().add(i)), scale_v));
        i += 4;
    }
    while i < y.len() {
        y[i] *= scale;
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn vec_scale_f32_avx2(y: &mut [f32], v: f32) {
    use std::arch::x86_64::*;
    let vv = _mm256_set1_ps(v);
    let n = y.len();
    let mut i = 0;
    while i + 8 <= n {
        let yi = _mm256_loadu_ps(y.as_ptr().add(i));
        _mm256_storeu_ps(y.as_mut_ptr().add(i), _mm256_mul_ps(yi, vv));
        i += 8;
    }
    if i + 4 <= n {
        let vv128 = _mm256_castps256_ps128(vv);
        let yi = _mm_loadu_ps(y.as_ptr().add(i));
        _mm_storeu_ps(y.as_mut_ptr().add(i), _mm_mul_ps(yi, vv128));
        i += 4;
    }
    while i < n {
        y[i] *= v;
        i += 1;
    }
}

#[inline(always)]
pub fn vec_mad_f32(y: &mut [f32], x: &[f32], v: f32) {
    debug_assert_eq!(y.len(), x.len());
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2_fma() {
            unsafe { vec_mad_f32_avx2(y, x, v); }
            return;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if has_neon() {
            unsafe { vec_mad_f32_neon(y, x, v); }
            return;
        }
    }
    let vv = v;
    for i in 0..y.len() { y[i] += vv * x[i]; }
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
    while i < y.len() {
        y[i] += x[i] * scale;
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn vec_mad_f32_avx2(y: &mut [f32], x: &[f32], v: f32) {
    use std::arch::x86_64::*;
    let vv = _mm256_set1_ps(v);
    let n = y.len();
    let mut i = 0;
    while i + 8 <= n {
        let yi = _mm256_loadu_ps(y.as_ptr().add(i));
        let xi = _mm256_loadu_ps(x.as_ptr().add(i));
        _mm256_storeu_ps(y.as_mut_ptr().add(i), _mm256_fmadd_ps(vv, xi, yi));
        i += 8;
    }
    if i + 4 <= n {
        let vv128 = _mm256_castps256_ps128(vv);
        let yi = _mm_loadu_ps(y.as_ptr().add(i));
        let xi = _mm_loadu_ps(x.as_ptr().add(i));
        _mm_storeu_ps(y.as_mut_ptr().add(i), _mm_fmadd_ps(vv128, xi, yi));
        i += 4;
    }
    while i < n {
        y[i] += v * x[i];
        i += 1;
    }
}

pub fn softmax(x: &mut [f32]) {
    if x.is_empty() { return; }
    #[cfg(target_arch = "aarch64")]
    if has_neon() {
        unsafe { softmax_neon_ggml(x); }
        return;
    }
    let max_val = x.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let mut sum = 0.0f32;
    for v in x.iter_mut() {
        *v = (*v - max_val).exp();
        sum += *v;
    }
    if sum > 0.0 {
        for v in x.iter_mut() { *v /= sum; }
    }
}

pub fn attention_value_f32(values: &[f32], weights: &[f32], n_cached: usize, n_padded: usize) -> f32 {
    debug_assert!(n_cached <= n_padded);
    dot_f32(values, weights, n_padded)
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn softmax_neon_ggml(x: &mut [f32]) {
    use std::arch::aarch64::*;

    let max = x.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut sum = 0.0f64;
    let mut i = 0;
    while i + 4 <= x.len() {
        let values = ggml_expf_neon(vsubq_f32(
            vld1q_f32(x.as_ptr().add(i)),
            vdupq_n_f32(max),
        ));
        vst1q_f32(x.as_mut_ptr().add(i), values);
        sum += f64::from(vaddvq_f32(values));
        i += 4;
    }
    while i < x.len() {
        x[i] = (x[i] - max).exp();
        sum += f64::from(x[i]);
        i += 1;
    }
    vec_scale_f32_neon(x, (1.0 / sum) as f32);
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn ggml_expf_neon(x: std::arch::aarch64::float32x4_t) -> std::arch::aarch64::float32x4_t {
    use std::arch::aarch64::*;

    let r = vdupq_n_f32(f32::from_bits(0x4b40_0000));
    let z = vfmaq_f32(r, x, vdupq_n_f32(f32::from_bits(0x3fb8_aa3b)));
    let n = vsubq_f32(z, r);
    let b = vfmsq_f32(
        vfmsq_f32(x, n, vdupq_n_f32(f32::from_bits(0x3f31_7200))),
        n,
        vdupq_n_f32(f32::from_bits(0x35bf_be8e)),
    );
    let e = vshlq_n_u32(vreinterpretq_u32_f32(z), 23);
    let k = vreinterpretq_f32_u32(vaddq_u32(
        e,
        vreinterpretq_u32_f32(vdupq_n_f32(1.0)),
    ));
    let c = vcagtq_f32(n, vdupq_n_f32(126.0));
    let u = vmulq_f32(b, b);
    let j = vfmaq_f32(
        vmulq_f32(vdupq_n_f32(f32::from_bits(0x3f7f_fff6)), b),
        vfmaq_f32(
            vfmaq_f32(
                vdupq_n_f32(f32::from_bits(0x3eff_fedb)),
                vdupq_n_f32(f32::from_bits(0x3e2a_af33)),
                b,
            ),
            vfmaq_f32(
                vdupq_n_f32(f32::from_bits(0x3d2b_9f17)),
                vdupq_n_f32(f32::from_bits(0x3c07_2010)),
                b,
            ),
            u,
        ),
        u,
    );
    if vaddvq_u32(c) == 0 {
        return vfmaq_f32(k, j, k);
    }
    let d = vandq_u32(vclezq_f32(n), vdupq_n_u32(0x8200_0000));
    let s1 = vreinterpretq_f32_u32(vaddq_u32(d, vdupq_n_u32(0x7f00_0000)));
    let s2 = vreinterpretq_f32_u32(vsubq_u32(e, d));
    vbslq_f32(
        vcagtq_f32(n, vdupq_n_f32(192.0)),
        vmulq_f32(s1, s1),
        vbslq_f32(
            c,
            vmulq_f32(vfmaq_f32(s2, s2, j), s1),
            vfmaq_f32(k, k, j),
        ),
    )
}

pub fn quantize_q8_0_into(input: &[f32], n: usize, q8: &mut [u8], scales: &mut [f32]) {
    let blocks = n / 32;
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2_fma() {
            unsafe { quantize_q8_0_into_avx2(input, n, q8, scales); }
            return;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if has_neon() {
            unsafe { quantize_q8_0_into_neon_range(input, q8, scales, 0, blocks); }
            return;
        }
    }
    quantize_q8_0_into_scalar_range(input, q8, scales, 0, blocks);
}

pub fn quantize_q8_0_into_parallel(input: &[f32], n: usize, q8: &mut [u8], scales: &mut [f32], ith: usize, nth: usize) {
    let blocks = n / 32;
    let b_start = ith * blocks / nth;
    let b_end = (ith + 1) * blocks / nth;
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2_fma() {
            unsafe { quantize_q8_0_into_avx2_range(input, q8, scales, b_start, b_end); }
            return;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if has_neon() {
            unsafe { quantize_q8_0_into_neon_range(input, q8, scales, b_start, b_end); }
            return;
        }
    }
    quantize_q8_0_into_scalar_range(input, q8, scales, b_start, b_end);
}

fn quantize_q8_0_into_scalar_range(
    input: &[f32],
    q8: &mut [u8],
    scales: &mut [f32],
    block_start: usize,
    block_end: usize,
) {
    for block in block_start..block_end {
        let values = &input[block * 32..(block + 1) * 32];
        let amax = values.iter().fold(0.0f32, |current, value| current.max(value.abs()));
        let scale = if amax == 0.0 { 0.0 } else { amax / 127.0 };
        let inverse = if scale == 0.0 { 0.0 } else { 1.0 / scale };
        scales[block] = f16_to_f32(f32_to_f16(scale));
        for lane in 0..32 {
            q8[block * 32 + lane] = (values[lane] * inverse).round().clamp(-128.0, 127.0) as i8 as u8;
        }
    }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn quantize_q8_0_into_neon_range(
    input: &[f32],
    q8: &mut [u8],
    scales: &mut [f32],
    block_start: usize,
    block_end: usize,
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
        let inverse = if scale == 0.0 { 0.0 } else { 1.0 / scale };
        scales[block] = f16_to_f32(f32_to_f16(scale));
        let inverse = vdupq_n_f32(inverse);
        let q0 = vcvtnq_s32_f32(vmulq_f32(v0, inverse));
        let q1 = vcvtnq_s32_f32(vmulq_f32(v1, inverse));
        let q2 = vcvtnq_s32_f32(vmulq_f32(v2, inverse));
        let q3 = vcvtnq_s32_f32(vmulq_f32(v3, inverse));
        let q4 = vcvtnq_s32_f32(vmulq_f32(v4, inverse));
        let q5 = vcvtnq_s32_f32(vmulq_f32(v5, inverse));
        let q6 = vcvtnq_s32_f32(vmulq_f32(v6, inverse));
        let q7 = vcvtnq_s32_f32(vmulq_f32(v7, inverse));
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

#[cfg(target_arch = "x86_64")]
unsafe fn quantize_q8_0_into_avx2(input: &[f32], n: usize, q8: &mut [u8], scales: &mut [f32]) {
    quantize_q8_0_into_avx2_range(input, q8, scales, 0, n / 32);
}

#[cfg(target_arch = "x86_64")]
unsafe fn quantize_q8_0_into_avx2_range(input: &[f32], q8: &mut [u8], scales: &mut [f32], b_start: usize, b_end: usize) {
    use std::arch::x86_64::*;
    let sign_mask = _mm256_set1_ps(-0.0f32);
    let max_i8 = _mm256_set1_ps(127.0);
    let min_i8 = _mm256_set1_ps(-128.0);
    for b in b_start..b_end {
        let ptr = input.as_ptr().add(b * 32);
        let v0 = _mm256_loadu_ps(ptr);
        let v1 = _mm256_loadu_ps(ptr.add(8));
        let v2 = _mm256_loadu_ps(ptr.add(16));
        let v3 = _mm256_loadu_ps(ptr.add(24));
        let a0 = _mm256_andnot_ps(sign_mask, v0);
        let a1 = _mm256_andnot_ps(sign_mask, v1);
        let a2 = _mm256_andnot_ps(sign_mask, v2);
        let a3 = _mm256_andnot_ps(sign_mask, v3);
        let m01 = _mm256_max_ps(a0, a1);
        let m23 = _mm256_max_ps(a2, a3);
        let m0123 = _mm256_max_ps(m01, m23);
        let hi = _mm256_extractf128_ps(m0123, 1);
        let lo = _mm256_castps256_ps128(m0123);
        let m128 = _mm_max_ps(hi, lo);
        let shuf = _mm_movehdup_ps(m128);
        let m2 = _mm_max_ps(m128, shuf);
        let m3 = _mm_movehl_ps(shuf, m2);
        let amax = _mm_cvtss_f32(_mm_max_ss(m2, m3));
        let d = if amax == 0.0 { 0.0 } else { amax / 127.0 };
        let id = if amax == 0.0 { 0.0 } else { 127.0 / amax };
        scales[b] = f16_to_f32(f32_to_f16(d));
        let id_v = _mm256_set1_ps(id);
        let r0 = _mm256_round_ps(_mm256_mul_ps(v0, id_v), _MM_FROUND_TO_NEAREST_INT);
        let r1 = _mm256_round_ps(_mm256_mul_ps(v1, id_v), _MM_FROUND_TO_NEAREST_INT);
        let r2 = _mm256_round_ps(_mm256_mul_ps(v2, id_v), _MM_FROUND_TO_NEAREST_INT);
        let r3 = _mm256_round_ps(_mm256_mul_ps(v3, id_v), _MM_FROUND_TO_NEAREST_INT);
        let c0 = _mm256_min_ps(_mm256_max_ps(r0, min_i8), max_i8);
        let c1 = _mm256_min_ps(_mm256_max_ps(r1, min_i8), max_i8);
        let c2 = _mm256_min_ps(_mm256_max_ps(r2, min_i8), max_i8);
        let c3 = _mm256_min_ps(_mm256_max_ps(r3, min_i8), max_i8);
        let i0 = _mm256_cvtps_epi32(c0);
        let i1 = _mm256_cvtps_epi32(c1);
        let i2 = _mm256_cvtps_epi32(c2);
        let i3 = _mm256_cvtps_epi32(c3);
        let p01 = _mm256_packs_epi32(i0, i1);
        let p23 = _mm256_packs_epi32(i2, i3);
        let packed = _mm256_packs_epi16(p01, p23);
        let perm = _mm256_setr_epi32(0, 4, 1, 5, 2, 6, 3, 7);
        let fixed = _mm256_permutevar8x32_epi32(packed, perm);
        _mm256_storeu_si256(q8.as_mut_ptr().add(b * 32) as *mut __m256i, fixed);
    }
}

pub fn quantize_q8_0(input: &[f32], n: usize) -> (Vec<u8>, Vec<f32>) {
    let blocks = n / 32;
    let mut q8 = vec![0u8; n];
    let mut scales = vec![0.0f32; blocks];
    quantize_q8_0_into(input, n, &mut q8, &mut scales);
    (q8, scales)
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn dot_i8x32_neon(a: *const u8, b: *const u8) -> i32 {
    use std::arch::aarch64::*;
    let a0 = vld1q_s8(a as *const i8);
    let b0 = vld1q_s8(b as *const i8);
    let a1 = vld1q_s8(a.add(16) as *const i8);
    let b1 = vld1q_s8(b.add(16) as *const i8);
    let p0 = vaddq_s32(
        vpaddlq_s16(vmull_s8(vget_low_s8(a0), vget_low_s8(b0))),
        vpaddlq_s16(vmull_high_s8(a0, b0)),
    );
    let p1 = vaddq_s32(
        vpaddlq_s16(vmull_s8(vget_low_s8(a1), vget_low_s8(b1))),
        vpaddlq_s16(vmull_high_s8(a1, b1)),
    );
    vaddvq_s32(vaddq_s32(p0, p1))
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn matmul_q8_0_vs_q8_0_neon(
    weight: &[u8],
    input_q8: &[u8],
    input_scales: &[f32],
    output: &mut [f32],
    n_in: usize,
    row_start: usize,
    row_end: usize,
) {
    let blocks = n_in / 32;
    let stride = blocks * 34;
    for (out_idx, row) in (row_start..row_end).enumerate() {
        let mut sum = 0.0f32;
        for block in 0..blocks {
            let off = row * stride + block * 34;
            let wd = f16_to_f32(u16::from_le_bytes([weight[off], weight[off + 1]]));
            let dot = dot_i8x32_neon(
                weight.as_ptr().add(off + 2),
                input_q8.as_ptr().add(block * 32),
            );
            sum = (dot as f32).mul_add(wd * input_scales[block], sum);
        }
        output[out_idx] = sum;
    }
}

#[cfg(target_arch = "x86_64")]
#[inline(never)]
unsafe fn matmul_q8_0_vs_q8_0_avx2(weight: &[u8], input_q8: &[u8], input_scales: &[f32], output: &mut [f32], n_in: usize, row_start: usize, row_end: usize) {
    use std::arch::x86_64::*;
    let blocks_per_row = n_in / 32;
    let row_stride = blocks_per_row * 34;
    let ones = _mm256_set1_epi16(1);
    let n_rows = row_end - row_start;
    let w_ptr = weight.as_ptr();
    let sc_ptr = input_scales.as_ptr();
    let out_ptr = output.as_mut_ptr();

    let full4 = n_rows / 4;
    for tile in 0..full4 {
        let r0 = row_start + tile * 4;
        let off0 = r0 * row_stride;
        let off1 = (r0 + 1) * row_stride;
        let off2 = (r0 + 2) * row_stride;
        let off3 = (r0 + 3) * row_stride;
        let mut cv0 = _mm256_setzero_ps();
        let mut cv1 = _mm256_setzero_ps();
        let mut cv2 = _mm256_setzero_ps();
        let mut cv3 = _mm256_setzero_ps();
        for b in 0..blocks_per_row {
            let qy = _mm256_loadu_si256(input_q8.as_ptr().add(b * 32) as *const __m256i);
            let bd = *sc_ptr.add(b);

            let p0 = w_ptr.add(off0 + b * 34);
            let p1 = w_ptr.add(off1 + b * 34);
            let p2 = w_ptr.add(off2 + b * 34);
            let p3 = w_ptr.add(off3 + b * 34);

            let a0_d = std::ptr::read_unaligned(p0 as *const u16);
            let a1_d = std::ptr::read_unaligned(p1 as *const u16);
            let a2_d = std::ptr::read_unaligned(p2 as *const u16);
            let a3_d = std::ptr::read_unaligned(p3 as *const u16);

            let da = _mm_mul_ps(_mm_cvtph_ps(_mm_set_epi16(0, 0, 0, 0, a3_d as i16, a2_d as i16, a1_d as i16, a0_d as i16)), _mm_set1_ps(bd));
            let s0 = _mm256_broadcastss_ps(da);
            let s1 = _mm256_broadcastss_ps(_mm_shuffle_ps(da, da, 0x55));
            let s2 = _mm256_broadcastss_ps(_mm_shuffle_ps(da, da, 0xAA));
            let s3 = _mm256_broadcastss_ps(_mm_shuffle_ps(da, da, 0xFF));

            let av0 = _mm256_loadu_si256(p0.add(2) as *const __m256i);
            let av1 = _mm256_loadu_si256(p1.add(2) as *const __m256i);
            let av2 = _mm256_loadu_si256(p2.add(2) as *const __m256i);
            let av3 = _mm256_loadu_si256(p3.add(2) as *const __m256i);

            let ax0 = _mm256_sign_epi8(av0, av0);
            let ax1 = _mm256_sign_epi8(av1, av1);
            let ax2 = _mm256_sign_epi8(av2, av2);
            let ax3 = _mm256_sign_epi8(av3, av3);
            let sy0 = _mm256_sign_epi8(qy, av0);
            let sy1 = _mm256_sign_epi8(qy, av1);
            let sy2 = _mm256_sign_epi8(qy, av2);
            let sy3 = _mm256_sign_epi8(qy, av3);

            cv0 = _mm256_fmadd_ps(s0, _mm256_cvtepi32_ps(_mm256_madd_epi16(ones, _mm256_maddubs_epi16(ax0, sy0))), cv0);
            cv1 = _mm256_fmadd_ps(s1, _mm256_cvtepi32_ps(_mm256_madd_epi16(ones, _mm256_maddubs_epi16(ax1, sy1))), cv1);
            cv2 = _mm256_fmadd_ps(s2, _mm256_cvtepi32_ps(_mm256_madd_epi16(ones, _mm256_maddubs_epi16(ax2, sy2))), cv2);
            cv3 = _mm256_fmadd_ps(s3, _mm256_cvtepi32_ps(_mm256_madd_epi16(ones, _mm256_maddubs_epi16(ax3, sy3))), cv3);
        }
        let base_out = tile * 4;
        *out_ptr.add(base_out) = hsum_ps(cv0);
        *out_ptr.add(base_out + 1) = hsum_ps(cv1);
        *out_ptr.add(base_out + 2) = hsum_ps(cv2);
        *out_ptr.add(base_out + 3) = hsum_ps(cv3);
    }

    for (out_idx, j) in (row_start + full4 * 4..row_end).enumerate() {
        let row_off = j * row_stride;
        let mut acc = _mm256_setzero_ps();
        for b in 0..blocks_per_row {
            let w_off = row_off + b * 34;
            let wd = std::ptr::read_unaligned(w_ptr.add(w_off) as *const u16);
            let d = _mm_cvtss_f32(_mm_cvtph_ps(_mm_set1_epi16(wd as i16))) * *sc_ptr.add(b);
            let d_v = _mm256_set1_ps(d);
            let qx = _mm256_loadu_si256(w_ptr.add(w_off + 2) as *const __m256i);
            let qy = _mm256_loadu_si256(input_q8.as_ptr().add(b * 32) as *const __m256i);
            let ax = _mm256_sign_epi8(qx, qx);
            let sy = _mm256_sign_epi8(qy, qx);
            let dot = _mm256_maddubs_epi16(ax, sy);
            let summed = _mm256_madd_epi16(ones, dot);
            acc = _mm256_fmadd_ps(d_v, _mm256_cvtepi32_ps(summed), acc);
        }
        *out_ptr.add(full4 * 4 + out_idx) = hsum_ps(acc);
    }
}

#[cfg(target_arch = "x86_64")]
unsafe fn matmul_q8_0_avx2_range(weight: &[u8], input: &[f32], output: &mut [f32], n_in: usize, row_start: usize, row_end: usize) {
    use std::arch::x86_64::*;
    let blocks_per_row = n_in / 32;
    let row_stride = blocks_per_row * 34;
    for (out_idx, j) in (row_start..row_end).enumerate() {
        let row_off = j * row_stride;
        let mut acc0 = _mm256_setzero_ps();
        let mut acc1 = _mm256_setzero_ps();
        for b in 0..blocks_per_row {
            let off = row_off + b * 34;
            let d = f16_to_f32(u16::from_le_bytes([weight[off], weight[off + 1]]));
            let d_v = _mm256_set1_ps(d);
            let qs = weight.as_ptr().add(off + 2);
            let inp = input.as_ptr().add(b * 32);
            let q0 = _mm256_cvtepi8_epi32(_mm_loadl_epi64(qs as *const __m128i));
            let q1 = _mm256_cvtepi8_epi32(_mm_loadl_epi64(qs.add(8) as *const __m128i));
            let q2 = _mm256_cvtepi8_epi32(_mm_loadl_epi64(qs.add(16) as *const __m128i));
            let q3 = _mm256_cvtepi8_epi32(_mm_loadl_epi64(qs.add(24) as *const __m128i));
            let i0 = _mm256_loadu_ps(inp);
            let i1 = _mm256_loadu_ps(inp.add(8));
            let i2 = _mm256_loadu_ps(inp.add(16));
            let i3 = _mm256_loadu_ps(inp.add(24));
            acc0 = _mm256_fmadd_ps(_mm256_mul_ps(d_v, _mm256_cvtepi32_ps(q0)), i0, acc0);
            acc1 = _mm256_fmadd_ps(_mm256_mul_ps(d_v, _mm256_cvtepi32_ps(q1)), i1, acc1);
            acc0 = _mm256_fmadd_ps(_mm256_mul_ps(d_v, _mm256_cvtepi32_ps(q2)), i2, acc0);
            acc1 = _mm256_fmadd_ps(_mm256_mul_ps(d_v, _mm256_cvtepi32_ps(q3)), i3, acc1);
        }
        let s = _mm256_add_ps(acc0, acc1);
        output[out_idx] = hsum_ps(s);
    }
}

#[inline]
#[cfg(target_arch = "x86_64")]
pub unsafe fn hsum_ps(v: std::arch::x86_64::__m256) -> f32 {
    use std::arch::x86_64::*;
    let hi = _mm256_extractf128_ps(v, 1);
    let lo = _mm256_castps256_ps128(v);
    let s128 = _mm_add_ps(hi, lo);
    let shuf = _mm_movehdup_ps(s128);
    let s2 = _mm_add_ps(s128, shuf);
    let s3 = _mm_movehl_ps(shuf, s2);
    _mm_cvtss_f32(_mm_add_ss(s2, s3))
}

pub fn matmul_q8_0_via_q8(weight: &[u8], input: &[f32], output: &mut [f32], n_in: usize, n_out: usize, q8_buf: &mut [u8], scale_buf: &mut [f32]) {
    quantize_q8_0_into(input, n_in, q8_buf, scale_buf);
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2_fma() {
            unsafe { matmul_q8_0_vs_q8_0_avx2(weight, q8_buf, scale_buf, output, n_in, 0, n_out); }
            return;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if has_neon() {
            unsafe { matmul_q8_0_vs_q8_0_neon(weight, q8_buf, scale_buf, output, n_in, 0, n_out); }
            return;
        }
    }
    matmul_q8_0_quantized_scalar_range(weight, q8_buf, scale_buf, output, n_in, 0, n_out);
}

pub fn matmul_q8_0_via_q8_parallel(weight: &[u8], input: &[f32], output: &mut [f32], n_in: usize, n_out: usize, q8_buf: &mut [u8], scale_buf: &mut [f32]) {
    quantize_q8_0_into(input, n_in, q8_buf, scale_buf);
    matmul_q8_0_quantized_parallel(weight, q8_buf, scale_buf, output, n_in, n_out);
}

fn matmul_q8_0_fallback_range(weight: &[u8], input: &[f32], output: &mut [f32], n_in: usize, row_start: usize, row_end: usize) {
    let blocks_per_row = n_in / 32;
    let row_stride = blocks_per_row * 34;
    for (out_idx, j) in (row_start..row_end).enumerate() {
        let row_off = j * row_stride;
        let mut sum = 0.0f32;
        for b in 0..blocks_per_row {
            let off = row_off + b * 34;
            let d = f16_to_f32(u16::from_le_bytes([weight[off], weight[off + 1]]));
            let qs = &weight[off + 2..off + 34];
            let inp = &input[b * 32..];
            let mut local = 0.0f32;
            for k in 0..32 {
                local += (qs[k] as i8 as f32) * inp[k];
            }
            sum += d * local;
        }
        output[out_idx] = sum;
    }
}

pub fn matmul_q8_0_quantized(weight: &[u8], input_q8: &[u8], input_scales: &[f32], output: &mut [f32], n_in: usize, n_out: usize) {
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2_fma() {
            unsafe { matmul_q8_0_vs_q8_0_avx2(weight, input_q8, input_scales, output, n_in, 0, n_out); }
            return;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if has_neon() {
            unsafe { matmul_q8_0_vs_q8_0_neon(weight, input_q8, input_scales, output, n_in, 0, n_out); }
            return;
        }
    }
    matmul_q8_0_quantized_scalar_range(weight, input_q8, input_scales, output, n_in, 0, n_out);
}

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

pub fn matmul_q8_0_quantized_parallel_rows(weight: &[u8], input_q8: &[u8], input_scales: &[f32], output: &mut [f32], n_in: usize, n_out: usize, ith: usize, nth: usize) {
    if nth <= 1 || n_out == 0 {
        matmul_q8_0_quantized_range(weight, input_q8, input_scales, output, n_in, 0, n_out);
        return;
    }
    let per_thread = (n_out + nth - 1) / nth;
    let my_start = ith * per_thread;
    let my_end = (my_start + per_thread).min(n_out);
    if my_start >= my_end { return; }
    matmul_q8_0_quantized_range(weight, input_q8, input_scales, &mut output[my_start..my_end], n_in, my_start, my_end);
}

pub fn matmul_q8_0_quantized_dynamic(weight: &[u8], input_q8: &[u8], input_scales: &[f32], output: &mut [f32], n_in: usize, n_out: usize, pool: &crate::thread_pool::ComputePool) {
    if n_out == 0 { return; }
    let chunk_size = 16.max(n_out / (pool.n_threads() * 4));
    let n_chunks = (n_out as i32 + chunk_size as i32 - 1) / chunk_size as i32;
    let w_ptr = weight.as_ptr() as usize;
    let w_len = weight.len();
    let iq_ptr = input_q8.as_ptr() as usize;
    let iq_len = input_q8.len();
    let sc_ptr = input_scales.as_ptr() as usize;
    let sc_len = input_scales.len();
    let out_ptr = output.as_mut_ptr() as usize;
    pool.compute_with_chunks(n_chunks, move |_ith, chunk_id| {
        let row_start = (chunk_id as usize) * chunk_size;
        let row_end = (row_start + chunk_size).min(n_out);
        if row_start >= row_end { return; }
        let w = unsafe { std::slice::from_raw_parts(w_ptr as *const u8, w_len) };
        let iq = unsafe { std::slice::from_raw_parts(iq_ptr as *const u8, iq_len) };
        let sc = unsafe { std::slice::from_raw_parts(sc_ptr as *const f32, sc_len) };
        let out_slice = unsafe { std::slice::from_raw_parts_mut((out_ptr as *mut f32).add(row_start), row_end - row_start) };
        matmul_q8_0_quantized_range(w, iq, sc, out_slice, n_in, row_start, row_end);
    });
}

pub fn matmul_q8_0_quantized_range(weight: &[u8], input_q8: &[u8], input_scales: &[f32], output: &mut [f32], n_in: usize, row_start: usize, row_end: usize) {
    debug_assert_eq!(output.len(), row_end - row_start);
    #[cfg(target_arch = "x86_64")]
    if has_avx2_fma() {
        unsafe { matmul_q8_0_vs_q8_0_avx2(weight, input_q8, input_scales, output, n_in, row_start, row_end); }
        return;
    }
    #[cfg(target_arch = "aarch64")]
    if has_neon() {
        unsafe { matmul_q8_0_vs_q8_0_neon(weight, input_q8, input_scales, output, n_in, row_start, row_end); }
        return;
    }
    matmul_q8_0_quantized_scalar_range(weight, input_q8, input_scales, output, n_in, row_start, row_end);
}

pub fn q8_0_dot_row(weight: &[u8], input_q8: &[u8], input_scales: &[f32], n_in: usize, row: usize, _use_avx2: bool) -> f32 {
    #[cfg(target_arch = "x86_64")]
    let blocks_per_row = n_in / 32;
    #[cfg(target_arch = "x86_64")]
    let row_stride = blocks_per_row * 34;
    #[cfg(target_arch = "x86_64")]
    if _use_avx2 {
        return unsafe { q8_0_dot_row_avx2(weight, input_q8, input_scales, n_in, row, blocks_per_row, row_stride) };
    }
    #[cfg(target_arch = "aarch64")]
    if has_neon() {
        let mut output = [0.0];
        unsafe {
            matmul_q8_0_vs_q8_0_neon(
                weight,
                input_q8,
                input_scales,
                &mut output,
                n_in,
                row,
                row + 1,
            );
        }
        return output[0];
    }
    let mut output = [0.0];
    matmul_q8_0_quantized_scalar_range(weight, input_q8, input_scales, &mut output, n_in, row, row + 1);
    output[0]
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn q8_0_dot_row_avx2(weight: &[u8], input_q8: &[u8], input_scales: &[f32], n_in: usize, row: usize, blocks_per_row: usize, row_stride: usize) -> f32 {
    use std::arch::x86_64::*;
    let ones = _mm256_set1_epi16(1);
    let row_off = row * row_stride;
    let mut acc = _mm256_setzero_ps();
    for b in 0..blocks_per_row {
        let w_off = row_off + b * 34;
        let d = f16_to_f32(u16::from_le_bytes([*weight.as_ptr().add(w_off), *weight.as_ptr().add(w_off + 1)])) * *input_scales.as_ptr().add(b);
        let d_v = _mm256_set1_ps(d);
        let qx = _mm256_loadu_si256(weight.as_ptr().add(w_off + 2) as *const __m256i);
        let qy = _mm256_loadu_si256(input_q8.as_ptr().add(b * 32) as *const __m256i);
        let ax = _mm256_sign_epi8(qx, qx);
        let sy = _mm256_sign_epi8(qy, qx);
        let dot = _mm256_maddubs_epi16(ax, sy);
        let summed = _mm256_madd_epi16(ones, dot);
        acc = _mm256_fmadd_ps(d_v, _mm256_cvtepi32_ps(summed), acc);
    }
    hsum_ps(acc)
}

pub fn matmul_q8_0_quantized_parallel(weight: &[u8], input_q8: &[u8], input_scales: &[f32], output: &mut [f32], n_in: usize, n_out: usize) {
    let use_avx2 = has_avx2_fma();
    let min_rows = 64;
    parallel_range(weight, input_q8, input_scales, output, n_in, 0, n_out, use_avx2, min_rows);
}

fn parallel_range(weight: &[u8], input_q8: &[u8], input_scales: &[f32], output: &mut [f32], n_in: usize, row_start: usize, row_end: usize, use_avx2: bool, min_rows: usize) {
    let n = row_end - row_start;
    if n <= min_rows {
        #[cfg(target_arch = "x86_64")]
        if use_avx2 {
            unsafe { matmul_q8_0_vs_q8_0_avx2(weight, input_q8, input_scales, output, n_in, row_start, row_end); }
            return;
        }
        #[cfg(target_arch = "aarch64")]
        if has_neon() {
            unsafe {
                matmul_q8_0_vs_q8_0_neon(
                    weight,
                    input_q8,
                    input_scales,
                    output,
                    n_in,
                    row_start,
                    row_end,
                );
            }
            return;
        }
        matmul_q8_0_quantized_scalar_range(weight, input_q8, input_scales, output, n_in, row_start, row_end);
        return;
    }
    let mid_row = row_start + n / 2;
    let mid_idx = mid_row - row_start;
    let (lo, hi) = output.split_at_mut(mid_idx);
    rayon::join(
        || parallel_range(weight, input_q8, input_scales, lo, n_in, row_start, mid_row, use_avx2, min_rows),
        || parallel_range(weight, input_q8, input_scales, hi, n_in, mid_row, row_end, use_avx2, min_rows),
    );
}

pub fn matmul_q8_0(weight: &[u8], input: &[f32], output: &mut [f32], n_in: usize, n_out: usize) {
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2_fma() {
            unsafe { matmul_q8_0_avx2_range(weight, input, output, n_in, 0, n_out); }
            return;
        }
    }
    matmul_q8_0_fallback_range(weight, input, output, n_in, 0, n_out);
}

pub fn matmul_q8_0_parallel(weight: &[u8], input: &[f32], output: &mut [f32], n_in: usize, n_out: usize, _n_threads: usize) {
    use rayon::prelude::*;
    #[cfg(target_arch = "x86_64")]
    let use_avx2 = has_avx2_fma();
    let chunk = 128;
    output.par_chunks_mut(chunk).enumerate().for_each(|(i, out_slice)| {
        let rs = i * chunk;
        let re = (rs + chunk).min(n_out);
        #[cfg(target_arch = "x86_64")]
        if use_avx2 {
            unsafe { matmul_q8_0_avx2_range(weight, input, out_slice, n_in, rs, re); }
            return;
        }
        matmul_q8_0_fallback_range(weight, input, out_slice, n_in, rs, re);
    });
}

pub struct MatmulTask<'a> {
    pub weight: &'a [u8],
    pub input: &'a [f32],
    pub output: &'a mut [f32],
    pub n_in: usize,
    pub n_out: usize,
}

pub fn matmul_q8_0_batch(tasks: &mut [MatmulTask<'_>]) {
    use rayon::prelude::*;
    #[cfg(target_arch = "x86_64")]
    let use_avx2 = has_avx2_fma();
    let chunk = 128;
    struct TaskInfo {
        w_ptr: usize, w_len: usize,
        i_ptr: usize, i_len: usize,
        o_ptr: usize,
        n_in: usize,
    }
    unsafe impl Sync for TaskInfo {}
    let mut infos: Vec<TaskInfo> = Vec::new();
    let mut work_items: Vec<(usize, usize, usize)> = Vec::new();
    for task in tasks.iter_mut() {
        infos.push(TaskInfo {
            w_ptr: task.weight.as_ptr() as usize, w_len: task.weight.len(),
            i_ptr: task.input.as_ptr() as usize, i_len: task.input.len(),
            o_ptr: task.output.as_mut_ptr() as usize,
            n_in: task.n_in,
        });
        let n_chunks = (task.n_out + chunk - 1) / chunk;
        let ti = infos.len() - 1;
        for ci in 0..n_chunks {
            let rs = ci * chunk;
            let re = (rs + chunk).min(task.n_out);
            work_items.push((ti, rs, re));
        }
    }
    work_items.par_iter().for_each(|&(ti, rs, re)| {
        let info = &infos[ti];
        let weight = unsafe { std::slice::from_raw_parts(info.w_ptr as *const u8, info.w_len) };
        let input = unsafe { std::slice::from_raw_parts(info.i_ptr as *const f32, info.i_len) };
        let out_slice = unsafe { std::slice::from_raw_parts_mut((info.o_ptr as *mut f32).add(rs), re - rs) };
        #[cfg(target_arch = "x86_64")]
        if use_avx2 {
            unsafe { matmul_q8_0_avx2_range(weight, input, out_slice, info.n_in, rs, re); }
            return;
        }
        matmul_q8_0_fallback_range(weight, input, out_slice, info.n_in, rs, re);
    });
}

pub fn embedding_lookup_q8_0(weight: &[u8], token_id: u32, n_embd: usize, out: &mut [f32]) {
    let blocks_per_row = n_embd / 32;
    let row_off = token_id as usize * blocks_per_row * 34;
    for b in 0..blocks_per_row {
        let off = row_off + b * 34;
        let d = f16_to_f32(u16::from_le_bytes([weight[off], weight[off + 1]]));
        for j in 0..32usize {
            out[b * 32 + j] = d * (weight[off + 2 + j] as i8 as f32);
        }
    }
}

pub fn argmax(x: &[f32]) -> usize {
    let mut best_idx = 0;
    let mut best_val = x[0];
    for (i, &v) in x.iter().enumerate().skip(1) {
        if v > best_val { best_val = v; best_idx = i; }
    }
    best_idx
}

pub fn sample_top_k(logits: &[f32], k: usize) -> Vec<(usize, f32)> {
    let n = logits.len();
    let keep = k.min(n);
    let mut top: Vec<(usize, f32)> = Vec::with_capacity(keep);
    let mut min_in_top = f32::NEG_INFINITY;
    let mut worst_idx = 0;
    for (i, &v) in logits.iter().enumerate() {
        if top.len() < keep {
            top.push((i, v));
            if top.len() == keep {
                let mut w = 0;
                for j in 1..keep { if top[j].1 < top[w].1 { w = j; } }
                worst_idx = w;
                min_in_top = top[w].1;
            }
        } else if v > min_in_top {
            top[worst_idx] = (i, v);
            let mut w = 0;
            for j in 1..keep { if top[j].1 < top[w].1 { w = j; } }
            worst_idx = w;
            min_in_top = top[w].1;
        }
    }
    let max_val = top.iter().map(|&(_, v)| v).fold(f32::NEG_INFINITY, f32::max);
    let mut sum = 0.0f32;
    for (_, v) in top.iter_mut() {
        *v = (*v - max_val).exp();
        sum += *v;
    }
    if sum > 0.0 {
        for (_, p) in top.iter_mut() { *p /= sum; }
    }
    top
}

#[inline(always)]
pub fn ssm_state_decay(state: &mut [f32], decay: f32) {
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2_fma() {
            unsafe { ssm_state_decay_avx2(state, decay) };
            return;
        }
    }
    for v in state.iter_mut() { *v *= decay; }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn ssm_state_decay_avx2(state: &mut [f32], decay: f32) {
    use std::arch::x86_64::*;
    let vdecay = _mm256_set1_ps(decay);
    let n = state.len();
    let mut i = 0;
    while i + 8 <= n {
        let s = _mm256_loadu_ps(state.as_ptr().add(i));
        _mm256_storeu_ps(state.as_mut_ptr().add(i), _mm256_mul_ps(s, vdecay));
        i += 8;
    }
    while i < n {
        state[i] *= decay;
        i += 1;
    }
}

#[inline(always)]
pub fn ssm_matvec(state: &[f32], vec: &[f32], dim: usize, n_rows: usize, out: &mut [f32]) {
    debug_assert_eq!(state.len(), n_rows * dim);
    debug_assert_eq!(vec.len(), dim);
    debug_assert!(out.len() >= n_rows);
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2_fma() {
            unsafe { ssm_matvec_avx2(state, vec, dim, n_rows, out) };
            return;
        }
    }
    for r in 0..n_rows {
        out[r] = dot_f32(&state[r * dim..][..dim], vec, dim);
    }
}

#[inline(always)]
pub fn ssm_matvec_scaled(state: &[f32], vec: &[f32], dim: usize, n_rows: usize, out: &mut [f32], scale: f32) {
    ssm_matvec(state, vec, dim, n_rows, out);
    vec_scale_f32(&mut out[..n_rows], scale);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn ssm_matvec_avx2(state: &[f32], vec: &[f32], dim: usize, n_rows: usize, out: &mut [f32]) {
    use std::arch::x86_64::*;
    let n8 = dim / 8 * 8;
    for r in 0..n_rows {
        let row = state.as_ptr().add(r * dim);
        let mut acc = _mm256_setzero_ps();
        let mut i = 0;
        while i < n8 {
            let vs = _mm256_loadu_ps(row.add(i));
            let vv = _mm256_loadu_ps(vec.as_ptr().add(i));
            acc = _mm256_fmadd_ps(vs, vv, acc);
            i += 8;
        }
        let mut sum = hsum_ps(acc);
        while i < dim {
            sum += *row.add(i) * vec[i];
            i += 1;
        }
        out[r] = sum;
    }
}

#[inline(always)]
pub fn ssm_outer_product_update(state: &mut [f32], k: &[f32], d_vec: &[f32], dim: usize) {
    debug_assert_eq!(state.len(), dim * dim);
    debug_assert_eq!(k.len(), dim);
    debug_assert_eq!(d_vec.len(), dim);
    for d in 0..dim {
        vec_mad_f32(&mut state[d * dim..(d + 1) * dim], k, d_vec[d]);
    }
}

#[inline(always)]
pub fn silu_inplace(values: &mut [f32]) {
    #[cfg(target_arch = "aarch64")]
    if has_neon() {
        unsafe {
            silu_inplace_neon(values);
        }
        return;
    }
    for value in values {
        *value = *value / (1.0 + (-*value).exp());
    }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn silu_inplace_neon(values: &mut [f32]) {
    use std::arch::aarch64::*;

    let mut i = 0;
    while i + 4 <= values.len() {
        let x = vld1q_f32(values.as_ptr().add(i));
        let neg_x = vsubq_f32(vdupq_n_f32(0.0), x);
        let exp_neg_x = ggml_expf_neon(neg_x);
        let one_plus_exp_neg_x = vaddq_f32(vdupq_n_f32(1.0), exp_neg_x);
        vst1q_f32(values.as_mut_ptr().add(i), vdivq_f32(x, one_plus_exp_neg_x));
        i += 4;
    }
    while i < values.len() {
        let value = values[i];
        values[i] = value / (1.0 + (-value).exp());
        i += 1;
    }
}

#[inline(always)]
pub fn silu_mul_inplace(gate: &[f32], up: &mut [f32]) {
    debug_assert_eq!(gate.len(), up.len());
    #[cfg(target_arch = "aarch64")]
    if has_neon() {
        unsafe { silu_mul_inplace_neon(gate, up); }
        return;
    }
    let n = gate.len();
    for i in 0..n {
        let g = gate[i];
        up[i] *= g / (1.0 + (-g).exp());
    }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn silu_mul_inplace_neon(gate: &[f32], up: &mut [f32]) {
    use std::arch::aarch64::*;

    let mut i = 0;
    while i + 4 <= gate.len() {
        let x = vld1q_f32(gate.as_ptr().add(i));
        let multiplier = vld1q_f32(up.as_ptr().add(i));
        let one = vdupq_n_f32(1.0);
        let zero = vdupq_n_f32(0.0);
        let neg_x = vsubq_f32(zero, x);
        let exp_neg_x = ggml_expf_neon(neg_x);
        let one_plus_exp_neg_x = vaddq_f32(one, exp_neg_x);
        let silu = vdivq_f32(x, one_plus_exp_neg_x);
        vst1q_f32(up.as_mut_ptr().add(i), vmulq_f32(silu, multiplier));
        i += 4;
    }
    while i < gate.len() {
        let g = gate[i];
        up[i] *= g / (1.0 + (-g).exp());
        i += 1;
    }
}

#[inline(always)]
pub fn vec_mul_inplace(a: &[f32], b: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2_fma() {
            unsafe { vec_mul_avx2(a, b) };
            return;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if has_neon() {
            unsafe { vec_mul_neon(a, b); }
            return;
        }
    }
    for i in 0..a.len() { b[i] *= a[i]; }
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
    while i < b.len() {
        b[i] *= a[i];
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn vec_mul_avx2(a: &[f32], b: &mut [f32]) {
    use std::arch::x86_64::*;
    let n = a.len();
    let n8 = n / 8 * 8;
    let mut i = 0;
    while i < n8 {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));
        _mm256_storeu_ps(b.as_mut_ptr().add(i), _mm256_mul_ps(va, vb));
        i += 8;
    }
    while i < n { b[i] *= a[i]; i += 1; }
}

#[inline(always)]
pub fn vec_add_into(a: &[f32], b: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2_fma() {
            unsafe { vec_add_avx2(a, b) };
            return;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if has_neon() {
            unsafe { vec_add_neon(a, b); }
            return;
        }
    }
    for i in 0..a.len() { b[i] += a[i]; }
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
    while i < b.len() {
        b[i] += a[i];
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn vec_add_avx2(a: &[f32], b: &mut [f32]) {
    use std::arch::x86_64::*;
    let n = a.len();
    let n8 = n / 8 * 8;
    let mut i = 0;
    while i < n8 {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));
        _mm256_storeu_ps(b.as_mut_ptr().add(i), _mm256_add_ps(va, vb));
        i += 8;
    }
    while i < n { b[i] += a[i]; i += 1; }
}

pub fn conv1d_silu(kernel: &[f32], state: &[f32], d_conv: usize, conv_dim: usize, output: &mut [f32]) {
    for c in 0..conv_dim {
        let mut conv_val = 0.0f32;
        for k in 0..d_conv {
            conv_val += kernel[c * d_conv + k] * state[k * conv_dim + c];
        }
        output[c] = conv_val / (1.0 + (-conv_val).exp());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imrope_matches_pinned_llama_cpp_qwen3vl_vector() {
        let mut values: [f32; 128] =
            std::array::from_fn(|index| (index as f32 - 64.0) * 0.03125);
        rope_mrope_interleaved(
            &mut values,
            [1, 2, 3, 4],
            [24, 20, 20, 0],
            128,
            1_000_000.0,
            128,
        );

        let expected = [
            (0, 0xbf8a_5140), (1, 0x3d49_bc65), (2, 0x3f27_e158),
            (20, 0xbfb3_0f0c), (21, 0xbfac_e489),
            (40, 0xbf40_1d22), (41, 0xbf38_2418),
            (59, 0xbe20_0444), (60, 0xbe00_012a),
            (61, 0xbdc0_07a4), (62, 0xbd80_0642), (63, 0xbd00_0290),
            (64, 0xbfd7_6aa4), (65, 0xbffb_f3f1), (66, 0xbfe9_7fe5),
            (84, 0x3f11_cb34), (85, 0x3f24_4b31),
            (104, 0x3f9f_f741), (105, 0x3fa3_f5df),
            (123, 0x3feb_fff4), (124, 0x3fef_fffe),
            (125, 0x3ff3_fffa), (126, 0x3ff7_fffd), (127, 0x3ffc_0000),
        ];
        for (index, bits) in expected {
            assert_eq!(values[index].to_bits(), bits, "index={index}");
        }
    }
}

#[cfg(test)]
mod neon_tests {
    use super::*;

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn ssm_matvec_qwen35_state_by_k_matches_llama_bits() {
        const ROW_BITS: [u32; 128] = [
            0x343dd31d, 0x35247646, 0x355ddbf1, 0x347ba9ca, 0xb331a47b, 0x35518fa0, 0xb3000a16, 0xb2c9392d,
            0xb46c7875, 0x32260c85, 0x3210e6c4, 0x3570cae0, 0x35ba3a7a, 0x32954284, 0x33fad6a2, 0xb615773a,
            0xb380a3a9, 0x347a533a, 0x33cad215, 0x3343c9ed, 0xb309f1d9, 0xb5adcd3b, 0x33eb313b, 0x32fa6046,
            0x353c4a6b, 0x33211098, 0x329bd8ba, 0x337e2ca9, 0xb5325f4d, 0xb5f37d3a, 0x322f7c4b, 0x35a70d90,
            0xb386bb9c, 0x3427645c, 0xb3ba3dbc, 0xb56bc37c, 0x3401b468, 0xb293977b, 0x33522d68, 0xb2b69671,
            0xb59235b6, 0xb40d4f55, 0xb52af7e9, 0x361881ea, 0x332b5de4, 0xb6d012d6, 0xb67968ae, 0xb2b9d2fc,
            0xb5b3d3c0, 0xb355744d, 0xb25d0d87, 0x35afc80f, 0xb5c3bca6, 0x32d8659b, 0xb5013b27, 0x3257a66c,
            0x33c714ea, 0xacb4ba4c, 0x357f5dcc, 0xb5c67c05, 0x32ac0503, 0x31618e38, 0x3558949b, 0xb2d1614f,
            0xb3a47828, 0x35062e30, 0x3312bcfd, 0x3373072a, 0x32f15e48, 0x359afcda, 0x322c1c69, 0xb5fed574,
            0xb2971111, 0x3374d88f, 0x3409ea44, 0xb32e0c2e, 0xb6630338, 0x328e9746, 0x35f69b4f, 0x34ddf079,
            0x35e9bc66, 0x3413994f, 0x33212afd, 0x3699623d, 0x3207c216, 0xb3d96568, 0xb2465f62, 0xb39fb8c5,
            0x33896754, 0x3651aceb, 0x32b0a443, 0xb39bd9f5, 0xb2fe18a4, 0xb28029eb, 0xb3c57440, 0x345d62dc,
            0xb2fa3cb1, 0x34ddc3ab, 0xb6218127, 0xb5f1fb9b, 0xb6015a8f, 0x351c31fd, 0xb4634deb, 0xb1bb91ef,
            0x33c00f10, 0x3387e788, 0x33e69ff7, 0x32aff1f2, 0xb258a1e3, 0xb5997910, 0xb0ad833f, 0xb59118f0,
            0x35af228e, 0x34487ed7, 0xb0a7f3d4, 0x356189d7, 0xb2d15576, 0x33047350, 0xb456a248, 0x313f972d,
            0xb6eb84a2, 0xb31a2cc7, 0x3301a0b5, 0x334ceec8, 0x362a9aec, 0x3252560e, 0x356127db, 0xb29ec1b7,
        ];
        const K_BITS: [u32; 128] = [
            0xbe5bc7ee, 0xbc72693b, 0x3cb64e01, 0x3c8a3926, 0x3dbcf543, 0xbd1307f9, 0x3e105106, 0x3d00dda0,
            0x3e04c4fc, 0xbd07398f, 0xbbc8e6f3, 0xbd543e95, 0x3db81179, 0xbc8b8c61, 0xbd78ace0, 0xbd4ad402,
            0x3cf5ebd7, 0x3d02337c, 0xbda6371a, 0xbcd93669, 0x3df39308, 0xbcd3e4b6, 0xbe1a9c1f, 0xbcd12e44,
            0xbc08f288, 0xbd80de8d, 0xbda2b205, 0xbd5239e1, 0xbc637ab3, 0xbd81f478, 0xbd0f87e2, 0x3ae46dca,
            0x3d99ae2b, 0xbd92dc0a, 0x3d4264bb, 0xbc994982, 0xbdce788b, 0x3d745a54, 0xbd39b5a4, 0x3d0b3d07,
            0xbd919342, 0x3dad50fc, 0x3df3d1a3, 0xbd8db3e4, 0xbd58a8ca, 0xbeb782f3, 0x3daf3868, 0x3ca5846e,
            0xbdb2638b, 0x3dccf713, 0x3d986a22, 0x3d284a25, 0xbc495e1a, 0xbd390fd2, 0x3dafcd38, 0xbc99c4be,
            0x3cfcbf67, 0x39a59b94, 0xbc956928, 0x3d78278d, 0x3e0b0392, 0xbca6a426, 0x3de48e9b, 0x3cdb0c71,
            0x3dfa6142, 0x3de05676, 0xbda183c3, 0x3c161c62, 0xbcc140d5, 0x3d5e0139, 0xbc78f2dc, 0xbd152690,
            0x3d439f32, 0xbd23e5c2, 0xbdc065ca, 0x3d107d70, 0x3c984c25, 0xbccd0741, 0xbd7b6cd1, 0x3aa305ca,
            0x3d261914, 0xbdea6a09, 0xbcf0d63a, 0xbdc3fe4e, 0xbcde1ac6, 0x3d3d7843, 0x3c47185d, 0x3e0117b8,
            0xbdc85e9e, 0x3ca011a7, 0xbc625d4c, 0xbb3e7541, 0xbdfcfb7d, 0xbd285f17, 0x3dad8de7, 0xbe808674,
            0x3d9bc937, 0xbdcb8055, 0xbd36264a, 0x3dd9a50b, 0x3dc1e5cc, 0xbc8ffaa5, 0x3e50a873, 0x3d338048,
            0xbdcdcdbd, 0xbdcb5998, 0xbdd6a87f, 0xbccf4036, 0x3ca03e4e, 0xbc7046b1, 0x3b064c5e, 0x3d0abb81,
            0xbd39915b, 0xbe2531e1, 0xbaef4e49, 0xbd68f73f, 0x3d8a5161, 0xbdaba148, 0x3bb1ce34, 0x3cf30beb,
            0x3ed6228d, 0x3d384424, 0xbc807c2a, 0xbd19b95c, 0xbd41865e, 0xbc583265, 0x3d8e5c27, 0x3da4d68f,
        ];
        const EXPECTED_BITS: u32 = 0xb60098fc;
        let row = ROW_BITS.map(f32::from_bits);
        let k = K_BITS.map(f32::from_bits);
        let mut out = [0.0f32; 1];

        ssm_matvec(&row, &k, 128, 1, &mut out);

        assert_eq!(out[0].to_bits(), EXPECTED_BITS);
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn ssm_matvec_scaled_qwen35_matches_llama_bits() {
        const STATE_ROW_BITS: [u32; 128] = [
            0x373c2b56, 0x3823071e, 0x385beca7, 0x377977f7, 0xb63017e6, 0x384fbbca, 0xb5fdd87d, 0xb5c777f4,
            0xb76a688c, 0x352499d2, 0x350fa347, 0x386eb152, 0x38b89abb, 0x3593f54d, 0x36f8a6a7, 0xb914298d,
            0xb67f08f4, 0x37782463, 0x36c90d4b, 0x364214d6, 0xb608bde5, 0xb8ac493a, 0x36e9242d, 0x35f83152,
            0x383aa611, 0x361fa906, 0x359a7cce, 0x367bf53a, 0xb830d117, 0xb8f15da7, 0x352df487, 0x38a598a0,
            0xb6858ed4, 0x3725eeaa, 0xb6b89df5, 0xb869b526, 0x370092d8, 0xb5924dfd, 0x36505831, 0xb5b4fed3,
            0xb890ef4e, 0xb70c13dd, 0xb8297a3b, 0x39172d72, 0x3629df52, 0xb9ce4252, 0xb9773be3, 0xb5b83424,
            0xb8b2424a, 0xb65397c5, 0xb55b2009, 0x38ae3fa2, 0xb8c207ac, 0x35d68282, 0xb8001aa7, 0x3555c4fe,
            0x36c55879, 0xafb326d4, 0x387d23b5, 0xb8c4c0e9, 0x35aa84fd, 0x345f96ae, 0x3856b11a, 0xb5cf8de0,
            0xb6a308fc, 0x380502a3, 0x36117567, 0x3670e89c, 0x35ef4371, 0x3899a2da, 0x352a9c2e, 0xb8fc9c8c,
            0xb595bfd1, 0x3672b5f4, 0x3708b660, 0xb62c87a0, 0xb961086d, 0x358d58f2, 0x38f474c6, 0x37dc0100,
            0x38e7b298, 0x37124fcd, 0x361fc330, 0x39980bd1, 0x35069303, 0xb6d78015, 0xb544a487, 0xb69e5433,
            0x36883494, 0x394fd8d4, 0x35af19ea, 0xb69a7e07, 0xb5fbe162, 0xb57e1799, 0xb6c3bb71, 0x375b749f,
            0xb5f80e0c, 0x37dbd497, 0xb920189a, 0xb8efdf64, 0xb90039c8, 0x381ad54a, 0xb7615278, 0xb4b9ef30,
            0x36be624d, 0x3686b822, 0x36e49d1b, 0x35ae6927, 0xb556be44, 0xb8982271, 0xb3abffe3, 0xb88fd504,
            0x38ad9b94, 0x3746bf3e, 0xb3a67ce2, 0x385f9255, 0xb5cf8221, 0x36034ba0, 0xb754c31f, 0x343deb76,
            0xb9e976d9, 0xb618d497, 0x36007f52, 0x364b2547, 0x39291e0e, 0x3550807c, 0x385f3135, 0xb59d5f4c,
        ];
        const Q_BITS: [u32; 128] = [
            0x3c05bcb8, 0x3bac0000, 0x3b60906f, 0xbf27d235, 0x3d49d6dc, 0xbc6a0077, 0x3ddfb552, 0x3d264bbc,
            0xbe06e9d2, 0x3e031a4c, 0xbdedf0cd, 0xbb94b02d, 0xbbeb8fdb, 0x3c8b46a4, 0x3cfe7bbe, 0x3c1c64ae,
            0x3d76eaac, 0x3a18f07d, 0xbd7606d0, 0x3d8fca57, 0xbe1dcee5, 0xbb2d1b7f, 0x3d0d4ba1, 0x3d588e5d,
            0xbb049d1f, 0x3dcb852c, 0xbabac748, 0x3d04fb24, 0x399f4aa6, 0x3b0ee976, 0xbd90f3d1, 0xb92237ac,
            0xbdd39f8b, 0x3d6361ce, 0x3b8da58c, 0x3b949b3f, 0xbd7096cc, 0xbdbd0ffc, 0xbbcfe9d2, 0x3d0701e2,
            0xbba9249b, 0xbe57571a, 0xba1e01f6, 0x3c19d00d, 0x3dc87814, 0x3a83f369, 0xba7a1f83, 0x3d6591c5,
            0x3b92ba79, 0x3cef2594, 0x3de8277f, 0x3ae99153, 0x3a99c355, 0xbdc37d6e, 0xba5efd0e, 0x3dbf5aff,
            0xbb384a95, 0x3e151a1b, 0x3aeb3a5a, 0x398cbc89, 0x3e172d40, 0x3d89005e, 0xb9a93ea7, 0xbab4aad2,
            0x3e0da1de, 0xba5494a0, 0xbd88b357, 0xbc7921cb, 0xbd21ec9a, 0x3bc031c5, 0x3d75b8a7, 0xb976802e,
            0x3ca0c40e, 0xbe4b1fec, 0xbc6f5a09, 0xbd39e37d, 0x3b94beab, 0x3e1d7004, 0xbb41d966, 0x3aa8b126,
            0xbbc17d0d, 0xbdef0548, 0xbd6377b4, 0x3a2c085b, 0x3d896296, 0xbb860fec, 0xbc8bfd4c, 0xbba16379,
            0x3c832238, 0x3a02c934, 0x3cae87ed, 0xbc6b660e, 0xba18a2e6, 0xbde0a121, 0x3d451bb1, 0x3bed995e,
            0xbd597c6f, 0x3c22383d, 0xbadb7e90, 0xb9a07583, 0xbb66db29, 0xba1b8d82, 0x3d8e9c48, 0xbd811b24,
            0xbdc2099b, 0x3e150dc4, 0x3dbcc0c0, 0xbe1b9e1c, 0x3e30f405, 0x3a569067, 0x3d8f6212, 0xba85b0e3,
            0xba628be5, 0xbaa1a4c7, 0x3a8333cf, 0xb6d6c5c4, 0xbe318ab8, 0x3cd39ff2, 0x3adfd249, 0x3def2514,
            0x3c300785, 0xbced9eed, 0x3ca33f05, 0xbb35862b, 0xbb7d54e2, 0x3c193845, 0xbac146f5, 0xbcd8eb85,
        ];
        const EXPECTED_BITS: u32 = 0xb5b15158;
        let state_row = STATE_ROW_BITS.map(f32::from_bits);
        let q = Q_BITS.map(f32::from_bits);
        let mut out = [0.0f32; 1];

        ssm_matvec_scaled(
            &state_row,
            &q,
            128,
            1,
            &mut out,
            f32::from_bits(0x3db504f3),
        );

        assert_eq!(out[0].to_bits(), EXPECTED_BITS);
    }

    fn assert_close(actual: f32, expected: f32) {
        let tolerance = 1e-4 + 1e-4 * expected.abs();
        assert!((actual - expected).abs() <= tolerance, "actual={actual} expected={expected}");
    }

    #[test]
    fn rms_norm_accumulates_f32_squares_in_f64() {
        let input = [
            f32::from_bits(0x3f80_0000),
            f32::from_bits(0x3980_0000),
            f32::from_bits(0x3980_0000),
        ];
        let weight = [1.0f32; 3];
        let mut output = [0.0f32; 3];

        rms_norm(&input, &weight, &mut output, 0.0);

        assert_eq!(
            output.map(f32::to_bits),
            [0x3fdd_b3d6, 0x39dd_b3d6, 0x39dd_b3d6],
        );
    }

    #[test]
    fn rms_norm_inplace_matches_ggml_sequential_f64_accumulation() {
        let mut values = [
            f32::from_bits(0x3e57_d77b),
            f32::from_bits(0xbd82_8687),
            f32::from_bits(0xbe10_2e16),
            f32::from_bits(0x3d2f_9fea),
            f32::from_bits(0x3df4_8d0b),
            f32::from_bits(0x3ded_164c),
            f32::from_bits(0x3bcc_65ad),
            f32::from_bits(0xbe18_b60f),
        ];
        let weight = [
            f32::from_bits(0x4091_0000),
            f32::from_bits(0x3f9f_0000),
            f32::from_bits(0xbf3c_0000),
            f32::from_bits(0x3fda_0000),
            f32::from_bits(0x4026_0000),
            f32::from_bits(0x3fc7_0000),
            f32::from_bits(0x3f8c_0000),
            f32::from_bits(0x3fbb_0000),
        ];

        rms_norm_inplace(&mut values, &weight, f32::from_bits(0x3586_37bd));

        assert_eq!(
            values.map(f32::to_bits),
            [
                0x40f9_71a9,
                0xbf25_68fe,
                0x3f58_0a09,
                0x3f18_930b,
                0x4021_c6f1,
                0x3fbc_04bd,
                0x3d64_1282,
                0xbfe3_9aea,
            ],
        );
    }

    #[test]
    fn rope_neox_matches_pinned_ggml_recurrence_and_fused_rotation() {
        let mut values = [0.0f32; 128];
        values[0] = f32::from_bits(0x402a_4f21);
        values[1] = f32::from_bits(0x3fad_b711);
        values[64] = f32::from_bits(0xbe0e_7273);
        values[65] = f32::from_bits(0x3ef5_b8f9);

        rope_neox(&mut values, 1, 128, 1_000_000.0);

        assert_eq!(
            [values[0], values[1], values[64], values[65]].map(f32::to_bits),
            [0x3fc7_0519, 0x3f17_f682, 0x400a_7ff8, 0x3fa7_dc8a],
        );
    }

    #[test]
    fn rope_mrope_matches_pinned_ggml_frequency_and_fused_rotation() {
        let mut values = [0.0f32; 64];
        values[0] = f32::from_bits(0x3eae_1c0f);
        values[1] = f32::from_bits(0x3e8d_d676);
        values[5] = f32::from_bits(0xbff5_1f9f);
        values[11] = f32::from_bits(0x3e8d_d676);
        values[22] = f32::from_bits(0xbff5_1f9f);
        values[32] = f32::from_bits(0xbfb7_1543);
        values[33] = f32::from_bits(0xc016_5e3a);
        values[37] = f32::from_bits(0x3f02_9876);
        values[43] = f32::from_bits(0xc016_5e3a);
        values[54] = f32::from_bits(0x3f02_9876);

        rope_mrope(&mut values, [1, 2, 3, 0], [11, 11, 10, 0], 64, 10_000_000.0);

        assert_eq!(
            [
                values[0], values[1], values[5], values[11], values[22], values[32],
                values[33], values[37], values[43], values[54],
            ]
            .map(f32::to_bits),
            [
                0x3fb1_93b8,
                0x3fc8_0d88,
                0xbff9_9597,
                0x3e97_4641,
                0xbff5_2065,
                0xbef9_2c2e,
                0xbfe3_5435,
                0x3eb5_6aa7,
                0xc016_396b,
                0x3f02_92aa,
            ],
        );
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn neon_dot_f32_matches_ggml_four_accumulator_reduction() {
        let a = [
            1035226804, 3189287613, 3181886457, 3193572547, 1042787479, 3172027867,
            1034549374, 3188264325, 1056115932, 3201905443, 3188891406, 1051732974,
            1049988604, 3191259790, 3197162127, 1039754039,
        ]
        .map(f32::from_bits);
        let b = [
            1075433967, 1074732927, 1057635659, 3215511252, 3198768119, 1079784837,
            1023408184, 3203974092, 3215464821, 3191576483, 1049344900, 1021989171,
            3207345090, 3229966761, 3189361871, 1036416888,
        ]
        .map(f32::from_bits);

        assert_eq!(unsafe { dot_f32_neon(&a, &b, a.len()) }.to_bits(), 0x3d07_1678);
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn neon_q8_matmul_matches_repacked_fused_block_accumulation() {
        let mut weights = Vec::with_capacity(68);
        for _ in 0..2 {
            weights.extend_from_slice(&0x1800u16.to_le_bytes());
            weights.push(127u8);
            weights.extend_from_slice(&[0; 31]);
        }
        let mut input_q8 = vec![0u8; 64];
        input_q8[0] = -127i8 as u8;
        input_q8[32] = 127;
        let input_scales = [f16_to_f32(0x1800), f16_to_f32(0x1a7d)];
        let mut output = [0.0f32];

        unsafe {
            matmul_q8_0_vs_q8_0_neon(
                &weights,
                &input_q8,
                &input_scales,
                &mut output,
                64,
                0,
                1,
            );
        }

        assert_eq!(output[0].to_bits(), 0x3d1c_c57d);
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn neon_attention_value_matches_ggml_256_padded_reduction() {
        let values = [
            3206143318, 1061541424, 1061305652, 3210998438, 3195547419, 3163016063,
            3212048900, 3189385624, 1062212878, 3209077215, 1044797186, 3208768978,
            1042361759, 1061840183, 3206023529, 3212559954, 3210034948,
        ]
        .map(f32::from_bits);
        let weights = [
            1034085704, 3212250841, 3209990221, 3151333903, 1062699944, 3190005432,
            3192954545, 1049496568, 3209702283, 1042509379, 3207046873, 1046413531,
            1063954866, 3211019113, 1038190425, 1046076976, 3207827037,
        ]
        .map(f32::from_bits);
        let mut padded_values = [0.0f32; 256];
        let mut padded_weights = [0.0f32; 256];
        padded_values[..values.len()].copy_from_slice(&values);
        padded_weights[..weights.len()].copy_from_slice(&weights);

        let actual = attention_value_f32(&padded_values, &padded_weights, values.len(), 256);

        assert_eq!(actual.to_bits(), 0xc032_d8db);
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn neon_softmax_matches_ggml_vector_exp_and_f64_sum() {
        let mut values = [-1.0, 0.0, 1.0, f32::NEG_INFINITY];

        softmax(&mut values);

        assert_eq!(values.map(f32::to_bits), [0x3db8_61f1, 0x3e7a_9a1a, 0x3f2a_4d3b, 0]);
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn neon_silu_matches_qwen35_recurrent_four_lane_fixture() {
        let mut values = [0xbb80_90bc, 0x3c17_08bd, 0x3c89_776f, 0x3ba9_f008].map(f32::from_bits);

        silu_inplace(&mut values);

        assert_eq!(
            values.map(f32::to_bits),
            [0xbb00_502c, 0x3b97_baf3, 0x3c0a_9eb1, 0x3b2a_60d7],
        );
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn neon_silu_mul_matches_pinned_ggml_four_lane_fixture() {
        let gate = [0xbf46_e0d2, 0xbf47_ebea, 0xbeee_8b6b, 0xbe1b_692b].map(f32::from_bits);
        let mut up = [0xbdbd_ab3d, 0xbdf0_16eb, 0x3e08_064c, 0xbf87_c095].map(f32::from_bits);

        silu_mul_inplace(&gate, &mut up);

        assert_eq!(
            up.map(f32::to_bits),
            [0x3cb9_a7c4, 0x3ceb_9536, 0xbcc3_7dcb, 0x3d98_56f8],
        );
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn ssm_outer_product_update_qwen35_matches_llama_bits() {
        const DIM: usize = 8;
        let mut state = [0.0f32; DIM * DIM];
        state[..DIM].copy_from_slice(&[
            0x3494_80df, 0x3580_a966, 0x35ad_9089, 0x34c4_e183,
            0xb38a_f91b, 0x35a3_f18c, 0xb348_55d4, 0xb31d_6bba,
        ].map(f32::from_bits));
        let k = [
            0xbe5b_c7ee, 0xbc72_693b, 0x3cb6_4e01, 0x3c8a_3926,
            0x3dbc_f543, 0xbd13_07f9, 0x3e10_5106, 0x3d00_dda0,
        ].map(f32::from_bits);
        let mut d_vec = [0.0f32; DIM];
        d_vec[0] = f32::from_bits(0xbc50_0f83);

        ssm_outer_product_update(&mut state, &k, &d_vec, DIM);

        assert_eq!(
            state[..DIM].iter().map(|value| value.to_bits()).collect::<Vec<_>>(),
            [
                0x3b32_a467, 0x3946_0583, 0xb993_7cdc, 0xb960_4b2d,
                0xba99_94e5, 0x39ef_a2b8, 0xbaea_96b8, 0xb9d1_7cad,
            ],
        );
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
        for (actual, source) in scaled.iter().zip(&a) {
            assert_close(*actual, source * -0.25);
        }

        let mut mad = a.clone();
        unsafe { vec_mad_f32_neon(&mut mad, &b, 0.5) };
        for i in 0..mad.len() {
            assert_close(mad[i], a[i] + 0.5 * b[i]);
        }
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

    #[test]
    fn f16_dot_dispatch_matches_native_or_scalar_reduction() {
        fn pinned_inputs(n: usize) -> (Vec<u16>, Vec<u8>) {
            let x = (0..n)
                .map(|index| {
                    ((if index % 3 == 0 { 0x8000 } else { 0 })
                        | ((14 + index % 3) << 10)
                        | ((index * 73 + 19) & 0x03ff)) as u16
                })
                .collect();
            let mut y = Vec::with_capacity(n * 2);
            for index in 0..n {
                let bits = ((if matches!(index % 5, 1 | 2) { 0x8000 } else { 0 })
                    | ((13 + index % 4) << 10)
                    | ((index * 151 + 7) & 0x03ff)) as u16;
                y.extend_from_slice(&bits.to_le_bytes());
            }
            (x, y)
        }

        #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
        let native_fp16 = std::arch::is_aarch64_feature_detected!("fp16");
        #[cfg(not(all(target_arch = "aarch64", target_endian = "little")))]
        let native_fp16 = false;
        let expected = if native_fp16 {
            [(32, 0xc086_b000), (37, 0x4035_3bf4), (64, 0x4122_9e00)]
        } else {
            [(32, 0xc086_612e), (37, 0x4035_d999), (64, 0x4122_a161)]
        };
        for (n, expected) in expected {
            let (x, y) = pinned_inputs(n);
            assert_eq!(dot_f16_f16_bytes(&x, &y, n).to_bits(), expected, "n={n}");
        }

        if !native_fp16 {
            return;
        }
        let mut x = vec![0; 64];
        let mut y = vec![0; 128];
        x[0] = 0x3c00;
        y[..2].copy_from_slice(&0x3c00u16.to_le_bytes());
        x[32] = 0x0475;
        y[64..66].copy_from_slice(&0x472eu16.to_le_bytes());
        assert_eq!(dot_f16_f16_bytes(&x, &y, 64).to_bits(), 0x3f80_2000);
    }

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
    fn neon_q8_quantization_uses_ties_to_even() {
        let mut input = [0.0f32; 32];
        input[0] = 127.0;
        input[1] = 34.5;
        let mut q8 = [0u8; 32];
        let mut scales = [0.0f32; 1];

        unsafe { quantize_q8_0_into_neon_range(&input, &mut q8, &mut scales, 0, 1) };

        assert_eq!(q8[1] as i8, 34);
    }

    #[test]
    fn q8_quantization_stores_f16_scale_without_requantizing_values() {
        let mut input = [0.0f32; 32];
        input[0] = 1.0;
        input[1] = f32::from_bits(0x3d11_213e);
        let mut q8 = [0u8; 32];
        let mut scales = [0.0f32; 1];

        quantize_q8_0_into(&input, input.len(), &mut q8, &mut scales);

        assert_eq!(q8[0] as i8, 127);
        assert_eq!(q8[1] as i8, 4);
        assert_eq!(scales[0].to_bits(), 0x3c01_0000);
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
        for i in 0..5 {
            assert_close(neon[i], scalar[i]);
        }
    }
}
