use std::collections::HashMap;
use std::cell::UnsafeCell;
use std::sync::atomic::Ordering;
use std::io::{self, Write};
use std::time::Instant;
use thread_pool::ComputePool;

use rust_model_inference::*;

#[derive(Clone, Copy, PartialEq)]
enum KvFormat { F16, F32 }

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut model_path = String::new();
    let mut prompt = String::new();
    let mut max_tokens = 128usize;
    let mut temperature = 0.6f32;

    let mut n_threads = 0usize;
    let mut dump_logits = false;
    let mut bench = false;
    let mut profile = false;
    let mut kv_format = KvFormat::F16;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--model" => { if i + 1 < args.len() { model_path = args[i + 1].clone(); i += 1; } }
            "--prompt" => { if i + 1 < args.len() { prompt = args[i + 1].clone(); i += 1; } }
            "--max-tokens" => { if i + 1 < args.len() { max_tokens = args[i + 1].parse().unwrap_or(128); i += 1; } }
            "--n-gen" => { if i + 1 < args.len() { max_tokens = args[i + 1].parse().unwrap_or(128); i += 1; } }
            "--temp" => { if i + 1 < args.len() { temperature = args[i + 1].parse().unwrap_or(0.6); i += 1; } }
            "--threads" => { if i + 1 < args.len() { n_threads = args[i + 1].parse().unwrap_or(0); i += 1; } }
            "--dump-logits" => { dump_logits = true; }
            "--bench" => { bench = true; }
            "--profile" => { profile = true; }
            "--kv-cache" => { if i + 1 < args.len() { kv_format = match args[i + 1].as_str() { "f32" => KvFormat::F32, _ => KvFormat::F16 }; i += 1; } }
            _ => {}
        }
        i += 1;
    }

    if !model_path.is_empty() && !prompt.is_empty() {
        if dump_logits {
            run_dump_logits(&model_path, &prompt, max_tokens, n_threads, kv_format);
        } else {
            run_inference(&model_path, &prompt, max_tokens, temperature, n_threads, bench, profile, kv_format);
        }
    } else if !model_path.is_empty() {
        run_interactive(&model_path, max_tokens, temperature, n_threads);
    } else {
        run_self_test();
    }
}

struct LayerWeights<'a> {
    attn_norm: Vec<f32>,
    ffn_norm: Vec<f32>,
    q_norm: Option<Vec<f32>>,
    k_norm: Option<Vec<f32>>,
    wq: &'a [u8],
    wk: &'a [u8],
    wv: &'a [u8],
    wo: &'a [u8],
    w_gate: &'a [u8],
    w_up: &'a [u8],
    w_down: &'a [u8],
}

fn run_dump_logits(model_path: &str, prompt: &str, max_tokens: usize, n_threads_arg: usize, kv_format: KvFormat) {
    let loader = GGUFLoader::from_file(model_path).expect("Failed to load GGUF");
    let config = loader.model_config().expect("Failed to parse model config");

    let mut bin_out = std::fs::File::create("/tmp/rust_logits.bin").expect("create bin");

    let arch = loader.metadata("general.architecture").and_then(|v| v.to_string_val()).unwrap_or_default();
    let is_qwen3 = arch == "qwen3";

    let mut tokenizer = BPETokenizer::from_gguf_metadata(|k| loader.metadata(k).cloned())
        .expect("Failed to init tokenizer");

    let special_tokens = detect_special_tokens(&loader, &tokenizer);
    tokenizer.set_special_tokens(special_tokens.clone());

    let max_ctx = 512usize.min(config.n_ctx);
    let n_embd = config.n_embd;
    let n_layer = config.n_layer;
    let n_head = config.n_head;
    let n_head_kv = config.n_head_kv;
    let n_embd_head = config.n_embd_head;
    let n_embd_head_k = if let Some(v) = loader.metadata(&format!("{}.attention.key_length", arch)) {
        v.to_u64().unwrap_or(n_embd_head as u64) as usize
    } else { n_embd_head };
    let n_embd_head_v = if let Some(v) = loader.metadata(&format!("{}.attention.value_length", arch)) {
        v.to_u64().unwrap_or(n_embd_head as u64) as usize
    } else { n_embd_head };
    let n_embd_q = n_head * n_embd_head_k;
    let n_embd_gqa = n_head_kv * n_embd_head_v;
    let n_ff = config.n_ff;
    let eps = config.norm_eps;
    let freq_base = config.rope_freq_base;

    let output_norm = get_f32_tensor(&loader, "output_norm.weight", n_embd);
    let embd_weight = loader.tensor_slice("token_embd.weight").expect("no embd");
    let output_weight = loader.tensor_slice("output.weight").unwrap_or(embd_weight);

    let layers: Vec<LayerWeights> = (0..n_layer).map(|l| LayerWeights {
        attn_norm: get_f32_tensor(&loader, &format!("blk.{}.attn_norm.weight", l), n_embd),
        ffn_norm: get_f32_tensor(&loader, &format!("blk.{}.ffn_norm.weight", l), n_embd),
        q_norm: if is_qwen3 { Some(get_f32_tensor(&loader, &format!("blk.{}.attn_q_norm.weight", l), n_embd_head_k)) } else { None },
        k_norm: if is_qwen3 { Some(get_f32_tensor(&loader, &format!("blk.{}.attn_k_norm.weight", l), n_embd_head_k)) } else { None },
        wq: loader.tensor_slice(&format!("blk.{}.attn_q.weight", l)).unwrap(),
        wk: loader.tensor_slice(&format!("blk.{}.attn_k.weight", l)).unwrap(),
        wv: loader.tensor_slice(&format!("blk.{}.attn_v.weight", l)).unwrap(),
        wo: loader.tensor_slice(&format!("blk.{}.attn_output.weight", l)).unwrap(),
        w_gate: loader.tensor_slice(&format!("blk.{}.ffn_gate.weight", l)).unwrap(),
        w_up: loader.tensor_slice(&format!("blk.{}.ffn_up.weight", l)).unwrap(),
        w_down: loader.tensor_slice(&format!("blk.{}.ffn_down.weight", l)).unwrap(),
    }).collect();

    eprintln!("Model: {} | n_embd={} n_layer={} n_head={} n_head_kv={} n_ff={}", arch, n_embd, n_layer, n_head, n_head_kv, n_ff);

    let prompt_tokens = tokenizer.encode(prompt);
    eprintln!("Tokenized to {} tokens: {:?}", prompt_tokens.len(), prompt_tokens);

    let vocab = tokenizer.vocab_size();

    let n_threads = if n_threads_arg > 0 { n_threads_arg } else { 1 };

    {
        use std::io::Write as IoWrite;
        let header: [i32; 3] = [vocab as i32, prompt_tokens.len() as i32, max_tokens as i32];
        bin_out.write_all(unsafe { std::slice::from_raw_parts(header.as_ptr() as *const u8, 12) }).unwrap();
        let pt: Vec<i32> = prompt_tokens.iter().map(|&t| t as i32).collect();
        bin_out.write_all(unsafe { std::slice::from_raw_parts(pt.as_ptr() as *const u8, pt.len() * 4) }).unwrap();
    }

    let k_cache_f16 = UnsafeCell::new(vec![0u16; n_layer * max_ctx * n_embd_gqa].into_boxed_slice());
    let v_cache_f16 = UnsafeCell::new(vec![0u16; n_layer * max_ctx * n_embd_gqa].into_boxed_slice());
    let k_cache_f32 = UnsafeCell::new(vec![0.0f32; n_layer * max_ctx * n_embd_gqa].into_boxed_slice());
    let v_cache_f32 = UnsafeCell::new(vec![0.0f32; n_layer * max_ctx * n_embd_gqa].into_boxed_slice());
    let x = UnsafeCell::new(vec![0.0f32; n_embd]);
    let normed = UnsafeCell::new(vec![0.0f32; n_embd]);
    let q = UnsafeCell::new(vec![0.0f32; n_embd_q]);
    let k_new = UnsafeCell::new(vec![0.0f32; n_embd_gqa]);
    let v_new = UnsafeCell::new(vec![0.0f32; n_embd_gqa]);
    let attn_out = UnsafeCell::new(vec![0.0f32; n_embd_q]);
    let attn_proj = UnsafeCell::new(vec![0.0f32; n_embd]);
    let down_buf = UnsafeCell::new(vec![0.0f32; n_embd]);
    let gate_buf = UnsafeCell::new(vec![0.0f32; n_ff]);
    let up_buf = UnsafeCell::new(vec![0.0f32; n_ff]);
    let logits = UnsafeCell::new(vec![0.0f32; vocab]);
    let attn_barrier = UnsafeCell::new(std::sync::atomic::AtomicI32::new(0));

    let max_n_in = n_embd_q.max(n_ff);
    let q8_buf = UnsafeCell::new(vec![0u8; max_n_in]);
    let scale_buf = UnsafeCell::new(vec![0.0f32; max_n_in / 32]);

    let input_tokens = prompt_tokens.clone();
    let scores = UnsafeCell::new(vec![0.0f32; n_threads * max_ctx]);
    let pool = std::sync::Arc::new(thread_pool::ComputePool::new(n_threads));

    let group_size = n_head / n_head_kv;
    let kq_scale = 1.0f32 / (n_embd_head_k as f32).sqrt();

    let mut generated_tokens: Vec<u32> = Vec::new();
    let mut all_tokens: Vec<u32> = input_tokens.clone();

    for step in 0..(input_tokens.len() + max_tokens) {
        let token_id = if step < input_tokens.len() {
            input_tokens[step]
        } else {
            *generated_tokens.last().unwrap_or(&0)
        };

        let pos = step;

        embedding_lookup_q8_0(embd_weight, token_id, n_embd, unsafe { &mut *x.get() });

        for layer in 0..n_layer {
            let lw = &layers[layer];

            unsafe {
            let x_ptr = x.get();
            let normed_ptr = normed.get();
            let q_ptr = q.get();
            let k_ptr = k_new.get();
            let v_ptr = v_new.get();
            let attn_out_ptr = attn_out.get();
            let attn_proj_ptr = attn_proj.get();
            let down_buf_ptr = down_buf.get();
            let scores_ptr = scores.get();
            let gate_buf_ptr = gate_buf.get();
            let up_buf_ptr = up_buf.get();
            let q8_buf_ptr = q8_buf.get();
            let scale_buf_ptr = scale_buf.get();
             let k_cache_f16_ptr = k_cache_f16.get();
             let v_cache_f16_ptr = v_cache_f16.get();
             let attn_barrier_ptr = attn_barrier.get();

             let k_cache_f32_ptr = k_cache_f32.get();
             let v_cache_f32_ptr = v_cache_f32.get();
            let x = &mut *x_ptr;
            let normed = &mut *normed_ptr;
            let q8_buf = &mut *q8_buf_ptr;
            let scale_buf = &mut *scale_buf_ptr;

            rms_norm(x, &lw.attn_norm, normed, eps);
            quantize_q8_0_into(normed, n_embd, &mut q8_buf[..n_embd], &mut scale_buf[..n_embd / 32]);

            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();
            let q_norm = lw.q_norm.as_deref();
            let k_norm = lw.k_norm.as_deref();

            let attn_barrier_ref = &*attn_barrier_ptr;
            attn_barrier_ref.store(0, Ordering::SeqCst);

            pool.compute(move |ith: usize, nth: usize| {
                let q8 = std::slice::from_raw_parts(q8, n_embd);
                let sc = std::slice::from_raw_parts(sc, n_embd / 32);
                let q = &mut *q_ptr;
                let k_new = &mut *k_ptr;
                let v_new = &mut *v_ptr;

                matmul_q8_0_quantized_parallel_rows(lw.wq, q8, sc, q, n_embd, n_embd_q, ith, nth);
                matmul_q8_0_quantized_parallel_rows(lw.wk, q8, sc, k_new, n_embd, n_embd_gqa, ith, nth);
                matmul_q8_0_quantized_parallel_rows(lw.wv, q8, sc, v_new, n_embd, n_embd_gqa, ith, nth);
            });

            pool.compute(move |ith: usize, nth: usize| {
                let q = &mut *q_ptr;
                let k_new = &mut *k_ptr;
                let v_new = &mut *v_ptr;
                let attn_out = &mut *attn_out_ptr;
                let scores = &mut *scores_ptr;
                 let h_start = ith * n_head / nth;
                 let h_end = (ith + 1) * n_head / nth;
                 let kv_h_start = h_start / group_size;
                 let kv_h_end = (h_end + group_size - 1) / group_size;

                 if let (Some(qn), Some(kn)) = (q_norm, k_norm) {
                     for h in h_start..h_end {
                         rms_norm_inplace(&mut q[h * n_embd_head_k..(h + 1) * n_embd_head_k], qn, eps);
                     }
                     for h in kv_h_start..kv_h_end {
                         rms_norm_inplace(&mut k_new[h * n_embd_head_k..(h + 1) * n_embd_head_k], kn, eps);
                     }
                 }

                 for h in h_start..h_end {
                     rope_neox(&mut q[h * n_embd_head_k..(h + 1) * n_embd_head_k], pos, n_embd_head_k, freq_base);
                 }
                 for h in kv_h_start..kv_h_end {
                     rope_neox(&mut k_new[h * n_embd_head_k..(h + 1) * n_embd_head_k], pos, n_embd_head_v, freq_base);
                 }

                 let kb = layer * max_ctx * n_embd_gqa;

                if kv_format == KvFormat::F16 {
                     let k_cache = &mut *k_cache_f16_ptr;
                     let v_cache = &mut *v_cache_f16_ptr;
                      for h in kv_h_start..kv_h_end {
                          let off = h * n_embd_head_k;
                          f32_slice_to_f16(&k_new[off..off + n_embd_head_k], &mut k_cache[kb + pos * n_embd_gqa + off..kb + pos * n_embd_gqa + off + n_embd_head_k]);
                          f32_slice_to_f16(&v_new[off..off + n_embd_head_v], &mut v_cache[kb + pos * n_embd_gqa + off..kb + pos * n_embd_gqa + off + n_embd_head_v]);
                      }
                      attn_barrier_ref.fetch_add(1, Ordering::SeqCst);
                      while attn_barrier_ref.load(Ordering::Acquire) < nth as i32 {
                          std::hint::spin_loop();
                      }
                      for h in h_start..h_end {
                          let kv_h = h / group_size;
                          let q_off = h * n_embd_head_k;
                          let n_cached = pos + 1;
                          let s_off = ith * max_ctx;
                          for t in 0..n_cached {
                              scores[s_off + t] = dot_f16_f32(
                                 &q[q_off..q_off + n_embd_head_k],
                                 &k_cache[kb + t * n_embd_gqa + kv_h * n_embd_head_v..kb + t * n_embd_gqa + kv_h * n_embd_head_v + n_embd_head_k],
                                 n_embd_head_k,
                             ) * kq_scale;
                         }
                         softmax(&mut scores[s_off..s_off + n_cached]);
                         for d in 0..n_embd_head_v {
                             let mut val = 0.0f32;
                             for t in 0..n_cached {
                                 val += scores[s_off + t] * f16_to_f32(v_cache[kb + t * n_embd_gqa + kv_h * n_embd_head_v + d]);
                             }
                             attn_out[h * n_embd_head_v + d] = val;
                         }
                     }
                 } else {
                     let k_cache = &mut *k_cache_f32_ptr;
                     let v_cache = &mut *v_cache_f32_ptr;
                      for h in kv_h_start..kv_h_end {
                          let off = h * n_embd_head_k;
                          k_cache[kb + pos * n_embd_gqa + off..kb + pos * n_embd_gqa + off + n_embd_head_k]
                              .copy_from_slice(&k_new[off..off + n_embd_head_k]);
                          v_cache[kb + pos * n_embd_gqa + off..kb + pos * n_embd_gqa + off + n_embd_head_v]
                              .copy_from_slice(&v_new[off..off + n_embd_head_v]);
                      }
                      attn_barrier_ref.fetch_add(1, Ordering::SeqCst);
                      while attn_barrier_ref.load(Ordering::Acquire) < nth as i32 {
                          std::hint::spin_loop();
                      }
                      for h in h_start..h_end {
                         let kv_h = h / group_size;
                         let q_off = h * n_embd_head_k;
                         let n_cached = pos + 1;
                         let s_off = ith * max_ctx;
                         for t in 0..n_cached {
                             scores[s_off + t] = dot_f32(
                                 &q[q_off..q_off + n_embd_head_k],
                                 &k_cache[kb + t * n_embd_gqa + kv_h * n_embd_head_v..kb + t * n_embd_gqa + kv_h * n_embd_head_v + n_embd_head_k],
                                 n_embd_head_k,
                             ) * kq_scale;
                         }
                         softmax(&mut scores[s_off..s_off + n_cached]);
                         for d in 0..n_embd_head_v {
                             let mut val = 0.0f32;
                             for t in 0..n_cached {
                                 val += scores[s_off + t] * v_cache[kb + t * n_embd_gqa + kv_h * n_embd_head_v + d];
                             }
                             attn_out[h * n_embd_head_v + d] = val;
                         }
                     }
                 }
            });

            let attn_out = &mut *attn_out_ptr;
            quantize_q8_0_into(attn_out, n_embd_q, &mut q8_buf[..n_embd_q], &mut scale_buf[..n_embd_q / 32]);

            let q8 = q8_buf[..n_embd_q].as_ptr();
            let sc = scale_buf[..n_embd_q / 32].as_ptr();
            pool.compute(move |ith: usize, nth: usize| {
                let q8 = std::slice::from_raw_parts(q8, n_embd_q);
                let sc = std::slice::from_raw_parts(sc, n_embd_q / 32);
                let attn_proj = &mut *attn_proj_ptr;
                matmul_q8_0_quantized_parallel_rows(lw.wo, q8, sc, attn_proj, n_embd_q, n_embd, ith, nth);
            });

            let attn_proj = &mut *attn_proj_ptr;
            let x = &mut *x_ptr;
            let normed = &mut *normed_ptr;
            for i in 0..n_embd { x[i] += attn_proj[i]; }

            rms_norm(x, &lw.ffn_norm, normed, eps);
            quantize_q8_0_into(normed, n_embd, &mut q8_buf[..n_embd], &mut scale_buf[..n_embd / 32]);

            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();
            pool.compute(move |ith: usize, nth: usize| {
                let q8 = std::slice::from_raw_parts(q8, n_embd);
                let sc = std::slice::from_raw_parts(sc, n_embd / 32);
                let gate_buf = &mut *gate_buf_ptr;
                let up_buf = &mut *up_buf_ptr;
                matmul_q8_0_quantized_parallel_rows(lw.w_gate, q8, sc, gate_buf, n_embd, n_ff, ith, nth);
                matmul_q8_0_quantized_parallel_rows(lw.w_up, q8, sc, up_buf, n_embd, n_ff, ith, nth);

                let rows_per = n_ff / nth;
                let r_start = ith * rows_per;
                let r_end = if ith == nth - 1 { n_ff } else { r_start + rows_per };
                for i in r_start..r_end {
                    gate_buf[i] = silu(gate_buf[i]) * up_buf[i];
                }
            });

            let gate_buf = &mut *gate_buf_ptr;
            quantize_q8_0_into(gate_buf, n_ff, &mut q8_buf[..n_ff], &mut scale_buf[..n_ff / 32]);

            let q8 = q8_buf[..n_ff].as_ptr();
            let sc = scale_buf[..n_ff / 32].as_ptr();
            pool.compute(move |ith: usize, nth: usize| {
                let q8 = std::slice::from_raw_parts(q8, n_ff);
                let sc = std::slice::from_raw_parts(sc, n_ff / 32);
                let down_buf = &mut *down_buf_ptr;
                matmul_q8_0_quantized_parallel_rows(lw.w_down, q8, sc, down_buf, n_ff, n_embd, ith, nth);
            });

            let down_buf = &mut *down_buf_ptr;
            let x = &mut *x_ptr;
            for i in 0..n_embd { x[i] += down_buf[i]; }
            }
        }

        unsafe {
        let x = &mut *x.get();
        let normed = &mut *normed.get();
        let logits_ptr = logits.get();
        let q8_buf = &mut *q8_buf.get();
        let scale_buf = &mut *scale_buf.get();

        rms_norm(x, &output_norm, normed, eps);
        quantize_q8_0_into(normed, n_embd, &mut q8_buf[..n_embd], &mut scale_buf[..n_embd / 32]);

        let q8 = q8_buf[..n_embd].as_ptr();
        let sc = scale_buf[..n_embd / 32].as_ptr();
        pool.compute(move |ith: usize, nth: usize| {
            let q8 = std::slice::from_raw_parts(q8, n_embd);
            let sc = std::slice::from_raw_parts(sc, n_embd / 32);
            let logits = &mut *logits_ptr;
            matmul_q8_0_quantized_parallel_rows(output_weight, q8, sc, logits, n_embd, vocab, ith, nth);
        });
        }

        if step < input_tokens.len() - 1 { continue; }

        let logits = unsafe { &*logits.get() };

        let mut best_idx = 0usize;
        let mut best_val = logits[0];
        for (i, &v) in logits.iter().enumerate().skip(1) {
            if v > best_val { best_val = v; best_idx = i; }
        }

        println!("=== Step {} token={} ===", step, token_id);
        println!("  argmax={} logit={:.8}", best_idx, best_val);

        let mut indexed: Vec<(usize, f32)> = logits.iter().enumerate().map(|(i, &v)| (i, v)).collect();
        indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        for k in 0..5 {
            println!("  [{}] token={} logit={:.8}", k, indexed[k].0, indexed[k].1);
        }

        let sum: f32 = logits.iter().sum();
        let sq_sum: f32 = logits.iter().map(|&v| v * v).sum();
        let mn = logits.iter().cloned().fold(f32::INFINITY, f32::min);
        let mx = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let mean = sum / vocab as f32;
        let std = (sq_sum / vocab as f32 - mean * mean).sqrt();
        println!("  stats: sum={:.6} mean={:.6} std={:.6} min={:.6} max={:.6}", sum, mean, std, mn, mx);

        {
            use std::io::Write as IoWrite;
            bin_out.write_all(unsafe { std::slice::from_raw_parts(logits.as_ptr() as *const u8, vocab * 4) }).unwrap();
        }

        let chosen = best_idx as u32;
        if generated_tokens.len() >= max_tokens { break; }
        generated_tokens.push(chosen);
        all_tokens.push(chosen);
    }
}

fn run_inference(model_path: &str, prompt: &str, max_tokens: usize, temperature: f32, n_threads_arg: usize, bench: bool, profile: bool, kv_format: KvFormat) {
    let t0 = Instant::now();
    println!("Loading {} ...", model_path);
    let loader = GGUFLoader::from_file(model_path).expect("Failed to load GGUF");
    let config = loader.model_config().expect("Failed to parse model config");

    let arch = loader.metadata("general.architecture").and_then(|v| v.to_string_val()).unwrap_or_default();
    let is_qwen3 = arch == "qwen3";

    let mut tokenizer = BPETokenizer::from_gguf_metadata(|k| loader.metadata(k).cloned())
        .expect("Failed to init tokenizer");

    let special_tokens = detect_special_tokens(&loader, &tokenizer);
    tokenizer.set_special_tokens(special_tokens.clone());

    let max_ctx = 512usize.min(config.n_ctx);
    let n_embd = config.n_embd;
    let n_layer = config.n_layer;
    let n_head = config.n_head;
    let n_head_kv = config.n_head_kv;
    let n_embd_head = config.n_embd_head;
    let n_embd_head_k = if let Some(v) = loader.metadata(&format!("{}.attention.key_length", arch)) {
        v.to_u64().unwrap_or(n_embd_head as u64) as usize
    } else { n_embd_head };
    let n_embd_head_v = if let Some(v) = loader.metadata(&format!("{}.attention.value_length", arch)) {
        v.to_u64().unwrap_or(n_embd_head as u64) as usize
    } else { n_embd_head };
    let n_embd_q = n_head * n_embd_head_k;
    let n_embd_gqa = n_head_kv * n_embd_head_v;
    let n_ff = config.n_ff;
    let eps = config.norm_eps;
    let freq_base = config.rope_freq_base;

    let output_norm = get_f32_tensor(&loader, "output_norm.weight", n_embd);
    let embd_weight = loader.tensor_slice("token_embd.weight").expect("no embd");
    let output_weight = loader.tensor_slice("output.weight").unwrap_or(embd_weight);

    let layers: Vec<LayerWeights> = (0..n_layer).map(|l| LayerWeights {
        attn_norm: get_f32_tensor(&loader, &format!("blk.{}.attn_norm.weight", l), n_embd),
        ffn_norm: get_f32_tensor(&loader, &format!("blk.{}.ffn_norm.weight", l), n_embd),
        q_norm: if is_qwen3 { Some(get_f32_tensor(&loader, &format!("blk.{}.attn_q_norm.weight", l), n_embd_head_k)) } else { None },
        k_norm: if is_qwen3 { Some(get_f32_tensor(&loader, &format!("blk.{}.attn_k_norm.weight", l), n_embd_head_k)) } else { None },
        wq: loader.tensor_slice(&format!("blk.{}.attn_q.weight", l)).unwrap(),
        wk: loader.tensor_slice(&format!("blk.{}.attn_k.weight", l)).unwrap(),
        wv: loader.tensor_slice(&format!("blk.{}.attn_v.weight", l)).unwrap(),
        wo: loader.tensor_slice(&format!("blk.{}.attn_output.weight", l)).unwrap(),
        w_gate: loader.tensor_slice(&format!("blk.{}.ffn_gate.weight", l)).unwrap(),
        w_up: loader.tensor_slice(&format!("blk.{}.ffn_up.weight", l)).unwrap(),
        w_down: loader.tensor_slice(&format!("blk.{}.ffn_down.weight", l)).unwrap(),
    }).collect();

    let load_ms = t0.elapsed().as_millis();
    println!("Model: {} | n_embd={} n_layer={} n_head={} n_head_kv={} n_ff={} | loaded in {}ms",
        arch, n_embd, n_layer, n_head, n_head_kv, n_ff, load_ms);

    let has_chat = special_tokens.contains_key("im_start") && special_tokens.contains_key("im_end");
    if has_chat {
        println!("  chat: im_start={} im_end={}", special_tokens["im_start"], special_tokens["im_end"]);
    }

    let k_cache_f16 = UnsafeCell::new(vec![0u16; n_layer * max_ctx * n_embd_gqa].into_boxed_slice());
    let v_cache_f16 = UnsafeCell::new(vec![0u16; n_layer * max_ctx * n_embd_gqa].into_boxed_slice());
    let k_cache_f32 = UnsafeCell::new(vec![0.0f32; n_layer * max_ctx * n_embd_gqa].into_boxed_slice());
    let v_cache_f32 = UnsafeCell::new(vec![0.0f32; n_layer * max_ctx * n_embd_gqa].into_boxed_slice());

    let ffn_barrier = UnsafeCell::new(std::sync::atomic::AtomicI32::new(0));
    let attn_barrier = UnsafeCell::new(std::sync::atomic::AtomicI32::new(0));
    let x = UnsafeCell::new(vec![0.0f32; n_embd]);
    let normed = UnsafeCell::new(vec![0.0f32; n_embd]);
    let q = UnsafeCell::new(vec![0.0f32; n_embd_q]);
    let k_new = UnsafeCell::new(vec![0.0f32; n_embd_gqa]);
    let v_new = UnsafeCell::new(vec![0.0f32; n_embd_gqa]);
    let attn_out = UnsafeCell::new(vec![0.0f32; n_embd_q]);
    let attn_proj = UnsafeCell::new(vec![0.0f32; n_embd]);
    let down_buf = UnsafeCell::new(vec![0.0f32; n_embd]);
    let gate_buf = UnsafeCell::new(vec![0.0f32; n_ff]);
    let up_buf = UnsafeCell::new(vec![0.0f32; n_ff]);
    let vocab = tokenizer.vocab_size();
    let logits = UnsafeCell::new(vec![0.0f32; vocab]);

    let max_n_in = n_embd_q.max(n_ff);
    let q8_buf = UnsafeCell::new(vec![0u8; max_n_in]);
    let scale_buf = UnsafeCell::new(vec![0.0f32; max_n_in / 32]);

    let prompt_tokens = tokenizer.encode(prompt);
    let n_threads = if n_threads_arg > 0 { n_threads_arg } else { std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) };
    let scores = UnsafeCell::new(vec![0.0f32; n_threads * max_ctx]);
    let pool = std::sync::Arc::new(thread_pool::ComputePool::new(n_threads));
    eprintln!("compute pool: {} threads", pool.n_threads());
    println!("Prompt: {} ({} tokens)", prompt, prompt_tokens.len());

    let mut input_tokens: Vec<u32> = Vec::new();
    if has_chat && !bench {
        input_tokens.push(special_tokens["im_start"]);
        input_tokens.extend_from_slice(&tokenizer.encode("user\n"));
        input_tokens.extend_from_slice(&prompt_tokens);
        input_tokens.push(special_tokens["im_end"]);
        input_tokens.push(special_tokens["im_start"]);
        input_tokens.extend_from_slice(&tokenizer.encode("assistant\n"));
    } else {
        input_tokens.extend_from_slice(&prompt_tokens);
    }

    let eos_id = tokenizer.eos_id();
    let im_end_id = *special_tokens.get("im_end").unwrap_or(&u32::MAX);
    let mut generated_tokens: Vec<u32> = Vec::new();
    let mut all_tokens: Vec<u32> = input_tokens.clone();

    let group_size = n_head / n_head_kv;
    let kq_scale = 1.0f32 / (n_embd_head_k as f32).sqrt();

    let mut t_norm: f64 = 0.0;
    let mut t_quant: f64 = 0.0;
    let mut t_qkv: f64 = 0.0;
    let mut t_wo: f64 = 0.0;
    let mut t_ffn1: f64 = 0.0;
    let mut t_silu: f64 = 0.0;
    let mut t_down: f64 = 0.0;
    let mut t_logits: f64 = 0.0;

    print!("Output: ");
    io::stdout().flush().unwrap();

    let t_infer = Instant::now();

        for step in 0..(input_tokens.len() + max_tokens) {
        let token_id = if step < input_tokens.len() {
            input_tokens[step]
        } else {
            *generated_tokens.last().unwrap_or(&0)
        };

        let pos = step;

        embedding_lookup_q8_0(embd_weight, token_id, n_embd, unsafe { &mut *x.get() });

        for layer in 0..n_layer {
            let lw = &layers[layer];

            unsafe {
            let x_ptr = x.get();
            let normed_ptr = normed.get();
            let q_ptr = q.get();
            let k_ptr = k_new.get();
            let v_ptr = v_new.get();
            let attn_out_ptr = attn_out.get();
            let attn_proj_ptr = attn_proj.get();
            let down_buf_ptr = down_buf.get();
            let gate_buf_ptr = gate_buf.get();
            let up_buf_ptr = up_buf.get();
            let q8_buf_ptr = q8_buf.get();
            let scale_buf_ptr = scale_buf.get();
            let k_cache_f16_ptr = k_cache_f16.get();
            let v_cache_f16_ptr = v_cache_f16.get();
            let ffn_barrier_ptr = ffn_barrier.get();
            let attn_barrier_ptr = attn_barrier.get();
            let k_cache_f32_ptr = k_cache_f32.get();
            let v_cache_f32_ptr = v_cache_f32.get();

            let x = &mut *x_ptr;
            let normed = &mut *normed_ptr;
            let q8_buf = &mut *q8_buf_ptr;
            let scale_buf = &mut *scale_buf_ptr;

            let t0 = Instant::now();
            rms_norm(x, &lw.attn_norm, normed, eps);
            quantize_q8_0_into(normed, n_embd, &mut q8_buf[..n_embd], &mut scale_buf[..n_embd / 32]);
            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();
            let q_norm = lw.q_norm.as_deref();
            let k_norm = lw.k_norm.as_deref();

            let attn_barrier_ref = &*attn_barrier_ptr;
            attn_barrier_ref.store(0, Ordering::SeqCst);

            pool.compute(move |ith: usize, nth: usize| {
                let q8 = std::slice::from_raw_parts(q8, n_embd);
                let sc = std::slice::from_raw_parts(sc, n_embd / 32);
                let q = &mut *q_ptr;
                let k_new = &mut *k_ptr;
                let v_new = &mut *v_ptr;
                let attn_out = &mut *attn_out_ptr;

                matmul_q8_0_quantized_parallel_rows(lw.wq, q8, sc, q, n_embd, n_embd_q, ith, nth);
                matmul_q8_0_quantized_parallel_rows(lw.wk, q8, sc, k_new, n_embd, n_embd_gqa, ith, nth);
                matmul_q8_0_quantized_parallel_rows(lw.wv, q8, sc, v_new, n_embd, n_embd_gqa, ith, nth);

                let h_start = ith * n_head / nth;
                let h_end = (ith + 1) * n_head / nth;
                let kv_h_start = h_start / group_size;
                let kv_h_end = (h_end + group_size - 1) / group_size;

                if let (Some(qn), Some(kn)) = (q_norm, k_norm) {
                    for h in h_start..h_end {
                        rms_norm_inplace(&mut q[h * n_embd_head_k..(h + 1) * n_embd_head_k], qn, eps);
                    }
                    for h in kv_h_start..kv_h_end {
                        rms_norm_inplace(&mut k_new[h * n_embd_head_k..(h + 1) * n_embd_head_k], kn, eps);
                    }
                }

                for h in h_start..h_end {
                    rope_neox(&mut q[h * n_embd_head_k..(h + 1) * n_embd_head_k], pos, n_embd_head_k, freq_base);
                }
                for h in kv_h_start..kv_h_end {
                    rope_neox(&mut k_new[h * n_embd_head_k..(h + 1) * n_embd_head_k], pos, n_embd_head_v, freq_base);
                }

                let kb = layer * max_ctx * n_embd_gqa;

                if kv_format == KvFormat::F16 {
                    let k_cache = &mut *k_cache_f16_ptr;
                    let v_cache = &mut *v_cache_f16_ptr;
                    for h in kv_h_start..kv_h_end {
                        let off = h * n_embd_head_k;
                        f32_slice_to_f16(&k_new[off..off + n_embd_head_k], &mut k_cache[kb + pos * n_embd_gqa + off..kb + pos * n_embd_gqa + off + n_embd_head_k]);
                        f32_slice_to_f16(&v_new[off..off + n_embd_head_v], &mut v_cache[kb + pos * n_embd_gqa + off..kb + pos * n_embd_gqa + off + n_embd_head_v]);
                    }
                    attn_barrier_ref.fetch_add(1, Ordering::SeqCst);
                    while attn_barrier_ref.load(Ordering::Acquire) < nth as i32 {
                        std::hint::spin_loop();
                    }
                    for h in h_start..h_end {
                        let kv_h = h / group_size;
                        let q_off = h * n_embd_head_k;
                        let n_cached = pos + 1;
                        let out_base = h * n_embd_head_v;
                        let mut ms = 0.0f32;
                        let mut s_sum = 0.0f32;
                        for d in 0..n_embd_head_v { attn_out[out_base + d] = 0.0; }
                        for t in 0..n_cached {
                            let score = dot_f16_f32(
                                &q[q_off..q_off + n_embd_head_k],
                                &k_cache[kb + t * n_embd_gqa + kv_h * n_embd_head_v..kb + t * n_embd_gqa + kv_h * n_embd_head_v + n_embd_head_k],
                                n_embd_head_k,
                            ) * kq_scale;
                            if score > ms {
                                let rescale = (ms - score).exp();
                                vec_scale_f32(&mut attn_out[out_base..out_base + n_embd_head_v], rescale);
                                s_sum *= rescale;
                                ms = score;
                            }
                            let vs = (score - ms).exp();
                            let v_base = kb + t * n_embd_gqa + kv_h * n_embd_head_v;
                            vec_mad_f16_f32(&mut attn_out[out_base..out_base + n_embd_head_v], &v_cache[v_base..v_base + n_embd_head_v], vs);
                            s_sum += vs;
                        }
                        let inv_sum = 1.0 / s_sum;
                        vec_scale_f32(&mut attn_out[out_base..out_base + n_embd_head_v], inv_sum);
                    }
                } else {
                    let k_cache = &mut *k_cache_f32_ptr;
                    let v_cache = &mut *v_cache_f32_ptr;
                    for h in kv_h_start..kv_h_end {
                        let off = h * n_embd_head_k;
                        k_cache[kb + pos * n_embd_gqa + off..kb + pos * n_embd_gqa + off + n_embd_head_k]
                            .copy_from_slice(&k_new[off..off + n_embd_head_k]);
                        v_cache[kb + pos * n_embd_gqa + off..kb + pos * n_embd_gqa + off + n_embd_head_v]
                            .copy_from_slice(&v_new[off..off + n_embd_head_v]);
                    }
                    attn_barrier_ref.fetch_add(1, Ordering::SeqCst);
                    while attn_barrier_ref.load(Ordering::Acquire) < nth as i32 {
                        std::hint::spin_loop();
                    }
                    for h in h_start..h_end {
                        let kv_h = h / group_size;
                        let q_off = h * n_embd_head_k;
                        let n_cached = pos + 1;
                        let out_base = h * n_embd_head_v;
                        let mut ms = 0.0f32;
                        let mut s_sum = 0.0f32;
                        for d in 0..n_embd_head_v { attn_out[out_base + d] = 0.0; }
                        for t in 0..n_cached {
                            let score = dot_f32(
                                &q[q_off..q_off + n_embd_head_k],
                                &k_cache[kb + t * n_embd_gqa + kv_h * n_embd_head_v..kb + t * n_embd_gqa + kv_h * n_embd_head_v + n_embd_head_k],
                                n_embd_head_k,
                            ) * kq_scale;
                            if score > ms {
                                let rescale = (ms - score).exp();
                                vec_scale_f32(&mut attn_out[out_base..out_base + n_embd_head_v], rescale);
                                s_sum *= rescale;
                                ms = score;
                            }
                            let vs = (score - ms).exp();
                            let v_base = kb + t * n_embd_gqa + kv_h * n_embd_head_v;
                            vec_mad_f32(&mut attn_out[out_base..out_base + n_embd_head_v], &v_cache[v_base..v_base + n_embd_head_v], vs);
                            s_sum += vs;
                        }
                        let inv_sum = 1.0 / s_sum;
                        vec_scale_f32(&mut attn_out[out_base..out_base + n_embd_head_v], inv_sum);
                    }
                }
            });
            t_qkv += t0.elapsed().as_secs_f64();

            let attn_out = &mut *attn_out_ptr;
            let q8_buf = &mut *q8_buf_ptr;
            let scale_buf = &mut *scale_buf_ptr;
            let t0 = Instant::now();
            quantize_q8_0_into(attn_out, n_embd_q, &mut q8_buf[..n_embd_q], &mut scale_buf[..n_embd_q / 32]);
            let q8 = q8_buf[..n_embd_q].as_ptr();
            let sc = scale_buf[..n_embd_q / 32].as_ptr();
            pool.compute(move |ith: usize, nth: usize| {
                let q8 = std::slice::from_raw_parts(q8, n_embd_q);
                let sc = std::slice::from_raw_parts(sc, n_embd_q / 32);
                let attn_proj = &mut *attn_proj_ptr;
                matmul_q8_0_quantized_parallel_rows(lw.wo, q8, sc, attn_proj, n_embd_q, n_embd, ith, nth);
            });
            t_wo += t0.elapsed().as_secs_f64();

            let attn_proj = &mut *attn_proj_ptr;
            let x = &mut *x_ptr;
            let normed = &mut *normed_ptr;
            for i in 0..n_embd { x[i] += attn_proj[i]; }

            let t0 = Instant::now();
            rms_norm(x, &lw.ffn_norm, normed, eps);
            quantize_q8_0_into(normed, n_embd, &mut q8_buf[..n_embd], &mut scale_buf[..n_embd / 32]);
            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();

            let ffn_barrier_ref = &*ffn_barrier_ptr;
            ffn_barrier_ref.store(0, Ordering::SeqCst);

            pool.compute(move |ith: usize, nth: usize| {
                let q8 = std::slice::from_raw_parts(q8, n_embd);
                let sc = std::slice::from_raw_parts(sc, n_embd / 32);
                let gate_buf = &mut *gate_buf_ptr;
                let up_buf = &mut *up_buf_ptr;
                matmul_q8_0_quantized_parallel_rows(lw.w_gate, q8, sc, gate_buf, n_embd, n_ff, ith, nth);
                matmul_q8_0_quantized_parallel_rows(lw.w_up, q8, sc, up_buf, n_embd, n_ff, ith, nth);

                let rows_per = n_ff / nth;
                let r_start = ith * rows_per;
                let r_end = if ith == nth - 1 { n_ff } else { r_start + rows_per };
                for i in r_start..r_end {
                    gate_buf[i] = silu(gate_buf[i]) * up_buf[i];
                }

                ffn_barrier_ref.fetch_add(1, Ordering::SeqCst);
                while ffn_barrier_ref.load(Ordering::Acquire) < nth as i32 {
                    std::hint::spin_loop();
                }

                let gate_buf = &*gate_buf_ptr;
                let q8_buf = &mut *q8_buf_ptr;
                let scale_buf = &mut *scale_buf_ptr;
                quantize_q8_0_into_parallel(gate_buf, n_ff, &mut q8_buf[..n_ff], &mut scale_buf[..n_ff / 32], ith, nth);

                let q8 = std::slice::from_raw_parts(q8_buf[..n_ff].as_ptr(), n_ff);
                let sc = std::slice::from_raw_parts(scale_buf[..n_ff / 32].as_ptr(), n_ff / 32);
                let down_buf = &mut *down_buf_ptr;
                matmul_q8_0_quantized_parallel_rows(lw.w_down, q8, sc, down_buf, n_ff, n_embd, ith, nth);
            });
            t_ffn1 += t0.elapsed().as_secs_f64();

            let down_buf = &mut *down_buf_ptr;
            let x = &mut *x_ptr;
            for i in 0..n_embd { x[i] += down_buf[i]; }
            }
        }

        unsafe {
        let x = &mut *x.get();
        let normed = &mut *normed.get();
        let logits_ptr = logits.get();
        let q8_buf = &mut *q8_buf.get();
        let scale_buf = &mut *scale_buf.get();

        let t0 = Instant::now();
        rms_norm(x, &output_norm, normed, eps);
        t_norm += t0.elapsed().as_secs_f64();

        let t0 = Instant::now();
        quantize_q8_0_into(normed, n_embd, &mut q8_buf[..n_embd], &mut scale_buf[..n_embd / 32]);
        let q8 = q8_buf[..n_embd].as_ptr();
        let sc = scale_buf[..n_embd / 32].as_ptr();
        pool.compute(move |ith: usize, nth: usize| {
            let q8 = std::slice::from_raw_parts(q8, n_embd);
            let sc = std::slice::from_raw_parts(sc, n_embd / 32);
            let logits = &mut *logits_ptr;
            matmul_q8_0_quantized_parallel_rows(output_weight, q8, sc, logits, n_embd, vocab, ith, nth);
        });
        t_logits += t0.elapsed().as_secs_f64();
        }

        if step < input_tokens.len() - 1 { continue; }

        let logits = unsafe { &mut *logits.get() };
        if temperature > 0.0 {
            for l in logits.iter_mut() { *l /= temperature; }
        }
        let top = sample_top_k(logits, 40);

        let mut rng = 0u64;
        for &t in &all_tokens { rng = rng.wrapping_mul(6364136223846793005).wrapping_add(t as u64); }
        let r = ((rng >> 33) as f32) / (1u64 << 31) as f32;
        let mut cum = 0.0f32;
        let mut chosen = top[0].0;
        for &(idx, prob) in &top {
            cum += prob;
            if cum >= r { chosen = idx; break; }
        }

        if chosen == eos_id as usize || chosen == im_end_id as usize { break; }
        if generated_tokens.len() >= max_tokens { break; }

        generated_tokens.push(chosen as u32);
        all_tokens.push(chosen as u32);

        let text = tokenizer.decode(&[chosen as u32]);
        print!("{}", text);
        io::stdout().flush().unwrap();

        if generated_tokens.len() == 1 {
            eprintln!();
        }
    }

    let infer_ms = t_infer.elapsed().as_millis();
    let tok_s = if infer_ms > 0 { generated_tokens.len() as f64 / infer_ms as f64 * 1000.0 } else { 0.0 };
    let total = t_norm + t_quant + t_qkv + t_wo + t_ffn1 + t_logits;
    if profile {
        eprintln!("PROFILE: norm={:.1}% quant={:.1}% qkv+attn={:.1}% wo={:.1}% ffn={:.1}% logits={:.1}%",
            t_norm/total*100.0, t_quant/total*100.0, t_qkv/total*100.0, t_wo/total*100.0, t_ffn1/total*100.0, t_logits/total*100.0);
        eprintln!("PROFILE: norm={:.3}s quant={:.3}s qkv+attn={:.3}s wo={:.3}s ffn={:.3}s logits={:.3}s",
            t_norm, t_quant, t_qkv, t_wo, t_ffn1, t_logits);
    }
    println!();
    println!("[{} tokens in {}ms | {:.1} tok/s]", generated_tokens.len(), infer_ms, tok_s);
}

fn detect_special_tokens(_loader: &GGUFLoader, tokenizer: &BPETokenizer) -> HashMap<String, u32> {
    let mut specials = HashMap::new();
    let candidates = [("<|im_start|>", "im_start"), ("<|im_end|>", "im_end"), ("</s>", "eos")];
    for (text, name) in &candidates {
        for i in 0..tokenizer.vocab_size() {
            if tokenizer.token_str(i as u32) == *text {
                specials.insert(name.to_string(), i as u32);
                break;
            }
        }
    }
    specials
}

fn get_f32_tensor(loader: &GGUFLoader, name: &str, expected_len: usize) -> Vec<f32> {
    let ti = loader.tensor_info(name).expect(&format!("tensor {} not found", name));
    let slice = loader.tensor_slice(name).expect(&format!("slice {} not found", name));
    let mut out = vec![0.0f32; expected_len];
    if ti.ggml_type == GGMLType::F32 {
        let n = expected_len.min(slice.len() / 4);
        for i in 0..n {
            let bytes = [slice[i * 4], slice[i * 4 + 1], slice[i * 4 + 2], slice[i * 4 + 3]];
            out[i] = f32::from_le_bytes(bytes);
        }
    }
    out
}

fn run_interactive(model_path: &str, max_tokens: usize, temperature: f32, n_threads_arg: usize) {
    println!("=== RustModelInference Interactive Mode ===");
    println!("Model: {}", model_path);
    println!("Type your prompt and press Enter. Ctrl+C to exit.\n");

    loop {
        print!("> ");
        io::stdout().flush().unwrap();
        let mut line = String::new();
        if io::stdin().read_line(&mut line).unwrap() == 0 { break; }
        let line = line.trim();
        if line.is_empty() { continue; }
        run_inference(model_path, line, max_tokens, temperature, n_threads_arg, false, false, KvFormat::F16);
        println!();
    }
}

fn run_self_test() {
    println!("=== RustModelInference MVP Self-Test ===\n");
    let config = ModelConfig::qwen2_0_6b();
    println!("[Config] Qwen2-0.6B: n_embd={}, n_layer={}, n_head={}, n_ff={}",
        config.n_embd, config.n_layer, config.n_head, config.n_ff);

    let mut alloc = BlockAllocator::new(64);
    let b0 = alloc.alloc().unwrap();
    let b1 = alloc.alloc().unwrap();
    alloc.free(b1);
    let b3 = alloc.alloc().unwrap();
    println!("BlockAllocator: alloc {},{}, free {}, re-alloc {} [OK]", b0, b1, b1, b3);

    let mut arena = MemoryArena::new(1024, 1024);
    let ptr = arena.scratch_slice().as_ptr() as usize;
    arena.scratch_slice()[0] = 42.0;
    assert_eq!(arena.scratch_slice().as_ptr() as usize, ptr);
    println!("MemoryArena: ptr stable [OK]");

    println!("\nUsage: cargo run -- --model <path.gguf> --prompt \"hello\"");
    println!("       cargo run -- --model <path.gguf>  (interactive mode)");
}
