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
    let sum_sq: f32 = input[..n].iter().map(|&x| x * x).sum();
    let scale = 1.0f32 / (sum_sq / n as f32 + eps).sqrt();
    for i in 0..n {
        output[i] = input[i] * scale * weight[i];
    }
}

pub fn rms_norm_inplace(x: &mut [f32], weight: &[f32], eps: f32) {
    let n = x.len().min(weight.len());
    let sum_sq = sum_sq_f32(&x[..n]);
    let scale = 1.0f32 / (sum_sq / n as f32 + eps).sqrt();
    scale_mul_inplace(scale, &weight[..n], &mut x[..n]);
}

fn sum_sq_f32(x: &[f32]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2_fma() {
            return unsafe { sum_sq_f32_avx2(x) };
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if has_neon() {
            return unsafe { sum_sq_f32_neon(x) };
        }
    }
    x.iter().map(|&v| v * v).sum()
}

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
    while i < x.len() {
        sum += x[i] * x[i];
        i += 1;
    }
    sum
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn sum_sq_f32_avx2(x: &[f32]) -> f32 {
    use std::arch::x86_64::*;
    let n = x.len();
    let n8 = n / 8 * 8;
    let mut acc = _mm256_setzero_ps();
    let mut i = 0;
    while i < n8 {
        let v = _mm256_loadu_ps(x.as_ptr().add(i));
        acc = _mm256_fmadd_ps(v, v, acc);
        i += 8;
    }
    let mut sum = hsum_ps(acc);
    while i < n { sum += x[i] * x[i]; i += 1; }
    sum
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
    for h in 0..n_heads {
        let base = h * head_dim;
        for i in 0..half {
            let freq = 1.0f32 / freq_base.powf(2.0 * i as f32 / head_dim as f32);
            let angle = pos as f32 * freq;
            let cos_a = angle.cos();
            let sin_a = angle.sin();
            let x0 = x[base + i];
            let x1 = x[base + i + half];
            x[base + i] = x0 * cos_a - x1 * sin_a;
            x[base + i + half] = x0 * sin_a + x1 * cos_a;
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
    for h in 0..n_heads {
        let base = h * head_dim;
        let mut dim_offset = 0usize;
        for s in 0..4 {
            let sec_len = sections[s] as usize;
            if sec_len == 0 { continue; }
            let pos = positions[s];
            for i in 0..sec_len {
                let freq = 1.0f32 / freq_base.powf(2.0 * (dim_offset + i) as f32 / total_sections as f32);
                let angle = pos as f32 * freq;
                let cos_a = angle.cos();
                let sin_a = angle.sin();
                let idx0 = base + dim_offset + i;
                let idx1 = base + dim_offset + i + half;
                if idx1 < x.len() {
                    let x0 = x[idx0];
                    let x1 = x[idx1];
                    x[idx0] = x0 * cos_a - x1 * sin_a;
                    x[idx1] = x0 * sin_a + x1 * cos_a;
                }
            }
            dim_offset += sec_len;
        }
    }
}

pub fn rope_mrope_interleaved(x: &mut [f32], positions: [usize; 4], sections: [i32; 4], head_dim: usize, freq_base: f32, n_rope_dims: usize) {
    let n_heads = x.len() / head_dim;
    let half = head_dim / 2;
    let sect_dims: usize = sections.iter().map(|&s| s as usize).sum();

    for h in 0..n_heads {
        let base = h * head_dim;
        let mut theta_t = positions[0] as f32;
        let mut theta_h = positions[1] as f32;
        let mut theta_w = positions[2] as f32;
        let mut theta_e = positions[3] as f32;
        let theta_scale = 1.0f32 / freq_base;

        for i0 in 0..n_rope_dims {
            let sector = i0 % sect_dims;
            let pos_f = if sector % 3 == 0 && sector < 3 * sections[0] as usize {
                theta_t
            } else if sector % 3 == 1 && sector < 3 * sections[1] as usize {
                theta_h
            } else if sector % 3 == 2 && sector < 3 * sections[2] as usize {
                theta_w
            } else {
                theta_e
            };

            let freq = 1.0f32 / freq_base.powf(2.0 * i0 as f32 / sect_dims as f32);
            let angle = pos_f * freq;
            let cos_a = angle.cos();
            let sin_a = angle.sin();

            let idx0 = base + i0;
            let idx1 = base + i0 + half;
            if idx1 < x.len() {
                let x0 = x[idx0];
                let x1 = x[idx1];
                x[idx0] = x0 * cos_a - x1 * sin_a;
                x[idx1] = x0 * sin_a + x1 * cos_a;
            }

            theta_t *= theta_scale;
            theta_h *= theta_scale;
            theta_w *= theta_scale;
            theta_e *= theta_scale;
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
    let mut acc = vdupq_n_f32(0.0);
    let mut i = 0;
    while i + 4 <= n {
        acc = vfmaq_f32(acc, vld1q_f32(a.as_ptr().add(i)), vld1q_f32(b.as_ptr().add(i)));
        i += 4;
    }
    let mut sum = vaddvq_f32(acc);
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
        scales[block] = scale;
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
        scales[b] = d;
        let id = if amax == 0.0 { 0.0 } else { 127.0 / amax };
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
    let mut scales = Vec::with_capacity(blocks);
    for b in 0..blocks {
        let slice = &input[b * 32..(b + 1) * 32];
        let mut amax = 0.0f32;
        for &v in slice {
            let a = v.abs();
            if a > amax { amax = a; }
        }
        let d = if amax == 0.0 { 0.0 } else { amax / 127.0 };
        scales.push(d);
        let id = if d == 0.0 { 0.0 } else { 1.0 / d };
        for (k, &v) in slice.iter().enumerate() {
            q8[b * 32 + k] = (v * id).round().clamp(-128.0, 127.0) as i8 as u8;
        }
    }
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
            sum += wd * input_scales[block] * dot as f32;
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
        out[r] = dot_f32_scalar(&state[r * dim..][..dim], vec, dim);
    }
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
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2_fma() {
            unsafe { ssm_outer_product_update_avx2(state, k, d_vec, dim) };
            return;
        }
    }
    for d in 0..dim {
        let dv = d_vec[d];
        for s in 0..dim {
            state[d * dim + s] += k[s] * dv;
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn ssm_outer_product_update_avx2(state: &mut [f32], k: &[f32], d_vec: &[f32], dim: usize) {
    use std::arch::x86_64::*;
    let n8 = dim / 8 * 8;
    for d in 0..dim {
        let dv = _mm256_set1_ps(d_vec[d]);
        let row = state.as_mut_ptr().add(d * dim);
        let mut s = 0;
        while s < n8 {
            let vk = _mm256_loadu_ps(k.as_ptr().add(s));
            let vs = _mm256_loadu_ps(row.add(s));
            let updated = _mm256_fmadd_ps(vk, dv, vs);
            _mm256_storeu_ps(row.add(s), updated);
            s += 8;
        }
        while s < dim {
            *row.add(s) += k[s] * d_vec[d];
            s += 1;
        }
    }
}

#[inline(always)]
pub fn silu_mul_inplace(gate: &[f32], up: &mut [f32]) {
    debug_assert_eq!(gate.len(), up.len());
    let n = gate.len();
    for i in 0..n {
        let g = gate[i];
        up[i] *= g / (1.0 + (-g).exp());
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
        for i in 0..5 {
            assert_close(neon[i], scalar[i]);
        }
    }
}
