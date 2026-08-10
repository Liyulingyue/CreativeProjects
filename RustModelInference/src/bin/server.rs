use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

use rust_model_inference::*;

struct Qwen3State {
    layers: Vec<LayerWeightsOwned>,
    output_norm: Vec<f32>,
    embd_weight: &'static [u8],
    output_weight: &'static [u8],
    config: Qwen3Config,
    tokenizer: Arc<BPETokenizer>,
    pool: Arc<thread_pool::ComputePool>,
}

struct Qwen35State {
    model: Arc<Qwen35Model>,
    tokenizer: Arc<BPETokenizer>,
    pool: Arc<thread_pool::ComputePool>,
}

struct Qwen3Config {
    n_embd: usize,
    n_layer: usize,
    n_head: usize,
    n_head_kv: usize,
    n_embd_head: usize,
    n_embd_head_k: usize,
    n_embd_head_v: usize,
    n_ff: usize,
    vocab: usize,
    n_ctx: usize,
    eps: f32,
    freq_base: f32,
    has_qk_norm: bool,
}

struct LayerWeightsOwned {
    attn_norm: Vec<f32>,
    ffn_norm: Vec<f32>,
    q_norm: Option<Vec<f32>>,
    k_norm: Option<Vec<f32>>,
    wq: &'static [u8],
    wk: &'static [u8],
    wv: &'static [u8],
    wo: &'static [u8],
    w_gate: &'static [u8],
    w_up: &'static [u8],
    w_down: &'static [u8],
}

enum ModelBackend {
    Qwen3(Qwen3State),
    Qwen35(Qwen35State),
}

unsafe impl Send for ModelBackend {}
unsafe impl Sync for ModelBackend {}
unsafe impl Send for LayerWeightsOwned {}
unsafe impl Sync for LayerWeightsOwned {}
unsafe impl Send for Qwen3State {}
unsafe impl Sync for Qwen3State {}
unsafe impl Send for Qwen35State {}
unsafe impl Sync for Qwen35State {}

#[derive(Clone)]
struct AppState {
    model: Arc<ModelBackend>,
    model_name: String,
}

#[derive(Deserialize)]
struct ChatCompletionRequest {
    model: Option<String>,
    messages: Vec<ChatMessage>,
    temperature: Option<f32>,
    max_tokens: Option<usize>,
    stream: Option<bool>,
}

#[derive(Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatCompletionResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<ChatChoice>,
    usage: Usage,
}

#[derive(Serialize)]
struct ChatChoice {
    index: usize,
    message: ChatResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Serialize)]
struct ChatResponseMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct Usage {
    prompt_tokens: usize,
    completion_tokens: usize,
    total_tokens: usize,
}

#[derive(Serialize)]
struct ChatCompletionChunk {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<ChunkChoice>,
}

#[derive(Serialize)]
struct ChunkChoice {
    index: usize,
    delta: ChunkDelta,
    finish_reason: Option<String>,
}

#[derive(Serialize)]
struct ChunkDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
}

#[derive(Serialize)]
struct ModelsResponse {
    object: String,
    data: Vec<ModelInfo>,
}

#[derive(Serialize)]
struct ModelInfo {
    id: String,
    object: String,
    created: u64,
    owned_by: String,
}

fn make_id() -> String {
    format!("chatcmpl-{}", rand::random::<u64>())
}

fn sample_token_from_logits(logits: &[f32], temperature: f32) -> i32 {
    if temperature <= 0.0 {
        return logits
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i as i32)
            .unwrap_or(0);
    }
    let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let mut sum = 0.0f32;
    let mut probs = vec![0.0f32; logits.len()];
    for (i, l) in logits.iter().enumerate() {
        probs[i] = ((l - max_logit) / temperature).exp();
        sum += probs[i];
    }
    for p in probs.iter_mut() {
        *p /= sum;
    }
    let r: f32 = rand::random();
    let mut cumsum = 0.0f32;
    for (i, p) in probs.iter().enumerate() {
        cumsum += p;
        if cumsum >= r {
            return i as i32;
        }
    }
    (logits.len() - 1) as i32
}

async fn health() -> &'static str {
    "ok"
}

async fn list_models(State(state): State<AppState>) -> Json<ModelsResponse> {
    Json(ModelsResponse {
        object: "list".to_string(),
        data: vec![ModelInfo {
            id: state.model_name.clone(),
            object: "model".to_string(),
            created: 0,
            owned_by: "local".to_string(),
        }],
    })
}

async fn chat_completions(
    State(state): State<AppState>,
    Json(req): Json<ChatCompletionRequest>,
) -> impl IntoResponse {
    let temperature = req.temperature.unwrap_or(0.6);
    let max_tokens = req.max_tokens.unwrap_or(512);
    let stream = req.stream.unwrap_or(false);

    if stream {
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, axum::Error>>(32);
        let model = state.model.clone();
        let model_name = state.model_name.clone();

        tokio::task::spawn_blocking(move || {
            let id = make_id();
            let created = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let chunk_zero = ChatCompletionChunk {
                id: id.clone(),
                object: "chat.completion.chunk".to_string(),
                created,
                model: model_name.clone(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: ChunkDelta {
                        role: Some("assistant".to_string()),
                        content: None,
                    },
                    finish_reason: None,
                }],
            };
            let data = serde_json::to_string(&chunk_zero).unwrap();
            if tx.blocking_send(Ok(Event::default().data(data))).is_err() {
                return;
            }

            let result = match generate(&model, &req.messages, max_tokens, temperature) {
                Ok(result) => result,
                Err(error) => {
                    let data = serde_json::json!({ "error": error }).to_string();
                    let _ = tx.blocking_send(Ok(Event::default().event("error").data(data)));
                    return;
                }
            };

            for text in &result.tokens {
                let chunk = ChatCompletionChunk {
                    id: id.clone(),
                    object: "chat.completion.chunk".to_string(),
                    created,
                    model: model_name.clone(),
                    choices: vec![ChunkChoice {
                        index: 0,
                        delta: ChunkDelta {
                            role: None,
                            content: Some(text.clone()),
                        },
                        finish_reason: None,
                    }],
                };
                let data = serde_json::to_string(&chunk).unwrap();
                if tx.blocking_send(Ok(Event::default().data(data))).is_err() {
                    return;
                }
            }

            let chunk_end = ChatCompletionChunk {
                id,
                object: "chat.completion.chunk".to_string(),
                created,
                model: model_name,
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: ChunkDelta {
                        role: None,
                        content: None,
                    },
                    finish_reason: Some("stop".to_string()),
                }],
            };
            let data = serde_json::to_string(&chunk_end).unwrap();
            let _ = tx.blocking_send(Ok(Event::default().data(data)));
        });

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        let sse = Sse::new(stream).keep_alive(KeepAlive::default());
        (StatusCode::OK, [("content-type", "text/event-stream")], sse).into_response()
    } else {
        let id = make_id();
        let created = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let model_clone = state.model.clone();
        let result = match tokio::task::spawn_blocking(move || {
            generate(&model_clone, &req.messages, max_tokens, temperature)
        })
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(ErrorResponse { error }),
                )
                    .into_response();
            }
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("generation worker failed: {error}"),
                    }),
                )
                    .into_response();
            }
        };

        let response = ChatCompletionResponse {
            id,
            object: "chat.completion".to_string(),
            created,
            model: state.model_name.clone(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: "assistant".to_string(),
                    content: result.text,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: Usage {
                prompt_tokens: result.prompt_tokens,
                completion_tokens: result.completion_tokens,
                total_tokens: result.prompt_tokens + result.completion_tokens,
            },
        };

        (StatusCode::OK, Json(response)).into_response()
    }
}

struct GenerateResult {
    text: String,
    tokens: Vec<String>,
    prompt_tokens: usize,
    completion_tokens: usize,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

fn server_prompt_tokens(
    tokenizer: &BPETokenizer,
    messages: &[ChatMessage],
) -> Result<Vec<u32>, String> {
    let prompt_messages: Vec<QwenMessage<'_>> = messages
        .iter()
        .map(|message| QwenMessage {
            role: &message.role,
            content: &message.content,
        })
        .collect();
    build_qwen_chat_prompt(tokenizer, &prompt_messages)
}

fn generate(
    model: &ModelBackend,
    messages: &[ChatMessage],
    max_tokens: usize,
    temperature: f32,
) -> Result<GenerateResult, String> {
    match model {
        ModelBackend::Qwen3(s) => generate_qwen3(s, messages, max_tokens, temperature),
        ModelBackend::Qwen35(s) => generate_qwen35(s, messages, max_tokens, temperature),
    }
}

fn generate_qwen3(
    s: &Qwen3State,
    messages: &[ChatMessage],
    max_tokens: usize,
    temperature: f32,
) -> Result<GenerateResult, String> {
    let cfg = &s.config;
    let n_embd = cfg.n_embd;
    let n_layer = cfg.n_layer;
    let n_head = cfg.n_head;
    let n_head_kv = cfg.n_head_kv;
    let n_embd_head_k = cfg.n_embd_head_k;
    let n_embd_head_v = cfg.n_embd_head_v;
    let n_embd_q = n_head * n_embd_head_k;
    let n_embd_gqa = n_head_kv * n_embd_head_v;
    let n_ff = cfg.n_ff;
    let eps = cfg.eps;
    let freq_base = cfg.freq_base;
    let vocab = cfg.vocab;
    let max_ctx = cfg.n_ctx;
    let group_size = n_head / n_head_kv;
    let kq_scale = 1.0f32 / (n_embd_head_k as f32).sqrt();

    let input_tokens = server_prompt_tokens(&s.tokenizer, messages)?;

    let n_prompt = input_tokens.len();
    let mut kv_cache = KvCache::new_f16(n_layer, max_ctx, n_embd_gqa);
    let mut scratch = ExecutionScratchpad::new(
        n_embd,
        n_embd_q,
        n_embd_gqa,
        n_ff,
        vocab,
        s.pool.n_threads(),
        max_ctx,
    );
    let mut all_tokens: Vec<u32> = input_tokens.clone();
    let mut generated_tokens: Vec<u32> = Vec::new();
    let mut token_strings: Vec<String> = Vec::new();
    let mut decoder = s.tokenizer.streaming_decoder(false);

    for step in 0..(n_prompt + max_tokens) {
        let token_id = if step < n_prompt {
            input_tokens[step]
        } else {
            *generated_tokens.last().unwrap_or(&0)
        };
        let pos = step;

        embedding_lookup_q8_0(s.embd_weight, token_id, n_embd, &mut scratch.x);

        for layer in 0..n_layer {
            let lw = &s.layers[layer];

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

            let max_n_in = n_embd_q.max(n_ff);
            let x = unsafe { std::slice::from_raw_parts_mut(x_ptr, n_embd) };
            let normed = unsafe { std::slice::from_raw_parts_mut(normed_ptr, n_embd) };
            let q8_buf = unsafe { std::slice::from_raw_parts_mut(q8_buf_ptr, max_n_in) };
            let scale_buf = unsafe { std::slice::from_raw_parts_mut(scale_buf_ptr, max_n_in / 32) };

            rms_norm(x, &lw.attn_norm, normed, eps);
            quantize_q8_0_into(
                normed,
                n_embd,
                &mut q8_buf[..n_embd],
                &mut scale_buf[..n_embd / 32],
            );
            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();

            let wq = lw.wq;
            let wk = lw.wk;
            let wv = lw.wv;
            let n_embd_v = n_embd;
            let n_embd_q_v = n_embd_q;
            let n_embd_gqa_v = n_embd_gqa;
            let pool = s.pool.clone();

            pool.compute(move |ith: usize, nth: usize| {
                let q8 = unsafe { std::slice::from_raw_parts(q8, n_embd_v) };
                let sc = unsafe { std::slice::from_raw_parts(sc, n_embd_v / 32) };
                let q = unsafe { std::slice::from_raw_parts_mut(q_ptr, n_embd_q_v) };
                let k_new = unsafe { std::slice::from_raw_parts_mut(k_ptr, n_embd_gqa_v) };
                let v_new = unsafe { std::slice::from_raw_parts_mut(v_ptr, n_embd_gqa_v) };
                matmul_q8_0_quantized_parallel_rows(wq, q8, sc, q, n_embd_v, n_embd_q_v, ith, nth);
                matmul_q8_0_quantized_parallel_rows(
                    wk,
                    q8,
                    sc,
                    k_new,
                    n_embd_v,
                    n_embd_gqa_v,
                    ith,
                    nth,
                );
                matmul_q8_0_quantized_parallel_rows(
                    wv,
                    q8,
                    sc,
                    v_new,
                    n_embd_v,
                    n_embd_gqa_v,
                    ith,
                    nth,
                );
            });

            {
                let q = unsafe { std::slice::from_raw_parts_mut(q_ptr, n_embd_q) };
                let k_new = unsafe { std::slice::from_raw_parts_mut(k_ptr, n_embd_gqa) };
                let v_new = unsafe { std::slice::from_raw_parts_mut(v_ptr, n_embd_gqa) };
                let q_norm = lw.q_norm.as_deref();
                let k_norm = lw.k_norm.as_deref();

                if let (Some(qn), Some(kn)) = (q_norm, k_norm) {
                    for h in 0..n_head {
                        rms_norm_inplace(
                            &mut q[h * n_embd_head_k..(h + 1) * n_embd_head_k],
                            qn,
                            eps,
                        );
                    }
                    for h in 0..n_head_kv {
                        rms_norm_inplace(
                            &mut k_new[h * n_embd_head_k..(h + 1) * n_embd_head_k],
                            kn,
                            eps,
                        );
                    }
                }

                for h in 0..n_head {
                    rope_neox(
                        &mut q[h * n_embd_head_k..(h + 1) * n_embd_head_k],
                        pos,
                        n_embd_head_k,
                        freq_base,
                    );
                }
                for h in 0..n_head_kv {
                    rope_neox(
                        &mut k_new[h * n_embd_head_k..(h + 1) * n_embd_head_k],
                        pos,
                        n_embd_head_k,
                        freq_base,
                    );
                }

                let kb = layer * max_ctx * n_embd_gqa;
                let k_cache =
                    unsafe { std::slice::from_raw_parts_mut(k_cache_f16_ptr, kv_cache_size) };
                let v_cache =
                    unsafe { std::slice::from_raw_parts_mut(v_cache_f16_ptr, kv_cache_size) };
                for h in 0..n_head_kv {
                    let off = h * n_embd_head_k;
                    f32_slice_to_f16(
                        &k_new[off..off + n_embd_head_k],
                        &mut k_cache[kb + pos * n_embd_gqa + off
                            ..kb + pos * n_embd_gqa + off + n_embd_head_k],
                    );
                    f32_slice_to_f16(
                        &v_new[off..off + n_embd_head_v],
                        &mut v_cache[kb + pos * n_embd_gqa + off
                            ..kb + pos * n_embd_gqa + off + n_embd_head_v],
                    );
                }
            }

            let pool2 = s.pool.clone();
            pool2.compute(move |ith: usize, nth: usize| {
                let q = unsafe { std::slice::from_raw_parts(q_ptr, n_embd_q) };
                let attn_out = unsafe { std::slice::from_raw_parts_mut(attn_out_ptr, n_embd_q) };
                let h_start = ith * n_head / nth;
                let h_end = (ith + 1) * n_head / nth;

                let kb = layer * max_ctx * n_embd_gqa;
                let k_cache = unsafe { std::slice::from_raw_parts(k_cache_f16_ptr, kv_cache_size) };
                let v_cache = unsafe { std::slice::from_raw_parts(v_cache_f16_ptr, kv_cache_size) };

                for h in h_start..h_end {
                    let kv_h = h / group_size;
                    let q_off = h * n_embd_head_k;
                    let n_cached = pos + 1;
                    let out_base = h * n_embd_head_v;
                    let mut ms = 0.0f32;
                    let mut s_sum = 0.0f32;
                    for d in 0..n_embd_head_v {
                        attn_out[out_base + d] = 0.0;
                    }
                    for t in 0..n_cached {
                        let score = dot_f16_f32(
                            &q[q_off..q_off + n_embd_head_k],
                            &k_cache[kb + t * n_embd_gqa + kv_h * n_embd_head_v
                                ..kb + t * n_embd_gqa + kv_h * n_embd_head_v + n_embd_head_k],
                            n_embd_head_k,
                        ) * kq_scale;
                        if score > ms {
                            let rescale = (ms - score).exp();
                            vec_scale_f32(
                                &mut attn_out[out_base..out_base + n_embd_head_v],
                                rescale,
                            );
                            s_sum *= rescale;
                            ms = score;
                        }
                        let vs = (score - ms).exp();
                        let v_base = kb + t * n_embd_gqa + kv_h * n_embd_head_v;
                        vec_mad_f16_f32(
                            &mut attn_out[out_base..out_base + n_embd_head_v],
                            &v_cache[v_base..v_base + n_embd_head_v],
                            vs,
                        );
                        s_sum += vs;
                    }
                    let inv_sum = 1.0 / s_sum;
                    vec_scale_f32(&mut attn_out[out_base..out_base + n_embd_head_v], inv_sum);
                }
            });

            let attn_out = unsafe { std::slice::from_raw_parts_mut(attn_out_ptr, n_embd_q) };
            let q8_buf = unsafe { std::slice::from_raw_parts_mut(q8_buf_ptr, max_n_in) };
            let scale_buf = unsafe { std::slice::from_raw_parts_mut(scale_buf_ptr, max_n_in / 32) };
            quantize_q8_0_into(
                attn_out,
                n_embd_q,
                &mut q8_buf[..n_embd_q],
                &mut scale_buf[..n_embd_q / 32],
            );
            let q8 = q8_buf[..n_embd_q].as_ptr();
            let sc = scale_buf[..n_embd_q / 32].as_ptr();
            let wo = lw.wo;
            let pool3 = s.pool.clone();
            pool3.compute(move |ith: usize, nth: usize| {
                let q8 = unsafe { std::slice::from_raw_parts(q8, n_embd_q) };
                let sc = unsafe { std::slice::from_raw_parts(sc, n_embd_q / 32) };
                let attn_proj = unsafe { std::slice::from_raw_parts_mut(attn_proj_ptr, n_embd) };
                matmul_q8_0_quantized_parallel_rows(
                    wo, q8, sc, attn_proj, n_embd_q, n_embd, ith, nth,
                );
            });

            let attn_proj = unsafe { std::slice::from_raw_parts_mut(attn_proj_ptr, n_embd) };
            let x = unsafe { std::slice::from_raw_parts_mut(x_ptr, n_embd) };
            for i in 0..n_embd {
                x[i] += attn_proj[i];
            }

            let normed = unsafe { std::slice::from_raw_parts_mut(normed_ptr, n_embd) };
            rms_norm(x, &lw.ffn_norm, normed, eps);
            quantize_q8_0_into(
                normed,
                n_embd,
                &mut q8_buf[..n_embd],
                &mut scale_buf[..n_embd / 32],
            );
            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();
            let w_gate = lw.w_gate;
            let w_up = lw.w_up;
            let pool4 = s.pool.clone();
            pool4.compute(move |ith: usize, nth: usize| {
                let q8 = unsafe { std::slice::from_raw_parts(q8, n_embd) };
                let sc = unsafe { std::slice::from_raw_parts(sc, n_embd / 32) };
                let gate_buf = unsafe { std::slice::from_raw_parts_mut(gate_buf_ptr, n_ff) };
                let up_buf = unsafe { std::slice::from_raw_parts_mut(up_buf_ptr, n_ff) };
                matmul_q8_0_quantized_parallel_rows(
                    w_gate, q8, sc, gate_buf, n_embd, n_ff, ith, nth,
                );
                matmul_q8_0_quantized_parallel_rows(w_up, q8, sc, up_buf, n_embd, n_ff, ith, nth);
                let rows_per = n_ff / nth;
                let r_start = ith * rows_per;
                let r_end = if ith == nth - 1 {
                    n_ff
                } else {
                    r_start + rows_per
                };
                for i in r_start..r_end {
                    gate_buf[i] = silu(gate_buf[i]) * up_buf[i];
                }
            });

            {
                let gate_buf = unsafe { std::slice::from_raw_parts_mut(gate_buf_ptr, n_ff) };
                let q8_buf = unsafe { std::slice::from_raw_parts_mut(q8_buf_ptr, max_n_in) };
                let scale_buf =
                    unsafe { std::slice::from_raw_parts_mut(scale_buf_ptr, max_n_in / 32) };
                quantize_q8_0_into(
                    gate_buf,
                    n_ff,
                    &mut q8_buf[..n_ff],
                    &mut scale_buf[..n_ff / 32],
                );
            }

            let q8 = q8_buf[..n_ff].as_ptr();
            let sc = scale_buf[..n_ff / 32].as_ptr();
            let w_down = lw.w_down;
            let pool5 = s.pool.clone();
            pool5.compute(move |ith: usize, nth: usize| {
                let q8 = unsafe { std::slice::from_raw_parts(q8, n_ff) };
                let sc = unsafe { std::slice::from_raw_parts(sc, n_ff / 32) };
                let down_buf = unsafe { std::slice::from_raw_parts_mut(down_buf_ptr, n_embd) };
                matmul_q8_0_quantized_parallel_rows(
                    w_down, q8, sc, down_buf, n_ff, n_embd, ith, nth,
                );
            });

            let down_buf = unsafe { std::slice::from_raw_parts_mut(down_buf_ptr, n_embd) };
            let x = unsafe { std::slice::from_raw_parts_mut(x_ptr, n_embd) };
            for i in 0..n_embd {
                x[i] += down_buf[i];
            }
        }

        {
            let x = &mut scratch.x;
            let normed = &mut scratch.normed;
            let logits_ptr = scratch.logits.as_mut_ptr();
            let q8_buf = &mut scratch.q8_buf;
            let scale_buf = &mut scratch.scale_buf;

            rms_norm(x, &s.output_norm, normed, eps);
            quantize_q8_0_into(
                normed,
                n_embd,
                &mut q8_buf[..n_embd],
                &mut scale_buf[..n_embd / 32],
            );
            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();
            let ow = s.output_weight;
            let pool6 = s.pool.clone();
            pool6.compute(move |ith: usize, nth: usize| {
                let q8 = unsafe { std::slice::from_raw_parts(q8, n_embd) };
                let sc = unsafe { std::slice::from_raw_parts(sc, n_embd / 32) };
                let logits = unsafe { std::slice::from_raw_parts_mut(logits_ptr, vocab) };
                matmul_q8_0_quantized_parallel_rows(ow, q8, sc, logits, n_embd, vocab, ith, nth);
            });
        }

        if step < n_prompt - 1 {
            continue;
        }

        let next_token = sample_token_from_logits(&scratch.logits, temperature);
        if s.tokenizer.eos_id() == Some(next_token as u32)
            || s.tokenizer.special_token_id("im_end") == Some(next_token as u32)
        {
            break;
        }
        if generated_tokens.len() >= max_tokens {
            break;
        }

        let text = decoder.push(next_token as u32);
        if !text.is_empty() {
            token_strings.push(text);
        }
        generated_tokens.push(next_token as u32);
        all_tokens.push(next_token as u32);
    }

    let tail = decoder.finish();
    if !tail.is_empty() {
        token_strings.push(tail);
    }

    Ok(GenerateResult {
        text: token_strings.join(""),
        tokens: token_strings,
        prompt_tokens: n_prompt,
        completion_tokens: generated_tokens.len(),
    })
}

fn generate_qwen35(
    state: &Qwen35State,
    messages: &[ChatMessage],
    max_tokens: usize,
    temperature: f32,
) -> Result<GenerateResult, String> {
    let prompt_ids = server_prompt_tokens(&state.tokenizer, messages)?;
    let (prompt_positions, mut next_text_position) =
        build_qwen35_positions(&prompt_ids, None, &[])?;
    let prompt_tokens: Vec<i32> = prompt_ids
        .iter()
        .copied()
        .map(|id| i32::try_from(id).map_err(|_| format!("Token ID {id} exceeds i32")))
        .collect::<Result<_, _>>()?;

    let n_prompt = prompt_tokens.len();
    let max_seq = state.model.config.n_ctx;
    let mut kv_cache = KvCache::new_f32(
        state.model.config.n_layer,
        max_seq,
        state.model.config.n_embd_head() * state.model.config.n_head_kv,
    );
    let mut llm_scratch =
        qwen35::Qwen35Scratchpad::new(&state.model.config, n_prompt.max(max_tokens));

    let mut all_tokens = prompt_tokens.clone();
    let mut decoder = state.tokenizer.streaming_decoder(false);
    let mut generated_ids = Vec::<u32>::new();
    let mut rendered_chunks = Vec::<String>::new();

    for step in 0..max_tokens {
        let tokens = if step == 0 {
            &prompt_tokens[..]
        } else {
            &all_tokens[all_tokens.len() - 1..]
        };

        if step == 0 {
            for t in 0..n_prompt {
                let embd_off = t * state.model.config.n_embd;
                let tok = prompt_tokens[t] as usize;
                let tok_off = tok * state.model.config.n_embd;
                for e in 0..state.model.config.n_embd {
                    if tok_off + e < state.model.tok_embd.len() {
                        llm_scratch.x[embd_off + e] = state.model.tok_embd[tok_off + e];
                    }
                }
            }
        } else {
            let tok = tokens[0] as usize;
            let tok_off = tok * state.model.config.n_embd;
            for e in 0..state.model.config.n_embd {
                if tok_off + e < state.model.tok_embd.len() {
                    llm_scratch.x[e] = state.model.tok_embd[tok_off + e];
                }
            }
        }

        let decode_position = [[
            next_text_position,
            next_text_position,
            next_text_position,
            0,
        ]];
        let positions = if step == 0 {
            &prompt_positions[..]
        } else {
            &decode_position[..]
        };
        let logits = state.model.forward(
            tokens.len(),
            &mut kv_cache,
            &mut llm_scratch,
            &state.pool,
            positions,
        )?;
        if step > 0 {
            next_text_position = next_text_position
                .checked_add(1)
                .ok_or("Qwen3.5 server decode position overflow")?;
        }
        let next_token = sample_token_from_logits(&logits, temperature);
        let next_id = u32::try_from(next_token)
            .map_err(|_| format!("Model produced negative token ID {next_token}"))?;
        if state.tokenizer.eos_id() == Some(next_id)
            || state.tokenizer.special_token_id("im_end") == Some(next_id)
        {
            break;
        }
        let rendered = decoder.push(next_id);
        if !rendered.is_empty() {
            rendered_chunks.push(rendered);
        }
        generated_ids.push(next_id);
        all_tokens.push(next_token);
    }

    let tail = decoder.finish();
    if !tail.is_empty() {
        rendered_chunks.push(tail);
    }
    let text = rendered_chunks.concat();
    Ok(GenerateResult {
        text,
        tokens: rendered_chunks,
        prompt_tokens: n_prompt,
        completion_tokens: generated_ids.len(),
    })
}

fn get_f32_tensor_from_loader(loader: &GGUFLoader, name: &str, expected_len: usize) -> Vec<f32> {
    let ti = loader
        .tensor_info(name)
        .unwrap_or_else(|| panic!("tensor {} not found", name));
    let slice = loader
        .tensor_slice(name)
        .unwrap_or_else(|| panic!("slice {} not found", name));
    let mut out = vec![0.0f32; expected_len];
    if ti.ggml_type == GGMLType::F32 {
        let n = expected_len.min(slice.len() / 4);
        for i in 0..n {
            let bytes = [
                slice[i * 4],
                slice[i * 4 + 1],
                slice[i * 4 + 2],
                slice[i * 4 + 3],
            ];
            out[i] = f32::from_le_bytes(bytes);
        }
    }
    out
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut model_path = String::new();
    let mut host = "0.0.0.0".to_string();
    let mut port = 8080u16;
    let mut n_threads = 0usize;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--model" => {
                if i + 1 < args.len() {
                    model_path = args[i + 1].clone();
                    i += 1;
                }
            }
            "--host" => {
                if i + 1 < args.len() {
                    host = args[i + 1].clone();
                    i += 1;
                }
            }
            "--port" => {
                if i + 1 < args.len() {
                    port = args[i + 1].parse().unwrap_or(8080);
                    i += 1;
                }
            }
            "--threads" => {
                if i + 1 < args.len() {
                    n_threads = args[i + 1].parse().unwrap_or(0);
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    if model_path.is_empty() {
        eprintln!("Usage: rust-model-server --model <path.gguf> [--host 0.0.0.0] [--port 8080] [--threads 4]");
        std::process::exit(1);
    }

    let n_threads = if n_threads > 0 { n_threads } else { 4 };
    eprintln!("Loading model: {} ...", model_path);

    let loader = Arc::new(GGUFLoader::from_file(&model_path).unwrap_or_else(|e| {
        eprintln!("Failed to load model: {}", e);
        std::process::exit(1);
    }));
    let arch = loader
        .metadata("general.architecture")
        .and_then(|v| v.to_string_val())
        .unwrap_or_default();
    let pool = Arc::new(thread_pool::ComputePool::new(n_threads));

    let tokenizer = BPETokenizer::from_gguf_metadata(|k| loader.metadata(k).cloned())
        .unwrap_or_else(|e| {
            eprintln!("Failed to init tokenizer: {}", e);
            std::process::exit(1);
        });

    let model: ModelBackend = if arch == "qwen35" {
        let model = Qwen35Model::from_gguf(&loader).unwrap_or_else(|e| {
            eprintln!("Failed to parse Qwen3.5 model: {}", e);
            std::process::exit(1);
        });
        ModelBackend::Qwen35(Qwen35State {
            model: Arc::new(model),
            tokenizer: Arc::new(tokenizer),
            pool,
        })
    } else {
        let config = loader.model_config().unwrap_or_else(|e| {
            eprintln!("Failed to parse config: {}", e);
            std::process::exit(1);
        });
        let n_embd = config.n_embd;
        let n_layer = config.n_layer;
        let n_head = config.n_head;
        let n_head_kv = config.n_head_kv;
        let n_embd_head = config.n_embd_head;
        let n_embd_head_k =
            if let Some(v) = loader.metadata(&format!("{}.attention.key_length", arch)) {
                v.to_u64().unwrap_or(n_embd_head as u64) as usize
            } else {
                n_embd_head
            };
        let n_embd_head_v =
            if let Some(v) = loader.metadata(&format!("{}.attention.value_length", arch)) {
                v.to_u64().unwrap_or(n_embd_head as u64) as usize
            } else {
                n_embd_head
            };
        let n_ff = config.n_ff;
        let eps = config.norm_eps;
        let freq_base = config.rope_freq_base;
        let vocab = tokenizer.vocab_size();
        let n_ctx = config.n_ctx.min(4096);
        let is_qwen3 = arch == "qwen3";

        let output_norm = get_f32_tensor_from_loader(&loader, "output_norm.weight", n_embd);
        let embd_weight = unsafe {
            std::mem::transmute::<&[u8], &'static [u8]>(
                loader.tensor_slice("token_embd.weight").expect("no embd"),
            )
        };
        let output_weight = unsafe {
            std::mem::transmute::<&[u8], &'static [u8]>(
                loader
                    .tensor_slice("output.weight")
                    .unwrap_or(loader.tensor_slice("token_embd.weight").unwrap()),
            )
        };

        let layers: Vec<LayerWeightsOwned> = (0..n_layer)
            .map(|l| LayerWeightsOwned {
                attn_norm: get_f32_tensor_from_loader(
                    &loader,
                    &format!("blk.{}.attn_norm.weight", l),
                    n_embd,
                ),
                ffn_norm: get_f32_tensor_from_loader(
                    &loader,
                    &format!("blk.{}.ffn_norm.weight", l),
                    n_embd,
                ),
                q_norm: if is_qwen3 {
                    Some(get_f32_tensor_from_loader(
                        &loader,
                        &format!("blk.{}.attn_q_norm.weight", l),
                        n_embd_head_k,
                    ))
                } else {
                    None
                },
                k_norm: if is_qwen3 {
                    Some(get_f32_tensor_from_loader(
                        &loader,
                        &format!("blk.{}.attn_k_norm.weight", l),
                        n_embd_head_k,
                    ))
                } else {
                    None
                },
                wq: unsafe {
                    std::mem::transmute::<&[u8], &'static [u8]>(
                        loader
                            .tensor_slice(&format!("blk.{}.attn_q.weight", l))
                            .unwrap(),
                    )
                },
                wk: unsafe {
                    std::mem::transmute::<&[u8], &'static [u8]>(
                        loader
                            .tensor_slice(&format!("blk.{}.attn_k.weight", l))
                            .unwrap(),
                    )
                },
                wv: unsafe {
                    std::mem::transmute::<&[u8], &'static [u8]>(
                        loader
                            .tensor_slice(&format!("blk.{}.attn_v.weight", l))
                            .unwrap(),
                    )
                },
                wo: unsafe {
                    std::mem::transmute::<&[u8], &'static [u8]>(
                        loader
                            .tensor_slice(&format!("blk.{}.attn_output.weight", l))
                            .unwrap(),
                    )
                },
                w_gate: unsafe {
                    std::mem::transmute::<&[u8], &'static [u8]>(
                        loader
                            .tensor_slice(&format!("blk.{}.ffn_gate.weight", l))
                            .unwrap(),
                    )
                },
                w_up: unsafe {
                    std::mem::transmute::<&[u8], &'static [u8]>(
                        loader
                            .tensor_slice(&format!("blk.{}.ffn_up.weight", l))
                            .unwrap(),
                    )
                },
                w_down: unsafe {
                    std::mem::transmute::<&[u8], &'static [u8]>(
                        loader
                            .tensor_slice(&format!("blk.{}.ffn_down.weight", l))
                            .unwrap(),
                    )
                },
            })
            .collect();

        let qwen3_cfg = Qwen3Config {
            n_embd,
            n_layer,
            n_head,
            n_head_kv,
            n_embd_head,
            n_embd_head_k,
            n_embd_head_v,
            n_ff,
            vocab,
            n_ctx,
            eps,
            freq_base,
            has_qk_norm: is_qwen3,
        };

        ModelBackend::Qwen3(Qwen3State {
            layers,
            output_norm,
            embd_weight,
            output_weight,
            config: qwen3_cfg,
            tokenizer: Arc::new(tokenizer),
            pool,
        })
    };

    let model_name = std::path::Path::new(&model_path)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    eprintln!(
        "Model '{}' loaded (arch={}), {} threads",
        model_name, arch, n_threads
    );

    let state = AppState {
        model: Arc::new(model),
        model_name,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(list_models))
        .route("/v1/chat/completions", post(chat_completions))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("{}:{}", host, port);
    eprintln!("Server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
