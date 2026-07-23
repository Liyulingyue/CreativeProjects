use half::f16;

pub const QK_K: usize = 256;
pub const K_SCALE_SIZE: usize = 12;
pub const BLOCK_Q4K_SIZE: usize = 144;

#[repr(C, packed)]
pub struct BlockQ4K {
    pub d: f16,
    pub dmin: f16,
    pub scales: [u8; K_SCALE_SIZE],
    pub qs: [u8; QK_K / 2],
}

impl BlockQ4K {
    pub fn from_bytes(bytes: &[u8]) -> Option<&Self> {
        if bytes.len() < BLOCK_Q4K_SIZE {
            return None;
        }
        let ptr = bytes.as_ptr() as *const Self;
        unsafe { Some(&*ptr) }
    }

    pub fn from_bytes_mut(bytes: &mut [u8]) -> Option<&mut Self> {
        if bytes.len() < BLOCK_Q4K_SIZE {
            return None;
        }
        let ptr = bytes.as_mut_ptr() as *mut Self;
        unsafe { Some(&mut *ptr) }
    }
}

#[inline]
fn get_scale_min_k4(j: usize, scales: &[u8; K_SCALE_SIZE]) -> (u8, u8) {
    if j < 4 {
        (scales[j] & 63, scales[j + 4] & 63)
    } else {
        let sc = (scales[j + 4] & 0xF) | ((scales[j - 4] >> 6) << 4);
        let mn = (scales[j + 4] >> 4) | ((scales[j] >> 6) << 4);
        (sc, mn)
    }
}

pub fn dequantize_row_q4_k(block_bytes: &[u8], output: &mut [f32]) {
    let num_blocks = output.len() / QK_K;
    for block_idx in 0..num_blocks {
        let byte_offset = block_idx * BLOCK_Q4K_SIZE;
        let block = match BlockQ4K::from_bytes(&block_bytes[byte_offset..]) {
            Some(b) => b,
            None => break,
        };

        let d = f32::from(block.d);
        let dmin = f32::from(block.dmin);
        let out_base = block_idx * QK_K;

        let mut j = 0usize;
        let mut is = 0usize;
        while is < 8 {
            let (sc1, m1) = get_scale_min_k4(is, &block.scales);
            let (sc2, m2) = get_scale_min_k4(is + 1, &block.scales);

            let d1 = d * sc1 as f32;
            let m1_eff = dmin * m1 as f32;
            let d2 = d * sc2 as f32;
            let m2_eff = dmin * m2 as f32;

            for l in 0..32 {
                let ql = block.qs[j + l];
                output[out_base + j + l] = d1 * (ql & 0xF) as f32 - m1_eff;
                output[out_base + j + l + 32] = d2 * (ql >> 4) as f32 - m2_eff;
            }

            j += 32;
            is += 2;
        }
    }
}

pub fn dequantize_q4_k_weight(
    weight_bytes: &[u8],
    n_rows: usize,
    n_cols: usize,
    output: &mut [f32],
) {
    debug_assert_eq!(output.len(), n_rows * n_cols);
    let blocks_per_row = n_cols / QK_K;
    for row in 0..n_rows {
        let byte_offset = row * blocks_per_row * BLOCK_Q4K_SIZE;
        let out_offset = row * n_cols;
        dequantize_row_q4_k(
            &weight_bytes[byte_offset..],
            &mut output[out_offset..out_offset + n_cols],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_q4k_size() {
        assert_eq!(core::mem::size_of::<BlockQ4K>(), BLOCK_Q4K_SIZE);
    }

    #[test]
    fn test_get_scale_min_k4_low_indices() {
        let scales = [0x3F, 0x3E, 0x3D, 0x3C, 0x3B, 0x3A, 0x39, 0x38, 0, 0, 0, 0];
        for j in 0..4 {
            let (sc, mn) = get_scale_min_k4(j, &scales);
            assert_eq!(sc, scales[j] & 63);
            assert_eq!(mn, scales[j + 4] & 63);
        }
    }

    #[test]
    fn test_dequantize_zero_block() {
        let block_bytes = vec![0u8; BLOCK_Q4K_SIZE];
        let mut output = vec![0.0f32; QK_K];
        dequantize_row_q4_k(&block_bytes, &mut output);
        for &val in &output {
            assert_eq!(val, 0.0);
        }
    }
}
