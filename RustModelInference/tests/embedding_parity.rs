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

#[test]
#[ignore = "requires QWEN3_EMBEDDING_MODEL and LLAMA_EMBEDDING_BIN"]
fn qwen3_embedding_vectors_match_pinned_llama_cpp() {
    let model = std::env::var("QWEN3_EMBEDDING_MODEL").unwrap();
    let llama = std::env::var("LLAMA_EMBEDDING_BIN").unwrap();
    let rust = env!("CARGO_BIN_EXE_rust-model-inference");
    let mut rust_vectors = Vec::new();
    let mut llama_vectors = Vec::new();

    for &prompt in FIXTURES {
        let rust_stdout = run(Command::new(rust).args([
            "--model",
            &model,
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
