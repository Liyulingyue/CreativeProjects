use std::io::{self, Write};

use rust_model_inference::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut model_path = String::new();
    let mut prompt = String::new();
    let mut max_tokens = 128usize;
    let mut temperature = 0.6f32;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--model" => { if i + 1 < args.len() { model_path = args[i + 1].clone(); i += 1; } }
            "--prompt" => { if i + 1 < args.len() { prompt = args[i + 1].clone(); i += 1; } }
            "--max-tokens" => { if i + 1 < args.len() { max_tokens = args[i + 1].parse().unwrap_or(128); i += 1; } }
            "--temp" => { if i + 1 < args.len() { temperature = args[i + 1].parse().unwrap_or(0.6); i += 1; } }
            _ => {}
        }
        i += 1;
    }

    if !model_path.is_empty() && !prompt.is_empty() {
        run_inference(&model_path, &prompt, max_tokens, temperature);
    } else if !model_path.is_empty() {
        run_interactive(&model_path, max_tokens, temperature);
    } else {
        run_self_test();
    }
}

fn run_inference(model_path: &str, prompt: &str, max_tokens: usize, temperature: f32) {
    println!("Loading {} ...", model_path);
    let loader = GGUFLoader::from_file(model_path).expect("Failed to load GGUF");
    let config = loader.model_config().expect("Failed to parse model config");

    let arch = loader.metadata("general.architecture").and_then(|v| v.to_string_val()).unwrap_or_default();
    let is_qwen3 = arch == "qwen3";

    println!("Model: {} | n_embd={} n_layer={} n_head={} n_head_kv={} n_ff={}",
        arch, config.n_embd, config.n_layer, config.n_head, config.n_head_kv, config.n_ff);

    let tokenizer = BPETokenizer::from_gguf_metadata(|k| loader.metadata(k).cloned())
        .expect("Failed to init tokenizer");
    println!("Tokenizer: vocab_size={} bos={} eos={}", tokenizer.vocab_size(), tokenizer.bos_id(), tokenizer.eos_id());

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
    println!("  head_k={} head_v={} n_embd_q={} n_embd_gqa={}", n_embd_head_k, n_embd_head_v, n_embd_q, n_embd_gqa);
    let n_ff = config.n_ff;
    let eps = config.norm_eps;
    let freq_base = config.rope_freq_base;

    let debug_layer = std::env::var("DEBUG_LAYER").ok().and_then(|s| s.parse::<usize>().ok()).unwrap_or(999);
    let debug_pos = std::env::var("DEBUG_POS").ok().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
    let verbose = std::env::var("VERBOSE").is_ok();

    let k_cache = vec![0.0f32; n_layer * max_ctx * n_embd_gqa];
    let v_cache = vec![0.0f32; n_layer * max_ctx * n_embd_gqa];
    let mut k_cache = k_cache.into_boxed_slice();
    let mut v_cache = v_cache.into_boxed_slice();

    let mut x = vec![0.0f32; n_embd];
    let mut normed = vec![0.0f32; n_embd];
    let mut q = vec![0.0f32; n_embd_q];
    let mut k_new = vec![0.0f32; n_embd_gqa];
    let mut v_new = vec![0.0f32; n_embd_gqa];
    let mut attn_out = vec![0.0f32; n_embd_q];
    let mut scores = vec![0.0f32; max_ctx];
    let mut gate_buf = vec![0.0f32; n_ff];
    let mut up_buf = vec![0.0f32; n_ff];
    let mut row_buf = vec![0.0f32; n_ff.max(n_embd_q)];
    let mut logits = vec![0.0f32; tokenizer.vocab_size()];

    let embd_weight = loader.tensor_slice("token_embd.weight").expect("no embd");
    let output_norm = get_f32_tensor(&loader, "output_norm.weight", n_embd);

    let output_weight = loader.tensor_slice("output.weight");
    let output_weight = match output_weight {
        Some(w) => w,
        None => embd_weight,
    };

    let prompt_tokens = tokenizer.encode(prompt);
    println!("Prompt: {} ({} tokens)", prompt, prompt_tokens.len());
    if verbose {
        for &tid in &prompt_tokens {
            println!("  token {} = {:?}", tid, tokenizer.token_str(tid));
        }
    }

    let mut input_tokens: Vec<u32> = Vec::new();

    let im_start_id = tokenizer.encode("<|im_start|>");
    let has_chat_template = im_start_id.len() == 1 && im_start_id[0] > 100000;

    if has_chat_template {
        input_tokens.push(im_start_id[0]);
        let user_id = tokenizer.encode("user\n");
        input_tokens.extend_from_slice(&user_id);
        input_tokens.extend_from_slice(&prompt_tokens);
        let im_end_id = tokenizer.encode("<|im_end|>");
        input_tokens.push(im_end_id[0]);
        input_tokens.push(im_start_id[0]);
        let asst_id = tokenizer.encode("assistant\n");
        input_tokens.extend_from_slice(&asst_id);
    } else {
        input_tokens.extend_from_slice(&prompt_tokens);
    }

    let eos_id = tokenizer.eos_id();
    let mut generated_tokens: Vec<u32> = Vec::new();
    let mut all_tokens: Vec<u32> = input_tokens.clone();

    print!("Output: ");
    io::stdout().flush().unwrap();

    for step in 0..(input_tokens.len() + max_tokens) {
        let token_id = if step < input_tokens.len() {
            input_tokens[step]
        } else {
            *generated_tokens.last().unwrap_or(&0)
        };

        let pos = step;

        embedding_lookup_q8_0(embd_weight, token_id, n_embd, &mut x);

        for layer in 0..n_layer {
            let attn_norm = get_f32_tensor(&loader, &format!("blk.{}.attn_norm.weight", layer), n_embd);
            rms_norm(&x, &attn_norm, &mut normed, eps);

            let wq = loader.tensor_slice(&format!("blk.{}.attn_q.weight", layer)).unwrap_or_else(|| panic!("missing blk.{}.attn_q.weight", layer));
            let wk = loader.tensor_slice(&format!("blk.{}.attn_k.weight", layer)).unwrap_or_else(|| panic!("missing blk.{}.attn_k.weight", layer));
            let wv = loader.tensor_slice(&format!("blk.{}.attn_v.weight", layer)).unwrap_or_else(|| panic!("missing blk.{}.attn_v.weight", layer));

            matmul_q8_0(wq, &normed, &mut q, n_embd, n_embd_q, &mut row_buf);
            matmul_q8_0(wk, &normed, &mut k_new, n_embd, n_embd_gqa, &mut row_buf);
            matmul_q8_0(wv, &normed, &mut v_new, n_embd, n_embd_gqa, &mut row_buf);

            if is_qwen3 {
                let q_norm = get_f32_tensor(&loader, &format!("blk.{}.attn_q_norm.weight", layer), n_embd_head_k);
                let k_norm = get_f32_tensor(&loader, &format!("blk.{}.attn_k_norm.weight", layer), n_embd_head_k);
                for h in 0..n_head {
                    let off = h * n_embd_head_k;
                    rms_norm_inplace(&mut q[off..off + n_embd_head_k], &q_norm, eps);
                }
                for h in 0..n_head_kv {
                    let off = h * n_embd_head_k;
                    rms_norm_inplace(&mut k_new[off..off + n_embd_head_k], &k_norm, eps);
                }
            }

            rope_neox(&mut q, pos, n_embd_head_k, freq_base);
            rope_neox(&mut k_new, pos, n_embd_head_v, freq_base);

            if pos == debug_pos && layer == debug_layer {
                eprintln!("  [L{}] after Q/K norm+RoPE: q[0..4]={:?} k[0..4]={:?}", layer, &q[..4], &k_new[..4]);
            }

            let k_base = layer * max_ctx * n_embd_gqa;
            let v_base = layer * max_ctx * n_embd_gqa;
            k_cache[k_base + pos * n_embd_gqa..k_base + (pos + 1) * n_embd_gqa]
                .copy_from_slice(&k_new);
            v_cache[v_base + pos * n_embd_gqa..v_base + (pos + 1) * n_embd_gqa]
                .copy_from_slice(&v_new);

            let group_size = n_head / n_head_kv;
            let kq_scale = 1.0f32 / (n_embd_head_k as f32).sqrt();

            for h in 0..n_head {
                let kv_h = h / group_size;
                let q_off = h * n_embd_head_k;

                let n_cached = pos + 1;
                for t in 0..n_cached {
                    let mut score = 0.0f32;
                    for d in 0..n_embd_head_k {
                        let k_val = k_cache[k_base + t * n_embd_gqa + kv_h * n_embd_head_v + d];
                        score += q[q_off + d] * k_val;
                    }
                    scores[t] = score * kq_scale;
                }
                softmax(&mut scores[..n_cached]);

                for d in 0..n_embd_head_v {
                    let mut val = 0.0f32;
                    for t in 0..n_cached {
                        let v_val = v_cache[v_base + t * n_embd_gqa + kv_h * n_embd_head_v + d];
                        val += scores[t] * v_val;
                    }
                    attn_out[h * n_embd_head_v + d] = val;
                }
            }

            let wo = loader.tensor_slice(&format!("blk.{}.attn_output.weight", layer)).unwrap_or_else(|| panic!("missing blk.{}.attn_output.weight", layer));
            let mut attn_proj = vec![0.0f32; n_embd];
            matmul_q8_0(wo, &attn_out, &mut attn_proj, n_embd_q, n_embd, &mut row_buf);
            for i in 0..n_embd { x[i] += attn_proj[i]; }

            let ffn_norm = get_f32_tensor(&loader, &format!("blk.{}.ffn_norm.weight", layer), n_embd);
            rms_norm(&x, &ffn_norm, &mut normed, eps);

            let w_gate = loader.tensor_slice(&format!("blk.{}.ffn_gate.weight", layer)).unwrap();
            let w_up = loader.tensor_slice(&format!("blk.{}.ffn_up.weight", layer)).unwrap();
            let w_down = loader.tensor_slice(&format!("blk.{}.ffn_down.weight", layer)).unwrap();

            matmul_q8_0(w_gate, &normed, &mut gate_buf, n_embd, n_ff, &mut row_buf);
            matmul_q8_0(w_up, &normed, &mut up_buf, n_embd, n_ff, &mut row_buf);

            for i in 0..n_ff { gate_buf[i] = silu(gate_buf[i]) * up_buf[i]; }

            let mut down_buf = vec![0.0f32; n_embd];
            matmul_q8_0(w_down, &gate_buf, &mut down_buf, n_ff, n_embd, &mut row_buf);
            for i in 0..n_embd { x[i] += down_buf[i]; }

            if pos == debug_pos && layer == debug_layer {
                eprintln!("  [L{}] after ffn: x[0..4]={:?}", layer, &x[..4]);
            }
        }

        if step < input_tokens.len() - 1 { continue; }

        rms_norm(&x, &output_norm, &mut normed, eps);

        let vocab = tokenizer.vocab_size();
        logits.resize(vocab, 0.0);
        matmul_q8_0(output_weight, &normed, &mut logits, n_embd, vocab, &mut row_buf);

        if generated_tokens.is_empty() && verbose {
            let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let min_logit = logits.iter().cloned().fold(f32::INFINITY, f32::min);
            let mean_logit: f32 = logits.iter().sum::<f32>() / logits.len() as f32;
            eprintln!("  logits: min={:.4} max={:.4} mean={:.4}", min_logit, max_logit, mean_logit);
        }

        let next_token = {
            if temperature > 0.0 {
                for l in logits.iter_mut() { *l /= temperature; }
            }
            softmax(&mut logits[..vocab]);
            let mut ranked: Vec<usize> = (0..vocab).collect();
            ranked.sort_by(|&a, &b| logits[b].partial_cmp(&logits[a]).unwrap_or(std::cmp::Ordering::Equal));
            let top_k = 40.min(vocab);
            for i in top_k..vocab { logits[ranked[i]] = 0.0; }
            let mut cum = 0.0f32;
            let mut keep = top_k;
            for i in 0..top_k {
                cum += logits[ranked[i]];
                if cum >= 0.9 { keep = i + 1; break; }
            }
            for i in keep..top_k { logits[ranked[i]] = 0.0; }
            let sum: f32 = logits.iter().sum();
            if sum > 0.0 { for v in logits.iter_mut() { *v /= sum; } }

            if generated_tokens.is_empty() && verbose {
                eprintln!("\n  top10 after sampling:");
                let mut top: Vec<(usize, f32)> = logits.iter().enumerate().filter(|&(_, p)| *p > 0.0).map(|(i, p)| (i, *p)).collect();
                top.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                for (rank, (idx, prob)) in top[..10.min(top.len())].iter().enumerate() {
                    eprintln!("    #{} token={} ({:?}) prob={:.6}", rank+1, idx, tokenizer.token_str(*idx as u32), prob);
                }
            }

            let mut rng = 0u64;
            for &t in &all_tokens { rng = rng.wrapping_mul(6364136223846793005).wrapping_add(t as u64); }
            let r = ((rng >> 33) as f32) / (1u64 << 31) as f32;
            let mut cum = 0.0f32;
            let mut chosen = argmax(&logits[..vocab]);
            for i in 0..vocab {
                cum += logits[i];
                if cum >= r { chosen = i; break; }
            }
            chosen
        };

        if next_token == eos_id as usize { break; }
        if generated_tokens.len() >= max_tokens { break; }

        generated_tokens.push(next_token as u32);
        all_tokens.push(next_token as u32);

        let text = tokenizer.decode(&[next_token as u32]);
        print!("{}", text);
        io::stdout().flush().unwrap();
    }

    println!();
    println!("\n[{} tokens generated in {} steps]", generated_tokens.len(), all_tokens.len());
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

fn run_interactive(model_path: &str, max_tokens: usize, temperature: f32) {
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
        run_inference(model_path, line, max_tokens, temperature);
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
