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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_parses_placement_and_rejects_unknown_flags() {
        let cli = parse_server_args([
            "--model".to_owned(),
            "model.gguf".to_owned(),
            "--placement".to_owned(),
            "llm:row=metal0@1".to_owned(),
            "--placement".to_owned(),
            "vision:layer=cpu0@1".to_owned(),
            "--port".to_owned(),
            "9000".to_owned(),
        ])
        .unwrap();
        assert_eq!(cli.model_path, "model.gguf");
        assert_eq!(cli.port, 9000);
        assert_eq!(
            cli.execution.placements,
            ["llm:row=metal0@1", "vision:layer=cpu0@1"]
        );
        assert!(parse_server_args(["--unknown".to_owned()]).is_err());
        assert!(parse_server_args(["--gpu-ratio".to_owned()])
            .unwrap_err()
            .contains("use --placement"));
    }
}

struct Qwen3State {
    model: Qwen3Model,
    compiled: CompiledModel,
    tokenizer: Arc<BPETokenizer>,
}

struct Qwen35State {
    model: Qwen35Model,
    compiled: CompiledModel,
    tokenizer: Arc<BPETokenizer>,
}

enum ModelBackend {
    Qwen3(Qwen3State),
    Qwen35(Qwen35State),
}

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

#[derive(Debug)]
struct ServerCli {
    model_path: String,
    mmproj_path: Option<String>,
    host: String,
    port: u16,
    execution: ExecutionOptions,
}

fn parse_server_args(args: impl IntoIterator<Item = String>) -> Result<ServerCli, String> {
    let mut cli = ServerCli {
        model_path: String::new(),
        mmproj_path: None,
        host: "0.0.0.0".to_owned(),
        port: 8080,
        execution: ExecutionOptions::default(),
    };
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        if parse_execution_flag(&flag, &mut args, &mut cli.execution)? {
            continue;
        }
        match flag.as_str() {
            "--model" => cli.model_path = next_server_value(&mut args, "--model")?,
            "--mmproj" => cli.mmproj_path = Some(next_server_value(&mut args, "--mmproj")?),
            "--host" => cli.host = next_server_value(&mut args, "--host")?,
            "--port" => {
                cli.port = next_server_value(&mut args, "--port")?
                    .parse()
                    .map_err(|_| "Invalid --port value")?
            }
            _ => return Err(format!("Unknown option: {flag}")),
        }
    }
    Ok(cli)
}

fn next_server_value(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("Missing value for {flag}"))
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
    state: &Qwen3State,
    messages: &[ChatMessage],
    max_tokens: usize,
    temperature: f32,
) -> Result<GenerateResult, String> {
    let input_tokens = server_prompt_tokens(&state.tokenizer, messages)?;
    let n_prompt = input_tokens.len();
    let mut all_tokens = input_tokens.clone();
    let mut generated_tokens = Vec::new();
    let mut token_strings = Vec::new();
    let mut decoder = state.tokenizer.streaming_decoder(false);
    let mut run = state
        .compiled
        .start_run()
        .map_err(|error| error.to_string())?;
    for step in 0..(n_prompt + max_tokens) {
        let token_id = if step < n_prompt {
            input_tokens[step]
        } else {
            *generated_tokens.last().unwrap_or(&0)
        };
        let position = u32::try_from(step).map_err(|_| "Qwen3 server position does not fit u32")?;
        let mut logits = vec![0.0; state.model.config.vocab];
        state.model.forward(
            &mut run,
            &[token_id],
            &[[position, position, position, 0]],
            &mut logits,
        )?;
        if step < n_prompt - 1 {
            continue;
        }
        let next_token = sample_token_from_logits(&logits, temperature);
        if state.tokenizer.eos_id() == Some(next_token as u32)
            || state.tokenizer.special_token_id("im_end") == Some(next_token as u32)
            || generated_tokens.len() >= max_tokens
        {
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
    let n_prompt = prompt_ids.len();
    let mut decoder = state.tokenizer.streaming_decoder(false);
    let mut generated_ids = Vec::<u32>::new();
    let mut rendered_chunks = Vec::<String>::new();

    let mut run = state
        .compiled
        .start_run()
        .map_err(|error| error.to_string())?;
    let mut logits = None;
    for (token, position) in prompt_ids.iter().copied().zip(prompt_positions.iter()) {
        let positions = [[
            u32::try_from(position[0]).map_err(|_| "Qwen3.5 server position does not fit u32")?,
            u32::try_from(position[1]).map_err(|_| "Qwen3.5 server position does not fit u32")?,
            u32::try_from(position[2]).map_err(|_| "Qwen3.5 server position does not fit u32")?,
            u32::try_from(position[3]).map_err(|_| "Qwen3.5 server position does not fit u32")?,
        ]];
        let mut current_logits = vec![0.0; state.model.config.vocab_size];
        state
            .model
            .forward_compiled(&mut run, &[token], &positions, &mut current_logits)?;
        logits = Some(current_logits);
    }
    let mut logits = logits.ok_or("Qwen3.5 prompt produced no tokens")?;
    for generated in 0..max_tokens {
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
        if generated + 1 == max_tokens {
            break;
        }
        let decode_position = [
            u32::try_from(next_text_position)
                .map_err(|_| "Qwen3.5 server position does not fit u32")?,
            u32::try_from(next_text_position)
                .map_err(|_| "Qwen3.5 server position does not fit u32")?,
            u32::try_from(next_text_position)
                .map_err(|_| "Qwen3.5 server position does not fit u32")?,
            0,
        ];
        let mut next_logits = vec![0.0; state.model.config.vocab_size];
        state
            .model
            .forward_compiled(&mut run, &[next_id], &[decode_position], &mut next_logits)?;
        logits = next_logits;
        next_text_position = next_text_position
            .checked_add(1)
            .ok_or("Qwen3.5 server decode position overflow")?;
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
    let cli = parse_server_args(std::env::args().skip(1)).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });

    if cli.model_path.is_empty() {
        eprintln!("Usage: rust-model-server --model <path.gguf-or-ggufrs> [--host 0.0.0.0] [--port 8080] [--threads 4] [--placement llm:row=cpu0@1]");
        std::process::exit(1);
    }

    let n_threads = cli.execution.thread_count;
    eprintln!("Loading model: {} ...", cli.model_path);

    let sources = load_model_sources(
        Path::new(&cli.model_path),
        cli.mmproj_path.as_deref().map(Path::new),
    )
    .unwrap_or_else(|error| {
        eprintln!("Failed to load model: {error}");
        std::process::exit(1);
    });
    let source = sources
        .iter()
        .find(|(component, _)| *component == ComponentId::Llm)
        .expect("load_model_sources always returns LLM")
        .1
        .clone();
    let arch = source
        .metadata("general.architecture")
        .and_then(|v| v.to_string_val())
        .unwrap_or_default();
    let tokenizer = BPETokenizer::from_gguf_metadata(|k| source.metadata(k).cloned())
        .unwrap_or_else(|e| {
            eprintln!("Failed to init tokenizer: {}", e);
            std::process::exit(1);
        });

    let (compiled, runner) = rust_model_inference::compile_model(sources, &cli.execution)
        .unwrap_or_else(|error| {
            eprintln!("Failed to compile model: {error}");
            std::process::exit(1);
        });
    let model = match runner {
        QwenRunner::Qwen3(model) => ModelBackend::Qwen3(Qwen3State {
            model,
            compiled,
            tokenizer: Arc::new(tokenizer),
        }),
        QwenRunner::Qwen35(model) => ModelBackend::Qwen35(Qwen35State {
            model,
            compiled,
            tokenizer: Arc::new(tokenizer),
        }),
    };

    let model_name = std::path::Path::new(&cli.model_path)
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

    let addr = format!("{}:{}", cli.host, cli.port);
    eprintln!("Server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
