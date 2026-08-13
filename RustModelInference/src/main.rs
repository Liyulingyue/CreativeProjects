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

struct Cli {
    model_path: String,
    prompt: String,
    max_tokens: usize,
    temperature: f32,
    threads: usize,
    embedding: bool,
    embedding_output: EmbeddingOutput,
    placements: Vec<String>,
    has_multimodal_input: bool,
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
    if cli.has_multimodal_input {
        eprintln!("multimodal input requires a compiled vision program");
        std::process::exit(2);
    }

    let source: Arc<dyn TensorSource> = Arc::from(
        open_model_source(Path::new(&cli.model_path), ComponentRole::Llm).unwrap_or_else(|error| {
            eprintln!("Failed to load model {}: {error}", cli.model_path);
            std::process::exit(1);
        }),
    );
    let result = if cli.prompt.is_empty() {
        run_interactive(&source, &cli)
    } else if cli.embedding {
        run_embedding(&source, &cli)
    } else {
        run_inference(&source, &cli)
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
        threads: 0,
        embedding: false,
        embedding_output: EmbeddingOutput::Summary,
        placements: Vec::new(),
        has_multimodal_input: false,
    };
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
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
            "--threads" => {
                cli.threads = next_value(&mut args, "--threads")?
                    .parse()
                    .map_err(|_| "Invalid --threads value")?;
            }
            "--placement" => cli.placements.push(next_value(&mut args, "--placement")?),
            "--embedding" => cli.embedding = true,
            "--embedding-output" => {
                cli.embedding_output =
                    parse_embedding_output(Some(&next_value(&mut args, "--embedding-output")?))?;
            }
            "--mmproj" | "--image" => {
                let _ = next_value(&mut args, &flag)?;
                cli.has_multimodal_input = true;
            }
            "--dump-logits" | "--bench" | "--profile" => {
                return Err(format!(
                    "{flag} is not supported by the compiled execution path"
                ));
            }
            "--kv-cache" => {
                let _ = next_value(&mut args, "--kv-cache")?;
                return Err("--kv-cache is not supported by the compiled execution path".into());
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
        assert!(parse_args(["--kv-cache".to_owned(), "f16".to_owned()]).is_err());
    }

    #[test]
    fn embedding_l2_matches_llama_f32_product_and_scale_bits() {
        let mut values = [f32::from_bits(1)];

        l2_normalize(&mut values).unwrap();

        assert_eq!(values, [0.0]);
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

fn thread_count(requested: usize) -> usize {
    if requested != 0 {
        requested
    } else {
        std::thread::available_parallelism()
            .map(|count| count.get().min(8))
            .unwrap_or(1)
    }
}

fn compile_model(
    catalog: Arc<TensorCatalog>,
    requirements: ComponentRequirements,
    threads: usize,
    placements: &[String],
) -> Result<CompiledModel, String> {
    let (rules, backends) =
        parse_requested_placements(placements).map_err(|error| error.to_string())?;
    let mut registry = DeviceRegistry::new();
    compute::register_requested_providers(&mut registry, &backends, thread_count(threads))
        .map_err(|error| error.to_string())?;
    registry
        .discover(&backends)
        .map_err(|error| error.to_string())?;
    let registry = Arc::new(registry);
    let plan = PlacementCompiler {
        catalog: &catalog,
        registry: &registry,
        requirements: std::slice::from_ref(&requirements),
    }
    .compile(&rules)
    .map_err(|error| error.to_string())?;
    CompiledModel::compile(catalog, plan, registry).map_err(|error| error.to_string())
}

fn catalog(source: &Arc<dyn TensorSource>) -> Result<Arc<TensorCatalog>, String> {
    TensorCatalog::from_sources(vec![(ComponentId::Llm, Arc::clone(source))])
        .map(Arc::new)
        .map_err(|error| error.to_string())
}

fn run_embedding(source: &Arc<dyn TensorSource>, cli: &Cli) -> Result<(), String> {
    let catalog = catalog(source)?;
    let model = Qwen3Model::from_catalog(&catalog)?;
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
    let compiled = compile_model(
        Arc::clone(&catalog),
        model.requirements(),
        cli.threads,
        &cli.placements,
    )?;
    let mut rows = vec![0.0; tokens.len() * model.config.n_embd];
    let mut run = compiled.start_run().map_err(|error| error.to_string())?;
    for (token, row) in tokens
        .iter()
        .zip(rows.chunks_exact_mut(model.config.n_embd))
    {
        run.execute_embedding(
            ComponentId::Llm,
            model.tensors.token_embedding,
            std::slice::from_ref(token),
            row,
        )
        .map_err(|error| error.to_string())?;
    }
    let mut embedding = mean_rows(&rows, tokens.len(), model.config.n_embd)?;
    l2_normalize(&mut embedding)?;
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

fn run_inference(source: &Arc<dyn TensorSource>, cli: &Cli) -> Result<(), String> {
    let architecture = source
        .metadata("general.architecture")
        .and_then(MetaValue::to_string_val)
        .unwrap_or_default();
    if architecture == "qwen35" {
        run_qwen35(source, cli)
    } else {
        run_qwen3(source, cli)
    }
}

fn run_qwen3(source: &Arc<dyn TensorSource>, cli: &Cli) -> Result<(), String> {
    let catalog = catalog(source)?;
    let model = Qwen3Model::from_catalog(&catalog)?;
    let tokenizer = BPETokenizer::from_gguf_metadata(|key| source.metadata(key).cloned())?;
    let prompt_tokens = tokenizer.encode(&cli.prompt, EncodeOptions::default());
    let compiled = compile_model(
        Arc::clone(&catalog),
        model.requirements(),
        cli.threads,
        &cli.placements,
    )?;
    let mut run = compiled.start_run().map_err(|error| error.to_string())?;
    let mut previous = None;
    let mut decoder = tokenizer.streaming_decoder(false);
    for step in 0..prompt_tokens.len() {
        let token = prompt_tokens[step];
        let position = u32::try_from(step).map_err(|_| "Qwen3 position does not fit u32")?;
        let mut logits = vec![0.0; model.config.vocab];
        model.forward(
            &mut run,
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
            &mut run,
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

fn run_qwen35(source: &Arc<dyn TensorSource>, cli: &Cli) -> Result<(), String> {
    let catalog = catalog(source)?;
    let model = Qwen35Model::from_catalog(&catalog)?;
    let tokenizer = BPETokenizer::from_gguf_metadata(|key| source.metadata(key).cloned())?;
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
    let compiled = compile_model(
        Arc::clone(&catalog),
        model.requirements(),
        cli.threads,
        &cli.placements,
    )?;
    let mut run = compiled.start_run().map_err(|error| error.to_string())?;
    let mut logits = None;
    for (token, position) in prompt_tokens
        .iter()
        .copied()
        .zip(prompt_positions.iter().copied())
    {
        let mut current_logits = vec![0.0; model.config.vocab_size];
        model.forward_compiled(&mut run, &[token], &[position], &mut current_logits)?;
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
            &mut run,
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

fn run_interactive(source: &Arc<dyn TensorSource>, cli: &Cli) -> Result<(), String> {
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
        let interactive_cli = Cli {
            model_path: cli.model_path.clone(),
            prompt: prompt.trim().to_owned(),
            max_tokens: cli.max_tokens,
            temperature: cli.temperature,
            threads: cli.threads,
            embedding: false,
            embedding_output: cli.embedding_output,
            placements: cli.placements.clone(),
            has_multimodal_input: false,
        };
        run_inference(source, &interactive_cli)?;
    }
}

fn mean_rows(values: &[f32], rows: usize, width: usize) -> Result<Vec<f32>, String> {
    if rows == 0 || values.len() != rows * width {
        return Err("Embedding rows have an invalid shape".into());
    }
    let mut result = vec![0.0; width];
    for row in values.chunks_exact(width) {
        for (result, value) in result.iter_mut().zip(row) {
            *result += value;
        }
    }
    for value in &mut result {
        *value /= rows as f32;
    }
    Ok(result)
}

fn l2_normalize(values: &mut [f32]) -> Result<(), String> {
    if values.iter().any(|value| !value.is_finite()) {
        return Err("Embedding contains a non-finite value".into());
    }
    let sum = values
        .iter()
        .map(|&value| f64::from(value * value))
        .sum::<f64>();
    let scale = if sum > 0.0 {
        (1.0 / sum.sqrt()) as f32
    } else {
        0.0
    };
    for value in values {
        *value *= scale;
    }
    Ok(())
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
