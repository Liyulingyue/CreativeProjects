#[inline]
fn f16_to_f32(bits: u16) -> f32 {
    #[cfg(target_feature = "f16c")]
    {
        unsafe {
            use std::arch::x86_64::*;
            let v = _mm_set1_epi16(bits as i16);
            _mm_cvtss_f32(_mm_cvtph_ps(v))
        }
    }
    #[cfg(not(target_feature = "f16c"))]
    {
        let sign = (bits >> 15) as u32;
        let exp = ((bits >> 10) & 0x1F) as u32;
        let frac = (bits & 0x3FF) as u32;
        if exp == 0 {
            if frac == 0 {
                f32::from_bits(sign << 31)
            } else {
                let mut e = 0u32;
                let mut f = frac;
                while f & 0x400 == 0 { f <<= 1; e += 1; }
                f32::from_bits((sign << 31) | ((112 - e) << 23) | ((f & 0x3FF) << 13))
            }
        } else if exp == 31 {
            f32::from_bits((sign << 31) | (0xFF << 23) | (frac << 13))
        } else {
            f32::from_bits((sign << 31) | ((exp + 112) << 23) | (frac << 13))
        }
    }
}

#[inline]
fn f16_to_f32_fast(bits: u16) -> f32 {
    let sign = (bits >> 15) as u32;
    let exp = ((bits >> 10) & 0x1F) as u32;
    let frac = (bits & 0x3FF) as u32;
    if exp == 0 {
        if frac == 0 { return f32::from_bits(sign << 31); }
        let norm = frac << 1;
        let leading = norm.leading_zeros();
        let e = leading as u32;
        f32::from_bits((sign << 31) | ((112 - e) << 23) | ((norm << (e + 1)) & 0x3FF) << 13)
    } else if exp == 31 {
        f32::from_bits((sign << 31) | (0xFF << 23) | (frac << 13))
    } else {
        f32::from_bits((sign << 31) | ((exp + 112) << 23) | (frac << 13))
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
    let sum_sq: f32 = x[..n].iter().map(|&v| v * v).sum();
    let scale = 1.0f32 / (sum_sq / n as f32 + eps).sqrt();
    for i in 0..n {
        x[i] = x[i] * scale * weight[i];
    }
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

#[inline(always)]
pub fn dot_f32(a: &[f32], b: &[f32], n: usize) -> f32 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            return unsafe { dot_f32_avx2(a, b, n) };
        }
    }
    dot_f32_scalar(a, b, n)
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
        if is_x86_feature_detected!("avx2") {
            unsafe { quantize_q8_0_into_avx2(input, n, q8, scales); }
            return;
        }
    }
    for b in 0..blocks {
        let slice = &input[b * 32..(b + 1) * 32];
        let mut amax = 0.0f32;
        for &v in slice {
            let a = v.abs();
            if a > amax { amax = a; }
        }
        let d = if amax == 0.0 { 0.0 } else { amax / 127.0 };
        scales[b] = d;
        let id = if d == 0.0 { 0.0 } else { 1.0 / d };
        for (k, &v) in slice.iter().enumerate() {
            q8[b * 32 + k] = (v * id).round().clamp(-128.0, 127.0) as i8 as u8;
        }
    }
}

#[cfg(target_arch = "x86_64")]
unsafe fn quantize_q8_0_into_avx2(input: &[f32], n: usize, q8: &mut [u8], scales: &mut [f32]) {
    use std::arch::x86_64::*;
    let blocks = n / 32;
    let sign_mask = _mm256_set1_ps(-0.0f32);
    let max_i8 = _mm256_set1_ps(127.0);
    let min_i8 = _mm256_set1_ps(-128.0);
    for b in 0..blocks {
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

unsafe fn matmul_q8_0_vs_q8_0_avx2(weight: &[u8], input_q8: &[u8], input_scales: &[f32], output: &mut [f32], n_in: usize, row_start: usize, row_end: usize) {
    use std::arch::x86_64::*;
    let blocks_per_row = n_in / 32;
    let row_stride = blocks_per_row * 34;
    let ones = _mm256_set1_epi16(1);
    for (out_idx, j) in (row_start..row_end).enumerate() {
        let row_off = j * row_stride;
        let mut acc = _mm256_setzero_ps();
        for b in 0..blocks_per_row {
            let w_off = row_off + b * 34;
            let d = f16_to_f32(u16::from_le_bytes([weight[w_off], weight[w_off + 1]])) * input_scales[b];
            let d_v = _mm256_set1_ps(d);
            let w_ptr = weight.as_ptr().add(w_off + 2);
            let i_ptr = input_q8.as_ptr().add(b * 32);
            let qx = _mm256_loadu_si256(w_ptr as *const __m256i);
            let qy = _mm256_loadu_si256(i_ptr as *const __m256i);
            let ax = _mm256_sign_epi8(qx, qx);
            let sy = _mm256_sign_epi8(qy, qx);
            let dot = _mm256_maddubs_epi16(ax, sy);
            let summed = _mm256_madd_epi16(ones, dot);
            acc = _mm256_fmadd_ps(d_v, _mm256_cvtepi32_ps(summed), acc);
        }
        output[out_idx] = hsum_ps(acc);
    }
}

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
unsafe fn hsum_ps(v: std::arch::x86_64::__m256) -> f32 {
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
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            unsafe { matmul_q8_0_vs_q8_0_avx2(weight, q8_buf, scale_buf, output, n_in, 0, n_out); }
            return;
        }
    }
    let blocks_per_row = n_in / 32;
    let row_stride = blocks_per_row * 34;
    for j in 0..n_out {
        let row_off = j * row_stride;
        let mut sum = 0.0f32;
        for b in 0..blocks_per_row {
            let w_off = row_off + b * 34;
            let wd = f16_to_f32(u16::from_le_bytes([weight[w_off], weight[w_off + 1]]));
            let id = scale_buf[b];
            let d = wd * id;
            let qs = &weight[w_off + 2..w_off + 34];
            let inp = &q8_buf[b * 32..(b + 1) * 32];
            let mut local = 0i32;
            for k in 0..32 { local += (qs[k] as i8 as i32) * (inp[k] as i8 as i32); }
            sum += d * local as f32;
        }
        output[j] = sum;
    }
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
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            unsafe { matmul_q8_0_vs_q8_0_avx2(weight, input_q8, input_scales, output, n_in, 0, n_out); }
            return;
        }
    }
    let blocks_per_row = n_in / 32;
    let row_stride = blocks_per_row * 34;
    for j in 0..n_out {
        let out_idx = j;
        let row_off = j * row_stride;
        let mut sum = 0.0f32;
        for b in 0..blocks_per_row {
            let w_off = row_off + b * 34;
            let wd = f16_to_f32(u16::from_le_bytes([weight[w_off], weight[w_off + 1]]));
            let id = input_scales[b];
            let d = wd * id;
            let qs = &weight[w_off + 2..w_off + 34];
            let inp = &input_q8[b * 32..(b + 1) * 32];
            let mut local = 0i32;
            for k in 0..32 {
                local += (qs[k] as i8 as i32) * (inp[k] as i8 as i32);
            }
            sum += d * local as f32;
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

pub fn matmul_q8_0_quantized_range(weight: &[u8], input_q8: &[u8], input_scales: &[f32], output: &mut [f32], n_in: usize, row_start: usize, row_end: usize) {
    debug_assert_eq!(output.len(), row_end - row_start);
    let use_avx2 = cfg!(target_arch = "x86_64") && is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma");
    if use_avx2 {
        unsafe { matmul_q8_0_vs_q8_0_avx2(weight, input_q8, input_scales, output, n_in, row_start, row_end); }
    } else {
        let blocks_per_row = n_in / 32;
        let row_stride = blocks_per_row * 34;
        for (out_idx, j) in (row_start..row_end).enumerate() {
            let row_off = j * row_stride;
            let mut sum = 0.0f32;
            for b in 0..blocks_per_row {
                let w_off = row_off + b * 34;
                let wd = f16_to_f32(u16::from_le_bytes([weight[w_off], weight[w_off + 1]]));
                let id = input_scales[b];
                let d = wd * id;
                let qs = &weight[w_off + 2..w_off + 34];
                let inp = &input_q8[b * 32..(b + 1) * 32];
                let mut local = 0i32;
                for k in 0..32 { local += (qs[k] as i8 as i32) * (inp[k] as i8 as i32); }
                sum += d * local as f32;
            }
            output[out_idx] = sum;
        }
    }
}

pub fn matmul_q8_0_quantized_parallel(weight: &[u8], input_q8: &[u8], input_scales: &[f32], output: &mut [f32], n_in: usize, n_out: usize) {
    let use_avx2 = cfg!(target_arch = "x86_64") && is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma");
    let min_rows = 64;
    parallel_range(weight, input_q8, input_scales, output, n_in, 0, n_out, use_avx2, min_rows);
}

fn parallel_range(weight: &[u8], input_q8: &[u8], input_scales: &[f32], output: &mut [f32], n_in: usize, row_start: usize, row_end: usize, use_avx2: bool, min_rows: usize) {
    let n = row_end - row_start;
    if n <= min_rows {
        if use_avx2 {
            unsafe { matmul_q8_0_vs_q8_0_avx2(weight, input_q8, input_scales, output, n_in, row_start, row_end); }
        } else {
            let blocks_per_row = n_in / 32;
            let row_stride = blocks_per_row * 34;
            for (out_idx, j) in (row_start..row_end).enumerate() {
                let row_off = j * row_stride;
                let mut sum = 0.0f32;
                for b in 0..blocks_per_row {
                    let w_off = row_off + b * 34;
                    let wd = f16_to_f32(u16::from_le_bytes([weight[w_off], weight[w_off + 1]]));
                    let id = input_scales[b];
                    let d = wd * id;
                    let qs = &weight[w_off + 2..w_off + 34];
                    let inp = &input_q8[b * 32..(b + 1) * 32];
                    let mut local = 0i32;
                    for k in 0..32 { local += (qs[k] as i8 as i32) * (inp[k] as i8 as i32); }
                    sum += d * local as f32;
                }
                output[out_idx] = sum;
            }
        }
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
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            unsafe { matmul_q8_0_avx2_range(weight, input, output, n_in, 0, n_out); }
            return;
        }
    }
    matmul_q8_0_fallback_range(weight, input, output, n_in, 0, n_out);
}

pub fn matmul_q8_0_parallel(weight: &[u8], input: &[f32], output: &mut [f32], n_in: usize, n_out: usize, _n_threads: usize) {
    use rayon::prelude::*;
    let use_avx2 = cfg!(target_arch = "x86_64") && is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma");
    let chunk = 128;
    output.par_chunks_mut(chunk).enumerate().for_each(|(i, out_slice)| {
        let rs = i * chunk;
        let re = (rs + chunk).min(n_out);
        if use_avx2 {
            unsafe { matmul_q8_0_avx2_range(weight, input, out_slice, n_in, rs, re); }
        } else {
            matmul_q8_0_fallback_range(weight, input, out_slice, n_in, rs, re);
        }
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
    let use_avx2 = cfg!(target_arch = "x86_64") && is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma");
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
        if use_avx2 {
            unsafe { matmul_q8_0_avx2_range(weight, input, out_slice, info.n_in, rs, re); }
        } else {
            matmul_q8_0_fallback_range(weight, input, out_slice, info.n_in, rs, re);
        }
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
    let mut indices: Vec<usize> = (0..n).collect();
    indices.sort_by(|&a, &b| logits[b].partial_cmp(&logits[a]).unwrap_or(std::cmp::Ordering::Equal));
    let keep = k.min(n);
    let max_val = logits[indices[0]];
    let mut sum = 0.0f32;
    let mut top = Vec::with_capacity(keep);
    for i in 0..keep {
        let idx = indices[i];
        let p = (logits[idx] - max_val).exp();
        sum += p;
        top.push((idx, p));
    }
    if sum > 0.0 {
        for (_, p) in top.iter_mut() { *p /= sum; }
    }
    top
}
