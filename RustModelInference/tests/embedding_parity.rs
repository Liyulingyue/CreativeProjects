use std::process::Command;

const FIXTURES: &[&str] = &[
    "hello",
    "Hello, 世界! 123",
    "What is the capital of China?",
    "The capital of China is Beijing.",
    "Photosynthesis converts light into chemical energy.",
    "中国的首都是北京。",
];

fn norm(values: &[f64]) -> f64 {
    values.iter().map(|value| value * value).sum::<f64>().sqrt()
}

fn cosine(left: &[f64], right: &[f64]) -> f64 {
    left.iter().zip(right).map(|(a, b)| a * b).sum::<f64>() / (norm(left) * norm(right))
}

fn parse_numbers(text: &str) -> Vec<f64> {
    text.split_whitespace()
        .map(|value| value.parse::<f64>().unwrap())
        .collect()
}

fn run(command: &mut Command) -> String {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn rust_embedding_model(reference_model: &str, override_model: Option<&str>) -> String {
    override_model.unwrap_or(reference_model).to_string()
}

fn f32_bits(bytes: &[u8]) -> Vec<u32> {
    assert_eq!(bytes.len() % 4, 0);
    bytes
        .chunks_exact(4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
        .collect()
}

fn llama_token_ids(bytes: &[u8]) -> Vec<u32> {
    assert_eq!(bytes.len() % 4, 0);
    bytes
        .chunks_exact(4)
        .map(|bytes| {
            let token = i32::from_le_bytes(bytes.try_into().unwrap());
            u32::try_from(token).expect("llama token IDs must be nonnegative")
        })
        .collect()
}

fn rust_trace_token_ids(trace: &std::path::Path) -> Vec<u32> {
    let record = std::fs::read_to_string(trace)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|record| record["name"] == "embedding.tokens")
        .expect("missing Rust embedding.tokens parity checkpoint");
    record["token_ids"]
        .as_array()
        .expect("embedding.tokens checkpoint must contain token_ids")
        .iter()
        .map(|token| {
            u32::try_from(
                token
                    .as_u64()
                    .expect("embedding.tokens token ID must be an unsigned integer"),
            )
            .expect("embedding.tokens token ID must fit u32")
        })
        .collect()
}

fn first_bit_mismatch(left: &[u32], right: &[u32]) -> Option<(usize, u32, u32)> {
    assert_eq!(left.len(), right.len());
    left.iter()
        .zip(right)
        .enumerate()
        .find_map(|(index, (&left, &right))| (left != right).then_some((index, left, right)))
}

fn fixture_dir(label: &str, fixture: usize) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "rmi-embedding-bits-{}-{label}-{fixture}",
        std::process::id()
    ))
}

fn reference_embedding_bits(
    llama_debug: &str,
    model: &str,
    prompt: &str,
    fixture: usize,
    normalize: i32,
) -> (Vec<u32>, Vec<u32>) {
    let directory = fixture_dir(&format!("llama-{normalize}"), fixture);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let normalize = normalize.to_string();
    let output = std::process::Command::new(llama_debug)
        .args([
            "-m",
            model,
            "-p",
            prompt,
            "-t",
            "1",
            "-tb",
            "1",
            "-b",
            "2048",
            "-ub",
            "512",
            "-ngl",
            "0",
            "-fa",
            "off",
            "-ctk",
            "f32",
            "-ctv",
            "f32",
            "--embedding",
            "--pooling",
            "last",
            "--embd-normalize",
            &normalize,
            "--no-warmup",
            "--save-logits",
            "--logits-output-dir",
            directory.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stem = std::path::Path::new(model)
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap();
    (
        f32_bits(
            &std::fs::read(directory.join(format!("llamacpp-{stem}-embeddings.bin"))).unwrap(),
        ),
        llama_token_ids(
            &std::fs::read(directory.join(format!("llamacpp-{stem}-embeddings-tokens.bin")))
                .expect("reference llama-debug must write embedding token IDs"),
        ),
    )
}

fn rust_embedding_checkpoints(
    rust: &str,
    model: &str,
    prompt: &str,
    fixture: usize,
) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
    let directory = fixture_dir("rust", fixture);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let trace = directory.join("trace.jsonl");
    let output = std::process::Command::new(rust)
        .env("RMI_PARITY_TRACE", &trace)
        .env(
            "RMI_PARITY_FILTER",
            "embedding.tokens,embedding.pooled,embedding.final",
        )
        .args([
            "--model",
            model,
            "--prompt",
            prompt,
            "--embedding",
            "--threads",
            "1",
            "--embedding-output",
            "raw",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (
        rust_trace_token_ids(&trace),
        f32_bits(&std::fs::read(format!("{}.embedding.pooled.f32", trace.display())).unwrap()),
        f32_bits(&std::fs::read(format!("{}.embedding.final.f32", trace.display())).unwrap()),
    )
}

#[test]
#[ignore = "requires QWEN3_EMBEDDING_MODEL and LLAMA_EMBEDDING_BIN"]
fn qwen3_embedding_vectors_match_pinned_llama_cpp() {
    let model = std::env::var("QWEN3_EMBEDDING_MODEL").unwrap();
    let rust_model_override = std::env::var("RMI_RUST_EMBEDDING_MODEL").ok();
    let rust_model = rust_embedding_model(&model, rust_model_override.as_deref());
    let llama = std::env::var("LLAMA_EMBEDDING_BIN").unwrap();
    let rust = env!("CARGO_BIN_EXE_rust-model-inference");
    let mut rust_vectors = Vec::new();
    let mut llama_vectors = Vec::new();

    for &prompt in FIXTURES {
        let rust_stdout = run(Command::new(rust).args([
            "--model",
            &rust_model,
            "--prompt",
            prompt,
            "--embedding",
            "--threads",
            "1",
            "--embedding-output",
            "raw",
        ]));
        let rust_line = rust_stdout.strip_suffix('\n').unwrap_or(&rust_stdout);
        assert!(
            !rust_line.contains('\n'),
            "raw stdout must contain exactly one line: {rust_stdout:?}"
        );
        let rust_values = rust_line
            .strip_prefix("embedding_raw: ")
            .expect("missing embedding_raw prefix");
        let actual = parse_numbers(rust_values);

        let reference_stdout = run(Command::new(&llama).args([
            "-m",
            &model,
            "-p",
            prompt,
            "-t",
            "1",
            "-ngl",
            "0",
            "-fa",
            "off",
            "-ctk",
            "f32",
            "-ctv",
            "f32",
            "--embd-normalize",
            "2",
            "--embd-output-format",
            "raw",
        ]));
        let reference = parse_numbers(&reference_stdout);

        assert_eq!(actual.len(), 1024, "{prompt:?}");
        assert_eq!(actual.len(), reference.len(), "{prompt:?}");
        assert!(actual.iter().all(|value| value.is_finite()), "{prompt:?}");
        assert!((norm(&actual) - 1.0).abs() <= 1e-5, "{prompt:?}");

        let similarity = cosine(&actual, &reference);
        let relative_l2 = actual
            .iter()
            .zip(&reference)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f64>()
            .sqrt()
            / norm(&reference);
        let max_abs = actual
            .iter()
            .zip(&reference)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f64, f64::max);

        assert!(similarity >= 0.9999, "{prompt:?}: cosine={similarity}");
        assert!(relative_l2 <= 0.02, "{prompt:?}: relative_l2={relative_l2}");
        assert!(max_abs <= 5e-3, "{prompt:?}: max_abs={max_abs}");
        rust_vectors.push(actual);
        llama_vectors.push(reference);
    }

    let mut max_matrix_diff = 0.0f64;
    for row in 0..FIXTURES.len() {
        for column in 0..FIXTURES.len() {
            max_matrix_diff = max_matrix_diff.max(
                (cosine(&rust_vectors[row], &rust_vectors[column])
                    - cosine(&llama_vectors[row], &llama_vectors[column]))
                .abs(),
            );
        }
    }
    assert!(
        max_matrix_diff <= 1e-3,
        "cosine matrix max diff={max_matrix_diff}"
    );
}

#[test]
#[ignore = "requires QWEN3_EMBEDDING_MODEL and patched/token-compatible LLAMA_DEBUG_BIN"]
fn qwen3_embedding_bits_match_pinned_llama_cpp() {
    let model = std::env::var("QWEN3_EMBEDDING_MODEL").unwrap();
    let rust_model_override = std::env::var("RMI_RUST_EMBEDDING_MODEL").ok();
    let rust_model = rust_embedding_model(&model, rust_model_override.as_deref());
    let llama_debug = std::env::var("LLAMA_DEBUG_BIN").unwrap();
    let rust = env!("CARGO_BIN_EXE_rust-model-inference");
    let mut failures = Vec::new();

    for (fixture, &prompt) in FIXTURES.iter().enumerate() {
        let (reference_pooled, reference_pooled_tokens) =
            reference_embedding_bits(&llama_debug, &model, prompt, fixture, -1);
        let (reference_final, reference_final_tokens) =
            reference_embedding_bits(&llama_debug, &model, prompt, fixture, 2);
        assert_eq!(
            reference_pooled_tokens, reference_final_tokens,
            "{prompt:?}: reference harness must match embedding tokenization"
        );
        let (rust_tokens, rust_pooled, rust_final) =
            rust_embedding_checkpoints(rust, &rust_model, prompt, fixture);
        assert_eq!(
            rust_tokens, reference_pooled_tokens,
            "{prompt:?}: reference harness must match embedding tokenization"
        );
        assert_eq!(rust_pooled.len(), 1024, "{prompt:?}");
        assert_eq!(rust_final.len(), 1024, "{prompt:?}");
        assert_eq!(reference_pooled.len(), 1024, "{prompt:?} reference pooled");
        assert_eq!(reference_final.len(), 1024, "{prompt:?} reference final");
        if let Some((index, rust, llama)) = first_bit_mismatch(&rust_pooled, &reference_pooled) {
            failures.push(format!(
                "{prompt:?} pooled[{index}]: rust=0x{rust:08x} llama=0x{llama:08x}"
            ));
        }
        if let Some((index, rust, llama)) = first_bit_mismatch(&rust_final, &reference_final) {
            failures.push(format!(
                "{prompt:?} final[{index}]: rust=0x{rust:08x} llama=0x{llama:08x}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn rust_embedding_model_resolves_override_and_fallback() {
    assert_eq!(
        rust_embedding_model("model.gguf", Some("/tmp/model.ggufrs")),
        "/tmp/model.ggufrs"
    );
    assert_eq!(rust_embedding_model("model.gguf", None), "model.gguf");
}
