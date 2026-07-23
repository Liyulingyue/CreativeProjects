use half::f16;

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

pub fn dequant_q8_0_row(weight: &[u8], row: usize, n_in: usize, out: &mut [f32]) {
    let blocks_per_row = n_in / 32;
    let row_offset = row * blocks_per_row * 34;
    for b in 0..blocks_per_row {
        let off = row_offset + b * 34;
        let d_bytes = [weight[off], weight[off + 1]];
        let d = f32::from(f16::from_bits(u16::from_le_bytes(d_bytes)));
        for j in 0..32usize {
            let qs = weight[off + 2 + j] as i8 as f32;
            out[b * 32 + j] = d * qs;
        }
    }
}

pub fn dequant_q8_0_full(weight: &[u8], n_out: usize, n_in: usize, out: &mut [f32]) {
    for row in 0..n_out {
        let row_off = row * n_in;
        dequant_q8_0_row(weight, row, n_in, &mut out[row_off..row_off + n_in]);
    }
}

pub fn matmul_q8_0(weight: &[u8], input: &[f32], output: &mut [f32], n_in: usize, n_out: usize, row_buf: &mut [f32]) {
    for j in 0..n_out {
        dequant_q8_0_row(weight, j, n_in, row_buf);
        let mut sum = 0.0f32;
        for i in 0..n_in {
            sum += row_buf[i] * input[i];
        }
        output[j] = sum;
    }
}

pub fn matmul_f32(weight: &[f32], input: &[f32], output: &mut [f32], n_in: usize, n_out: usize) {
    for j in 0..n_out {
        let mut sum = 0.0f32;
        for i in 0..n_in {
            sum += weight[j * n_in + i] * input[i];
        }
        output[j] = sum;
    }
}

pub fn embedding_lookup_q8_0(weight: &[u8], token_id: u32, n_embd: usize, out: &mut [f32]) {
    dequant_q8_0_row(weight, token_id as usize, n_embd, out);
}

pub fn argmax(x: &[f32]) -> usize {
    let mut best_idx = 0;
    let mut best_val = x[0];
    for (i, &v) in x.iter().enumerate().skip(1) {
        if v > best_val {
            best_val = v;
            best_idx = i;
        }
    }
    best_idx
}

pub fn sample_temperature(logits: &mut [f32], temp: f32) {
    if temp <= 0.0 { return; }
    for l in logits.iter_mut() { *l /= temp; }
}

pub fn sample_top_k(logits: &mut [f32], ids: &mut [usize], k: usize) -> usize {
    let n = logits.len().min(ids.len());
    for i in 0..n { ids[i] = i; }
    ids[..n].sort_by(|&a, &b| logits[b].partial_cmp(&logits[a]).unwrap_or(std::cmp::Ordering::Equal));
    let keep = k.min(n);
    for i in keep..n { logits[ids[i]] = f32::NEG_INFINITY; }
    keep
}

pub fn sample_top_p(logits: &mut [f32], ids: &mut [usize], n_valid: usize, p: f32) -> usize {
    if p >= 1.0 { return n_valid; }
    softmax(&mut logits[..n_valid]);
    ids[..n_valid].sort_by(|&a, &b| logits[b].partial_cmp(&logits[a]).unwrap_or(std::cmp::Ordering::Equal));
    let mut cum = 0.0f32;
    let mut keep = n_valid;
    for i in 0..n_valid {
        cum += logits[ids[i]];
        if cum >= p && i + 1 >= 1 {
            keep = i + 1;
            break;
        }
    }
    keep
}
