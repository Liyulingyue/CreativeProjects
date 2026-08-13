use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;

use rust_model_inference::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum EmbeddingOutput {
    #[default]
    Summary,
    Raw,
}

fn parse_embedding_output(value: Option<&str>) -> Result<EmbeddingOutput, String> {
    match value {
        Some("summary") => Ok(EmbeddingOutput::Summary),
        Some("raw") => Ok(EmbeddingOutput::Raw),
        Some(value) => Err(format!(
            "Invalid --embedding-output {value:?}; expected summary or raw"
        )),
        None => Err("Missing value for --embedding-output".into()),
    }
}

#[derive(Debug)]
struct Cli {
    model_path: String,
    prompt: String,
    max_tokens: usize,
    temperature: f32,
    execution: ExecutionOptions,
    embedding: bool,
    embedding_output: EmbeddingOutput,
    mmproj_path: Option<String>,
}

fn main() {
    let cli = parse_args(std::env::args().skip(1)).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    if cli.model_path.is_empty() {
        print_usage();
        return;
    }
    let sources = load_model_sources(
        Path::new(&cli.model_path),
        cli.mmproj_path.as_deref().map(Path::new),
    )
    .unwrap_or_else(|error| {
        eprintln!("Failed to load model {}: {error}", cli.model_path);
        std::process::exit(1);
    });
    let source = sources
        .iter()
        .find(|(component, _)| *component == ComponentId::Llm)
        .expect("load_model_sources always returns LLM")
        .1
        .clone();
    let tokenizer = BPETokenizer::from_gguf_metadata(|key| source.metadata(key).cloned())
        .unwrap_or_else(|error| {
            eprintln!("Failed to initialize tokenizer: {error}");
            std::process::exit(1);
        });
    let (compiled, runner) = rust_model_inference::compile_model(sources, &cli.execution)
        .unwrap_or_else(|error| {
            eprintln!("Failed to compile model: {error}");
            std::process::exit(1);
        });
    let mut run = compiled.start_run().unwrap_or_else(|error| {
        eprintln!("Failed to start compiled run: {error}");
        std::process::exit(1);
    });
    let result = if cli.prompt.is_empty() {
        run_interactive(&runner, &tokenizer, &mut run, &cli)
    } else if cli.embedding {
        run_embedding(&source, &runner, &mut run, &cli)
    } else {
        run_inference(&runner, &tokenizer, &mut run, &cli)
    };
    if let Err(error) = result {
        eprintln!("Inference error: {error}");
        std::process::exit(1);
    }
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Cli, String> {
    let mut cli = Cli {
        model_path: String::new(),
        prompt: String::new(),
        max_tokens: 128,
        temperature: 0.6,
        execution: ExecutionOptions::default(),
        embedding: false,
        embedding_output: EmbeddingOutput::Summary,
        mmproj_path: None,
    };
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        if parse_execution_flag(&flag, &mut args, &mut cli.execution)? {
            continue;
        }
        match flag.as_str() {
            "--model" => cli.model_path = next_value(&mut args, "--model")?,
            "--prompt" => cli.prompt = next_value(&mut args, "--prompt")?,
            "--max-tokens" | "--n-gen" => {
                cli.max_tokens = next_value(&mut args, &flag)?
                    .parse()
                    .map_err(|_| format!("Invalid {flag} value"))?;
            }
            "--temp" => {
                cli.temperature = next_value(&mut args, "--temp")?
                    .parse()
                    .map_err(|_| "Invalid --temp value")?;
            }
            "--embedding" => cli.embedding = true,
            "--embedding-output" => {
                cli.embedding_output =
                    parse_embedding_output(Some(&next_value(&mut args, "--embedding-output")?))?;
            }
            "--mmproj" => cli.mmproj_path = Some(next_value(&mut args, "--mmproj")?),
            "--image" => return Err("--image is not supported by the compiled vision path".into()),
            "--dump-logits" | "--bench" | "--profile" => {
                return Err(format!(
                    "{flag} is not supported by the compiled execution path"
                ));
            }
            _ => return Err(format!("Unknown option: {flag}")),
        }
    }
    Ok(cli)
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("Missing value for {flag}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiled_runner_rejects_legacy_execution_flags() {
        for flag in ["--dump-logits", "--bench", "--profile"] {
            assert!(parse_args([flag.to_owned()]).is_err(), "{flag}");
        }
        let cli = parse_args(["--kv-cache".to_owned(), "f16".to_owned()]).unwrap();
        assert_eq!(cli.execution.kv_cache, KvCacheType::F16);
        assert!(parse_args(["--gpu-ratio".to_owned(), "1".to_owned()])
            .unwrap_err()
            .contains("use --placement"));
    }

    #[test]
    fn inference_parser_preserves_mmproj_and_repeatable_placements() {
        let cli = parse_args([
            "--model".into(),
            "model.gguf".into(),
            "--mmproj".into(),
            "vision.gguf".into(),
            "--placement".into(),
            "llm:layer=cpu0@1".into(),
            "--placement".into(),
            "vision:layer=cpu0@1".into(),
        ])
        .unwrap();
        assert_eq!(cli.mmproj_path.as_deref(), Some("vision.gguf"));
        assert_eq!(cli.execution.placements.len(), 2);
    }

    #[test]
    fn generation_decodes_only_the_requested_token_budget() {
        let metadata: std::collections::HashMap<String, MetaValue> =
            std::collections::HashMap::from([
                (
                    "tokenizer.ggml.model".into(),
                    MetaValue::String("gpt2".into()),
                ),
                (
                    "tokenizer.ggml.pre".into(),
                    MetaValue::String("qwen2".into()),
                ),
                (
                    "tokenizer.ggml.tokens".into(),
                    MetaValue::Array(
                        MetaValueType::String,
                        ["A", "B", "C", "<|endoftext|>"]
                            .into_iter()
                            .map(|value| MetaValue::String(value.into()))
                            .collect(),
                    ),
                ),
                (
                    "tokenizer.ggml.token_type".into(),
                    MetaValue::Array(
                        MetaValueType::Uint32,
                        [1, 1, 1, 3].into_iter().map(MetaValue::Uint32).collect(),
                    ),
                ),
                (
                    "tokenizer.ggml.merges".into(),
                    MetaValue::Array(MetaValueType::String, vec![]),
                ),
            ]);
        let tokenizer = BPETokenizer::from_gguf_metadata(|key| metadata.get(key).cloned()).unwrap();
        let mut decoder = tokenizer.streaming_decoder(false);
        let text = [0, 1, 2]
            .into_iter()
            .take(2)
            .map(|token| decoder.push(token))
            .collect::<String>();

        assert_eq!(text, "AB");
        assert_eq!(decoder.finish(), "");
    }
}

fn run_embedding(
    source: &Arc<dyn TensorSource>,
    runner: &QwenRunner,
    run: &mut ExecutionRun<'_>,
    cli: &Cli,
) -> Result<(), String> {
    let QwenRunner::Qwen3(model) = runner else {
        return Err("Qwen3 embedding is unavailable for this architecture".into());
    };
    let tokenizer = BPETokenizer::from_gguf_metadata(|key| source.metadata(key).cloned())?;
    let tokens = tokenizer.encode(
        &cli.prompt,
        EncodeOptions {
            add_special: true,
            parse_special: true,
        },
    );
    if tokens.is_empty() {
        return Err("Embedding input produced no tokens".into());
    }
    let config = Qwen3EmbeddingConfig::from_metadata(|key| source.metadata(key).cloned())?;
    let embedding = model.embed(run, &tokens, config)?;
    match cli.embedding_output {
        EmbeddingOutput::Summary => println!(
            "Embedding ({} dims): {:?}",
            embedding.len(),
            &embedding[..embedding.len().min(8)]
        ),
        EmbeddingOutput::Raw => println!(
            "embedding_raw: {}",
            embedding
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        ),
    }
    Ok(())
}

fn run_inference(
    runner: &QwenRunner,
    tokenizer: &BPETokenizer,
    run: &mut ExecutionRun<'_>,
    cli: &Cli,
) -> Result<(), String> {
    match runner {
        QwenRunner::Qwen3(model) => run_qwen3(model, tokenizer, run, cli),
        QwenRunner::Qwen35(model) => run_qwen35(model, tokenizer, run, cli),
    }
}

fn run_qwen3(
    model: &Qwen3Model,
    tokenizer: &BPETokenizer,
    run: &mut ExecutionRun<'_>,
    cli: &Cli,
) -> Result<(), String> {
    let prompt_tokens = tokenizer.encode(&cli.prompt, EncodeOptions::default());
    let mut previous = None;
    let mut decoder = tokenizer.streaming_decoder(false);
    for step in 0..prompt_tokens.len() {
        let token = prompt_tokens[step];
        let position = u32::try_from(step).map_err(|_| "Qwen3 position does not fit u32")?;
        let mut logits = vec![0.0; model.config.vocab];
        model.forward(
            run,
            &[token],
            &[[position, position, position, 0]],
            &mut logits,
        )?;
        if step + 1 == prompt_tokens.len() {
            previous = Some(sample_token(&logits, cli.temperature)?);
        }
    }
    let mut next = previous.ok_or("Qwen3 prompt produced no tokens")?;
    for generated in 0..cli.max_tokens {
        if tokenizer.eos_id() == Some(next) || tokenizer.special_token_id("im_end") == Some(next) {
            break;
        }
        let text = decoder.push(next);
        if !text.is_empty() {
            print!("{text}");
            io::stdout()
                .flush()
                .map_err(|error| format!("Failed to flush generated text: {error}"))?;
        }
        if generated + 1 == cli.max_tokens {
            break;
        }
        let position = u32::try_from(prompt_tokens.len() + generated)
            .map_err(|_| "Qwen3 position does not fit u32")?;
        let mut logits = vec![0.0; model.config.vocab];
        model.forward(
            run,
            &[next],
            &[[position, position, position, 0]],
            &mut logits,
        )?;
        next = sample_token(&logits, cli.temperature)?;
    }
    let tail = decoder.finish();
    if !tail.is_empty() {
        print!("{tail}");
        io::stdout()
            .flush()
            .map_err(|error| format!("Failed to flush generated text: {error}"))?;
    }
    Ok(())
}

fn run_qwen35(
    model: &Qwen35Model,
    tokenizer: &BPETokenizer,
    run: &mut ExecutionRun<'_>,
    cli: &Cli,
) -> Result<(), String> {
    let prompt_tokens = tokenizer.encode(&cli.prompt, EncodeOptions::default());
    let (prompt_positions, mut next_position) = build_qwen35_positions(&prompt_tokens, None, &[])?;
    let prompt_positions = prompt_positions
        .into_iter()
        .map(|[time, height, width, channel]| {
            Ok([
                u32::try_from(time).map_err(|_| "Qwen3.5 time position does not fit u32")?,
                u32::try_from(height).map_err(|_| "Qwen3.5 height position does not fit u32")?,
                u32::try_from(width).map_err(|_| "Qwen3.5 width position does not fit u32")?,
                u32::try_from(channel).map_err(|_| "Qwen3.5 channel position does not fit u32")?,
            ])
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut logits = None;
    for (token, position) in prompt_tokens
        .iter()
        .copied()
        .zip(prompt_positions.iter().copied())
    {
        let mut current_logits = vec![0.0; model.config.vocab_size];
        model.forward_compiled(run, &[token], &[position], &mut current_logits)?;
        logits = Some(current_logits);
    }
    let mut logits = logits.ok_or("Qwen3.5 prompt produced no tokens")?;
    let mut decoder = tokenizer.streaming_decoder(false);
    for generated in 0..cli.max_tokens {
        let next = sample_token(&logits, cli.temperature)?;
        if tokenizer.eos_id() == Some(next) || tokenizer.special_token_id("im_end") == Some(next) {
            break;
        }
        let text = decoder.push(next);
        if !text.is_empty() {
            print!("{text}");
            io::stdout()
                .flush()
                .map_err(|error| format!("Failed to flush generated text: {error}"))?;
        }
        if generated + 1 == cli.max_tokens {
            break;
        }
        let position =
            u32::try_from(next_position).map_err(|_| "Qwen3.5 position does not fit u32")?;
        let mut next_logits = vec![0.0; model.config.vocab_size];
        model.forward_compiled(
            run,
            &[next],
            &[[position, position, position, 0]],
            &mut next_logits,
        )?;
        logits = next_logits;
        next_position = next_position
            .checked_add(1)
            .ok_or("Qwen3.5 decode position overflow")?;
    }
    let tail = decoder.finish();
    if !tail.is_empty() {
        print!("{tail}");
        io::stdout()
            .flush()
            .map_err(|error| format!("Failed to flush generated text: {error}"))?;
    }
    Ok(())
}

fn run_interactive(
    runner: &QwenRunner,
    tokenizer: &BPETokenizer,
    run: &mut ExecutionRun<'_>,
    cli: &Cli,
) -> Result<(), String> {
    loop {
        print!("> ");
        io::stdout()
            .flush()
            .map_err(|error| format!("Failed to flush prompt: {error}"))?;
        let mut prompt = String::new();
        if io::stdin()
            .read_line(&mut prompt)
            .map_err(|error| format!("Failed to read prompt: {error}"))?
            == 0
        {
            return Ok(());
        }
        if prompt.trim().is_empty() {
            continue;
        }
        run.reset_state().map_err(|error| error.to_string())?;
        let interactive_cli = Cli {
            model_path: cli.model_path.clone(),
            prompt: prompt.trim().to_owned(),
            max_tokens: cli.max_tokens,
            temperature: cli.temperature,
            execution: cli.execution.clone(),
            embedding: false,
            embedding_output: cli.embedding_output,
            mmproj_path: cli.mmproj_path.clone(),
        };
        run_inference(runner, tokenizer, run, &interactive_cli)?;
    }
}

fn sample_token(logits: &[f32], temperature: f32) -> Result<u32, String> {
    if logits.is_empty() {
        return Err("Model returned no logits".into());
    }
    let index = if temperature <= 0.0 {
        logits
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map(|(index, _)| index)
            .expect("non-empty logits were checked")
    } else {
        logits
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map(|(index, _)| index)
            .expect("non-empty logits were checked")
    };
    u32::try_from(index).map_err(|_| "Sampled token does not fit u32".into())
}

fn print_usage() {
    println!(
        "Usage: cargo run -- --model <path.gguf> --prompt <text> [--placement llm:row=cpu0@1]"
    );
}
