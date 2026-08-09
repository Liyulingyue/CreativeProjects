use rayon::prelude::*;

use crate::clip_config::Qwen35Config;
use crate::model::{GGUFLoader, GGMLType};
use crate::ops::{dot_f32, softmax, rope_neox, rope_mrope};
use crate::quant::{self, BlockQ8K, QK_K};
use crate::thread_pool::ComputePool;

pub struct Qwen35Model {
    pub config: Qwen35Config,
    pub tok_embd: Vec<f32>,
    pub output_norm: Vec<f32>,
    pub output_weight: QWeight,
    pub layers: Vec<Qwen35LayerWeights>,
}

pub enum QWeight {
    F32 { data: Vec<f32>, n_cols: usize, n_rows: usize },
    Q4K { data: Vec<u8>, n_cols: usize, n_rows: usize },
    Q5K { data: Vec<u8>, n_cols: usize, n_rows: usize },
    Q6K { data: Vec<u8>, n_cols: usize, n_rows: usize },
    Q8_0 { data: Vec<u8>, n_cols: usize, n_rows: usize },
    F16 { data: Vec<f32>, n_cols: usize, n_rows: usize },
}

impl QWeight {
    pub fn n_cols(&self) -> usize {
        match self {
            QWeight::F32 { n_cols, .. } => *n_cols,
            QWeight::Q4K { n_cols, .. } => *n_cols,
            QWeight::Q5K { n_cols, .. } => *n_cols,
            QWeight::Q6K { n_cols, .. } => *n_cols,
            QWeight::Q8_0 { n_cols, .. } => *n_cols,
            QWeight::F16 { n_cols, .. } => *n_cols,
        }
    }

    pub fn n_rows(&self) -> usize {
        match self {
            QWeight::F32 { n_rows, .. } => *n_rows,
            QWeight::Q4K { n_rows, .. } => *n_rows,
            QWeight::Q5K { n_rows, .. } => *n_rows,
            QWeight::Q6K { n_rows, .. } => *n_rows,
            QWeight::Q8_0 { n_rows, .. } => *n_rows,
            QWeight::F16 { n_rows, .. } => *n_rows,
        }
    }

    pub fn matmul(&self, input: &[f32]) -> Vec<f32> {
        let n_rows = self.n_rows();
        let mut output = vec![0.0f32; n_rows];
        self.matmul_into(input, &mut output, 0, n_rows);
        output
    }

    pub fn matmul_with_q8k(&self, q8k: &[quant::BlockQ8K]) -> Vec<f32> {
        let n_rows = self.n_rows();
        let mut output = vec![0.0f32; n_rows];
        self.matmul_into_with_q8k(q8k, &mut output, 0, n_rows);
        output
    }

    pub fn matmul_with_q8k_parallel(&self, q8k: &[quant::BlockQ8K], n_threads: usize) -> Vec<f32> {
        let n_rows = self.n_rows();
        if n_threads <= 1 || n_rows < 512 {
            return self.matmul_with_q8k(q8k);
        }
        let mut output = vec![0.0f32; n_rows];
        let chunk = (n_rows + n_threads - 1) / n_threads;
        let mut chunks: Vec<(usize, usize, Vec<f32>)> = (0..n_threads)
            .filter_map(|t| {
                let start = t * chunk;
                let end = (start + chunk).min(n_rows);
                if start < end { Some((start, end, vec![0.0f32; end - start])) } else { None }
            })
            .collect();
        std::thread::scope(|s| {
            for (start, end, out_chunk) in &mut chunks {
                let start = *start;
                let end = *end;
                let out_slice = out_chunk.as_mut_slice();
                let weight = self;
                s.spawn(move || {
                    weight.matmul_into_with_q8k(q8k, out_slice, start, end);
                });
            }
        });
        for (start, end, out_chunk) in chunks {
            output[start..end].copy_from_slice(&out_chunk);
        }
        output
    }

    pub fn matmul_with_q8k_into_buf(&self, q8k: &[quant::BlockQ8K], buf: &mut [f32]) {
        let n_rows = self.n_rows();
        self.matmul_into_with_q8k(q8k, &mut buf[..n_rows], 0, n_rows);
    }

    pub fn matmul_with_q8k_into_buf_parallel(&self, q8k: &[quant::BlockQ8K], buf: &mut [f32], n_threads: usize) {
        let n_rows = self.n_rows();
        if n_threads <= 1 || n_rows < 256 {
            self.matmul_into_with_q8k(q8k, &mut buf[..n_rows], 0, n_rows);
            return;
        }
        let chunk_size = (n_rows + n_threads - 1) / n_threads;
        let mut ranges: Vec<(usize, usize)> = Vec::with_capacity(n_threads);
        let mut row = 0usize;
        while row < n_rows {
            let end = (row + chunk_size).min(n_rows);
            ranges.push((row, end));
            row = end;
        }
        let actual_threads = ranges.len();
        if actual_threads <= 1 {
            self.matmul_into_with_q8k(q8k, &mut buf[..n_rows], 0, n_rows);
            return;
        }
        let mut per_thread_bufs: Vec<&mut [f32]> = Vec::with_capacity(actual_threads);
        {
            let mut remaining: &mut [f32] = &mut buf[..n_rows];
            for i in 0..actual_threads {
                let len = ranges[i].1 - ranges[i].0;
                let (left, right) = remaining.split_at_mut(len);
                per_thread_bufs.push(left);
                remaining = right;
            }
        }
        std::thread::scope(|s| {
            for (i, out_slice) in per_thread_bufs.iter_mut().enumerate() {
                let (row_start, row_end) = ranges[i];
                let weight = self;
                let out = &mut **out_slice;
                s.spawn(move || {
                    weight.matmul_into_with_q8k(q8k, out, row_start, row_end);
                });
            }
        });
    }

    pub fn matmul_with_q8k_into_buf_rayon(&self, q8k: &[quant::BlockQ8K], buf: &mut [f32]) {
        let n_rows = self.n_rows();
        if n_rows < 512 {
            self.matmul_into_with_q8k(q8k, &mut buf[..n_rows], 0, n_rows);
            return;
        }
        let chunk_size = 256.max(n_rows / rayon::current_num_threads());
        let n_chunks = (n_rows + chunk_size - 1) / chunk_size;
        struct ChunkTask { weight: usize, q8k: usize, buf: usize, n_rows: usize, n_cols_hint: usize }
        unsafe impl Send for ChunkTask {}
        unsafe impl Sync for ChunkTask {}
        let task = ChunkTask { weight: self as *const QWeight as usize, q8k: q8k.as_ptr() as usize, buf: buf.as_mut_ptr() as usize, n_rows, n_cols_hint: q8k.len() };
        (0..n_chunks).into_par_iter().for_each(|chunk_idx| {
            let start = chunk_idx * chunk_size;
            let end = (start + chunk_size).min(task.n_rows);
            unsafe {
                let w = &*(task.weight as *const QWeight);
                let q = std::slice::from_raw_parts(task.q8k as *const quant::BlockQ8K, task.n_cols_hint);
                let b = std::slice::from_raw_parts_mut(task.buf as *mut f32, task.n_rows);
                w.matmul_into_with_q8k(q, &mut b[start..end], start, end);
            }
        });
    }

    pub fn quantize_and_matmul(&self, input: &[f32], q8k_buf: &mut [quant::BlockQ8K], buf: &mut [f32]) {
        quant::quantize_row_q8_k_into(input, q8k_buf);
        self.matmul_with_q8k_into_buf_rayon(q8k_buf, buf);
    }

    pub fn matmul_with_q8k_into_buf_pooled(&self, q8k: &[quant::BlockQ8K], buf: &mut [f32], pool: &ComputePool) {
        let n_rows = self.n_rows();
        if pool.n_threads() <= 1 || n_rows < 256 {
            self.matmul_into_with_q8k(q8k, &mut buf[..n_rows], 0, n_rows);
            return;
        }
        let nth = pool.n_threads();
        let chunk_size = (n_rows + nth - 1) / nth;
        let weight_ptr = self as *const QWeight;
        let q8k_ptr = q8k as *const [quant::BlockQ8K];
        let buf_ptr = buf.as_mut_ptr();
        pool.compute(|ith, nth_pool| {
            let start = ith * chunk_size;
            let end = (start + chunk_size).min(n_rows);
            if start >= end { return; }
            unsafe {
                let w = &*weight_ptr;
                let q = &*q8k_ptr;
                let b = std::slice::from_raw_parts_mut(buf_ptr.add(start), end - start);
                w.matmul_into_with_q8k(q, b, start, end);
            }
        });
    }

    fn matmul_into_with_q8k(&self, q8k: &[quant::BlockQ8K], output: &mut [f32], row_start: usize, row_end: usize) {
        match self {
            QWeight::Q4K { data, n_cols, .. } => {
                let blocks_per_row = *n_cols / QK_K;
                for o in row_start..row_end {
                    let row_data = &data[o * blocks_per_row * quant::BLOCK_Q4K_SIZE..];
                    output[o - row_start] = vec_dot_q4k_q8k_fast(row_data, q8k);
                }
            }
            QWeight::Q5K { data, n_cols, .. } => {
                let blocks_per_row = *n_cols / QK_K;
                for o in row_start..row_end {
                    let row_data = &data[o * blocks_per_row * quant::BLOCK_Q5K_SIZE..];
                    output[o - row_start] = vec_dot_q5k_q8k_fast(row_data, q8k);
                }
            }
            QWeight::Q6K { data, n_cols, .. } => {
                let blocks_per_row = *n_cols / QK_K;
                for o in row_start..row_end {
                    let row_data = &data[o * blocks_per_row * quant::BLOCK_Q6K_SIZE..];
                    output[o - row_start] = vec_dot_q6k_q8k_fast(row_data, q8k);
                }
            }
            _ => panic!("matmul_with_q8k called on non-quantized weight type"),
        }
    }

    pub fn matmul_into_buf_parallel(&self, input: &[f32], buf: &mut [f32], n_threads: usize) {
        let n_rows = self.n_rows();
        if n_threads <= 1 || n_rows < 256 {
            self.matmul_into(input, &mut buf[..n_rows], 0, n_rows);
            return;
        }
        let chunk_size = (n_rows + n_threads - 1) / n_threads;
        let mut ranges: Vec<(usize, usize)> = Vec::with_capacity(n_threads);
        let mut row = 0usize;
        while row < n_rows {
            let end = (row + chunk_size).min(n_rows);
            ranges.push((row, end));
            row = end;
        }
        let actual_threads = ranges.len();
        if actual_threads <= 1 {
            self.matmul_into(input, &mut buf[..n_rows], 0, n_rows);
            return;
        }
        let mut per_thread_bufs: Vec<&mut [f32]> = Vec::with_capacity(actual_threads);
        {
            let mut remaining: &mut [f32] = &mut buf[..n_rows];
            for i in 0..actual_threads {
                let len = ranges[i].1 - ranges[i].0;
                let (left, right) = remaining.split_at_mut(len);
                per_thread_bufs.push(left);
                remaining = right;
            }
        }
        std::thread::scope(|s| {
            for (i, out_slice) in per_thread_bufs.iter_mut().enumerate() {
                let (row_start, row_end) = ranges[i];
                let weight = self;
                let out = &mut **out_slice;
                s.spawn(move || {
                    weight.matmul_into(input, out, row_start, row_end);
                });
            }
        });
    }

    pub fn matmul_into_buf_rayon(&self, input: &[f32], buf: &mut [f32]) {
        let n_rows = self.n_rows();
        let chunk_size = 256.max(n_rows / rayon::current_num_threads());
        let n_chunks = (n_rows + chunk_size - 1) / chunk_size;
        struct ChunkTask { weight: usize, input: usize, buf: usize, n_rows: usize, input_len: usize }
        unsafe impl Send for ChunkTask {}
        unsafe impl Sync for ChunkTask {}
        let task = ChunkTask { weight: self as *const QWeight as usize, input: input.as_ptr() as usize, buf: buf.as_mut_ptr() as usize, n_rows, input_len: input.len() };
        (0..n_chunks).into_par_iter().for_each(|chunk_idx| {
            let start = chunk_idx * chunk_size;
            let end = (start + chunk_size).min(task.n_rows);
            unsafe {
                let w = &*(task.weight as *const QWeight);
                let inp = std::slice::from_raw_parts(task.input as *const f32, task.input_len);
                let b = std::slice::from_raw_parts_mut(task.buf as *mut f32, task.n_rows);
                w.matmul_into(inp, &mut b[start..end], start, end);
            }
        });
    }

    pub fn matmul_into_buf_pooled(&self, input: &[f32], buf: &mut [f32], pool: &ComputePool) {
        let n_rows = self.n_rows();
        if pool.n_threads() <= 1 || n_rows < 256 {
            self.matmul_into(input, &mut buf[..n_rows], 0, n_rows);
            return;
        }
        let nth = pool.n_threads();
        let chunk_size = (n_rows + nth - 1) / nth;
        let weight_ptr = self as *const QWeight;
        let input_ptr = input.as_ptr();
        let buf_ptr = buf.as_mut_ptr();
        pool.compute(|ith, nth_pool| {
            let start = ith * chunk_size;
            let end = (start + chunk_size).min(n_rows);
            if start >= end { return; }
            unsafe {
                let w = &*weight_ptr;
                let inp = std::slice::from_raw_parts(input_ptr, input.len());
                let b = std::slice::from_raw_parts_mut(buf_ptr.add(start), end - start);
                w.matmul_into(inp, b, start, end);
            }
        });
    }

    pub fn matmul_parallel(&self, input: &[f32], n_threads: usize) -> Vec<f32> {
        let n_rows = self.n_rows();
        if n_threads <= 1 || n_rows < 512 {
            return self.matmul(input);
        }
        let mut output = vec![0.0f32; n_rows];
        let chunk = (n_rows + n_threads - 1) / n_threads;
        let mut chunks: Vec<(usize, usize, Vec<f32>)> = (0..n_threads)
            .filter_map(|t| {
                let start = t * chunk;
                let end = (start + chunk).min(n_rows);
                if start < end { Some((start, end, vec![0.0f32; end - start])) } else { None }
            })
            .collect();
        std::thread::scope(|s| {
            for (start, end, out_chunk) in &mut chunks {
                let start = *start;
                let end = *end;
                let out_slice = out_chunk.as_mut_slice();
                let weight = self;
                s.spawn(move || {
                    weight.matmul_into(input, out_slice, start, end);
                });
            }
        });
        for (start, end, out_chunk) in chunks {
            output[start..end].copy_from_slice(&out_chunk);
        }
        output
    }

    pub fn matmul_into(&self, input: &[f32], output: &mut [f32], row_start: usize, row_end: usize) {
        match self {
            QWeight::F32 { data, n_cols, .. } => {
                let in_dim = *n_cols;
                for o in row_start..row_end {
                    output[o - row_start] = dot_f32(&data[o * in_dim..o * in_dim + in_dim], input, in_dim);
                }
            }
            QWeight::Q4K { data, n_cols, .. } => {
                let q8k = quantize_row_q8_k_cached(input);
                let blocks_per_row = *n_cols / QK_K;
                for o in row_start..row_end {
                    let row_data = &data[o * blocks_per_row * quant::BLOCK_Q4K_SIZE..];
                    output[o - row_start] = vec_dot_q4k_q8k_fast(row_data, &q8k);
                }
            }
            QWeight::Q5K { data, n_cols, .. } => {
                let q8k = quantize_row_q8_k_cached(input);
                let blocks_per_row = *n_cols / QK_K;
                for o in row_start..row_end {
                    let row_data = &data[o * blocks_per_row * quant::BLOCK_Q5K_SIZE..];
                    output[o - row_start] = vec_dot_q5k_q8k_fast(row_data, &q8k);
                }
            }
            QWeight::Q6K { data, n_cols, .. } => {
                let q8k = quantize_row_q8_k_cached(input);
                let blocks_per_row = *n_cols / QK_K;
                for o in row_start..row_end {
                    let row_data = &data[o * blocks_per_row * quant::BLOCK_Q6K_SIZE..];
                    output[o - row_start] = vec_dot_q6k_q8k_fast(row_data, &q8k);
                }
            }
            QWeight::Q8_0 { data, n_cols, .. } => {
                let in_dim = input.len();
                let blocks_per_row = *n_cols / 32;
                for o in row_start..row_end {
                    let row_off = o * blocks_per_row * 34;
                    let mut sum = 0.0f32;
                    let mut dequant_buf = [0.0f32; 32];
                    for b in 0..blocks_per_row {
                        let w_off = row_off + b * 34;
                        let d = f16_at(data, w_off / 2);
                        for j in 0..32 {
                            dequant_buf[j] = d * data[w_off + 2 + j] as i8 as f32;
                        }
                        sum += dot_f32(&dequant_buf, &input[b * 32..b * 32 + 32], 32);
                    }
                    output[o - row_start] = sum;
                }
            }
            QWeight::F16 { data, n_cols, .. } => {
                let in_dim = *n_cols;
                for o in row_start..row_end {
                    output[o - row_start] = dot_f32(&data[o * in_dim..o * in_dim + in_dim], input, in_dim);
                }
            }
        }
    }

    pub fn dequant_to_f32_weight(self) -> Self {
        match self {
            QWeight::F32 { .. } | QWeight::F16 { .. } => self,
            QWeight::Q8_0 { data, n_cols, n_rows } => {
                let dequant = quant::dequant_q80_weight(&data, n_cols, n_rows);
                QWeight::F32 { data: dequant, n_cols, n_rows }
            }
            QWeight::Q4K { data, n_cols, n_rows } => {
                let mut out = vec![0.0f32; n_cols * n_rows];
                let bpr = n_cols / QK_K;
                for row in 0..n_rows {
                    quant::dequantize_row_q4_k(&data[row * bpr * quant::BLOCK_Q4K_SIZE..], &mut out[row * n_cols..row * n_cols + n_cols]);
                }
                QWeight::F32 { data: out, n_cols, n_rows }
            }
            QWeight::Q5K { data, n_cols, n_rows } => {
                let dequant = quant::dequant_q5k_weight(&data, n_cols, n_rows);
                QWeight::F32 { data: dequant, n_cols, n_rows }
            }
            QWeight::Q6K { data, n_cols, n_rows } => {
                let dequant = quant::dequant_q6k_weight(&data, n_cols, n_rows);
                QWeight::F32 { data: dequant, n_cols, n_rows }
            }
        }
    }

    fn f32_byte_size(&self) -> usize {
        match self {
            QWeight::F32 { data, .. } => data.len() * 4,
            QWeight::F16 { n_cols, n_rows, .. } => n_cols * n_rows * 4,
            QWeight::Q8_0 { n_cols, n_rows, .. } => n_cols * n_rows * 4,
            QWeight::Q4K { n_cols, n_rows, .. } => n_cols * n_rows * 4,
            QWeight::Q5K { n_cols, n_rows, .. } => n_cols * n_rows * 4,
            QWeight::Q6K { n_cols, n_rows, .. } => n_cols * n_rows * 4,
        }
    }

    pub fn fuse_vstack(a: &QWeight, b: &QWeight) -> Option<QWeight> {
        match (a, b) {
            (QWeight::Q5K { data: da, n_cols, n_rows: na }, QWeight::Q5K { data: db, n_cols: nc, n_rows: nb }) if n_cols == nc => {
                let row_bytes_a = da.len() / na;
                let row_bytes_b = db.len() / nb;
                if row_bytes_a != row_bytes_b { return None; }
                let mut fused = Vec::with_capacity(da.len() + db.len());
                fused.extend_from_slice(da);
                fused.extend_from_slice(db);
                Some(QWeight::Q5K { data: fused, n_cols: *n_cols, n_rows: na + nb })
            }
            (QWeight::Q4K { data: da, n_cols, n_rows: na }, QWeight::Q4K { data: db, n_cols: nc, n_rows: nb }) if n_cols == nc => {
                let row_bytes_a = da.len() / na;
                let row_bytes_b = db.len() / nb;
                if row_bytes_a != row_bytes_b { return None; }
                let mut fused = Vec::with_capacity(da.len() + db.len());
                fused.extend_from_slice(da);
                fused.extend_from_slice(db);
                Some(QWeight::Q4K { data: fused, n_cols: *n_cols, n_rows: na + nb })
            }
            (QWeight::Q6K { data: da, n_cols, n_rows: na }, QWeight::Q6K { data: db, n_cols: nc, n_rows: nb }) if n_cols == nc => {
                let row_bytes_a = da.len() / na;
                let row_bytes_b = db.len() / nb;
                if row_bytes_a != row_bytes_b { return None; }
                let mut fused = Vec::with_capacity(da.len() + db.len());
                fused.extend_from_slice(da);
                fused.extend_from_slice(db);
                Some(QWeight::Q6K { data: fused, n_cols: *n_cols, n_rows: na + nb })
            }
            _ => None,
        }
    }
}

fn load_weight(loader: &GGUFLoader, name: &str) -> Option<QWeight> {
    let ti = loader.tensor_info(name)?;
    let data = loader.tensor_slice(name)?;
    let n_cols = ti.dims[0] as usize;
    let n_rows = if ti.dims.len() >= 2 { ti.dims[1] as usize } else { 1 };

    match ti.ggml_type {
        GGMLType::F32 => {
            let mut out = Vec::with_capacity(n_cols * n_rows);
            for i in 0..n_cols * n_rows {
                let off = i * 4;
                if off + 4 <= data.len() {
                    out.push(f32::from_le_bytes([data[off], data[off+1], data[off+2], data[off+3]]));
                } else { out.push(0.0); }
            }
            Some(QWeight::F32 { data: out, n_cols, n_rows })
        }
        GGMLType::F16 => {
            let mut out = Vec::with_capacity(n_cols * n_rows);
            for i in 0..n_cols * n_rows {
                out.push(f16_at(data, i));
            }
            Some(QWeight::F16 { data: out, n_cols, n_rows })
        }
        GGMLType::Q8_0 => Some(QWeight::Q8_0 { data: data.to_vec(), n_cols, n_rows }),
        GGMLType::Q4K => Some(QWeight::Q4K { data: data.to_vec(), n_cols, n_rows }),
        GGMLType::Q5K => Some(QWeight::Q5K { data: data.to_vec(), n_cols, n_rows }),
        GGMLType::Q6K => Some(QWeight::Q6K { data: data.to_vec(), n_cols, n_rows }),
        _ => {
            eprintln!("WARNING: unsupported quant type {:?} for tensor {}", ti.ggml_type, name);
            None
        }
    }
}

fn load_weight_f32(loader: &GGUFLoader, name: &str) -> Option<Vec<f32>> {
    let ti = loader.tensor_info(name)?;
    let data = loader.tensor_slice(name)?;
    let n_el = ti.n_elements();
    match ti.ggml_type {
        GGMLType::F32 => {
            let mut out = Vec::with_capacity(n_el);
            for i in 0..n_el {
                let off = i * 4;
                if off + 4 <= data.len() {
                    out.push(f32::from_le_bytes([data[off], data[off+1], data[off+2], data[off+3]]));
                } else { out.push(0.0); }
            }
            Some(out)
        }
        GGMLType::F16 => {
            let mut out = Vec::with_capacity(n_el);
            for i in 0..n_el { out.push(f16_at(data, i)); }
            Some(out)
        }
        _ => None,
    }
}

pub struct Qwen35LayerWeights {
    pub attn_norm: Vec<f32>,
    pub attn_post_norm: Vec<f32>,
    pub wq: Option<QWeight>,
    pub wk: Option<QWeight>,
    pub wv: Option<QWeight>,
    pub wo: Option<QWeight>,
    pub attn_q_norm: Option<Vec<f32>>,
    pub attn_k_norm: Option<Vec<f32>>,
    pub wqkv: Option<QWeight>,
    pub wqkv_gate: Option<QWeight>,
    pub ssm_conv1d: Option<Vec<f32>>,
    pub ssm_dt: Option<Vec<f32>>,
    pub ssm_a: Option<Vec<f32>>,
    pub ssm_beta: Option<QWeight>,
    pub ssm_alpha: Option<QWeight>,
    pub ssm_norm: Option<Vec<f32>>,
    pub ssm_out: Option<QWeight>,
    pub ffn_gate: QWeight,
    pub ffn_up: QWeight,
    pub ffn_down: QWeight,
    pub ffn_gate_up: Option<QWeight>,
}

impl Qwen35Model {
    pub fn from_gguf(loader: &GGUFLoader) -> Result<Self, String> {
        let config = Qwen35Config::from_gguf(loader)?;

        let tok_embd = {
            let ti = loader.tensor_info("token_embd.weight").ok_or("Missing token_embd.weight")?;
            let data = loader.tensor_slice("token_embd.weight").unwrap();
            let n_cols = ti.dims[0] as usize;
            let n_rows = ti.dims[1] as usize;
            match ti.ggml_type {
                GGMLType::Q6K => quant::dequant_q6k_weight(data, n_cols, n_rows),
                _ => return Err("Unsupported token_embd type".into()),
            }
        };

        let output_norm = load_weight_f32(loader, "output_norm.weight").ok_or("Missing output_norm.weight")?;

        let output_weight = {
            let name = if loader.tensor_info("output.weight").is_some() { "output.weight" } else { "token_embd.weight" };
            load_weight(loader, name).ok_or("Missing output weight")?
        };

        let mut layers = Vec::with_capacity(config.n_layer);
        for i in 0..config.n_layer {
            let attn_norm = load_weight_f32(loader, &format!("blk.{}.attn_norm.weight", i))
                .ok_or_else(|| format!("Missing blk.{}.attn_norm.weight", i))?;
            let attn_post_norm = load_weight_f32(loader, &format!("blk.{}.post_attention_norm.weight", i))
                .ok_or_else(|| format!("Missing blk.{}.post_attention_norm.weight", i))?;
            let is_recr = config.is_recurrent[i];

            let (wq, wk, wv, wo, attn_q_norm, attn_k_norm) = if !is_recr {
                (
                    load_weight(loader, &format!("blk.{}.attn_q.weight", i)),
                    load_weight(loader, &format!("blk.{}.attn_k.weight", i)),
                    load_weight(loader, &format!("blk.{}.attn_v.weight", i)),
                    load_weight(loader, &format!("blk.{}.attn_output.weight", i)),
                    load_weight_f32(loader, &format!("blk.{}.attn_q_norm.weight", i)),
                    load_weight_f32(loader, &format!("blk.{}.attn_k_norm.weight", i)),
                )
            } else { (None, None, None, None, None, None) };

            let (wqkv, wqkv_gate, ssm_conv1d, ssm_dt, ssm_a, ssm_beta, ssm_alpha, ssm_norm, ssm_out) = if is_recr {
                (
                    load_weight(loader, &format!("blk.{}.attn_qkv.weight", i)),
                    load_weight(loader, &format!("blk.{}.attn_gate.weight", i)),
                    load_weight_f32(loader, &format!("blk.{}.ssm_conv1d.weight", i)),
                    load_weight_f32(loader, &format!("blk.{}.ssm_dt.bias", i)),
                    load_weight_f32(loader, &format!("blk.{}.ssm_a", i)),
                    load_weight(loader, &format!("blk.{}.ssm_beta.weight", i)),
                    load_weight(loader, &format!("blk.{}.ssm_alpha.weight", i)),
                    load_weight_f32(loader, &format!("blk.{}.ssm_norm.weight", i)),
                    load_weight(loader, &format!("blk.{}.ssm_out.weight", i)),
                )
            } else { (None, None, None, None, None, None, None, None, None) };

            let ffn_gate = load_weight(loader, &format!("blk.{}.ffn_gate.weight", i))
                .ok_or_else(|| format!("Missing blk.{}.ffn_gate.weight", i))?;
            let ffn_up = load_weight(loader, &format!("blk.{}.ffn_up.weight", i))
                .ok_or_else(|| format!("Missing blk.{}.ffn_up.weight", i))?;
            let ffn_down = load_weight(loader, &format!("blk.{}.ffn_down.weight", i))
                .ok_or_else(|| format!("Missing blk.{}.ffn_down.weight", i))?;
            let ffn_gate_up = QWeight::fuse_vstack(&ffn_gate, &ffn_up);

            layers.push(Qwen35LayerWeights {
                attn_norm, attn_post_norm, wq, wk, wv, wo,
                attn_q_norm, attn_k_norm,
                wqkv, wqkv_gate, ssm_conv1d, ssm_dt, ssm_a, ssm_beta, ssm_alpha, ssm_norm, ssm_out,
                ffn_gate, ffn_up, ffn_down,
                ffn_gate_up,
            });
        }

        Ok(Self { config, tok_embd, output_norm, output_weight, layers })
    }

    pub fn precompute_f32(&mut self) {
        let t0 = std::time::Instant::now();

        // Validate: compare matmul output before and after dequant for one weight
        let test_input: Vec<f32> = (0..self.config.n_embd).map(|i| (i as f32 * 0.01).sin()).collect();
        let ref_output = self.layers[0].ffn_gate.matmul(&test_input);

        self.output_weight = std::mem::replace(&mut self.output_weight, QWeight::F32 { data: Vec::new(), n_cols: 0, n_rows: 0 }).dequant_to_f32_weight();
        for layer in &mut self.layers {
            if let Some(w) = layer.wq.take() { layer.wq = Some(w.dequant_to_f32_weight()); }
            if let Some(w) = layer.wk.take() { layer.wk = Some(w.dequant_to_f32_weight()); }
            if let Some(w) = layer.wv.take() { layer.wv = Some(w.dequant_to_f32_weight()); }
            if let Some(w) = layer.wo.take() { layer.wo = Some(w.dequant_to_f32_weight()); }
            if let Some(w) = layer.wqkv.take() { layer.wqkv = Some(w.dequant_to_f32_weight()); }
            if let Some(w) = layer.wqkv_gate.take() { layer.wqkv_gate = Some(w.dequant_to_f32_weight()); }
            if let Some(w) = layer.ssm_beta.take() { layer.ssm_beta = Some(w.dequant_to_f32_weight()); }
            if let Some(w) = layer.ssm_alpha.take() { layer.ssm_alpha = Some(w.dequant_to_f32_weight()); }
            if let Some(w) = layer.ssm_out.take() { layer.ssm_out = Some(w.dequant_to_f32_weight()); }
            layer.ffn_gate = std::mem::replace(&mut layer.ffn_gate, QWeight::F32 { data: Vec::new(), n_cols: 0, n_rows: 0 }).dequant_to_f32_weight();
            layer.ffn_up = std::mem::replace(&mut layer.ffn_up, QWeight::F32 { data: Vec::new(), n_cols: 0, n_rows: 0 }).dequant_to_f32_weight();
            layer.ffn_down = std::mem::replace(&mut layer.ffn_down, QWeight::F32 { data: Vec::new(), n_cols: 0, n_rows: 0 }).dequant_to_f32_weight();
        }

        let new_output = self.layers[0].ffn_gate.matmul(&test_input);
        let max_diff = ref_output.iter().zip(new_output.iter()).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        eprintln!("precompute_f32: validation max_diff for ffn_gate[0] = {}", max_diff);
        if max_diff > 0.1 {
            let n = ref_output.len().min(5).min(new_output.len());
            eprintln!("WARNING: dequant validation failed! ref_len={} new_len={} first ref: {:?}, new: {:?}", ref_output.len(), new_output.len(), &ref_output[..n], &new_output[..n]);
        }

        eprintln!("precompute_f32: all weights dequantized to F32 in {:.0}ms", t0.elapsed().as_millis());
    }

    pub fn forward(
        &self,
        n_tokens: usize,
        kv_cache: &mut crate::scratchpad::KvCache,
        scratch: &mut Qwen35Scratchpad,
        pool: &ComputePool,
        mrope_positions: Option<&[[usize; 4]]>,
    ) -> Vec<f32> {
        let cfg = &self.config;
        let n_embd = cfg.n_embd;
        let n_layer = cfg.n_layer;
        let eps = cfg.norm_eps;
        let profile = std::env::var("PROFILE_QWEN35").is_ok();
        let mut t_attn: f64 = 0.0;
        let mut t_ffn: f64 = 0.0;

        for il in 0..n_layer {
            let layer = &self.layers[il];
            let is_recr = cfg.is_recurrent[il];

            for t in 0..n_tokens {
                let off = t * n_embd;
                scratch.normed_buf[off..off + n_embd].copy_from_slice(&scratch.x[off..off + n_embd]);
                crate::ops::rms_norm_inplace(&mut scratch.normed_buf[off..off + n_embd], &layer.attn_norm, eps);
            }

            let t0 = std::time::Instant::now();
            let normed_ptr = scratch.normed_buf.as_ptr();
            let normed_len = n_tokens * n_embd;
            let attn_out = if is_recr {
                let normed_input = unsafe { std::slice::from_raw_parts(normed_ptr, normed_len) };
                self.forward_recurrent_layer(il, normed_input, n_tokens, scratch, pool)
            } else {
                let normed_input = unsafe { std::slice::from_raw_parts(normed_ptr, normed_len) };
                self.forward_dense_attn_layer(il, normed_input, n_tokens, kv_cache, scratch, pool, mrope_positions)
            };
            t_attn += t0.elapsed().as_secs_f64();

            for t in 0..n_tokens {
                let off = t * n_embd;
                crate::ops::vec_add_into(&attn_out[off..off + n_embd], &mut scratch.x[off..off + n_embd]);
            }

            for t in 0..n_tokens {
                let off = t * n_embd;
                scratch.buf[off..off + n_embd].copy_from_slice(&scratch.x[off..off + n_embd]);
                crate::ops::rms_norm_inplace(&mut scratch.buf[off..off + n_embd], &layer.attn_post_norm, eps);
            }

            let t0 = std::time::Instant::now();
            let buf_ptr = scratch.buf.as_ptr();
            let buf_len = n_tokens * n_embd;
            let ffn_input = unsafe { std::slice::from_raw_parts(buf_ptr, buf_len) };
            self.forward_ffn_parallel(layer, ffn_input, n_tokens, scratch, pool);
            t_ffn += t0.elapsed().as_secs_f64();

            for t in 0..n_tokens {
                let off = t * n_embd;
                crate::ops::vec_add_into(&scratch.buf[off..off + n_embd], &mut scratch.x[off..off + n_embd]);
            }
        }

        if profile {
            let total = t_attn + t_ffn;
            eprintln!("PROFILE: attn={:.1}% ({:.3}s) ffn={:.1}% ({:.3}s)", t_attn/total*100.0, t_attn, t_ffn/total*100.0, t_ffn);
        }

        let mut normed = vec![0.0f32; n_tokens * n_embd];
        for t in 0..n_tokens {
            let off = t * n_embd;
            normed[off..off + n_embd].copy_from_slice(&scratch.x[off..off + n_embd]);
            crate::ops::rms_norm_inplace(&mut normed[off..off + n_embd], &self.output_norm, eps);
        }

        let last_normed = &normed[(n_tokens - 1) * n_embd..n_tokens * n_embd];
        let nb_embd = (n_embd + 255) / 256;
        quant::quantize_row_q8_k_into(last_normed, &mut scratch.q8k_buf[..nb_embd]);
        self.output_weight.matmul_with_q8k_into_buf_rayon(&scratch.q8k_buf[..nb_embd], &mut scratch.matmul_out);
        let mut result = vec![0.0f32; cfg.vocab_size];
        let n = scratch.matmul_out.len().min(cfg.vocab_size);
        result[..n].copy_from_slice(&scratch.matmul_out[..n]);
        result
    }

    fn forward_dense_attn_layer(
        &self, il: usize, input: &[f32], n_tokens: usize,
        kv_cache: &mut crate::scratchpad::KvCache, scratch: &mut Qwen35Scratchpad,
        pool: &ComputePool, mrope_positions: Option<&[[usize; 4]]>,
    ) -> Vec<f32> {
        let profile = std::env::var("PROFILE_QWEN35").is_ok();
        let cfg = &self.config;
        let n_embd = cfg.n_embd;
        let n_head = cfg.n_head;
        let n_head_kv = cfg.n_head_kv;
        let n_embd_head = cfg.n_embd_head();
        let eps = cfg.norm_eps;
        let nth = pool.n_threads();
        let layer = &self.layers[il];
        let wq = layer.wq.as_ref().unwrap();
        let wk = layer.wk.as_ref().unwrap();
        let wv = layer.wv.as_ref().unwrap();
        let wo = layer.wo.as_ref().unwrap();
        let q_norm_w = layer.attn_q_norm.as_ref().unwrap();
        let k_norm_w = layer.attn_k_norm.as_ref().unwrap();
        let q_dim = n_embd_head * n_head * 2;
        let k_dim = n_embd_head * n_head_kv;
        let v_dim = n_embd_head * n_head_kv;
        let n_embd_heads_total = n_embd_head * n_head;

        let mut t_qkv: f64 = 0.0;
        let mut t_score: f64 = 0.0;
        let mut t_wo: f64 = 0.0;

        let nb_embd = (n_embd + 255) / 256;
        for t in 0..n_tokens {
            let inp_off = t * n_embd;
            let t0 = std::time::Instant::now();
            let inp_slice = &input[inp_off..inp_off + n_embd];
            quant::quantize_row_q8_k_into(inp_slice, &mut scratch.q8k_buf[..nb_embd]);
            wq.matmul_with_q8k_into_buf_rayon(&scratch.q8k_buf[..nb_embd], &mut scratch.matmul_out);
            scratch.q_buf[t * q_dim..t * q_dim + q_dim].copy_from_slice(&scratch.matmul_out[..q_dim]);
            wk.matmul_with_q8k_into_buf_rayon(&scratch.q8k_buf[..nb_embd], &mut scratch.matmul_out);
            scratch.k_buf[t * k_dim..t * k_dim + k_dim].copy_from_slice(&scratch.matmul_out[..k_dim]);
            wv.matmul_with_q8k_into_buf_rayon(&scratch.q8k_buf[..nb_embd], &mut scratch.matmul_out);
            scratch.v_buf[t * v_dim..t * v_dim + v_dim].copy_from_slice(&scratch.matmul_out[..v_dim]);
            t_qkv += t0.elapsed().as_secs_f64();
        }

        for t in 0..n_tokens {
            for h in 0..n_head {
                let q_off = t * q_dim + h * n_embd_head * 2;
                crate::ops::rms_norm_inplace(&mut scratch.q_buf[q_off..q_off + n_embd_head], q_norm_w, eps);
            }
            for h in 0..n_head_kv { crate::ops::rms_norm_inplace(&mut scratch.k_buf[t * k_dim + h * n_embd_head..][..n_embd_head], k_norm_w, eps); }
        }

        let kv_pos = kv_cache_pos(kv_cache, il, k_dim, cfg.n_layer);
        let sections = cfg.rope_dimension_sections;
        let use_mrope = sections[0] > 0 && sections[1] > 0;
        for t in 0..n_tokens {
            let pos = kv_pos + t;
            let positions: [usize; 4] = if let Some(mrope) = mrope_positions {
                mrope[t]
            } else {
                [pos, pos, pos, 0]
            };
            for h in 0..n_head {
                let q_off = t * q_dim + h * n_embd_head * 2;
                if use_mrope {
                    rope_mrope(&mut scratch.q_buf[q_off..q_off + cfg.rope_dimension_count], positions, sections, cfg.rope_dimension_count, cfg.rope_freq_base);
                } else {
                    rope_neox(&mut scratch.q_buf[q_off..q_off + cfg.rope_dimension_count], positions[0], cfg.rope_dimension_count, cfg.rope_freq_base);
                }
            }
            for h in 0..n_head_kv {
                let k_off = t * k_dim + h * n_embd_head;
                if use_mrope {
                    rope_mrope(&mut scratch.k_buf[k_off..k_off + cfg.rope_dimension_count], positions, sections, cfg.rope_dimension_count, cfg.rope_freq_base);
                } else {
                    rope_neox(&mut scratch.k_buf[k_off..k_off + cfg.rope_dimension_count], positions[0], cfg.rope_dimension_count, cfg.rope_freq_base);
                }
            }
        }

        kv_cache_store(kv_cache, il, cfg.n_layer, &scratch.k_buf[..n_tokens * k_dim], &scratch.v_buf[..n_tokens * v_dim], k_dim, v_dim, kv_pos);
        let n_kv = kv_pos + n_tokens;
        let scale = 1.0 / (n_embd_head as f32).sqrt();

        let (k_cache, v_cache) = match kv_cache {
            crate::scratchpad::KvCache::F32(c) => (&c.k, &c.v),
            _ => return vec![0.0; n_tokens * n_embd],
        };
        let k_len = k_cache.len() / cfg.n_layer;
        let v_len = v_cache.len() / cfg.n_layer;

        let t0 = std::time::Instant::now();
        for t in 0..n_tokens {
            for h in 0..n_head {
                let q_off = t * q_dim + h * n_embd_head * 2;
                let kv_h = h / (n_head / n_head_kv);
                let n_attend = kv_pos + t + 1;
                for s in 0..n_attend {
                    let k_off = il * k_len + s * k_dim + kv_h * n_embd_head;
                    let dot = dot_f32(&scratch.q_buf[q_off..q_off + n_embd_head], &k_cache[k_off..k_off + n_embd_head], n_embd_head);
                    scratch.score_buf[s] = dot * scale;
                }
                softmax(&mut scratch.score_buf[..n_attend]);
                for d in 0..n_embd_head {
                    let mut val = 0.0f32;
                    for s in 0..n_attend { val += scratch.score_buf[s] * v_cache[il * v_len + s * v_dim + kv_h * n_embd_head + d]; }
                    scratch.attn_out_buf[t * n_embd_heads_total + h * n_embd_head + d] = val;
                }
            }
        }
        t_score += t0.elapsed().as_secs_f64();

        for t in 0..n_tokens {
            for h in 0..n_head {
                let gate_off = t * q_dim + h * n_embd_head * 2 + n_embd_head;
                let out_off = t * n_embd_heads_total + h * n_embd_head;
                for d in 0..n_embd_head {
                    scratch.attn_out_buf[out_off + d] *= sigmoid_f32(scratch.q_buf[gate_off + d]);
                }
            }
        }

        let mut result = vec![0.0f32; n_tokens * n_embd];
        let t0 = std::time::Instant::now();
        let nb_attn = (n_embd_heads_total + 255) / 256;
        for t in 0..n_tokens {
            let wo_input = &scratch.attn_out_buf[t * n_embd_heads_total..t * n_embd_heads_total + n_embd_heads_total];
            quant::quantize_row_q8_k_into(wo_input, &mut scratch.q8k_buf[..nb_attn]);
            wo.matmul_with_q8k_into_buf_rayon(&scratch.q8k_buf[..nb_attn], &mut scratch.matmul_out);
            result[t * n_embd..t * n_embd + n_embd].copy_from_slice(&scratch.matmul_out[..n_embd]);
        }        t_wo += t0.elapsed().as_secs_f64();
        if profile {
            eprintln!("  dense_attn[{}]: qkv={:.3}s score={:.3}s wo={:.3}s", il, t_qkv, t_score, t_wo);
        }
        result
    }

    fn forward_recurrent_layer(&self, il: usize, input: &[f32], n_tokens: usize, scratch: &mut Qwen35Scratchpad, pool: &ComputePool) -> Vec<f32> {
        let profile = std::env::var("PROFILE_QWEN35").is_ok();
        let cfg = &self.config;
        let n_embd = cfg.n_embd;
        let d_inner = cfg.ssm_d_inner;
        let head_k_dim = cfg.ssm_d_state;
        let num_k_heads = cfg.ssm_n_group;
        let num_v_heads = cfg.ssm_dt_rank;
        let head_v_dim = d_inner / num_v_heads;
        let key_dim = cfg.key_dim();
        let value_dim = cfg.value_dim();
        let conv_dim = cfg.conv_dim();
        let d_conv = cfg.ssm_d_conv;
        let eps = cfg.norm_eps;

        let layer = &self.layers[il];
        let wqkv = layer.wqkv.as_ref().unwrap();
        let wqkv_gate = layer.wqkv_gate.as_ref().unwrap();
        let ssm_conv1d = layer.ssm_conv1d.as_ref().unwrap();
        let ssm_dt = layer.ssm_dt.as_ref().unwrap();
        let ssm_a = layer.ssm_a.as_ref().unwrap();
        let ssm_beta = layer.ssm_beta.as_ref().unwrap();
        let ssm_alpha = layer.ssm_alpha.as_ref().unwrap();
        let ssm_norm_w = layer.ssm_norm.as_ref().unwrap();
        let ssm_out = layer.ssm_out.as_ref().unwrap();

        let t0 = std::time::Instant::now();
        for t in 0..n_tokens {
            let inp_off = t * n_embd;
            let inp_slice = &input[inp_off..inp_off + n_embd];
            let nb = (n_embd + 255) / 256;
            quant::quantize_row_q8_k_into(inp_slice, &mut scratch.q8k_buf[..nb]);
            let q8k_ref = &scratch.q8k_buf[..nb];
            wqkv.matmul_with_q8k_into_buf_rayon(q8k_ref, &mut scratch.matmul_out);
            scratch.qkv_buf[t * conv_dim..t * conv_dim + conv_dim].copy_from_slice(&scratch.matmul_out[..conv_dim]);
            wqkv_gate.matmul_with_q8k_into_buf_rayon(q8k_ref, &mut scratch.matmul_out);
            scratch.z_buf[t * value_dim..t * value_dim + value_dim].copy_from_slice(&scratch.matmul_out[..value_dim]);
            ssm_beta.matmul_into(inp_slice, &mut scratch.matmul_out, 0, ssm_beta.n_rows());
            let n_beta = ssm_beta.n_rows().min(num_v_heads);
            scratch.beta_buf[t * num_v_heads..t * num_v_heads + n_beta].copy_from_slice(&scratch.matmul_out[..n_beta]);
            for v in 0..num_v_heads { scratch.beta_buf[t * num_v_heads + v] = sigmoid_f32(scratch.beta_buf[t * num_v_heads + v]); }
            ssm_alpha.matmul_into(inp_slice, &mut scratch.matmul_out, 0, ssm_alpha.n_rows());
            let n_alpha = ssm_alpha.n_rows().min(num_v_heads);
            scratch.alpha_buf[t * num_v_heads..t * num_v_heads + n_alpha].copy_from_slice(&scratch.matmul_out[..n_alpha]);
            for v in 0..num_v_heads {
                let a_biased = scratch.alpha_buf[t * num_v_heads + v] + ssm_dt[v % ssm_dt.len()];
                scratch.alpha_buf[t * num_v_heads + v] = softplus_f32(a_biased) * ssm_a[v % ssm_a.len()];
            }
        }
        let t_matmul = t0.elapsed().as_secs_f64();

        let tc0 = std::time::Instant::now();
        let conv_state = &mut scratch.conv_states[il];
        for t in 0..n_tokens {
            let qkv_off = t * conv_dim;
            for c in 0..conv_dim {
                for k in 0..d_conv - 1 { conv_state[k * conv_dim + c] = conv_state[(k + 1) * conv_dim + c]; }
                conv_state[(d_conv - 1) * conv_dim + c] = scratch.qkv_buf[qkv_off + c];
            }
            for c in 0..conv_dim {
                let mut conv_val = 0.0f32;
                for k in 0..d_conv { conv_val += ssm_conv1d[c * d_conv + k] * conv_state[k * conv_dim + c]; }
                scratch.qkv_buf[qkv_off + c] = silu_f32(conv_val);
            }
        }

        for t in 0..n_tokens {
            let qkv_off = t * conv_dim;
            for h in 0..num_k_heads {
                for d in 0..head_k_dim { scratch.q_buf[t * key_dim + h * head_k_dim + d] = scratch.qkv_buf[qkv_off + h * head_k_dim + d]; }
                for d in 0..head_k_dim { scratch.k_buf2[t * key_dim + h * head_k_dim + d] = scratch.qkv_buf[qkv_off + key_dim + h * head_k_dim + d]; }
            }
            for h in 0..num_v_heads {
                for d in 0..head_v_dim { scratch.v_buf2[t * value_dim + h * head_v_dim + d] = scratch.qkv_buf[qkv_off + 2 * key_dim + h * head_v_dim + d]; }
            }
            for h in 0..num_k_heads {
                l2_norm(&mut scratch.q_buf[t * key_dim + h * head_k_dim..][..head_k_dim]);
                l2_norm(&mut scratch.k_buf2[t * key_dim + h * head_k_dim..][..head_k_dim]);
            }
        }

        let tc = tc0.elapsed().as_secs_f64();

        let ts0 = std::time::Instant::now();
        let q_scale = 1.0 / (head_k_dim as f32).sqrt();
        let ssm_state = &mut scratch.ssm_states[il];
        for t in 0..n_tokens {
            let q_off = t * key_dim;
            let k2_off = t * key_dim;
            let v2_off = t * value_dim;
            for v_h in 0..num_v_heads {
                let gate_val = scratch.alpha_buf[t * num_v_heads + v_h];
                let beta_val = scratch.beta_buf[t * num_v_heads + v_h];
                let state_off = v_h * head_v_dim * head_v_dim;
                let k_h = v_h % num_k_heads;
                let decay = gate_val.exp();
                crate::ops::ssm_state_decay(&mut ssm_state[state_off..state_off + head_v_dim * head_v_dim], decay);
                let k_slice = &scratch.k_buf2[k2_off + k_h * head_k_dim..][..head_v_dim];
                let mut sk = [0.0f32; 128];
                crate::ops::ssm_matvec(&ssm_state[state_off..][..head_v_dim * head_v_dim], k_slice, head_v_dim, head_v_dim, &mut sk[..head_v_dim]);
                let v_slice = &scratch.v_buf2[v2_off + v_h * head_v_dim..][..head_v_dim];
                let mut d_vec = [0.0f32; 128];
                for d in 0..head_v_dim { d_vec[d] = (v_slice[d] - sk[d]) * beta_val; }
                crate::ops::ssm_outer_product_update(&mut ssm_state[state_off..][..head_v_dim * head_v_dim], k_slice, &d_vec[..head_v_dim], head_v_dim);
                let q_slice = &scratch.q_buf[q_off + k_h * head_k_dim..][..head_v_dim];
                let out_off = t * value_dim + v_h * head_v_dim;
                let q_scaled = &mut scratch.q_scaled_buf[..head_v_dim];
                for i in 0..head_v_dim { q_scaled[i] = q_slice[i] * q_scale; }
                crate::ops::ssm_matvec(&ssm_state[state_off..][..head_v_dim * head_v_dim], q_scaled, head_v_dim, head_v_dim, &mut scratch.attn_out_buf[out_off..out_off + head_v_dim]);
            }
        }

        let tssm = ts0.elapsed().as_secs_f64();
        let tn0 = std::time::Instant::now();
        for t in 0..n_tokens {
            for h in 0..num_v_heads {
                let off = t * value_dim + h * head_v_dim;
                crate::ops::rms_norm_inplace(&mut scratch.attn_out_buf[off..off + head_v_dim], ssm_norm_w, eps);
            }
            let z_off = t * value_dim;
            crate::ops::silu_mul_inplace(&scratch.z_buf[z_off..z_off + value_dim], &mut scratch.attn_out_buf[t * value_dim..t * value_dim + value_dim]);
        }

        let tnorm = tn0.elapsed().as_secs_f64();
        let mut result = vec![0.0f32; n_tokens * n_embd];
        let t0 = std::time::Instant::now();
        let nb_out = (value_dim + 255) / 256;
        for t in 0..n_tokens {
            let inp = &scratch.attn_out_buf[t * value_dim..][..value_dim];
            quant::quantize_row_q8_k_into(inp, &mut scratch.q8k_buf[..nb_out]);
            ssm_out.matmul_with_q8k_into_buf_rayon(&scratch.q8k_buf[..nb_out], &mut scratch.matmul_out);
            result[t * n_embd..t * n_embd + n_embd].copy_from_slice(&scratch.matmul_out[..n_embd]);
        }
        let t_out_matmul = t0.elapsed().as_secs_f64();
        if profile {
            eprintln!("  recr[{}]: matmul={:.3}s conv={:.3}s ssm={:.3}s norm={:.3}s out={:.3}s", il, t_matmul, tc, tssm, tnorm, t_out_matmul);
        }
        result
    }

    fn forward_ffn_parallel(&self, layer: &Qwen35LayerWeights, hidden: &[f32], n_tokens: usize, scratch: &mut Qwen35Scratchpad, _pool: &ComputePool) {
        let n_embd = self.config.n_embd;
        let n_ff = self.config.n_ff;
        let nb_embd = (n_embd + 255) / 256;
        let nb_ff = (n_ff + 255) / 256;

        if let Some(ref gate_up) = layer.ffn_gate_up {
            for t in 0..n_tokens {
                let off = t * n_embd;
                let inp = &hidden[off..off + n_embd];
                quant::quantize_row_q8_k_into(inp, &mut scratch.q8k_buf[..nb_embd]);
                gate_up.matmul_with_q8k_into_buf_rayon(&scratch.q8k_buf[..nb_embd], &mut scratch.matmul_out);
                scratch.ffn_gate_buf[t * n_ff..t * n_ff + n_ff].copy_from_slice(&scratch.matmul_out[..n_ff]);
                scratch.ffn_up_buf[t * n_ff..t * n_ff + n_ff].copy_from_slice(&scratch.matmul_out[n_ff..2 * n_ff]);
            }
        } else {
            for t in 0..n_tokens {
                let off = t * n_embd;
                let inp = &hidden[off..off + n_embd];
                quant::quantize_row_q8_k_into(inp, &mut scratch.q8k_buf[..nb_embd]);
                layer.ffn_gate.matmul_with_q8k_into_buf_rayon(&scratch.q8k_buf[..nb_embd], &mut scratch.matmul_out);
                scratch.ffn_gate_buf[t * n_ff..t * n_ff + n_ff].copy_from_slice(&scratch.matmul_out[..n_ff]);
                layer.ffn_up.matmul_with_q8k_into_buf_rayon(&scratch.q8k_buf[..nb_embd], &mut scratch.matmul_out);
                scratch.ffn_up_buf[t * n_ff..t * n_ff + n_ff].copy_from_slice(&scratch.matmul_out[..n_ff]);
            }
        }

        crate::ops::silu_mul_inplace(&scratch.ffn_gate_buf[..n_tokens * n_ff], &mut scratch.ffn_up_buf[..n_tokens * n_ff]);

        for t in 0..n_tokens {
            let down_inp = &scratch.ffn_up_buf[t * n_ff..][..n_ff];
            quant::quantize_row_q8_k_into(down_inp, &mut scratch.q8k_buf[..nb_ff]);
            layer.ffn_down.matmul_with_q8k_into_buf_rayon(&scratch.q8k_buf[..nb_ff], &mut scratch.matmul_out);
            scratch.buf[t * n_embd..t * n_embd + n_embd].copy_from_slice(&scratch.matmul_out[..n_embd]);
        }
    }

    fn forward_ffn(&self, layer: &Qwen35LayerWeights, hidden: &[f32], n_tokens: usize, scratch: &mut Qwen35Scratchpad) {
        let n_embd = self.config.n_embd;
        let n_ff = self.config.n_ff;
        let profile = std::env::var("PROFILE_QWEN35").is_ok();
        let mut t_gate: f64 = 0.0;
        let mut t_up: f64 = 0.0;
        let mut t_down: f64 = 0.0;
        for t in 0..n_tokens {
            let off = t * n_embd;
            let t0 = std::time::Instant::now();
            let gate_out = layer.ffn_gate.matmul(&hidden[off..off + n_embd]);
            t_gate += t0.elapsed().as_secs_f64();
            scratch.ffn_gate_buf[t * n_ff..t * n_ff + gate_out.len().min(n_ff)].copy_from_slice(&gate_out[..gate_out.len().min(n_ff)]);
            let t0 = std::time::Instant::now();
            let up_out = layer.ffn_up.matmul(&hidden[off..off + n_embd]);
            t_up += t0.elapsed().as_secs_f64();
            scratch.ffn_up_buf[t * n_ff..t * n_ff + up_out.len().min(n_ff)].copy_from_slice(&up_out[..up_out.len().min(n_ff)]);
        }
        crate::ops::silu_mul_inplace(&scratch.ffn_gate_buf[..n_tokens * n_ff], &mut scratch.ffn_up_buf[..n_tokens * n_ff]);
        for t in 0..n_tokens {
            let t0 = std::time::Instant::now();
            let down_out = layer.ffn_down.matmul(&scratch.ffn_up_buf[t * n_ff..][..n_ff]);
            t_down += t0.elapsed().as_secs_f64();
            scratch.buf[t * n_embd..t * n_embd + down_out.len().min(n_embd)].copy_from_slice(&down_out[..down_out.len().min(n_embd)]);
        }
        if profile {
            eprintln!("  ffn: gate={:.3}s up={:.3}s down={:.3}s", t_gate, t_up, t_down);
        }
    }
}

fn kv_cache_pos(cache: &crate::scratchpad::KvCache, il: usize, k_dim: usize, n_layer: usize) -> usize {
    if let crate::scratchpad::KvCache::F32(c) = cache {
        let k_len = c.k.len() / n_layer;
        let mut pos = 0;
        for p in 0..k_len / k_dim {
            if c.k[il * k_len + p * k_dim..il * k_len + (p + 1) * k_dim].iter().all(|v| *v == 0.0) { pos = p; break; }
            pos = p + 1;
        }
        pos
    } else { 0 }
}

fn kv_cache_store(cache: &mut crate::scratchpad::KvCache, il: usize, n_layer: usize, k_data: &[f32], v_data: &[f32], k_dim: usize, v_dim: usize, pos: usize) {
    if let crate::scratchpad::KvCache::F32(c) = cache {
        let k_len = c.k.len() / n_layer;
        let v_len = c.v.len() / n_layer;
        let n_tokens = k_data.len() / k_dim;
        for t in 0..n_tokens {
            let k_dst = il * k_len + (pos + t) * k_dim;
            let v_dst = il * v_len + (pos + t) * v_dim;
            c.k[k_dst..k_dst + k_dim].copy_from_slice(&k_data[t * k_dim..(t + 1) * k_dim]);
            c.v[v_dst..v_dst + v_dim].copy_from_slice(&v_data[t * v_dim..(t + 1) * v_dim]);
        }
    }
}

pub struct Qwen35Scratchpad {
    pub x: Vec<f32>,
    pub buf: Vec<f32>,
    pub normed_buf: Vec<f32>,
    pub q_buf: Vec<f32>,
    pub k_buf: Vec<f32>,
    pub v_buf: Vec<f32>,
    pub k_buf2: Vec<f32>,
    pub v_buf2: Vec<f32>,
    pub qkv_buf: Vec<f32>,
    pub z_buf: Vec<f32>,
    pub beta_buf: Vec<f32>,
    pub alpha_buf: Vec<f32>,
    pub score_buf: Vec<f32>,
    pub attn_out_buf: Vec<f32>,
    pub ffn_up_buf: Vec<f32>,
    pub ffn_gate_buf: Vec<f32>,
    pub conv_states: Vec<Vec<f32>>,
    pub ssm_states: Vec<Vec<f32>>,
    pub matmul_out: Vec<f32>,
    pub q_scaled_buf: Vec<f32>,
    pub q8k_buf: Vec<quant::BlockQ8K>,
}

impl Qwen35Scratchpad {
    pub fn new(config: &Qwen35Config, max_tokens: usize) -> Self {
        let n_embd = config.n_embd;
        let n_head = config.n_head;
        let n_head_kv = config.n_head_kv;
        let n_embd_head = config.n_embd_head();
        let n_ff = config.n_ff;
        let n_layer = config.n_layer;
        let d_inner = config.ssm_d_inner;
        let key_dim = config.key_dim();
        let value_dim = config.value_dim();
        let conv_dim = config.conv_dim();
        let d_conv = config.ssm_d_conv;
        let num_v_heads = config.ssm_dt_rank;
        let head_v_dim = d_inner / num_v_heads;
        let q_dim = n_embd_head * n_head * 2;
        let dense_attn_out_dim = n_embd_head * n_head;

        Self {
            x: vec![0.0; max_tokens * n_embd],
            buf: vec![0.0; max_tokens * n_embd],
            q_buf: vec![0.0; max_tokens * q_dim.max(key_dim)],
            k_buf: vec![0.0; max_tokens * n_embd_head * n_head_kv],
            v_buf: vec![0.0; max_tokens * n_embd_head * n_head_kv],
            k_buf2: vec![0.0; max_tokens * key_dim],
            v_buf2: vec![0.0; max_tokens * value_dim],
            qkv_buf: vec![0.0; max_tokens * conv_dim],
            z_buf: vec![0.0; max_tokens * value_dim],
            beta_buf: vec![0.0; max_tokens * num_v_heads],
            alpha_buf: vec![0.0; max_tokens * num_v_heads],
            score_buf: vec![0.0; config.n_ctx],
            attn_out_buf: vec![0.0; max_tokens * dense_attn_out_dim.max(value_dim)],
            ffn_up_buf: vec![0.0; max_tokens * n_ff],
            ffn_gate_buf: vec![0.0; max_tokens * n_ff],
            conv_states: (0..n_layer).map(|_| vec![0.0; d_conv * conv_dim]).collect(),
            ssm_states: (0..n_layer).map(|_| vec![0.0; num_v_heads * head_v_dim * head_v_dim]).collect(),
            matmul_out: vec![0.0; (2 * n_ff).max(conv_dim).max(n_embd).max(config.vocab_size)],
            normed_buf: vec![0.0; max_tokens * n_embd],
            q_scaled_buf: vec![0.0; head_v_dim],
            q8k_buf: vec![quant::BlockQ8K { d: 0.0, qs: [0i8; 256], bsums: [0i16; 16] }; (n_embd.max(n_ff) + 255) / 256],
        }
    }
}

pub fn f16_at(data: &[u8], idx: usize) -> f32 {
    if idx * 2 + 2 > data.len() { return 0.0; }
    let bits = u16::from_le_bytes([data[idx * 2], data[idx * 2 + 1]]);
    f16_bits_to_f32(bits)
}

pub fn f16_bits_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0f32 } else { 1.0f32 };
    let exp = ((bits >> 10) & 0x1F) as i32;
    let frac = (bits & 0x3FF) as f32 / 1024.0;
    if exp == 0 { sign * frac * 2.0f32.powi(-14) }
    else if exp == 31 { if frac == 0.0 { sign * f32::INFINITY } else { sign * f32::NAN } }
    else { sign * (1.0 + frac) * 2.0f32.powi(exp - 15) }
}

fn l2_norm(x: &mut [f32]) {
    let ss: f32 = x.iter().map(|v| v * v).sum();
    let inv = 1.0 / (ss + 1e-8).sqrt();
    for v in x.iter_mut() { *v *= inv; }
}

fn silu_f32(x: f32) -> f32 { x / (1.0 + (-x).exp()) }
fn sigmoid_f32(x: f32) -> f32 { 1.0 / (1.0 + (-x).exp()) }
fn softplus_f32(x: f32) -> f32 { if x > 20.0 { x } else { (1.0 + x.exp()).ln() } }

fn quantize_row_q8_k_cached(input: &[f32]) -> Vec<BlockQ8K> {
    quant::quantize_row_q8_k(input)
}

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
