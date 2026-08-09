use std::collections::HashMap;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use rust_model_inference::*;

#[derive(Clone, Copy, PartialEq)]
enum KvFormat { F16, F32 }

const DEFAULT_THREAD_CAP: usize = 8;

fn resolve_thread_count(requested: usize, available: usize) -> usize {
    if requested > 0 {
        requested
    } else {
        // ponytail: avoid P/E-core barrier collapse; --threads remains the calibration knob.
        available.clamp(1, DEFAULT_THREAD_CAP)
    }
}

fn inference_step_budget(prompt_tokens: usize, max_tokens: usize, bench: bool) -> usize {
    prompt_tokens
        + if bench {
            max_tokens
        } else {
            max_tokens.saturating_sub(1)
        }
}

fn per_second(count: usize, elapsed: Duration) -> f64 {
    let seconds = elapsed.as_secs_f64();
    if seconds > 0.0 { count as f64 / seconds } else { 0.0 }
}

#[cfg(test)]
mod cli_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn default_threads_are_capped_but_explicit_value_wins() {
        assert_eq!(resolve_thread_count(0, 16), 8);
        assert_eq!(resolve_thread_count(0, 4), 4);
        assert_eq!(resolve_thread_count(0, 0), 1);
        assert_eq!(resolve_thread_count(12, 16), 12);
    }

    #[test]
    fn normal_generation_does_not_run_the_final_unused_forward() {
        assert_eq!(inference_step_budget(5, 32, false), 36);
        assert_eq!(inference_step_budget(5, 0, false), 5);
    }

    #[test]
    fn bench_budget_has_exact_decode_eval_count() {
        assert_eq!(inference_step_budget(5, 32, true), 37);
        assert_eq!(per_second(32, Duration::from_millis(250)), 128.0);
    }
}

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
    let mut embedding_mode = false;
    let mut kv_format = KvFormat::F16;
    let mut mmproj_path = String::new();
    let mut image_path = String::new();
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
            "--embedding" => { embedding_mode = true; }
            "--bench" => { bench = true; }
            "--profile" => { profile = true; }
            "--kv-cache" => { if i + 1 < args.len() { kv_format = match args[i + 1].as_str() { "f32" => KvFormat::F32, _ => KvFormat::F16 }; i += 1; } }
            "--mmproj" => { if i + 1 < args.len() { mmproj_path = args[i + 1].clone(); i += 1; } }
            "--image" => { if i + 1 < args.len() { image_path = args[i + 1].clone(); i += 1; } }
            _ => {}
        }
        i += 1;
    }

    if !model_path.is_empty() && (!mmproj_path.is_empty() || !image_path.is_empty()) {
        run_multimodal(&model_path, &mmproj_path, &image_path, &prompt, max_tokens, temperature, n_threads);
    } else if !model_path.is_empty() && !prompt.is_empty() {
        let loader = GGUFLoader::from_file(&model_path).unwrap_or_else(|e| { eprintln!("Failed to load model: {}", e); std::process::exit(1); });
        let arch = loader.metadata("general.architecture").and_then(|v| v.to_string_val()).unwrap_or_default();
        if arch == "qwen35" {
            run_multimodal(&model_path, "", "", &prompt, max_tokens, temperature, n_threads);
        } else if embedding_mode {
            run_embedding(&model_path, &prompt, n_threads, kv_format);
        } else if dump_logits {
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

macro_rules! slice_from_mut {
    ($ptr:expr, $len:expr) => { unsafe { std::slice::from_raw_parts_mut($ptr, $len) } };
}

macro_rules! slice_from_ref {
    ($ptr:expr, $len:expr) => { unsafe { std::slice::from_raw_parts($ptr, $len) } };
}

macro_rules! raw_parts {
    ($ptr:expr, $len:expr) => { unsafe { std::slice::from_raw_parts($ptr, $len) } };
}

fn run_embedding(model_path: &str, prompt: &str, n_threads_arg: usize, _kv_format: KvFormat) {
    let t0 = Instant::now();
    println!("Loading {} ...", model_path);
    let loader = GGUFLoader::from_file(model_path).expect("Failed to load GGUF");
    let config = loader.model_config().expect("Failed to parse model config");

    let arch = loader.metadata("general.architecture").and_then(|v| v.to_string_val()).unwrap_or_default();
    let is_qwen3 = arch == "qwen3";

    let mut tokenizer = BPETokenizer::from_gguf_metadata(|k| loader.metadata(k).cloned())
        .expect("Failed to init tokenizer");

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

    let vocab = tokenizer.vocab_size();
    let prompt_tokens = tokenizer.encode(prompt);
    let n_tokens = prompt_tokens.len();
    let available_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let n_threads = resolve_thread_count(n_threads_arg, available_threads);

    let max_n_in = n_embd_q.max(n_ff);
    let pool = std::sync::Arc::new(thread_pool::ComputePool::new(n_threads));
    eprintln!("compute pool: {} threads", pool.n_threads());
    println!("Prompt: {} ({} tokens)", prompt, n_tokens);

    let kq_scale = 1.0f32 / (n_embd_head_k as f32).sqrt();
    let group_size = n_head / n_head_kv;

    let mut hidden = vec![0.0f32; n_tokens * n_embd];
    let mut q_buf = vec![0.0f32; n_tokens * n_embd_q];
    let mut k_buf = vec![0.0f32; n_tokens * n_embd_gqa];
    let mut v_buf = vec![0.0f32; n_tokens * n_embd_gqa];
    let mut attn_out = vec![0.0f32; n_tokens * n_embd_q];
    let mut attn_proj = vec![0.0f32; n_tokens * n_embd];
    let mut normed = vec![0.0f32; n_tokens * n_embd];
    let mut q8_buf = vec![0u8; max_n_in];
    let mut scale_buf = vec![0.0f32; max_n_in / 32];
    let mut gate_buf = vec![0.0f32; n_ff];
    let mut up_buf = vec![0.0f32; n_ff];
    let mut down_buf = vec![0.0f32; n_ff];

    for t in 0..n_tokens {
        let token_id = prompt_tokens[t];
        let x_slice = &mut hidden[t * n_embd..(t + 1) * n_embd];
        embedding_lookup_q8_0(embd_weight, token_id, n_embd, x_slice);
    }

    eprintln!("DEBUG: initial embedding[0:8] = {:?}, n_embd={}, token_id={}", &hidden[..8], n_embd, prompt_tokens[0]);
    let t_embed = Instant::now();
    for layer in 0..n_layer {
        let lw = &layers[layer];

        for t in 0..n_tokens {
            rms_norm(&hidden[t * n_embd..(t + 1) * n_embd], &lw.attn_norm, &mut normed[t * n_embd..(t + 1) * n_embd], eps);
        }

        for t in 0..n_tokens {
            let x = &normed[t * n_embd..(t + 1) * n_embd];
            let q = &mut q_buf[t * n_embd_q..(t + 1) * n_embd_q];
            let k = &mut k_buf[t * n_embd_gqa..(t + 1) * n_embd_gqa];
            let v = &mut v_buf[t * n_embd_gqa..(t + 1) * n_embd_gqa];

            let mut local_q8 = vec![0u8; max_n_in];
            let mut local_sc = vec![0.0f32; max_n_in / 32];
            quantize_q8_0_into(x, n_embd, &mut local_q8[..n_embd], &mut local_sc[..n_embd / 32]);

            matmul_q8_0_quantized(lw.wq, &local_q8[..n_embd], &local_sc[..n_embd / 32], q, n_embd, n_embd_q);
            matmul_q8_0_quantized(lw.wk, &local_q8[..n_embd], &local_sc[..n_embd / 32], k, n_embd, n_embd_gqa);
            matmul_q8_0_quantized(lw.wv, &local_q8[..n_embd], &local_sc[..n_embd / 32], v, n_embd, n_embd_gqa);
        }

        if let (Some(qn), Some(kn)) = (&lw.q_norm, &lw.k_norm) {
            for t in 0..n_tokens {
                let q = &mut q_buf[t * n_embd_q..(t + 1) * n_embd_q];
                for h in 0..n_head {
                    rms_norm_inplace(&mut q[h * n_embd_head_k..(h + 1) * n_embd_head_k], qn, eps);
                }
            }
            for t in 0..n_tokens {
                let k = &mut k_buf[t * n_embd_gqa..(t + 1) * n_embd_gqa];
                for h in 0..n_head_kv {
                    rms_norm_inplace(&mut k[h * n_embd_head_k..(h + 1) * n_embd_head_k], kn, eps);
                }
            }
        }

        for t in 0..n_tokens {
            let q = &mut q_buf[t * n_embd_q..(t + 1) * n_embd_q];
            for h in 0..n_head {
                rope_neox(&mut q[h * n_embd_head_k..(h + 1) * n_embd_head_k], t, n_embd_head_k, freq_base);
            }
        }
        for t in 0..n_tokens {
            let k = &mut k_buf[t * n_embd_gqa..(t + 1) * n_embd_gqa];
            for h in 0..n_head_kv {
                rope_neox(&mut k[h * n_embd_head_k..(h + 1) * n_embd_head_k], t, n_embd_head_v, freq_base);
            }
        }

        for t in 0..n_tokens {
            let q_row = &q_buf[t * n_embd_q..(t + 1) * n_embd_q];
            let attn_row = &mut attn_out[t * n_embd_q..(t + 1) * n_embd_q];

            for h in 0..n_head {
                let kv_h = h / group_size;
                let q_off = h * n_embd_head_k;
                let out_base = h * n_embd_head_v;

                let mut ms = f32::NEG_INFINITY;
                let mut s_sum = 0.0f32;
                for d in 0..n_embd_head_v {
                    attn_row[out_base + d] = 0.0;
                }

                for s in 0..n_tokens {
                    let k_row = &k_buf[s * n_embd_gqa..(s + 1) * n_embd_gqa];
                    let v_row = &v_buf[s * n_embd_gqa..(s + 1) * n_embd_gqa];

                    let score = dot_f32(
                        &q_row[q_off..q_off + n_embd_head_k],
                        &k_row[kv_h * n_embd_head_v..kv_h * n_embd_head_v + n_embd_head_k],
                        n_embd_head_k,
                    ) * kq_scale;

                    if score > ms {
                        let rescale = (ms - score).exp();
                        vec_scale_f32(&mut attn_row[out_base..out_base + n_embd_head_v], rescale);
                        s_sum *= rescale;
                        ms = score;
                    }
                    let vs = (score - ms).exp();
                    vec_mad_f32(&mut attn_row[out_base..out_base + n_embd_head_v],
                        &v_row[kv_h * n_embd_head_v..kv_h * n_embd_head_v + n_embd_head_v], vs);
                    s_sum += vs;
                }

                let inv_sum = 1.0 / s_sum;
                vec_scale_f32(&mut attn_row[out_base..out_base + n_embd_head_v], inv_sum);
            }
        }

        for t in 0..n_tokens {
            let attn = &attn_out[t * n_embd_q..(t + 1) * n_embd_q];
            let proj = &mut attn_proj[t * n_embd..(t + 1) * n_embd];

            let mut local_q8 = vec![0u8; n_embd_q];
            let mut local_sc = vec![0.0f32; n_embd_q / 32];
            quantize_q8_0_into(attn, n_embd_q, &mut local_q8, &mut local_sc);

            matmul_q8_0_quantized(lw.wo, &local_q8, &local_sc, proj, n_embd_q, n_embd);
        }

        for t in 0..n_tokens {
            let x = &mut hidden[t * n_embd..(t + 1) * n_embd];
            let proj = &attn_proj[t * n_embd..(t + 1) * n_embd];
            for i in 0..n_embd {
                x[i] += proj[i];
            }
        }

        for t in 0..n_tokens {
            rms_norm(&hidden[t * n_embd..(t + 1) * n_embd], &lw.ffn_norm, &mut normed[t * n_embd..(t + 1) * n_embd], eps);
        }

        for t in 0..n_tokens {
            let x = &normed[t * n_embd..(t + 1) * n_embd];

            let mut local_q8 = vec![0u8; n_embd];
            let mut local_sc = vec![0.0f32; n_embd / 32];
            quantize_q8_0_into(x, n_embd, &mut local_q8, &mut local_sc);

            matmul_q8_0_quantized(lw.w_gate, &local_q8, &local_sc, &mut gate_buf, n_embd, n_ff);
            matmul_q8_0_quantized(lw.w_up, &local_q8, &local_sc, &mut up_buf, n_embd, n_ff);
        }

        for i in 0..n_ff {
            gate_buf[i] = silu(gate_buf[i]) * up_buf[i];
        }

        for t in 0..n_tokens {
            let down = &mut down_buf[t * n_embd..(t + 1) * n_embd];
            let mut local_q8 = vec![0u8; n_ff];
            let mut local_sc = vec![0.0f32; n_ff / 32];
            quantize_q8_0_into(&gate_buf, n_ff, &mut local_q8, &mut local_sc);
            matmul_q8_0_quantized(lw.w_down, &local_q8, &local_sc, down, n_ff, n_embd);
        }

        for t in 0..n_tokens {
            let x = &mut hidden[t * n_embd..(t + 1) * n_embd];
            let down = &down_buf[t * n_embd..(t + 1) * n_embd];
            for i in 0..n_embd {
                x[i] += down[i];
            }
        }

    }

    for t in 0..n_tokens {
        let x = &mut hidden[t * n_embd..(t + 1) * n_embd];
        rms_norm(x, &output_norm, &mut normed[t * n_embd..(t + 1) * n_embd], eps);
        x.copy_from_slice(&normed[t * n_embd..(t + 1) * n_embd]);
    }

    let mut pooled = vec![0.0f32; n_embd];
    let inv_n = 1.0 / n_tokens as f32;
    for i in 0..n_embd {
        let mut sum = 0.0f32;
        for t in 0..n_tokens {
            sum += hidden[t * n_embd + i];
        }
        pooled[i] = sum * inv_n;
    }

    let norm: f32 = pooled.iter().map(|&x| x * x).sum::<f32>().sqrt();
    for v in pooled.iter_mut() {
        *v /= norm;
    }

    let embed_ms = t_embed.elapsed().as_millis();
    println!("Embedding ({} dims, {} layers, {}ms):", n_embd, n_layer, embed_ms);
    for (i, &v) in pooled.iter().enumerate() {
        if i < 8 {
            print!("{:+.6} ", v);
        }
    }
    if n_embd > 8 {
        print!("... ");
        for i in (n_embd - 4)..n_embd {
            print!("{:+.6} ", pooled[i]);
        }
    }
    println!();
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

    let kv_cache = match kv_format {
        KvFormat::F16 => KvCache::new_f16(n_layer, max_ctx, n_embd_gqa),
        KvFormat::F32 => KvCache::new_f32(n_layer, max_ctx, n_embd_gqa),
    };

    let mut scratch = ExecutionScratchpad::new(n_embd, n_embd_q, n_embd_gqa, n_ff, vocab, n_threads, max_ctx);

    let input_tokens = prompt_tokens.clone();
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

        embedding_lookup_q8_0(embd_weight, token_id, n_embd, &mut scratch.x);

        for layer in 0..n_layer {
            let lw = &layers[layer];

            let x_ptr = scratch.x.as_mut_ptr();
            let normed_ptr = scratch.normed.as_mut_ptr();
            let q_ptr = scratch.q.as_mut_ptr();
            let k_ptr = scratch.k_new.as_mut_ptr();
            let v_ptr = scratch.v_new.as_mut_ptr();
            let attn_out_ptr = scratch.attn_out.as_mut_ptr();
            let attn_proj_ptr = scratch.attn_proj.as_mut_ptr();
            let down_buf_ptr = scratch.down_buf.as_mut_ptr();
            let scores_ptr = scratch.scores.as_mut_ptr();
            let gate_buf_ptr = scratch.gate_buf.as_mut_ptr();
            let up_buf_ptr = scratch.up_buf.as_mut_ptr();
            let q8_buf_ptr = scratch.q8_buf.as_mut_ptr();
            let scale_buf_ptr = scratch.scale_buf.as_mut_ptr();
            let kv_cache_size = n_layer * max_ctx * n_embd_gqa;
            let (k_cache_f16_ptr, v_cache_f16_ptr) = match &kv_cache {
                KvCache::F16(c) => (c.k.as_ptr() as *mut u16, c.v.as_ptr() as *mut u16),
                _ => (std::ptr::null_mut(), std::ptr::null_mut()),
            };
            let (k_cache_f32_ptr, v_cache_f32_ptr) = match &kv_cache {
                KvCache::F32(c) => (c.k.as_ptr() as *mut f32, c.v.as_ptr() as *mut f32),
                _ => (std::ptr::null_mut(), std::ptr::null_mut()),
            };

            let x = slice_from_mut!(x_ptr, n_embd);
            let normed = slice_from_mut!(normed_ptr, n_embd);
            let q8_buf = slice_from_mut!(q8_buf_ptr, n_embd_q.max(n_ff));
            let scale_buf = slice_from_mut!(scale_buf_ptr, n_embd_q.max(n_ff) / 32);

            rms_norm(x, &lw.attn_norm, normed, eps);
            quantize_q8_0_into(normed, n_embd, &mut q8_buf[..n_embd], &mut scale_buf[..n_embd / 32]);

            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();

            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_embd);
                let sc = raw_parts!(sc, n_embd / 32);
                let q = slice_from_mut!(q_ptr, n_embd_q);
                let k_new = slice_from_mut!(k_ptr, n_embd_gqa);
                let v_new = slice_from_mut!(v_ptr, n_embd_gqa);

                matmul_q8_0_quantized_parallel_rows(lw.wq, q8, sc, q, n_embd, n_embd_q, ith, nth);
                matmul_q8_0_quantized_parallel_rows(lw.wk, q8, sc, k_new, n_embd, n_embd_gqa, ith, nth);
                matmul_q8_0_quantized_parallel_rows(lw.wv, q8, sc, v_new, n_embd, n_embd_gqa, ith, nth);
            });

            {
                let q = slice_from_mut!(q_ptr, n_embd_q);
                let k_new = slice_from_mut!(k_ptr, n_embd_gqa);
                let v_new = slice_from_mut!(v_ptr, n_embd_gqa);
                let q_norm = lw.q_norm.as_deref();
                let k_norm = lw.k_norm.as_deref();

                if let (Some(qn), Some(kn)) = (q_norm, k_norm) {
                    for h in 0..n_head {
                        rms_norm_inplace(&mut q[h * n_embd_head_k..(h + 1) * n_embd_head_k], qn, eps);
                    }
                    for h in 0..n_head_kv {
                        rms_norm_inplace(&mut k_new[h * n_embd_head_k..(h + 1) * n_embd_head_k], kn, eps);
                    }
                }

                for h in 0..n_head {
                    rope_neox(&mut q[h * n_embd_head_k..(h + 1) * n_embd_head_k], pos, n_embd_head_k, freq_base);
                }
                for h in 0..n_head_kv {
                    rope_neox(&mut k_new[h * n_embd_head_k..(h + 1) * n_embd_head_k], pos, n_embd_head_v, freq_base);
                }

                let kb = layer * max_ctx * n_embd_gqa;

                if kv_format == KvFormat::F16 {
                    let k_cache = slice_from_mut!(k_cache_f16_ptr, kv_cache_size);
                    let v_cache = slice_from_mut!(v_cache_f16_ptr, kv_cache_size);
                    for h in 0..n_head_kv {
                        let off = h * n_embd_head_k;
                        f32_slice_to_f16(&k_new[off..off + n_embd_head_k], &mut k_cache[kb + pos * n_embd_gqa + off..kb + pos * n_embd_gqa + off + n_embd_head_k]);
                        f32_slice_to_f16(&v_new[off..off + n_embd_head_v], &mut v_cache[kb + pos * n_embd_gqa + off..kb + pos * n_embd_gqa + off + n_embd_head_v]);
                    }
                } else {
                    let k_cache = slice_from_mut!(k_cache_f32_ptr, kv_cache_size);
                    let v_cache = slice_from_mut!(v_cache_f32_ptr, kv_cache_size);
                    for h in 0..n_head_kv {
                        let off = h * n_embd_head_k;
                        k_cache[kb + pos * n_embd_gqa + off..kb + pos * n_embd_gqa + off + n_embd_head_k]
                            .copy_from_slice(&k_new[off..off + n_embd_head_k]);
                        v_cache[kb + pos * n_embd_gqa + off..kb + pos * n_embd_gqa + off + n_embd_head_v]
                            .copy_from_slice(&v_new[off..off + n_embd_head_v]);
                    }
                }
            }

            pool.compute(move |ith: usize, nth: usize| {
                let q = slice_from_ref!(q_ptr, n_embd_q);
                let attn_out = slice_from_mut!(attn_out_ptr, n_embd_q);
                let scores = slice_from_mut!(scores_ptr, n_threads * max_ctx);
                let h_start = ith * n_head / nth;
                let h_end = (ith + 1) * n_head / nth;

                let kb = layer * max_ctx * n_embd_gqa;

                if kv_format == KvFormat::F16 {
                    let k_cache = slice_from_ref!(k_cache_f16_ptr, kv_cache_size);
                    let v_cache = slice_from_ref!(v_cache_f16_ptr, kv_cache_size);
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
                    let k_cache = slice_from_ref!(k_cache_f32_ptr, kv_cache_size);
                    let v_cache = slice_from_ref!(v_cache_f32_ptr, kv_cache_size);
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

            let attn_out = slice_from_mut!(attn_out_ptr, n_embd_q);
            quantize_q8_0_into(attn_out, n_embd_q, &mut q8_buf[..n_embd_q], &mut scale_buf[..n_embd_q / 32]);

            let q8 = q8_buf[..n_embd_q].as_ptr();
            let sc = scale_buf[..n_embd_q / 32].as_ptr();
            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_embd_q);
                let sc = raw_parts!(sc, n_embd_q / 32);
                let attn_proj = slice_from_mut!(attn_proj_ptr, n_embd);
                matmul_q8_0_quantized_parallel_rows(lw.wo, q8, sc, attn_proj, n_embd_q, n_embd, ith, nth);
            });

            let attn_proj = slice_from_mut!(attn_proj_ptr, n_embd);
            let x = slice_from_mut!(x_ptr, n_embd);
            let normed = slice_from_mut!(normed_ptr, n_embd);
            for i in 0..n_embd { x[i] += attn_proj[i]; }

            rms_norm(x, &lw.ffn_norm, normed, eps);
            quantize_q8_0_into(normed, n_embd, &mut q8_buf[..n_embd], &mut scale_buf[..n_embd / 32]);
            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();

            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_embd);
                let sc = raw_parts!(sc, n_embd / 32);
                let gate_buf = slice_from_mut!(gate_buf_ptr, n_ff);
                let up_buf = slice_from_mut!(up_buf_ptr, n_ff);
                matmul_q8_0_quantized_parallel_rows(lw.w_gate, q8, sc, gate_buf, n_embd, n_ff, ith, nth);
                matmul_q8_0_quantized_parallel_rows(lw.w_up, q8, sc, up_buf, n_embd, n_ff, ith, nth);

                let rows_per = n_ff / nth;
                let r_start = ith * rows_per;
                let r_end = if ith == nth - 1 { n_ff } else { r_start + rows_per };
                for i in r_start..r_end {
                    gate_buf[i] = silu(gate_buf[i]) * up_buf[i];
                }
            });

            let gate_buf = slice_from_mut!(gate_buf_ptr, n_ff);
            quantize_q8_0_into(gate_buf, n_ff, &mut q8_buf[..n_ff], &mut scale_buf[..n_ff / 32]);

            let q8 = q8_buf[..n_ff].as_ptr();
            let sc = scale_buf[..n_ff / 32].as_ptr();
            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_ff);
                let sc = raw_parts!(sc, n_ff / 32);
                let down_buf = slice_from_mut!(down_buf_ptr, n_embd);
                matmul_q8_0_quantized_parallel_rows(lw.w_down, q8, sc, down_buf, n_ff, n_embd, ith, nth);
            });

            let down_buf = slice_from_mut!(down_buf_ptr, n_embd);
            let x = slice_from_mut!(x_ptr, n_embd);
            for i in 0..n_embd { x[i] += down_buf[i]; }
        }

        {
            let x = &mut scratch.x;
            let normed = &mut scratch.normed;
            let logits_ptr = scratch.logits.as_mut_ptr();
            let q8_buf = &mut scratch.q8_buf;
            let scale_buf = &mut scratch.scale_buf;

            rms_norm(x, &output_norm, normed, eps);
            quantize_q8_0_into(normed, n_embd, &mut q8_buf[..n_embd], &mut scale_buf[..n_embd / 32]);

            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();
            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_embd);
                let sc = raw_parts!(sc, n_embd / 32);
                let logits = slice_from_mut!(logits_ptr, vocab);
                matmul_q8_0_quantized_parallel_rows(output_weight, q8, sc, logits, n_embd, vocab, ith, nth);
            });
        }

        if step < input_tokens.len() - 1 { continue; }

        let logits = &scratch.logits;

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

    let kv_cache = match kv_format {
        KvFormat::F16 => KvCache::new_f16(n_layer, max_ctx, n_embd_gqa),
        KvFormat::F32 => KvCache::new_f32(n_layer, max_ctx, n_embd_gqa),
    };

    let vocab = tokenizer.vocab_size();
    let prompt_tokens = tokenizer.encode(prompt);
    let available_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let n_threads = resolve_thread_count(n_threads_arg, available_threads);

    let mut scratch = ExecutionScratchpad::new(n_embd, n_embd_q, n_embd_gqa, n_ff, vocab, n_threads, max_ctx);
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
    let _t_quant: f64 = 0.0;
    let mut t_qkv: f64 = 0.0;
    let mut t_wo: f64 = 0.0;
    let mut t_ffn1: f64 = 0.0;
    let _t_silu: f64 = 0.0;
    let _t_down: f64 = 0.0;
    let mut t_logits: f64 = 0.0;

    print!("Output: ");
    io::stdout().flush().unwrap();

    let t_infer = Instant::now();
    let total_steps = inference_step_budget(input_tokens.len(), max_tokens, bench);
    let mut prefill_evals = 0usize;
    let mut prefill_time = Duration::ZERO;
    let mut decode_evals = 0usize;
    let mut decode_time = Duration::ZERO;

    for step in 0..total_steps {
        let eval_started = Instant::now();
        let token_id = if step < input_tokens.len() {
            input_tokens[step]
        } else {
            *generated_tokens.last().unwrap_or(&0)
        };

        let pos = step;

        embedding_lookup_q8_0(embd_weight, token_id, n_embd, &mut scratch.x);

        for layer in 0..n_layer {
            let lw = &layers[layer];

            let x_ptr = scratch.x.as_mut_ptr();
            let normed_ptr = scratch.normed.as_mut_ptr();
            let q_ptr = scratch.q.as_mut_ptr();
            let k_ptr = scratch.k_new.as_mut_ptr();
            let v_ptr = scratch.v_new.as_mut_ptr();
            let attn_out_ptr = scratch.attn_out.as_mut_ptr();
            let attn_proj_ptr = scratch.attn_proj.as_mut_ptr();
            let down_buf_ptr = scratch.down_buf.as_mut_ptr();
            let gate_buf_ptr = scratch.gate_buf.as_mut_ptr();
            let up_buf_ptr = scratch.up_buf.as_mut_ptr();
            let q8_buf_ptr = scratch.q8_buf.as_mut_ptr();
            let scale_buf_ptr = scratch.scale_buf.as_mut_ptr();
            let kv_cache_size = n_layer * max_ctx * n_embd_gqa;
            let (k_cache_f16_ptr, v_cache_f16_ptr) = match &kv_cache {
                KvCache::F16(c) => (c.k.as_ptr() as *mut u16, c.v.as_ptr() as *mut u16),
                _ => (std::ptr::null_mut(), std::ptr::null_mut()),
            };
            let (k_cache_f32_ptr, v_cache_f32_ptr) = match &kv_cache {
                KvCache::F32(c) => (c.k.as_ptr() as *mut f32, c.v.as_ptr() as *mut f32),
                _ => (std::ptr::null_mut(), std::ptr::null_mut()),
            };

            let max_n_in = n_embd_q.max(n_ff);
            let x = slice_from_mut!(x_ptr, n_embd);
            let normed = slice_from_mut!(normed_ptr, n_embd);
            let q8_buf = slice_from_mut!(q8_buf_ptr, max_n_in);
            let scale_buf = slice_from_mut!(scale_buf_ptr, max_n_in / 32);

            let t0 = Instant::now();
            rms_norm(x, &lw.attn_norm, normed, eps);
            quantize_q8_0_into(normed, n_embd, &mut q8_buf[..n_embd], &mut scale_buf[..n_embd / 32]);
            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();

            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_embd);
                let sc = raw_parts!(sc, n_embd / 32);
                let q = slice_from_mut!(q_ptr, n_embd_q);
                let k_new = slice_from_mut!(k_ptr, n_embd_gqa);
                let v_new = slice_from_mut!(v_ptr, n_embd_gqa);

                matmul_q8_0_quantized_parallel_rows(lw.wq, q8, sc, q, n_embd, n_embd_q, ith, nth);
                matmul_q8_0_quantized_parallel_rows(lw.wk, q8, sc, k_new, n_embd, n_embd_gqa, ith, nth);
                matmul_q8_0_quantized_parallel_rows(lw.wv, q8, sc, v_new, n_embd, n_embd_gqa, ith, nth);
            });

            {
                let q = slice_from_mut!(q_ptr, n_embd_q);
                let k_new = slice_from_mut!(k_ptr, n_embd_gqa);
                let v_new = slice_from_mut!(v_ptr, n_embd_gqa);
                let q_norm = lw.q_norm.as_deref();
                let k_norm = lw.k_norm.as_deref();

                if let (Some(qn), Some(kn)) = (q_norm, k_norm) {
                    for h in 0..n_head {
                        rms_norm_inplace(&mut q[h * n_embd_head_k..(h + 1) * n_embd_head_k], qn, eps);
                    }
                    for h in 0..n_head_kv {
                        rms_norm_inplace(&mut k_new[h * n_embd_head_k..(h + 1) * n_embd_head_k], kn, eps);
                    }
                }

                for h in 0..n_head {
                    rope_neox(&mut q[h * n_embd_head_k..(h + 1) * n_embd_head_k], pos, n_embd_head_k, freq_base);
                }
                for h in 0..n_head_kv {
                    rope_neox(&mut k_new[h * n_embd_head_k..(h + 1) * n_embd_head_k], pos, n_embd_head_v, freq_base);
                }

                let kb = layer * max_ctx * n_embd_gqa;

                if kv_format == KvFormat::F16 {
                    let k_cache = slice_from_mut!(k_cache_f16_ptr, kv_cache_size);
                    let v_cache = slice_from_mut!(v_cache_f16_ptr, kv_cache_size);
                    for h in 0..n_head_kv {
                        let off = h * n_embd_head_k;
                        f32_slice_to_f16(&k_new[off..off + n_embd_head_k], &mut k_cache[kb + pos * n_embd_gqa + off..kb + pos * n_embd_gqa + off + n_embd_head_k]);
                        f32_slice_to_f16(&v_new[off..off + n_embd_head_v], &mut v_cache[kb + pos * n_embd_gqa + off..kb + pos * n_embd_gqa + off + n_embd_head_v]);
                    }
                } else {
                    let k_cache = slice_from_mut!(k_cache_f32_ptr, kv_cache_size);
                    let v_cache = slice_from_mut!(v_cache_f32_ptr, kv_cache_size);
                    for h in 0..n_head_kv {
                        let off = h * n_embd_head_k;
                        k_cache[kb + pos * n_embd_gqa + off..kb + pos * n_embd_gqa + off + n_embd_head_k]
                            .copy_from_slice(&k_new[off..off + n_embd_head_k]);
                        v_cache[kb + pos * n_embd_gqa + off..kb + pos * n_embd_gqa + off + n_embd_head_v]
                            .copy_from_slice(&v_new[off..off + n_embd_head_v]);
                    }
                }
            }

            pool.compute(move |ith: usize, nth: usize| {
                let q = slice_from_ref!(q_ptr, n_embd_q);
                let attn_out = slice_from_mut!(attn_out_ptr, n_embd_q);
                let h_start = ith * n_head / nth;
                let h_end = (ith + 1) * n_head / nth;

                let kb = layer * max_ctx * n_embd_gqa;

                if kv_format == KvFormat::F16 {
                    let k_cache = slice_from_ref!(k_cache_f16_ptr, kv_cache_size);
                    let v_cache = slice_from_ref!(v_cache_f16_ptr, kv_cache_size);
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
                    let k_cache = slice_from_ref!(k_cache_f32_ptr, kv_cache_size);
                    let v_cache = slice_from_ref!(v_cache_f32_ptr, kv_cache_size);
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

            let attn_out = slice_from_mut!(attn_out_ptr, n_embd_q);
            let q8_buf = slice_from_mut!(q8_buf_ptr, max_n_in);
            let scale_buf = slice_from_mut!(scale_buf_ptr, max_n_in / 32);
            let t0 = Instant::now();
            quantize_q8_0_into(attn_out, n_embd_q, &mut q8_buf[..n_embd_q], &mut scale_buf[..n_embd_q / 32]);
            let q8 = q8_buf[..n_embd_q].as_ptr();
            let sc = scale_buf[..n_embd_q / 32].as_ptr();
            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_embd_q);
                let sc = raw_parts!(sc, n_embd_q / 32);
                let attn_proj = slice_from_mut!(attn_proj_ptr, n_embd);
                matmul_q8_0_quantized_parallel_rows(lw.wo, q8, sc, attn_proj, n_embd_q, n_embd, ith, nth);
            });
            t_wo += t0.elapsed().as_secs_f64();

            let attn_proj = slice_from_mut!(attn_proj_ptr, n_embd);
            let x = slice_from_mut!(x_ptr, n_embd);
            let normed = slice_from_mut!(normed_ptr, n_embd);
            for i in 0..n_embd { x[i] += attn_proj[i]; }

            let t0 = Instant::now();
            rms_norm(x, &lw.ffn_norm, normed, eps);
            quantize_q8_0_into(normed, n_embd, &mut q8_buf[..n_embd], &mut scale_buf[..n_embd / 32]);
            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();

            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_embd);
                let sc = raw_parts!(sc, n_embd / 32);
                let gate_buf = slice_from_mut!(gate_buf_ptr, n_ff);
                let up_buf = slice_from_mut!(up_buf_ptr, n_ff);
                matmul_q8_0_quantized_parallel_rows(lw.w_gate, q8, sc, gate_buf, n_embd, n_ff, ith, nth);
                matmul_q8_0_quantized_parallel_rows(lw.w_up, q8, sc, up_buf, n_embd, n_ff, ith, nth);

                let rows_per = n_ff / nth;
                let r_start = ith * rows_per;
                let r_end = if ith == nth - 1 { n_ff } else { r_start + rows_per };
                for i in r_start..r_end {
                    gate_buf[i] = silu(gate_buf[i]) * up_buf[i];
                }
            });

            {
                let gate_buf = slice_from_mut!(gate_buf_ptr, n_ff);
                let q8_buf = slice_from_mut!(q8_buf_ptr, max_n_in);
                let scale_buf = slice_from_mut!(scale_buf_ptr, max_n_in / 32);
                quantize_q8_0_into(gate_buf, n_ff, &mut q8_buf[..n_ff], &mut scale_buf[..n_ff / 32]);
            }

            let q8 = q8_buf[..n_ff].as_ptr();
            let sc = scale_buf[..n_ff / 32].as_ptr();
            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_ff);
                let sc = raw_parts!(sc, n_ff / 32);
                let down_buf = slice_from_mut!(down_buf_ptr, n_embd);
                matmul_q8_0_quantized_parallel_rows(lw.w_down, q8, sc, down_buf, n_ff, n_embd, ith, nth);
            });
            t_ffn1 += t0.elapsed().as_secs_f64();

            let down_buf = slice_from_mut!(down_buf_ptr, n_embd);
            let x = slice_from_mut!(x_ptr, n_embd);
            for i in 0..n_embd { x[i] += down_buf[i]; }
        }

        {
            let x = &mut scratch.x;
            let normed = &mut scratch.normed;
            let logits_ptr = scratch.logits.as_mut_ptr();
            let q8_buf = &mut scratch.q8_buf;
            let scale_buf = &mut scratch.scale_buf;

            let t0 = Instant::now();
            rms_norm(x, &output_norm, normed, eps);
            t_norm += t0.elapsed().as_secs_f64();

            let t0 = Instant::now();
            quantize_q8_0_into(normed, n_embd, &mut q8_buf[..n_embd], &mut scale_buf[..n_embd / 32]);
            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();
            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_embd);
                let sc = raw_parts!(sc, n_embd / 32);
                let logits = slice_from_mut!(logits_ptr, vocab);
                matmul_q8_0_quantized_parallel_rows(output_weight, q8, sc, logits, n_embd, vocab, ith, nth);
            });
            t_logits += t0.elapsed().as_secs_f64();
        }

        let eval_elapsed = eval_started.elapsed();
        if step < input_tokens.len() {
            prefill_evals += 1;
            prefill_time += eval_elapsed;
        } else {
            decode_evals += 1;
            decode_time += eval_elapsed;
        }

        if step < input_tokens.len() - 1 { continue; }

        let logits = &mut scratch.logits;
        let chosen = if temperature <= 0.0 {
            logits.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).map(|(i, _)| i).unwrap_or(0)
        } else {
            for l in logits.iter_mut() { *l /= temperature; }
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
            chosen
        };

        if !bench && (chosen == eos_id as usize || chosen == im_end_id as usize) { break; }
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
    let total = t_norm + _t_quant + t_qkv + t_wo + t_ffn1 + t_logits;
    if bench || profile { eprintln!(); }
    if bench {
        eprintln!(
            "BENCH: pp {} evals in {:.3}s | {:.1} eval/s",
            prefill_evals,
            prefill_time.as_secs_f64(),
            per_second(prefill_evals, prefill_time),
        );
        eprintln!(
            "BENCH: tg {} evals in {:.3}s | {:.1} eval/s",
            decode_evals,
            decode_time.as_secs_f64(),
            per_second(decode_evals, decode_time),
        );
    }
    if profile {
        eprintln!("PROFILE: norm={:.1}% quant={:.1}% qkv+attn={:.1}% wo={:.1}% ffn={:.1}% logits={:.1}%",
            t_norm/total*100.0, _t_quant/total*100.0, t_qkv/total*100.0, t_wo/total*100.0, t_ffn1/total*100.0, t_logits/total*100.0);
        eprintln!("PROFILE: norm={:.3}s quant={:.3}s qkv+attn={:.3}s wo={:.3}s ffn={:.3}s logits={:.3}s",
            t_norm, _t_quant, t_qkv, t_wo, t_ffn1, t_logits);
    }
    println!();
    println!("[end-to-end: {} output tokens in {}ms | {:.1} tok/s]", generated_tokens.len(), infer_ms, tok_s);
}

fn detect_special_tokens(_loader: &GGUFLoader, tokenizer: &BPETokenizer) -> HashMap<String, u32> {
    let mut specials = HashMap::new();
    let candidates = [
        ("<|im_start|>", "im_start"),
        ("<|im_end|>", "im_end"),
        ("<|image_pad|>", "image_pad"),
        ("<|vision_pad|>", "vision_pad"),
        ("<|vision_start|>", "vision_start"),
        ("<|vision_end|>", "vision_end"),
        ("</s>", "eos"),
    ];
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
    println!("       cargo run -- --model <llm.gguf> --mmproj <mmproj.gguf> --image <image.png> --prompt \"describe\"");
}

fn tokenize_prompt(prompt: &str, image_token_id: i32, n_vis_tokens: usize) -> Vec<i32> {
    let mut tokens = vec![151644, 8946, 198];
    for _ in 0..n_vis_tokens {
        tokens.push(image_token_id);
    }
    tokens.extend_from_slice(&[151645, 198]);
    for b in prompt.bytes() {
        tokens.push(b as i32);
    }
    tokens
}

fn inject_vision_embeddings(llm: &Qwen35Model, tokens: &[i32], vis_embd: &[f32], n_vis_tokens: usize, proj_dim: usize) -> Vec<f32> {
    let n_embd = llm.config.n_embd;
    let n_tokens = tokens.len();
    let mut embeddings = vec![0.0f32; n_tokens * n_embd];

    let image_token_id: i32 = 248056;
    let mut vis_idx = 0;

    for t in 0..n_tokens {
        if tokens[t] == image_token_id && vis_idx < n_vis_tokens {
            let embd_off = t * n_embd;
            let vis_off = vis_idx * proj_dim;
            if proj_dim == n_embd {
                embeddings[embd_off..embd_off + n_embd].copy_from_slice(&vis_embd[vis_off..vis_off + n_embd]);
            } else {
                for e in 0..n_embd.min(proj_dim) {
                    embeddings[embd_off + e] = vis_embd[vis_off + e];
                }
            }
            vis_idx += 1;
        } else {
            let tok = tokens[t] as usize;
            let tok_off = tok * n_embd;
            let embd_off = t * n_embd;
            for e in 0..n_embd {
                if tok_off + e < llm.tok_embd.len() {
                    embeddings[embd_off + e] = llm.tok_embd[tok_off + e];
                }
            }
        }
    }

    embeddings
}

fn sample_token(logits: &[f32], temperature: f32) -> i32 {
    if temperature <= 0.0 {
        return logits.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).map(|(i, _)| i as i32).unwrap_or(0);
    }
    let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let mut sum = 0.0f32;
    let mut probs = vec![0.0f32; logits.len()];
    for (i, l) in logits.iter().enumerate() {
        probs[i] = ((l - max_logit) / temperature).exp();
        sum += probs[i];
    }
    for p in probs.iter_mut() { *p /= sum; }

    let r = 0.5f32;
    let mut cumsum = 0.0f32;
    for (i, p) in probs.iter().enumerate() {
        cumsum += p;
        if cumsum >= r { return i as i32; }
    }
    (logits.len() - 1) as i32
}

fn decode_token(loader: &GGUFLoader, token_id: i32) -> String {
    if let Some(MetaValue::Array(_, vals)) = loader.metadata("tokenizer.ggml.tokens") {
        let idx = token_id as usize;
        if idx < vals.len() {
            if let Some(s) = vals[idx].to_string_val() {
                return s.replace("Ġ", " ").replace("Ċ", "\n").replace("▁", " ");
            }
        }
    }
    format!("<{}>", token_id)
}

fn load_image_f32(path: &str, target_w: usize, target_h: usize, mean: &[f32; 3], std: &[f32; 3]) -> Vec<f32> {
    let img_bytes = std::fs::read(path).unwrap_or_else(|e| { eprintln!("Failed to read image: {}", e); std::process::exit(1); });
    let img = image::load_from_memory(&img_bytes).unwrap_or_else(|e| { eprintln!("Failed to decode image: {}", e); std::process::exit(1 ) });
    let img = img.resize_exact(target_w as u32, target_h as u32, image::imageops::FilterType::Lanczos3);
    let rgb = img.to_rgb8();
    let mut out = vec![0.0f32; target_w * target_h * 3];
    for y in 0..target_h {
        for x in 0..target_w {
            let px = rgb.get_pixel(x as u32, y as u32);
            let idx = (y * target_w + x) * 3;
            out[idx + 0] = (px[0] as f32 / 255.0 - mean[0]) / std[0];
            out[idx + 1] = (px[1] as f32 / 255.0 - mean[1]) / std[1];
            out[idx + 2] = (px[2] as f32 / 255.0 - mean[2]) / std[2];
        }
    }
    out
}

fn run_multimodal(model_path: &str, mmproj_path: &str, image_path: &str, prompt: &str, max_tokens: usize, temperature: f32, n_threads_arg: usize) {
    let has_image = !image_path.is_empty();
    let has_mmproj = !mmproj_path.is_empty();

    println!("Loading model {} ...", model_path);
    let llm_loader = GGUFLoader::from_file(model_path).unwrap_or_else(|e| { eprintln!("Failed to load model: {}", e); std::process::exit(1); });

    let arch = llm_loader.metadata("general.architecture").and_then(|v| v.to_string_val()).unwrap_or_default();
    println!("LLM arch: {}", arch);
    if arch != "qwen35" {
        eprintln!("Only qwen35 architecture is supported for multimodal, got: {}", arch);
        std::process::exit(1);
    }

    let (n_vis_tokens, vis_embeddings_vec) = if has_image && has_mmproj {
        println!("Loading mmproj {} ...", mmproj_path);
        let mmproj_loader = GGUFLoader::from_file(mmproj_path).unwrap_or_else(|e| { eprintln!("Failed to load mmproj: {}", e); std::process::exit(1); });
        let mut vision_encoder = VisionEncoder::from_gguf(&mmproj_loader).unwrap_or_else(|e| { eprintln!("Failed to parse vision encoder: {}", e); std::process::exit(1); });
        vision_encoder.precompute();
        println!("Vision encoder loaded: {} layers, n_embd={}, image_size={}, patch_size={}, merge={}",
                 vision_encoder.config.n_layer, vision_encoder.config.n_embd,
                 vision_encoder.config.image_size, vision_encoder.config.patch_size,
                 vision_encoder.config.spatial_merge_size);

        let cfg = &vision_encoder.config;
        let align = cfg.patch_size * cfg.spatial_merge_size;
        let img_w = (256 / align) * align;
        let img_h = (256 / align) * align;
        let pixels = load_image_f32(image_path, img_w, img_h, &cfg.image_mean, &cfg.image_std);
        println!("Image resized to {}x{} ({} patches, aligned to patch_size*merge={})", img_w, img_h, (img_w/cfg.patch_size)*(img_h/cfg.patch_size), align);

        let mut vis_scratch = VisionScratchpad::new(cfg);
        println!("Encoding image...");
        let n = vision_encoder.encode_image(&pixels, img_w, img_h, &mut vis_scratch);
        println!("Vision tokens: {} (dim={})", n, cfg.projection_dim);
        (n, vis_scratch.projected[..n * cfg.projection_dim].to_vec())
    } else {
        (0usize, Vec::new())
    };
    let vis_embeddings = &vis_embeddings_vec[..];
    if has_image {
        println!("First 5 vision embedding values: {:?}", &vis_embeddings[..5.min(vis_embeddings.len())]);
    }

    let mut llm = Qwen35Model::from_gguf(&llm_loader).unwrap_or_else(|e| { eprintln!("Failed to parse Qwen3.5 model: {}", e); std::process::exit(1); });
    println!("Qwen3.5 model loaded: {} layers, n_embd={}, n_head={}, n_ff={}, rope_freq_base={}, rope_sections={:?}, rope_dim_count={}", llm.config.n_layer, llm.config.n_embd, llm.config.n_head, llm.config.n_ff, llm.config.rope_freq_base, llm.config.rope_dimension_sections, llm.config.rope_dimension_count);
    // llm.precompute_f32();

    let mut tokenizer = BPETokenizer::from_gguf_metadata(|k| llm_loader.metadata(k).cloned())
        .unwrap_or_else(|e| { eprintln!("Failed to init tokenizer: {}", e); std::process::exit(1); });
    let special_tokens = detect_special_tokens(&llm_loader, &tokenizer);
    tokenizer.set_special_tokens(special_tokens.clone());

    let im_start = *special_tokens.get("im_start").unwrap_or(&248045u32) as i32;
    let im_end = *special_tokens.get("im_end").unwrap_or(&248046u32) as i32;
    let image_token_id = *special_tokens.get("image_pad").unwrap_or(&248056u32) as i32;

    let mut prompt_tokens: Vec<i32> = Vec::new();
    prompt_tokens.push(im_start);
    prompt_tokens.extend(tokenizer.encode("user\n").iter().map(|&t| t as i32));
    if !image_path.is_empty() {
        for _ in 0..n_vis_tokens {
            prompt_tokens.push(image_token_id);
        }
    }
    prompt_tokens.extend(tokenizer.encode(prompt).iter().map(|&t| t as i32));
    prompt_tokens.push(im_end);
    prompt_tokens.push(im_start);
    prompt_tokens.extend(tokenizer.encode("assistant\n").iter().map(|&t| t as i32));

    println!("Prompt tokens: {} (including {} vision placeholders)", prompt_tokens.len(), n_vis_tokens);

    let max_seq = llm.config.n_ctx;
    let mut kv_cache = crate::scratchpad::KvCache::new_f32(
        llm.config.n_layer,
        max_seq,
        llm.config.n_embd_head() * llm.config.n_head_kv,
    );
    let mut llm_scratch = crate::qwen35::Qwen35Scratchpad::new(&llm.config, prompt_tokens.len().max(max_tokens));

    let prompt_embd = inject_vision_embeddings(&llm, &prompt_tokens, vis_embeddings, n_vis_tokens, llm.config.n_embd);

    let n_prompt = prompt_tokens.len();
    let mut all_tokens = prompt_tokens.clone();

    let image_token_id_i32: i32 = *special_tokens.get("image_pad").unwrap_or(&248056u32) as i32;
    let spatial_merge = 2usize;
    let patch_size = 16usize;
    let vis_nx = if n_vis_tokens > 0 { (256 / patch_size) / spatial_merge } else { 0 };
    let vis_ny = if n_vis_tokens > 0 { (256 / patch_size) / spatial_merge } else { 0 };
    let vis_n_pos = if n_vis_tokens > 0 { vis_nx.max(vis_ny) } else { 0 };

    let mut mrope_positions_prompt: Vec<[usize; 4]> = Vec::with_capacity(n_prompt);
    {
        let mut seq_pos = 0usize;
        let mut vis_start_pos: Option<usize> = None;
        let mut vis_idx = 0usize;
        for t in 0..n_prompt {
            if prompt_tokens[t] == image_token_id_i32 && vis_idx < n_vis_tokens {
                if vis_start_pos.is_none() {
                    vis_start_pos = Some(seq_pos);
                }
                let sp = vis_start_pos.unwrap();
                let row = vis_idx / vis_nx;
                let col = vis_idx % vis_nx;
                mrope_positions_prompt.push([sp, sp + col, sp + row, 0]);
                vis_idx += 1;
            } else {
                mrope_positions_prompt.push([seq_pos, seq_pos, seq_pos, 0]);
                seq_pos += 1;
            }
        }
        if vis_idx > 0 {
            seq_pos += vis_n_pos;
        }
    }

    let n_threads = if n_threads_arg > 0 { n_threads_arg } else { 8 };
    let pool = std::sync::Arc::new(crate::thread_pool::ComputePool::new(n_threads));
    eprintln!("compute pool: {} threads", pool.n_threads());

    let mut generated = String::new();
    println!("\n--- Generation ---");
    let t_gen_start = std::time::Instant::now();

    for step in 0..max_tokens {
        let tokens = if step == 0 { &prompt_tokens } else { &all_tokens[all_tokens.len()-1..all_tokens.len()-1+1] };
        let n_tok = tokens.len();

        if step == 0 {
            for t in 0..n_prompt {
                let embd_off = t * llm.config.n_embd;
                llm_scratch.x[embd_off..embd_off + llm.config.n_embd].copy_from_slice(&prompt_embd[embd_off..embd_off + llm.config.n_embd]);
            }
        } else {
            let tok = tokens[0] as usize;
            let tok_off = tok * llm.config.n_embd;
            for e in 0..llm.config.n_embd {
                if tok_off + e < llm.tok_embd.len() {
                    llm_scratch.x[e] = llm.tok_embd[tok_off + e];
                }
            }
        }

        let mrope_ref = if step == 0 {
            Some(&mrope_positions_prompt[..])
        } else {
            None
        };
        let logits = llm.forward(n_tok, &mut kv_cache, &mut llm_scratch, &pool, mrope_ref);

        let next_token = if temperature <= 0.0 {
            logits.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).map(|(i, _)| i as i32).unwrap_or(0)
        } else {
            sample_token(&logits, temperature)
        };

        if next_token == 248046 || next_token == im_end { break; }

        let token_str = decode_token(&llm_loader, next_token);
        generated.push_str(&token_str);
        print!("{}", token_str);
        std::io::Write::flush(&mut std::io::stdout()).ok();

        all_tokens.push(next_token);
    }

    let gen_ms = t_gen_start.elapsed().as_millis();
    let n_gen = all_tokens.len() - n_prompt;
    let tok_s = if gen_ms > 0 { n_gen as f64 / gen_ms as f64 * 1000.0 } else { 0.0 };
    println!("\n--- End ---");
    eprintln!("[{} gen tokens in {}ms | {:.1} tok/s]", n_gen, gen_ms, tok_s);
}
