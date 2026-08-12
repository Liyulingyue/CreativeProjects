use std::path::Path;
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

struct Qwen35State {
    source: Arc<dyn TensorSource>,
    model: Arc<Qwen35Model>,
    tokenizer: Arc<BPETokenizer>,
    pool: Arc<thread_pool::ComputePool>,
}

enum ModelBackend {
    Qwen3(Arc<Qwen3Model>),
    Qwen35(Qwen35State),
}

unsafe impl Send for ModelBackend {}
unsafe impl Sync for ModelBackend {}
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
        ModelBackend::Qwen3(model) => {
            let token_ids = server_prompt_tokens(model.tokenizer(), messages)?;
            let positions = qwen_text_positions(token_ids.len());
            let generation = model.generate(
                Qwen3Input {
                    token_ids: &token_ids,
                    positions: &positions,
                    embeddings: None,
                },
                Qwen3GenerateOptions {
                    max_new_tokens: max_tokens,
                    temperature,
                },
            )?;
            Ok(GenerateResult {
                text: generation.text,
                tokens: generation.rendered_tokens,
                prompt_tokens: generation.prompt_tokens,
                completion_tokens: generation.token_ids.len(),
            })
        }
        ModelBackend::Qwen35(s) => generate_qwen_35(s, messages, max_tokens, temperature),
    }
}

fn generate_qwen_35(
    state: &Qwen35State,
    messages: &[ChatMessage],
    max_tokens: usize,
    temperature: f32,
) -> Result<GenerateResult, String> {
    let _source = &state.source;
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
        eprintln!("Usage: rust-model-server --model <path.gguf-or-ggufrs> [--host 0.0.0.0] [--port 8080] [--threads 4]");
        std::process::exit(1);
    }

    let n_threads = if n_threads > 0 { n_threads } else { 4 };
    eprintln!("Loading model: {} ...", model_path);

    let source: Arc<dyn TensorSource> = Arc::from(
        open_model_source(Path::new(&model_path), ComponentRole::Llm).unwrap_or_else(|error| {
            eprintln!("Failed to load model: {error}");
            std::process::exit(1);
        }),
    );
    let arch = source
        .metadata("general.architecture")
        .and_then(|v| v.to_string_val())
        .unwrap_or_default();
    let pool = Arc::new(thread_pool::ComputePool::new(n_threads));

    let tokenizer = BPETokenizer::from_gguf_metadata(|k| source.metadata(k).cloned())
        .unwrap_or_else(|e| {
            eprintln!("Failed to init tokenizer: {}", e);
            std::process::exit(1);
        });

    let model: ModelBackend = if arch == "qwen35" {
        let model = Qwen35Model::from_source(source.as_ref()).unwrap_or_else(|e| {
            eprintln!("Failed to parse Qwen3.5 model: {}", e);
            std::process::exit(1);
        });
        ModelBackend::Qwen35(Qwen35State {
            source: Arc::clone(&source),
            model: Arc::new(model),
            tokenizer: Arc::new(tokenizer),
            pool,
        })
    } else {
        let model = Qwen3Model::from_source(Arc::clone(&source), Arc::new(tokenizer), pool)
            .unwrap_or_else(|error| {
                eprintln!("Failed to parse Qwen2/Qwen3 model: {error}");
                std::process::exit(1);
            });
        ModelBackend::Qwen3(Arc::new(model))
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
