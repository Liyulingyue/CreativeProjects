use crate::clip_config::ClipVisionConfig;
use crate::model::GGUFLoader;
use crate::ops::{dot_f32, dot_f16_f32, rope_mrope_interleaved, softmax};
use rayon::prelude::*;

struct Q8Weight {
    data: Vec<u8>,
    n_in: usize,
    n_out: usize,
}

impl Q8Weight {
    fn from_f32(weight: &[f32], n_in: usize, n_out: usize) -> Self {
        assert_eq!(n_in % 32, 0);
        let blocks_per_row = n_in / 32;
        let row_stride = blocks_per_row * 34;
        let mut data = vec![0u8; n_out * row_stride];
        let mut q8_row = vec![0u8; n_in];
        let mut scales = vec![0.0f32; blocks_per_row];
        for o in 0..n_out {
            let row = &weight[o * n_in..(o + 1) * n_in];
            crate::ops::quantize_q8_0_into(row, n_in, &mut q8_row, &mut scales);
            let off = o * row_stride;
            for b in 0..blocks_per_row {
                let block_off = off + b * 34;
                let scale_f16 = crate::ops::f32_to_f16(scales[b]);
                data[block_off] = (scale_f16 & 0xFF) as u8;
                data[block_off + 1] = (scale_f16 >> 8) as u8;
                data[block_off + 2..block_off + 34].copy_from_slice(&q8_row[b * 32..(b + 1) * 32]);
            }
        }
        Q8Weight { data, n_in, n_out }
    }

    fn matmul_batch(&self, input: &[f32], output: &mut [f32], n_tokens: usize, q8_buf: &mut [u8], scale_buf: &mut [f32]) {
        let n_in = self.n_in;
        let n_out = self.n_out;
        let blocks = n_in / 32;
        let weight = &self.data;
        for t in 0..n_tokens {
            crate::ops::quantize_q8_0_into(&input[t * n_in..(t + 1) * n_in], n_in, &mut q8_buf[t * n_in..(t + 1) * n_in], &mut scale_buf[t * blocks..(t + 1) * blocks]);
        }
        let total_rows = n_tokens * n_out;
        if total_rows >= 256 {
            output.par_chunks_mut(n_out).enumerate().for_each(|(t, out_chunk)| {
                let q8_off = t * n_in;
                let scale_off = t * blocks;
                crate::ops::matmul_q8_0_quantized_parallel(weight, &q8_buf[q8_off..q8_off + n_in], &scale_buf[scale_off..scale_off + blocks], out_chunk, n_in, n_out);
            });
        } else {
            for t in 0..n_tokens {
                let q8_off = t * n_in;
                let scale_off = t * blocks;
                crate::ops::matmul_q8_0_quantized_parallel(weight, &q8_buf[q8_off..q8_off + n_in], &scale_buf[scale_off..scale_off + blocks], &mut output[t * n_out..(t + 1) * n_out], n_in, n_out);
            }
        }
    }

    fn matmul_single(&self, input: &[f32], output: &mut [f32], q8_buf: &mut [u8], scale_buf: &mut [f32]) {
        let n_in = self.n_in;
        let n_out = self.n_out;
        let blocks = n_in / 32;
        crate::ops::quantize_q8_0_into(input, n_in, &mut q8_buf[..n_in], &mut scale_buf[..blocks]);
        crate::ops::matmul_q8_0_quantized_parallel(&self.data, &q8_buf[..n_in], &scale_buf[..blocks], output, n_in, n_out);
    }
}

pub struct VisionEncoder<'a> {
    pub config: ClipVisionConfig,
    pub patch_embd_weight: &'a [u8],
    pub patch_embd_weight_1: Option<&'a [u8]>,
    pub position_embd: Option<&'a [u8]>,
    pub post_ln_weight: Option<&'a [u8]>,
    pub post_ln_bias: Option<&'a [u8]>,
    pub patch_bias: Option<&'a [u8]>,
    pub layers: Vec<VisionLayer<'a>>,
    pub mm_0_weight: &'a [u8],
    pub mm_0_bias: Option<&'a [u8]>,
    pub mm_2_weight: &'a [u8],
    pub mm_2_bias: Option<&'a [u8]>,
    pub precomputed: Option<VisionPrecomputed>,
}

pub struct VisionPrecomputed {
    pub qkv_weights: Vec<Q8Weight>,
    pub qkv_biases: Vec<Option<Vec<f32>>>,
    pub out_weights: Vec<Q8Weight>,
    pub out_biases: Vec<Option<Vec<f32>>>,
    pub ffn_up_weights: Vec<Q8Weight>,
    pub ffn_up_biases: Vec<Option<Vec<f32>>>,
    pub ffn_down_weights: Vec<Q8Weight>,
    pub ffn_down_biases: Vec<Option<Vec<f32>>>,
    pub ln1_weights: Vec<Vec<f32>>,
    pub ln1_biases: Vec<Option<Vec<f32>>>,
    pub ln2_weights: Vec<Vec<f32>>,
    pub ln2_biases: Vec<Option<Vec<f32>>>,
    pub post_ln_weight: Vec<f32>,
    pub post_ln_bias: Option<Vec<f32>>,
    pub patch_bias: Option<Vec<f32>>,
    pub mm_0_weight: Q8Weight,
    pub mm_0_bias: Option<Vec<f32>>,
    pub mm_2_weight: Q8Weight,
    pub mm_2_bias: Option<Vec<f32>>,
}

pub struct VisionLayer<'a> {
    pub ln1_weight: &'a [u8],
    pub ln1_bias: Option<&'a [u8]>,
    pub ln2_weight: &'a [u8],
    pub ln2_bias: Option<&'a [u8]>,
    pub qkv_weight: &'a [u8],
    pub qkv_bias: Option<&'a [u8]>,
    pub out_weight: &'a [u8],
    pub out_bias: Option<&'a [u8]>,
    pub ffn_up_weight: &'a [u8],
    pub ffn_up_bias: Option<&'a [u8]>,
    pub ffn_down_weight: &'a [u8],
    pub ffn_down_bias: Option<&'a [u8]>,
}

impl<'a> VisionEncoder<'a> {
    pub fn from_gguf(loader: &'a GGUFLoader) -> Result<Self, String> {
        let config = ClipVisionConfig::from_gguf(loader)?;

        let patch_embd_weight = loader.tensor_slice("v.patch_embd.weight")
            .ok_or("Missing v.patch_embd.weight")?;
        let patch_embd_weight_1 = loader.tensor_slice("v.patch_embd.weight.1");
        let position_embd = loader.tensor_slice("v.position_embd.weight");
        let post_ln_weight = loader.tensor_slice("v.post_ln.weight");
        let post_ln_bias = loader.tensor_slice("v.post_ln.bias");
        let patch_bias = loader.tensor_slice("v.patch_embd.bias");

        let mut layers = Vec::with_capacity(config.n_layer);
        for i in 0..config.n_layer {
            let ln1_weight = loader.tensor_slice(&format!("v.blk.{}.ln1.weight", i))
                .ok_or_else(|| format!("Missing v.blk.{}.ln1.weight", i))?;
            let ln1_bias = loader.tensor_slice(&format!("v.blk.{}.ln1.bias", i));
            let ln2_weight = loader.tensor_slice(&format!("v.blk.{}.ln2.weight", i))
                .ok_or_else(|| format!("Missing v.blk.{}.ln2.weight", i))?;
            let ln2_bias = loader.tensor_slice(&format!("v.blk.{}.ln2.bias", i));
            let qkv_weight = loader.tensor_slice(&format!("v.blk.{}.attn_qkv.weight", i))
                .ok_or_else(|| format!("Missing v.blk.{}.attn_qkv.weight", i))?;
            let qkv_bias = loader.tensor_slice(&format!("v.blk.{}.attn_qkv.bias", i));
            let out_weight = loader.tensor_slice(&format!("v.blk.{}.attn_out.weight", i))
                .ok_or_else(|| format!("Missing v.blk.{}.attn_out.weight", i))?;
            let out_bias = loader.tensor_slice(&format!("v.blk.{}.attn_out.bias", i));
            let ffn_up_weight = loader.tensor_slice(&format!("v.blk.{}.ffn_up.weight", i))
                .ok_or_else(|| format!("Missing v.blk.{}.ffn_up.weight", i))?;
            let ffn_up_bias = loader.tensor_slice(&format!("v.blk.{}.ffn_up.bias", i));
            let ffn_down_weight = loader.tensor_slice(&format!("v.blk.{}.ffn_down.weight", i))
                .ok_or_else(|| format!("Missing v.blk.{}.ffn_down.weight", i))?;
            let ffn_down_bias = loader.tensor_slice(&format!("v.blk.{}.ffn_down.bias", i));

            layers.push(VisionLayer {
                ln1_weight, ln1_bias,
                ln2_weight, ln2_bias,
                qkv_weight, qkv_bias,
                out_weight, out_bias,
                ffn_up_weight, ffn_up_bias,
                ffn_down_weight, ffn_down_bias,
            });
        }

        let mm_0_weight = loader.tensor_slice("mm.0.weight")
            .ok_or("Missing mm.0.weight")?;
        let mm_0_bias = loader.tensor_slice("mm.0.bias");
        let mm_2_weight = loader.tensor_slice("mm.2.weight")
            .ok_or("Missing mm.2.weight")?;
        let mm_2_bias = loader.tensor_slice("mm.2.bias");

        Ok(Self {
            config,
            patch_embd_weight,
            patch_embd_weight_1,
            position_embd,
            post_ln_weight,
            post_ln_bias,
            patch_bias,
            layers,
            mm_0_weight,
            mm_0_bias,
            mm_2_weight,
            mm_2_bias,
            precomputed: None,
        })
    }

    pub fn precompute(&mut self) {
        let n_layer = self.config.n_layer;
        let n_embd = self.config.n_embd;
        let n_ff = self.config.n_ff;
        let mut qkv_weights = Vec::with_capacity(n_layer);
        let mut qkv_biases = Vec::with_capacity(n_layer);
        let mut out_weights = Vec::with_capacity(n_layer);
        let mut out_biases = Vec::with_capacity(n_layer);
        let mut ffn_up_weights = Vec::with_capacity(n_layer);
        let mut ffn_up_biases = Vec::with_capacity(n_layer);
        let mut ffn_down_weights = Vec::with_capacity(n_layer);
        let mut ffn_down_biases = Vec::with_capacity(n_layer);
        let mut ln1_weights = Vec::with_capacity(n_layer);
        let mut ln1_biases = Vec::with_capacity(n_layer);
        let mut ln2_weights = Vec::with_capacity(n_layer);
        let mut ln2_biases = Vec::with_capacity(n_layer);

        for layer in &self.layers {
            let qkv_f32 = decode_f16_slice_to_f32(layer.qkv_weight);
            qkv_weights.push(Q8Weight::from_f32(&qkv_f32, n_embd, n_embd * 3));
            qkv_biases.push(layer.qkv_bias.map(decode_f32_slice));
            let out_f32 = decode_f16_slice_to_f32(layer.out_weight);
            out_weights.push(Q8Weight::from_f32(&out_f32, n_embd, n_embd));
            out_biases.push(layer.out_bias.map(decode_f32_slice));
            let ffn_up_f32 = decode_f16_slice_to_f32(layer.ffn_up_weight);
            ffn_up_weights.push(Q8Weight::from_f32(&ffn_up_f32, n_embd, n_ff));
            ffn_up_biases.push(layer.ffn_up_bias.map(decode_f32_slice));
            let ffn_down_f32 = decode_f16_slice_to_f32(layer.ffn_down_weight);
            ffn_down_weights.push(Q8Weight::from_f32(&ffn_down_f32, n_ff, n_embd));
            ffn_down_biases.push(layer.ffn_down_bias.map(decode_f32_slice));
            ln1_weights.push(decode_f32_slice(layer.ln1_weight));
            ln1_biases.push(layer.ln1_bias.map(decode_f32_slice));
            ln2_weights.push(decode_f32_slice(layer.ln2_weight));
            ln2_biases.push(layer.ln2_bias.map(decode_f32_slice));
        }

        let mm0_f32 = decode_f16_slice_to_f32(self.mm_0_weight);
        let mm2_f32 = decode_f16_slice_to_f32(self.mm_2_weight);
        let mm_merged_embd = n_embd * self.config.spatial_merge_size * self.config.spatial_merge_size;

        self.precomputed = Some(VisionPrecomputed {
            qkv_weights, qkv_biases,
            out_weights, out_biases,
            ffn_up_weights, ffn_up_biases,
            ffn_down_weights, ffn_down_biases,
            ln1_weights, ln1_biases,
            ln2_weights, ln2_biases,
            post_ln_weight: self.post_ln_weight.map_or_else(Vec::new, decode_f32_slice),
            post_ln_bias: self.post_ln_bias.map(decode_f32_slice),
            patch_bias: self.patch_bias.map(decode_f32_slice),
            mm_0_weight: Q8Weight::from_f32(&mm0_f32, mm_merged_embd, mm_merged_embd),
            mm_0_bias: self.mm_0_bias.map(decode_f32_slice),
            mm_2_weight: Q8Weight::from_f32(&mm2_f32, mm_merged_embd, self.config.projection_dim),
            mm_2_bias: self.mm_2_bias.map(decode_f32_slice),
        });
    }

    pub fn encode_image(
        &self,
        image_pixels: &[f32],
        img_w: usize,
        img_h: usize,
        scratch: &mut VisionScratchpad,
    ) -> usize {
        let cfg = &self.config;
        let ps = cfg.patch_size;
        let n_embd = cfg.n_embd;
        let merge = cfg.spatial_merge_size;

        let n_patches_x = img_w / ps;
        let n_patches_y = img_h / ps;
        let n_patches = n_patches_x * n_patches_y;

        self.patch_embed(image_pixels, img_w, img_h, scratch);

        spatial_merge(&mut scratch.patch_embd[..n_patches * n_embd], n_patches_x, n_patches_y, n_embd, merge, &mut scratch.merged[..n_patches * n_embd]);
        let n_tokens = n_patches;

        if let Some(ref pc) = self.precomputed {
            if let Some(ref bias) = pc.patch_bias {
                for t in 0..n_tokens {
                    let off = t * n_embd;
                    for e in 0..n_embd {
                        scratch.merged[off + e] += bias[e];
                    }
                }
            }
        } else if let Some(patch_bias_data) = self.patch_bias {
            let bias = decode_f32_slice(patch_bias_data);
            for t in 0..n_tokens {
                let off = t * n_embd;
                for e in 0..n_embd {
                    scratch.merged[off + e] += bias[e];
                }
            }
        }

        if let Some(pos_embd_data) = self.position_embd {
            self.apply_position_embedding_merged(&mut scratch.merged[..n_tokens * n_embd], n_patches_x, n_patches_y, n_embd, merge, pos_embd_data, &mut scratch.pos_embd_buf);
        }

        let mrope_positions = build_vit_mrope_positions(n_patches_x, n_patches_y, merge);

        let t_start = std::time::Instant::now();
        for il in 0..cfg.n_layer {
            self.forward_vit_layer(il, scratch, n_tokens, &mrope_positions);
        }
        eprintln!("ViT total: {:.2}ms", t_start.elapsed().as_millis());

        if let Some(ref pc) = self.precomputed {
            if !pc.post_ln_weight.is_empty() {
                for t in 0..n_tokens {
                    let off = t * n_embd;
                    if let Some(ref b) = pc.post_ln_bias {
                        layer_norm_with_bias(&mut scratch.merged[off..off + n_embd], &pc.post_ln_weight, b, cfg.eps);
                    } else {
                        layer_norm_without_bias(&mut scratch.merged[off..off + n_embd], &pc.post_ln_weight, cfg.eps);
                    }
                }
            }
        } else if let (Some(ln_w), Some(ln_b)) = (self.post_ln_weight, self.post_ln_bias) {
            let w = decode_f32_slice(ln_w);
            let b = decode_f32_slice(ln_b);
            for t in 0..n_tokens {
                let off = t * n_embd;
                layer_norm_with_bias(&mut scratch.merged[off..off + n_embd], &w, &b, cfg.eps);
            }
        }

        let n_projected = n_tokens / (merge * merge);
        self.project(n_patches_x, n_patches_y, n_embd, merge, scratch);

        n_projected
    }

    fn patch_embed(&self, pixels: &[f32], img_w: usize, img_h: usize, scratch: &mut VisionScratchpad) {
        let cfg = &self.config;
        let ps = cfg.patch_size;
        let n_embd = cfg.n_embd;
        let n_patches_x = img_w / ps;
        let n_patches_y = img_h / ps;

        if scratch.patch_weight_buf.is_empty() {
            scratch.patch_weight_buf = decode_f16_slice_to_f32(self.patch_embd_weight);
            scratch.patch_weight_1_buf = self.patch_embd_weight_1.map(|d| decode_f16_slice_to_f32(d));
        }
        let w0 = &scratch.patch_weight_buf;
        let w1 = &scratch.patch_weight_1_buf;

        for py in 0..n_patches_y {
            for px in 0..n_patches_x {
                let patch_idx = py * n_patches_x + px;
                let out_off = patch_idx * n_embd;
                for e in 0..n_embd {
                    let mut sum0 = 0.0f32;
                    let mut sum1 = 0.0f32;
                    for c in 0..3usize {
                        for ky in 0..ps {
                            for kx in 0..ps {
                                let pix_x = px * ps + kx;
                                let pix_y = py * ps + ky;
                                let pix_val = pixels[(pix_y * img_w + pix_x) * 3 + c];
                                let w0_idx = kx + ky * ps + c * ps * ps + e * ps * ps * 3;
                                sum0 += w0[w0_idx] * pix_val;
                                if let Some(ref w1d) = w1 {
                                    let w1_idx = kx + ky * ps + c * ps * ps + e * ps * ps * 3;
                                    sum1 += w1d[w1_idx] * pix_val;
                                }
                            }
                        }
                    }
                    scratch.patch_embd[out_off + e] = sum0 + sum1;
                }
            }
        }
    }

    fn apply_position_embedding_merged(&self, merged: &mut [f32], n_patches_x: usize, n_patches_y: usize, n_embd: usize, merge: usize, pos_data: &[u8], pos_merged_buf: &mut [f32]) {
        let pos_len = pos_data.len() / 4;
        let pos_per_side = (pos_len / n_embd) as usize;
        let pos_side = (pos_per_side as f64).sqrt() as usize;

        let decoded_pos: Vec<f32>;
        if pos_side == n_patches_x && pos_side == n_patches_y {
            decoded_pos = decode_f32_slice(pos_data);
        } else {
            let raw = decode_f32_slice(pos_data);
            decoded_pos = bilinear_resize_2d(&raw, pos_side, pos_side, n_embd, n_patches_x, n_patches_y);
        }

        let total = n_patches_x * n_patches_y * n_embd;
        spatial_merge(&decoded_pos[..total], n_patches_x, n_patches_y, n_embd, merge, &mut pos_merged_buf[..total]);

        for i in 0..total {
            if i < merged.len() {
                merged[i] += pos_merged_buf[i];
            }
        }
    }

    fn forward_vit_layer(&self, il: usize, scratch: &mut VisionScratchpad, n_tokens: usize, mrope_positions: &[[usize; 4]]) {
        let cfg = &self.config;
        let n_embd = cfg.n_embd;
        let n_head = cfg.n_head;
        let d_head = cfg.d_head();
        let eps = cfg.eps;

        scratch.residual[..n_tokens * n_embd].copy_from_slice(&scratch.merged[..n_tokens * n_embd]);

        if let Some(ref pc) = self.precomputed {
            if let Some(ref b) = pc.ln1_biases[il] {
                for t in 0..n_tokens {
                    let off = t * n_embd;
                    layer_norm_with_bias(&mut scratch.merged[off..off + n_embd], &pc.ln1_weights[il], b, eps);
                }
            } else {
                for t in 0..n_tokens {
                    let off = t * n_embd;
                    layer_norm_without_bias(&mut scratch.merged[off..off + n_embd], &pc.ln1_weights[il], eps);
                }
            }

            pc.qkv_weights[il].matmul_batch(
                &scratch.merged[..n_tokens * n_embd],
                &mut scratch.qkv_buf[..n_tokens * n_embd * 3],
                n_tokens, &mut scratch.q8_buf, &mut scratch.q8_scale_buf,
            );

            if let Some(ref bias) = pc.qkv_biases[il] {
                for t in 0..n_tokens {
                    for j in 0..n_embd * 3 {
                        scratch.qkv_buf[t * n_embd * 3 + j] += bias[j];
                    }
                }
            }
        } else {
            let layer = &self.layers[il];

            if let Some(bias_data) = layer.ln1_bias {
                let w = decode_f32_slice(layer.ln1_weight);
                let b = decode_f32_slice(bias_data);
                for t in 0..n_tokens {
                    let off = t * n_embd;
                    layer_norm_with_bias(&mut scratch.merged[off..off + n_embd], &w, &b, eps);
                }
            } else {
                let w = decode_f32_slice(layer.ln1_weight);
                for t in 0..n_tokens {
                    let off = t * n_embd;
                    layer_norm_without_bias(&mut scratch.merged[off..off + n_embd], &w, eps);
                }
            }

            for t in 0..n_tokens {
                let inp_off = t * n_embd;
                let out_off = t * n_embd * 3;
                matmul_f16_f32_single(layer.qkv_weight, &scratch.merged[inp_off..inp_off + n_embd], &mut scratch.qkv_buf[out_off..out_off + n_embd * 3], n_embd, n_embd * 3);
            }

            if let Some(bias_data) = layer.qkv_bias {
                let bias = decode_f32_slice(bias_data);
                for t in 0..n_tokens {
                    for j in 0..n_embd * 3 {
                        scratch.qkv_buf[t * n_embd * 3 + j] += bias[j];
                    }
                }
            }
        }

        for t in 0..n_tokens {
            let src_off = t * n_embd * 3;
            for h in 0..n_head {
                for d in 0..d_head {
                    scratch.attn_buf[h * n_tokens * d_head + t * d_head + d] = scratch.qkv_buf[src_off + h * d_head + d];
                    scratch.attn_buf[n_head * n_tokens * d_head + h * n_tokens * d_head + t * d_head + d] = scratch.qkv_buf[src_off + n_embd + h * d_head + d];
                    scratch.attn_buf[2 * n_head * n_tokens * d_head + h * n_tokens * d_head + t * d_head + d] = scratch.qkv_buf[src_off + 2 * n_embd + h * d_head + d];
                }
            }
        }

        let mrope_sections: [i32; 4] = [(d_head / 4) as i32, (d_head / 4) as i32, (d_head / 4) as i32, (d_head / 4) as i32];
        let freq_base = 10000.0f32;
        for h in 0..n_head {
            let q_base = h * n_tokens * d_head;
            let k_base = n_head * n_tokens * d_head + h * n_tokens * d_head;
            for t in 0..n_tokens {
                rope_mrope_interleaved(
                    &mut scratch.attn_buf[q_base + t * d_head..q_base + t * d_head + d_head],
                    mrope_positions[t],
                    mrope_sections,
                    d_head,
                    freq_base,
                    d_head / 2,
                );
                rope_mrope_interleaved(
                    &mut scratch.attn_buf[k_base + t * d_head..k_base + t * d_head + d_head],
                    mrope_positions[t],
                    mrope_sections,
                    d_head,
                    freq_base,
                    d_head / 2,
                );
            }
        }

        let scale = 1.0 / (d_head as f32).sqrt();
        let attn_buf = &scratch.attn_buf[..3 * n_head * n_tokens * d_head];

        let scores = scratch.score_buf.as_mut_ptr();
        let out_buf = scratch.attn_out_buf.as_mut_ptr();

        struct PtrWrap(*mut f32);
        unsafe impl Sync for PtrWrap {}
        unsafe impl Send for PtrWrap {}
        impl PtrWrap {
            unsafe fn slice(&self, offset: usize, len: usize) -> &mut [f32] {
                std::slice::from_raw_parts_mut(self.0.add(offset), len)
            }
        }

        let sw = PtrWrap(scores);
        let ow = PtrWrap(out_buf);
        #[cfg(target_arch = "x86_64")]
        let use_avx2 = crate::ops::has_avx2_fma();

        (0..n_head).into_par_iter().for_each(move |h| {
            let q_base = h * n_tokens * d_head;
            let k_base = n_head * n_tokens * d_head + h * n_tokens * d_head;
            let v_base = 2 * n_head * n_tokens * d_head + h * n_tokens * d_head;
            let score_off = h * n_tokens * n_tokens;
            unsafe {
                let score_slice = sw.slice(score_off, n_tokens * n_tokens);
                let out_slice = ow.slice(h * n_tokens * d_head, n_tokens * d_head);
                for t in 0..n_tokens {
                    let q_ptr = attn_buf.as_ptr().add(q_base + t * d_head);
                    #[cfg(target_arch = "x86_64")]
                    if use_avx2 {
                        unsafe { attention_qk_avx2(q_ptr, attn_buf.as_ptr().add(k_base), &mut score_slice[t * n_tokens..t * n_tokens + n_tokens], n_tokens, d_head, scale); }
                    } else {
                        for s in 0..n_tokens {
                            let k_ptr = attn_buf.as_ptr().add(k_base + s * d_head);
                            let mut sum = 0.0f32;
                            for i in 0..d_head { sum += *q_ptr.add(i) * *k_ptr.add(i); }
                            score_slice[t * n_tokens + s] = sum * scale;
                        }
                    }
                    #[cfg(not(target_arch = "x86_64"))]
                    for s in 0..n_tokens {
                        let k_ptr = attn_buf.as_ptr().add(k_base + s * d_head);
                        let mut sum = 0.0f32;
                        for i in 0..d_head { sum += *q_ptr.add(i) * *k_ptr.add(i); }
                        score_slice[t * n_tokens + s] = sum * scale;
                    }
                    softmax(&mut score_slice[t * n_tokens..t * n_tokens + n_tokens]);

                    let out_base = t * d_head;
                    for d in 0..d_head {
                        out_slice[out_base + d] = 0.0;
                    }
                    #[cfg(target_arch = "x86_64")]
                    if use_avx2 {
                        for s in 0..n_tokens {
                            let sc = score_slice[t * n_tokens + s];
                            unsafe { attn_scaled_add_avx2(&mut out_slice[out_base..out_base + d_head], attn_buf.as_ptr().add(v_base + s * d_head), sc, d_head); }
                        }
                    } else {
                        for s in 0..n_tokens {
                            let sc = score_slice[t * n_tokens + s];
                            let v_ptr = attn_buf.as_ptr().add(v_base + s * d_head);
                            for d in 0..d_head {
                                out_slice[out_base + d] += sc * *v_ptr.add(d);
                            }
                        }
                    }
                    #[cfg(not(target_arch = "x86_64"))]
                    for s in 0..n_tokens {
                        let sc = score_slice[t * n_tokens + s];
                        let v_ptr = attn_buf.as_ptr().add(v_base + s * d_head);
                        for d in 0..d_head {
                            out_slice[out_base + d] += sc * *v_ptr.add(d);
                        }
                    }
                }
            }
        });

        for t in 0..n_tokens {
            for h in 0..n_head {
                for d in 0..d_head {
                    scratch.attn_concat[t * n_embd + h * d_head + d] = scratch.attn_out_buf[h * n_tokens * d_head + t * d_head + d];
                }
            }
        }

        if let Some(ref pc) = self.precomputed {
            pc.out_weights[il].matmul_batch(
                &scratch.attn_concat[..n_tokens * n_embd],
                &mut scratch.proj_buf[..n_tokens * n_embd],
                n_tokens, &mut scratch.q8_buf, &mut scratch.q8_scale_buf,
            );
            if let Some(ref bias) = pc.out_biases[il] {
                for t in 0..n_tokens {
                    for j in 0..n_embd {
                        scratch.proj_buf[t * n_embd + j] += bias[j];
                    }
                }
            }
        } else {
            let layer = &self.layers[il];
            for t in 0..n_tokens {
                let inp_off = t * n_embd;
                let out_off = t * n_embd;
                matmul_f16_f32_single(layer.out_weight, &scratch.attn_concat[inp_off..inp_off + n_embd], &mut scratch.proj_buf[out_off..out_off + n_embd], n_embd, n_embd);
            }
            if let Some(bias_data) = layer.out_bias {
                let bias = decode_f32_slice(bias_data);
                for t in 0..n_tokens {
                    for j in 0..n_embd {
                        scratch.proj_buf[t * n_embd + j] += bias[j];
                    }
                }
            }
        }

        for i in 0..n_tokens * n_embd {
            scratch.merged[i] = scratch.residual[i] + scratch.proj_buf[i];
        }

        scratch.residual[..n_tokens * n_embd].copy_from_slice(&scratch.merged[..n_tokens * n_embd]);

        if let Some(ref pc) = self.precomputed {
            if let Some(ref b) = pc.ln2_biases[il] {
                for t in 0..n_tokens {
                    let off = t * n_embd;
                    layer_norm_with_bias(&mut scratch.merged[off..off + n_embd], &pc.ln2_weights[il], b, eps);
                }
            } else {
                for t in 0..n_tokens {
                    let off = t * n_embd;
                    layer_norm_without_bias(&mut scratch.merged[off..off + n_embd], &pc.ln2_weights[il], eps);
                }
            }

            pc.ffn_up_weights[il].matmul_batch(
                &scratch.merged[..n_tokens * n_embd],
                &mut scratch.ffn_buf[..n_tokens * cfg.n_ff],
                n_tokens, &mut scratch.q8_buf, &mut scratch.q8_scale_buf,
            );
            if let Some(ref bias) = pc.ffn_up_biases[il] {
                for t in 0..n_tokens {
                    for j in 0..cfg.n_ff {
                        scratch.ffn_buf[t * cfg.n_ff + j] += bias[j];
                    }
                }
            }
        } else {
            let layer = &self.layers[il];
            if let Some(bias_data) = layer.ln2_bias {
                let w = decode_f32_slice(layer.ln2_weight);
                let b = decode_f32_slice(bias_data);
                for t in 0..n_tokens {
                    let off = t * n_embd;
                    layer_norm_with_bias(&mut scratch.merged[off..off + n_embd], &w, &b, eps);
                }
            } else {
                let w = decode_f32_slice(layer.ln2_weight);
                for t in 0..n_tokens {
                    let off = t * n_embd;
                    layer_norm_without_bias(&mut scratch.merged[off..off + n_embd], &w, eps);
                }
            }

            for t in 0..n_tokens {
                let inp_off = t * n_embd;
                let out_off = t * cfg.n_ff;
                matmul_f16_f32_single(layer.ffn_up_weight, &scratch.merged[inp_off..inp_off + n_embd], &mut scratch.ffn_buf[out_off..out_off + cfg.n_ff], n_embd, cfg.n_ff);
            }

            if let Some(bias_data) = layer.ffn_up_bias {
                let bias = decode_f32_slice(bias_data);
                for t in 0..n_tokens {
                    for j in 0..cfg.n_ff {
                        scratch.ffn_buf[t * cfg.n_ff + j] += bias[j];
                    }
                }
            }
        }

        for x in scratch.ffn_buf[..n_tokens * cfg.n_ff].iter_mut() {
            *x = gelu(*x);
        }

        if let Some(ref pc) = self.precomputed {
            pc.ffn_down_weights[il].matmul_batch(
                &scratch.ffn_buf[..n_tokens * cfg.n_ff],
                &mut scratch.proj_buf[..n_tokens * n_embd],
                n_tokens, &mut scratch.q8_buf, &mut scratch.q8_scale_buf,
            );
            if let Some(ref bias) = pc.ffn_down_biases[il] {
                for t in 0..n_tokens {
                    for j in 0..n_embd {
                        scratch.proj_buf[t * n_embd + j] += bias[j];
                    }
                }
            }
        } else {
            let layer = &self.layers[il];
            for t in 0..n_tokens {
                let inp_off = t * cfg.n_ff;
                let out_off = t * n_embd;
                matmul_f16_f32_single(layer.ffn_down_weight, &scratch.ffn_buf[inp_off..inp_off + cfg.n_ff], &mut scratch.proj_buf[out_off..out_off + n_embd], cfg.n_ff, n_embd);
            }

            if let Some(bias_data) = layer.ffn_down_bias {
                let bias = decode_f32_slice(bias_data);
                for t in 0..n_tokens {
                    for j in 0..n_embd {
                        scratch.proj_buf[t * n_embd + j] += bias[j];
                    }
                }
            }
        }

        for i in 0..n_tokens * n_embd {
            scratch.merged[i] = scratch.residual[i] + scratch.proj_buf[i];
        }
    }

    fn project(&self, n_patches_x: usize, n_patches_y: usize, n_embd: usize, merge: usize, scratch: &mut VisionScratchpad) {
        let cfg = &self.config;
        let n_merged_x = n_patches_x / merge;
        let n_merged_y = n_patches_y / merge;
        let n_projected = n_merged_x * n_merged_y;
        let proj_dim = cfg.projection_dim;
        let merged_embd = n_embd * merge * merge;
        let hidden = &scratch.merged;
        let out = &mut scratch.projected;

        let concat_size = n_projected * merged_embd;
        if scratch.project_concat_buf.len() < concat_size {
            scratch.project_concat_buf.resize(concat_size, 0.0);
        }
        if scratch.project_mm0_out.len() < concat_size {
            scratch.project_mm0_out.resize(concat_size, 0.0);
        }
        let concat_buf = &mut scratch.project_concat_buf[..concat_size];
        let mm0_out = &mut scratch.project_mm0_out[..concat_size];

        for my in 0..n_merged_y {
            for mx in 0..n_merged_x {
                let proj_idx = my * n_merged_x + mx;
                let dst_off = proj_idx * merged_embd;
                for dy in 0..merge {
                    for dx in 0..merge {
                        let src_py = my * merge + dy;
                        let src_px = mx * merge + dx;
                        let src_idx = src_py * n_patches_x + src_px;
                        let src_off = src_idx * n_embd;
                        let sub_off = (dy * merge + dx) * n_embd;
                        for e in 0..n_embd {
                            concat_buf[dst_off + sub_off + e] = hidden[src_off + e];
                        }
                    }
                }
            }
        }

        if let Some(ref pc) = self.precomputed {
            for t in 0..n_projected {
                let src_off = t * merged_embd;
                let dst_off = t * merged_embd;
                pc.mm_0_weight.matmul_single(&concat_buf[src_off..src_off + merged_embd], &mut mm0_out[dst_off..dst_off + merged_embd], &mut scratch.q8_buf, &mut scratch.q8_scale_buf);
            }
            if let Some(ref bias) = pc.mm_0_bias {
                for t in 0..n_projected {
                    for j in 0..bias.len().min(merged_embd) {
                        mm0_out[t * merged_embd + j] += bias[j];
                    }
                }
            }
        } else {
            for t in 0..n_projected {
                let src_off = t * merged_embd;
                let dst_off = t * merged_embd;
                matmul_f16_f32_single(self.mm_0_weight, &concat_buf[src_off..src_off + merged_embd], &mut mm0_out[dst_off..dst_off + merged_embd], merged_embd, merged_embd);
            }
            if let Some(bias_data) = self.mm_0_bias {
                let bias = decode_f32_slice(bias_data);
                for t in 0..n_projected {
                    for j in 0..bias.len().min(merged_embd) {
                        mm0_out[t * merged_embd + j] += bias[j];
                    }
                }
            }
        }

        for x in mm0_out[..n_projected * merged_embd].iter_mut() {
            *x = gelu(*x);
        }

        if let Some(ref pc) = self.precomputed {
            for t in 0..n_projected {
                let src_off = t * merged_embd;
                let dst_off = t * proj_dim;
                pc.mm_2_weight.matmul_single(&mm0_out[src_off..src_off + merged_embd], &mut out[dst_off..dst_off + proj_dim], &mut scratch.q8_buf, &mut scratch.q8_scale_buf);
            }
            if let Some(ref bias) = pc.mm_2_bias {
                for t in 0..n_projected {
                    for j in 0..proj_dim {
                        out[t * proj_dim + j] += bias[j];
                    }
                }
            }
        } else {
            for t in 0..n_projected {
                let src_off = t * merged_embd;
                let dst_off = t * proj_dim;
                matmul_f16_f32_single(self.mm_2_weight, &mm0_out[src_off..src_off + merged_embd], &mut out[dst_off..dst_off + proj_dim], merged_embd, proj_dim);
            }
            if let Some(bias_data) = self.mm_2_bias {
                let bias = decode_f32_slice(bias_data);
                for t in 0..n_projected {
                    for j in 0..proj_dim {
                        out[t * proj_dim + j] += bias[j];
                    }
                }
            }
        }
    }
}

fn spatial_merge(input: &[f32], n_patches_x: usize, n_patches_y: usize, n_embd: usize, merge: usize, output: &mut [f32]) {
    let n_merged_x = n_patches_x / merge;
    let n_merged_y = n_patches_y / merge;

    let mut ptr = 0usize;
    for my in 0..n_merged_y {
        for mx in 0..n_merged_x {
            for dy in 0..merge {
                for dx in 0..merge {
                    let src_py = my * merge + dy;
                    let src_px = mx * merge + dx;
                    let src_idx = src_py * n_patches_x + src_px;
                    let src_off = src_idx * n_embd;
                    let dst_off = ptr * n_embd;
                    for e in 0..n_embd {
                        if dst_off + e < output.len() && src_off + e < input.len() {
                            output[dst_off + e] = input[src_off + e];
                        }
                    }
                    ptr += 1;
                }
            }
        }
    }
}

fn build_vit_mrope_positions(n_patches_x: usize, n_patches_y: usize, merge: usize) -> Vec<[usize; 4]> {
    let pw = n_patches_x;
    let ph = n_patches_y;
    let n_tokens = pw * ph;
    let mut positions = vec![[0usize; 4]; n_tokens];

    let mut ptr = 0usize;
    for y in 0..ph {
        for x in 0..pw {
            positions[ptr] = [y, x, y, x];
            ptr += 1;
        }
    }

    let mut merged = vec![[0usize; 4]; n_tokens];
    spatial_merge_positions(&positions, n_patches_x, n_patches_y, merge, &mut merged);
    merged
}

fn spatial_merge_positions(input: &[[usize; 4]], n_patches_x: usize, n_patches_y: usize, merge: usize, output: &mut [[usize; 4]]) {
    let n_merged_x = n_patches_x / merge;
    let n_merged_y = n_patches_y / merge;

    let mut ptr = 0usize;
    for my in 0..n_merged_y {
        for mx in 0..n_merged_x {
            for dy in 0..merge {
                for dx in 0..merge {
                    let src_py = my * merge + dy;
                    let src_px = mx * merge + dx;
                    let src_idx = src_py * n_patches_x + src_px;
                    output[ptr] = input[src_idx];
                    ptr += 1;
                }
            }
        }
    }
}

fn bilinear_resize_2d(input: &[f32], src_h: usize, src_w: usize, n_embd: usize, dst_h: usize, dst_w: usize) -> Vec<f32> {
    let dst_size = dst_h * dst_w;
    let mut output = vec![0.0f32; dst_size * n_embd];

    for dy in 0..dst_h {
        for dx in 0..dst_w {
            let src_y = if dst_h > 1 { dy as f32 * (src_h as f32 - 1.0) / (dst_h as f32 - 1.0) } else { 0.0 };
            let src_x = if dst_w > 1 { dx as f32 * (src_w as f32 - 1.0) / (dst_w as f32 - 1.0) } else { 0.0 };

            let y0 = src_y.floor() as usize;
            let x0 = src_x.floor() as usize;
            let y1 = (y0 + 1).min(src_h - 1);
            let x1 = (x0 + 1).min(src_w - 1);

            let fy = src_y - y0 as f32;
            let fx = src_x - x0 as f32;

            let dst_idx = dy * dst_w + dx;
            let dst_off = dst_idx * n_embd;

            let i00 = y0 * src_w + x0;
            let i01 = y0 * src_w + x1;
            let i10 = y1 * src_w + x0;
            let i11 = y1 * src_w + x1;

            for e in 0..n_embd {
                let v00 = input[i00 * n_embd + e];
                let v01 = input[i01 * n_embd + e];
                let v10 = input[i10 * n_embd + e];
                let v11 = input[i11 * n_embd + e];

                let v = v00 * (1.0 - fx) * (1.0 - fy)
                      + v01 * fx * (1.0 - fy)
                      + v10 * (1.0 - fx) * fy
                      + v11 * fx * fy;

                output[dst_off + e] = v;
            }
        }
    }

    output
}

pub struct VisionScratchpad {
    pub patch_embd: Vec<f32>,
    pub merged: Vec<f32>,
    pub pos_embd_buf: Vec<f32>,
    pub qkv_buf: Vec<f32>,
    pub attn_buf: Vec<f32>,
    pub attn_out_buf: Vec<f32>,
    pub score_buf: Vec<f32>,
    pub proj_buf: Vec<f32>,
    pub ffn_buf: Vec<f32>,
    pub projected: Vec<f32>,
    pub attn_concat: Vec<f32>,
    pub residual: Vec<f32>,
    pub patch_weight_buf: Vec<f32>,
    pub patch_weight_1_buf: Option<Vec<f32>>,
    pub project_concat_buf: Vec<f32>,
    pub project_mm0_out: Vec<f32>,
    pub q8_buf: Vec<u8>,
    pub q8_scale_buf: Vec<f32>,
}

impl VisionScratchpad {
    pub fn new(config: &ClipVisionConfig) -> Self {
        let n_patches = config.n_patches();
        let n_embd = config.n_embd;
        let n_head = config.n_head;
        let d_head = config.d_head();
        let n_tokens = n_patches;

        Self {
            patch_embd: vec![0.0; n_patches * n_embd],
            merged: vec![0.0; n_patches * n_embd],
            pos_embd_buf: vec![0.0; n_patches * n_embd],
            qkv_buf: vec![0.0; n_tokens * n_embd * 3],
            attn_buf: vec![0.0; 3 * n_head * n_tokens * d_head],
            attn_out_buf: vec![0.0; n_head * n_tokens * d_head],
            score_buf: vec![0.0; n_head * n_tokens * n_tokens],
            proj_buf: vec![0.0; n_tokens * n_embd],
            ffn_buf: vec![0.0; n_tokens * config.n_ff],
            projected: vec![0.0; (n_patches / (config.spatial_merge_size * config.spatial_merge_size)) * config.projection_dim],
            attn_concat: vec![0.0; n_tokens * n_embd],
            residual: vec![0.0; n_tokens * n_embd],
            patch_weight_buf: Vec::new(),
            patch_weight_1_buf: None,
            project_concat_buf: Vec::new(),
            project_mm0_out: Vec::new(),
            q8_buf: vec![0u8; n_tokens * config.n_ff],
            q8_scale_buf: vec![0.0f32; n_tokens * config.n_ff / 32],
        }
    }
}

fn decode_f32_slice(data: &[u8]) -> Vec<f32> {
    let n = data.len() / 4;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push(f32_from_le_bytes(&data[i * 4..i * 4 + 4]));
    }
    out
}

fn decode_f16_slice_to_f32(data: &[u8]) -> Vec<f32> {
    let n = data.len() / 2;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let bits = u16::from_le_bytes([data[i * 2], data[i * 2 + 1]]);
        out.push(crate::ops::f16_to_f32(bits));
    }
    out
}

fn f32_from_le_bytes(b: &[u8]) -> f32 {
    f32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn layer_norm_with_bias(x: &mut [f32], w: &[f32], b: &[f32], eps: f32) {
    let n = x.len().min(w.len()).min(b.len());
    let mean = sum_f32(&x[..n]) / n as f32;
    let var = sum_sq_centered_f32(&x[..n], mean) / n as f32;
    let inv = 1.0 / (var + eps).sqrt();
    layer_norm_scale_bias(&mut x[..n], &w[..n], &b[..n], mean, inv);
}

fn layer_norm_without_bias(x: &mut [f32], w: &[f32], eps: f32) {
    let n = x.len().min(w.len());
    let mean = sum_f32(&x[..n]) / n as f32;
    let var = sum_sq_centered_f32(&x[..n], mean) / n as f32;
    let inv = 1.0 / (var + eps).sqrt();
    layer_norm_scale(&mut x[..n], &w[..n], mean, inv);
}

fn sum_f32(x: &[f32]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    {
        if crate::ops::has_avx2_fma() {
            return unsafe { sum_f32_avx2(x) };
        }
    }
    x.iter().sum()
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn sum_f32_avx2(x: &[f32]) -> f32 {
    use std::arch::x86_64::*;
    let n = x.len();
    let n8 = n / 8 * 8;
    let mut acc = _mm256_setzero_ps();
    let mut i = 0;
    while i < n8 {
        let v = _mm256_loadu_ps(x.as_ptr().add(i));
        acc = _mm256_add_ps(acc, v);
        i += 8;
    }
    let mut sum = crate::ops::hsum_ps(acc);
    while i < n { sum += x[i]; i += 1; }
    sum
}

fn sum_sq_centered_f32(x: &[f32], mean: f32) -> f32 {
    #[cfg(target_arch = "x86_64")]
    {
        if crate::ops::has_avx2_fma() {
            return unsafe { sum_sq_centered_f32_avx2(x, mean) };
        }
    }
    x.iter().map(|&v| (v - mean) * (v - mean)).sum()
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn sum_sq_centered_f32_avx2(x: &[f32], mean: f32) -> f32 {
    use std::arch::x86_64::*;
    let n = x.len();
    let n8 = n / 8 * 8;
    let vmean = _mm256_set1_ps(mean);
    let mut acc = _mm256_setzero_ps();
    let mut i = 0;
    while i < n8 {
        let v = _mm256_loadu_ps(x.as_ptr().add(i));
        let d = _mm256_sub_ps(v, vmean);
        acc = _mm256_fmadd_ps(d, d, acc);
        i += 8;
    }
    let mut sum = crate::ops::hsum_ps(acc);
    while i < n { let d = x[i] - mean; sum += d * d; i += 1; }
    sum
}

fn layer_norm_scale_bias(x: &mut [f32], w: &[f32], b: &[f32], mean: f32, inv: f32) {
    #[cfg(target_arch = "x86_64")]
    {
        if crate::ops::has_avx2_fma() {
            unsafe { layer_norm_scale_bias_avx2(x, w, b, mean, inv) };
            return;
        }
    }
    for i in 0..x.len() {
        x[i] = (x[i] - mean) * inv * w[i] + b[i];
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn layer_norm_scale_bias_avx2(x: &mut [f32], w: &[f32], b: &[f32], mean: f32, inv: f32) {
    use std::arch::x86_64::*;
    let n = x.len();
    let n8 = n / 8 * 8;
    let vmean = _mm256_set1_ps(mean);
    let vinv = _mm256_set1_ps(inv);
    let mut i = 0;
    while i < n8 {
        let vx = _mm256_loadu_ps(x.as_ptr().add(i));
        let vw = _mm256_loadu_ps(w.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));
        let d = _mm256_sub_ps(vx, vmean);
        let scaled = _mm256_mul_ps(_mm256_mul_ps(d, vinv), vw);
        _mm256_storeu_ps(x.as_mut_ptr().add(i), _mm256_add_ps(scaled, vb));
        i += 8;
    }
    while i < n { x[i] = (x[i] - mean) * inv * w[i] + b[i]; i += 1; }
}

fn layer_norm_scale(x: &mut [f32], w: &[f32], mean: f32, inv: f32) {
    #[cfg(target_arch = "x86_64")]
    {
        if crate::ops::has_avx2_fma() {
            unsafe { layer_norm_scale_avx2(x, w, mean, inv) };
            return;
        }
    }
    for i in 0..x.len() {
        x[i] = (x[i] - mean) * inv * w[i];
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn layer_norm_scale_avx2(x: &mut [f32], w: &[f32], mean: f32, inv: f32) {
    use std::arch::x86_64::*;
    let n = x.len();
    let n8 = n / 8 * 8;
    let vmean = _mm256_set1_ps(mean);
    let vinv = _mm256_set1_ps(inv);
    let mut i = 0;
    while i < n8 {
        let vx = _mm256_loadu_ps(x.as_ptr().add(i));
        let vw = _mm256_loadu_ps(w.as_ptr().add(i));
        let d = _mm256_sub_ps(vx, vmean);
        _mm256_storeu_ps(x.as_mut_ptr().add(i), _mm256_mul_ps(_mm256_mul_ps(d, vinv), vw));
        i += 8;
    }
    while i < n { x[i] = (x[i] - mean) * inv * w[i]; i += 1; }
}

fn matmul_f32_single(weight: &[f32], input: &[f32], output: &mut [f32], in_dim: usize, out_dim: usize) {
    if out_dim >= 512 {
        output.par_chunks_mut(64).enumerate().for_each(|(chunk_idx, chunk)| {
            let row_start = chunk_idx * 64;
            for (local, o) in (row_start..row_start + chunk.len()).enumerate() {
                chunk[local] = dot_f32(&weight[o * in_dim..][..in_dim], &input[..in_dim], in_dim);
            }
        });
    } else {
        for o in 0..out_dim {
            output[o] = dot_f32(&weight[o * in_dim..][..in_dim], &input[..in_dim], in_dim);
        }
    }
}

fn matmul_f32_batch(weight: &[f32], input: &[f32], output: &mut [f32], in_dim: usize, out_dim: usize, n_tokens: usize) {
    let total_rows = n_tokens * out_dim;
    if total_rows >= 512 {
        output.par_chunks_mut(64).enumerate().for_each(|(chunk_idx, chunk)| {
            let global_row = chunk_idx * 64;
            for (local, row) in (global_row..global_row + chunk.len()).enumerate() {
                let t = row / out_dim;
                let o = row % out_dim;
                let inp_off = t * in_dim;
                chunk[local] = dot_f32(&weight[o * in_dim..][..in_dim], &input[inp_off..inp_off + in_dim], in_dim);
            }
        });
    } else {
        for t in 0..n_tokens {
            let inp_off = t * in_dim;
            let out_off = t * out_dim;
            for o in 0..out_dim {
                output[out_off + o] = dot_f32(&weight[o * in_dim..][..in_dim], &input[inp_off..inp_off + in_dim], in_dim);
            }
        }
    }
}

fn matmul_f16_f32_single(weight_f16: &[u8], input: &[f32], output: &mut [f32], in_dim: usize, out_dim: usize) {
    let n_half = in_dim / 2;
    let u16_ptr = weight_f16.as_ptr() as *const u16;
    let w_u16: &[u16] = unsafe { std::slice::from_raw_parts(u16_ptr, weight_f16.len() / 2) };
    for o in 0..out_dim {
        let row_off = o * in_dim;
        let row_u16 = &w_u16[row_off..row_off + n_half];
        output[o] = dot_f16_f32(&input[..in_dim], row_u16, in_dim);
    }
}

fn gelu(x: f32) -> f32 {
    0.5 * x * (1.0 + ((2.0 / std::f32::consts::PI).sqrt() * (x + 0.044715 * x * x * x)).tanh())
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn attention_dot_avx2(q: *const f32, k: *const f32, d: usize) -> f32 {
    use std::arch::x86_64::*;
    let n8 = d / 8 * 8;
    let mut acc = _mm256_setzero_ps();
    let mut i = 0;
    while i < n8 {
        let vq = _mm256_loadu_ps(q.add(i));
        let vk = _mm256_loadu_ps(k.add(i));
        acc = _mm256_fmadd_ps(vq, vk, acc);
        i += 8;
    }
    let mut sum = crate::ops::hsum_ps(acc);
    while i < d {
        sum += *q.add(i) * *k.add(i);
        i += 1;
    }
    sum
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn attention_qk_avx2(q: *const f32, k_base: *const f32, scores: &mut [f32], n_tokens: usize, d_head: usize, scale: f32) {
    use std::arch::x86_64::*;
    let n8 = d_head / 8 * 8;
    let d_stride = d_head as isize;
    let tile4 = n_tokens / 4 * 4;
    let mut s = 0;
    while s < tile4 {
        let k0 = k_base.offset((s as isize) * d_stride);
        let k1 = k_base.offset(((s + 1) as isize) * d_stride);
        let k2 = k_base.offset(((s + 2) as isize) * d_stride);
        let k3 = k_base.offset(((s + 3) as isize) * d_stride);
        let mut cv0 = _mm256_setzero_ps();
        let mut cv1 = _mm256_setzero_ps();
        let mut cv2 = _mm256_setzero_ps();
        let mut cv3 = _mm256_setzero_ps();
        let mut i = 0;
        while i < n8 {
            let vq = _mm256_loadu_ps(q.add(i));
            cv0 = _mm256_fmadd_ps(vq, _mm256_loadu_ps(k0.add(i)), cv0);
            cv1 = _mm256_fmadd_ps(vq, _mm256_loadu_ps(k1.add(i)), cv1);
            cv2 = _mm256_fmadd_ps(vq, _mm256_loadu_ps(k2.add(i)), cv2);
            cv3 = _mm256_fmadd_ps(vq, _mm256_loadu_ps(k3.add(i)), cv3);
            i += 8;
        }
        let mut r0 = crate::ops::hsum_ps(cv0);
        let mut r1 = crate::ops::hsum_ps(cv1);
        let mut r2 = crate::ops::hsum_ps(cv2);
        let mut r3 = crate::ops::hsum_ps(cv3);
        i = n8;
        while i < d_head {
            let qv = *q.add(i);
            r0 += qv * *k0.add(i);
            r1 += qv * *k1.add(i);
            r2 += qv * *k2.add(i);
            r3 += qv * *k3.add(i);
            i += 1;
        }
        scores[s] = r0 * scale;
        scores[s + 1] = r1 * scale;
        scores[s + 2] = r2 * scale;
        scores[s + 3] = r3 * scale;
        s += 4;
    }
    while s < n_tokens {
        let k_ptr = k_base.offset((s as isize) * d_stride);
        let dot = attention_dot_avx2(q, k_ptr, d_head);
        scores[s] = dot * scale;
        s += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn attn_scaled_add_avx2(out: &mut [f32], v: *const f32, scale: f32, d: usize) {
    use std::arch::x86_64::*;
    let vs = _mm256_set1_ps(scale);
    let n8 = d / 8 * 8;
    let out_ptr = out.as_mut_ptr();
    let mut i = 0;
    while i < n8 {
        let vv = _mm256_loadu_ps(v.add(i));
        let ov = _mm256_loadu_ps(out_ptr.add(i));
        _mm256_storeu_ps(out_ptr.add(i), _mm256_fmadd_ps(vs, vv, ov));
        i += 8;
    }
    while i < d {
        *out_ptr.add(i) += scale * *v.add(i);
        i += 1;
    }
}
