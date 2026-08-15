use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(feature = "parity-trace")]
use rust_model_inference::parity_trace;
use rust_model_inference::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum KvFormat {
    #[default]
    F16,
    F32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum EmbeddingOutput {
    #[default]
    Summary,
    Raw,
}

#[derive(Debug, Default)]
struct CliOptions {
    model: PathBuf,
    mmproj: Option<PathBuf>,
    audio: Option<PathBuf>,
    image: Option<PathBuf>,
    prompt: Option<String>,
    language: Option<String>,
    max_tokens: Option<usize>,
    temperature: Option<f32>,
    threads: usize,
    thinking: bool,
    embedding: bool,
    embedding_output: EmbeddingOutput,
    dump_logits: bool,
    bench: bool,
    profile: bool,
    kv_format: KvFormat,
    gpu: bool,
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

fn validate_qwen3vl_decoder_mode(
    arch: &str,
    dump_logits: bool,
    bench: bool,
    profile: bool,
    kv_format: KvFormat,
    interactive: bool,
) -> Result<(), String> {
    if arch != "qwen3vl" {
        return Ok(());
    }
    let unsupported = if dump_logits {
        Some("--dump-logits")
    } else if bench {
        Some("--bench")
    } else if profile {
        Some("--profile")
    } else if kv_format == KvFormat::F32 {
        Some("--kv-cache f32")
    } else if interactive {
        Some("interactive mode")
    } else {
        None
    };
    match unsupported {
        Some(option) => Err(format!(
            "{option} is not supported for qwen3vl; use default F16 generation"
        )),
        None => Ok(()),
    }
}

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
    if seconds > 0.0 {
        count as f64 / seconds
    } else {
        0.0
    }
}

#[cfg(test)]
mod cli_tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::Duration;

    struct TestTensorSource {
        info: TensorInfo,
        bytes: Vec<u8>,
    }

    impl TensorSource for TestTensorSource {
        fn metadata(&self, _key: &str) -> Option<&MetaValue> {
            None
        }

        fn tensor_info(&self, name: &str) -> Option<&TensorInfo> {
            (name == self.info.name).then_some(&self.info)
        }

        fn tensor_slice(&self, name: &str) -> Option<&[u8]> {
            (name == self.info.name).then_some(&self.bytes)
        }
    }

    #[test]
    fn f16_embedding_rows_decode_little_endian_half_values() {
        let source = TestTensorSource {
            info: TensorInfo {
                name: "token_embd.weight".into(),
                dims: vec![4, 1],
                ggml_type: GGMLType::F16,
                offset: 0,
            },
            bytes: [0x00, 0x3c, 0x00, 0xc0, 0x55, 0x35, 0x00, 0x00].to_vec(),
        };

        let weight = EmbeddingWeight::load(&source, "token_embd.weight", 4, 1).unwrap();
        let mut row = [0.0; 4];
        weight.get_row(0, &mut row).unwrap();
        assert_eq!(row, [1.0, -2.0, 0.333_251_95, 0.0]);
    }

    #[test]
    fn f16_embedding_matmul_uses_ggml_fp16_vector_accumulation() {
        let bytes = half::f16::from_f32(0.1).to_bits().to_le_bytes();
        let source = TestTensorSource {
            info: TensorInfo {
                name: "weight".into(),
                dims: vec![32, 1],
                ggml_type: GGMLType::F16,
                offset: 0,
            },
            bytes: bytes.repeat(32),
        };
        let weight = EmbeddingWeight::load(&source, "weight", 32, 1).unwrap();
        let mut scratch = EmbeddingActivationScratch::new(32);
        let activation = scratch.prepare(&weight, &[0.1; 32]).unwrap();
        let mut output = [0.0];

        weight.matmul_prepared(&activation, &mut output).unwrap();

        #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
        let expected = if std::arch::is_aarch64_feature_detected!("fp16") {
            0x3ea3_c000
        } else {
            0x3ea3_c28e
        };
        #[cfg(not(all(target_arch = "aarch64", target_endian = "little")))]
        let expected = 0x3ea3_c28e;
        assert_eq!(output[0].to_bits(), expected);
    }

    #[test]
    fn q8_embedding_matmul_uses_the_existing_quantized_kernel() {
        let source = TestTensorSource {
            info: TensorInfo {
                name: "weight".into(),
                dims: vec![32, 1],
                ggml_type: GGMLType::Q8_0,
                offset: 0,
            },
            bytes: [half::f16::from_f32(1.0).to_bits().to_le_bytes().as_slice(), &[1; 32]].concat(),
        };
        let weight = EmbeddingWeight::load(&source, "weight", 32, 1).unwrap();
        let mut scratch = EmbeddingActivationScratch::new(32);
        let activation = scratch.prepare(&weight, &[1.0; 32]).unwrap();
        let mut output = [0.0];

        weight.matmul_prepared(&activation, &mut output).unwrap();

        assert_eq!(output, [31.998_047]);
    }

    #[test]
    fn prepared_embedding_activation_is_reused_across_projections() {
        let f16_bytes = half::f16::from_f32(1.0).to_bits().to_le_bytes().repeat(32);
        let f16 = EmbeddingWeight {
            bytes: &f16_bytes,
            ggml_type: GGMLType::F16,
            n_cols: 32,
            n_rows: 1,
        };
        let q8_bytes = [
            half::f16::from_f32(1.0).to_bits().to_le_bytes().as_slice(),
            &[1; 32],
        ]
        .concat();
        let q8 = EmbeddingWeight {
            bytes: &q8_bytes,
            ggml_type: GGMLType::Q8_0,
            n_cols: 32,
            n_rows: 1,
        };
        let mut scratch = EmbeddingActivationScratch::new(32);
        let input = [1.0; 32];

        let f16_activation = scratch.prepare(&f16, &input).unwrap();
        let f16_ptr = f16_activation.f16.as_ptr();
        f16.matmul_prepared(&f16_activation, &mut [0.0]).unwrap();
        let f16_activation = scratch.prepare(&f16, &input).unwrap();
        assert_eq!(f16_activation.f16.as_ptr(), f16_ptr);

        let q8_activation = scratch.prepare(&q8, &input).unwrap();
        let q8_ptr = q8_activation.q8.as_ptr();
        q8.matmul_prepared(&q8_activation, &mut [0.0]).unwrap();
        let q8_activation = scratch.prepare(&q8, &input).unwrap();
        assert_eq!(q8_activation.q8.as_ptr(), q8_ptr);
    }

    #[test]
    fn embedding_weight_rejects_invalid_type_shape_length_and_row() {
        let invalid_type = TestTensorSource {
            info: TensorInfo {
                name: "weight".into(),
                dims: vec![4, 1],
                ggml_type: GGMLType::F32,
                offset: 0,
            },
            bytes: vec![0; 16],
        };
        assert!(EmbeddingWeight::load(&invalid_type, "weight", 4, 1)
            .unwrap_err()
            .contains("unsupported type"));

        let wrong_shape = TestTensorSource {
            info: TensorInfo {
                name: "weight".into(),
                dims: vec![2, 2],
                ggml_type: GGMLType::F16,
                offset: 0,
            },
            bytes: vec![0; 8],
        };
        assert!(EmbeddingWeight::load(&wrong_shape, "weight", 4, 1)
            .unwrap_err()
            .contains("shape"));

        let wrong_length = TestTensorSource {
            info: TensorInfo {
                name: "weight".into(),
                dims: vec![4, 1],
                ggml_type: GGMLType::F16,
                offset: 0,
            },
            bytes: vec![0; 7],
        };
        assert!(EmbeddingWeight::load(&wrong_length, "weight", 4, 1)
            .unwrap_err()
            .contains("expected 8"));

        let valid = TestTensorSource {
            info: TensorInfo {
                name: "weight".into(),
                dims: vec![4, 1],
                ggml_type: GGMLType::F16,
                offset: 0,
            },
            bytes: vec![0; 8],
        };
        let weight = EmbeddingWeight::load(&valid, "weight", 4, 1).unwrap();
        assert!(weight.get_row(1, &mut [0.0; 4]).unwrap_err().contains("out of range"));
    }

    #[test]
    fn embedding_output_accepts_only_summary_or_raw() {
        assert_eq!(
            parse_embedding_output(Some("summary")).unwrap(),
            EmbeddingOutput::Summary,
        );
        assert_eq!(
            parse_embedding_output(Some("raw")).unwrap(),
            EmbeddingOutput::Raw,
        );
        assert!(parse_embedding_output(Some("json")).is_err());
        assert!(parse_embedding_output(None).is_err());
    }

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

    fn tiny_embedding_tokenizer() -> BPETokenizer {
        let metadata: HashMap<String, MetaValue> = HashMap::from([
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
                    ["h", "e", "l", "o", "<|endoftext|>"]
                        .into_iter()
                        .map(|value| MetaValue::String(value.into()))
                        .collect(),
                ),
            ),
            (
                "tokenizer.ggml.token_type".into(),
                MetaValue::Array(
                    MetaValueType::Uint32,
                    [1, 1, 1, 1, 3].into_iter().map(MetaValue::Uint32).collect(),
                ),
            ),
            (
                "tokenizer.ggml.merges".into(),
                MetaValue::Array(MetaValueType::String, vec![]),
            ),
            ("tokenizer.ggml.bos_token_id".into(), MetaValue::Uint32(0)),
            ("tokenizer.ggml.eos_token_id".into(), MetaValue::Uint32(4)),
            (
                "tokenizer.ggml.add_bos_token".into(),
                MetaValue::Bool(false),
            ),
            ("tokenizer.ggml.add_eos_token".into(), MetaValue::Bool(true)),
        ]);

        BPETokenizer::from_gguf_metadata(|key| metadata.get(key).cloned()).unwrap()
    }

    fn q8_identity(size: usize) -> Vec<u8> {
        assert_eq!(size % 32, 0);
        let blocks_per_row = size / 32;
        let row_stride = blocks_per_row * 34;
        let mut weight = vec![0u8; size * row_stride];

        for row in 0..size {
            let block = row / 32;
            let lane = row % 32;
            let offset = row * row_stride + block * 34;
            weight[offset..offset + 2]
                .copy_from_slice(&half::f16::from_f32(1.0).to_bits().to_le_bytes());
            weight[offset + 2 + lane] = 1;
        }
        weight
    }

    #[test]
    fn embedding_ffn_keeps_each_tokens_projection_independent() {
        let identity = q8_identity(32);
        let weight = EmbeddingWeight {
            bytes: &identity,
            ggml_type: GGMLType::Q8_0,
            n_cols: 32,
            n_rows: 32,
        };
        let mut normed = vec![0.0f32; 64];
        normed[0] = 1.0;
        normed[33] = 2.0;

        let mut hidden = vec![0.0f32; 64];
        hidden[0] = 10.0;
        hidden[33] = 20.0;

        apply_embedding_ffn_typed(
            &mut hidden,
            &normed,
            32,
            32,
            &weight,
            &weight,
            &weight,
            &mut [0.0; 32],
            &mut [0.0; 32],
            &mut [0.0; 32],
            &mut EmbeddingActivationScratch::new(32),
        )
        .unwrap();

        assert!((hidden[0] - 10.731059).abs() < 1e-4, "{}", hidden[0]);
        assert!((hidden[33] - 23.523041).abs() < 1e-4, "{}", hidden[33]);
        assert_eq!(hidden[1], 0.0);
        assert_eq!(hidden[32], 0.0);
    }

    const EMBEDDING_TOKEN_CASES: &[(&str, &[u32])] = &[
        ("hello", &[14990, 151643]),
        (
            "Hello, 世界! 123",
            &[9707, 11, 220, 99489, 0, 220, 16, 17, 18, 151643],
        ),
        (
            "What is the capital of China?",
            &[3838, 374, 279, 6722, 315, 5616, 30, 151643],
        ),
        (
            "The capital of China is Beijing.",
            &[785, 6722, 315, 5616, 374, 26549, 13, 151643],
        ),
        (
            "Photosynthesis converts light into chemical energy.",
            &[31772, 73667, 32722, 3100, 1119, 11483, 4802, 13, 151643],
        ),
        (
            "中国的首都是北京。",
            &[105538, 59975, 100132, 68990, 1773, 151643],
        ),
    ];

    #[test]
    fn embedding_input_honors_tokenizer_eos_metadata() {
        assert_eq!(
            encode_embedding_input(&tiny_embedding_tokenizer(), "hello"),
            vec![0, 1, 2, 2, 3, 4],
        );
    }

    #[test]
    fn embedding_config_defaults_to_causal_and_reads_last_pooling() {
        let metadata = HashMap::from([("qwen3.pooling_type".to_string(), MetaValue::Uint32(3))]);

        assert_eq!(
            embedding_config("qwen3", |key| metadata.get(key).cloned()).unwrap(),
            EmbeddingConfig {
                causal_attn: true,
                pooling: EmbeddingPooling::Last,
            },
        );
    }

    #[test]
    fn embedding_config_reads_mean_and_non_causal_metadata() {
        let metadata = HashMap::from([
            ("qwen3.pooling_type".to_string(), MetaValue::Uint32(1)),
            ("qwen3.attention.causal".to_string(), MetaValue::Bool(false)),
        ]);

        assert_eq!(
            embedding_config("qwen3", |key| metadata.get(key).cloned()).unwrap(),
            EmbeddingConfig {
                causal_attn: false,
                pooling: EmbeddingPooling::Mean,
            },
        );
    }

    #[test]
    fn causal_embedding_attention_never_reads_future_keys() {
        assert_eq!(
            (0..3)
                .map(|query| attention_key_end(query, 3, true))
                .collect::<Vec<_>>(),
            vec![1, 2, 3],
        );
        assert_eq!(attention_key_end(0, 3, false), 3);
    }

    #[test]
    fn embedding_positions_are_contiguous_from_zero() {
        assert_eq!(embedding_positions(4).collect::<Vec<_>>(), vec![0, 1, 2, 3],);
    }

    #[test]
    fn qwen3vl_rejects_legacy_decoder_modes() {
        for (result, expected_mode) in [
            (
                validate_qwen3vl_decoder_mode("qwen3vl", true, false, false, KvFormat::F16, false),
                "--dump-logits",
            ),
            (
                validate_qwen3vl_decoder_mode("qwen3vl", false, true, false, KvFormat::F16, false),
                "--bench",
            ),
            (
                validate_qwen3vl_decoder_mode("qwen3vl", false, false, true, KvFormat::F16, false),
                "--profile",
            ),
            (
                validate_qwen3vl_decoder_mode("qwen3vl", false, false, false, KvFormat::F32, false),
                "--kv-cache f32",
            ),
            (
                validate_qwen3vl_decoder_mode("qwen3vl", false, false, false, KvFormat::F16, true),
                "interactive mode",
            ),
        ] {
            assert!(result.unwrap_err().contains(expected_mode));
        }

        assert!(validate_qwen3vl_decoder_mode(
            "qwen3vl",
            false,
            false,
            false,
            KvFormat::F16,
            false
        )
        .is_ok());
        assert!(
            validate_qwen3vl_decoder_mode("qwen3", true, true, true, KvFormat::F32, true).is_ok()
        );
    }

    #[test]
    fn embedding_config_rejects_missing_malformed_or_unsupported_pooling() {
        assert!(embedding_config("qwen3", |_| None)
            .unwrap_err()
            .contains("qwen3.pooling_type"));

        let error = embedding_config("qwen3", |key| match key {
            "qwen3.pooling_type" => Some(MetaValue::Bool(true)),
            _ => None,
        })
        .unwrap_err();
        assert!(error.contains("qwen3.pooling_type"), "{error}");

        let error = embedding_config("qwen3", |key| match key {
            "qwen3.pooling_type" => Some(MetaValue::Uint32(2)),
            _ => None,
        })
        .unwrap_err();
        assert!(error.contains("expected 1=MEAN or 3=LAST"), "{error}");
    }

    #[test]
    fn embedding_config_rejects_non_boolean_causal_metadata() {
        let metadata = HashMap::from([
            ("qwen3.pooling_type".to_string(), MetaValue::Uint32(3)),
            ("qwen3.attention.causal".to_string(), MetaValue::Uint32(1)),
        ]);

        let error = embedding_config("qwen3", |key| metadata.get(key).cloned()).unwrap_err();
        assert!(error.contains("expected bool"), "{error}");
    }

    #[test]
    fn embedding_pooling_supports_mean_and_last() {
        let hidden = [1.0, 2.0, 5.0, 6.0];
        assert_eq!(
            pool_embedding_rows(&hidden, 2, 2, EmbeddingPooling::Mean).unwrap(),
            vec![3.0, 4.0],
        );
        assert_eq!(
            pool_embedding_rows(&hidden, 2, 2, EmbeddingPooling::Last).unwrap(),
            vec![5.0, 6.0],
        );
    }

    #[test]
    fn embedding_pooling_rejects_invalid_shapes() {
        assert!(pool_embedding_rows(&[], 0, 2, EmbeddingPooling::Last).is_err());
        assert!(pool_embedding_rows(&[1.0], 1, 2, EmbeddingPooling::Last).is_err());
    }

    #[test]
    fn embedding_l2_uses_f64_accumulation_and_preserves_zero() {
        let mut values = vec![1.0f32];
        values.extend(std::iter::repeat(1e-4f32).take(4096));
        l2_normalize_embedding(&mut values).unwrap();
        assert!((values[0] - 0.9999795).abs() < 1e-6, "{}", values[0]);

        let mut zero = [0.0f32, 0.0];
        l2_normalize_embedding(&mut zero).unwrap();
        assert_eq!(zero, [0.0, 0.0]);
    }

    #[test]
    fn embedding_l2_matches_llama_f32_product_and_scale_bits() {
        let mut values = [1.0f32, 3.0];
        l2_normalize_embedding(&mut values).unwrap();
        assert_eq!(
            values.map(f32::to_bits),
            [0x3ea1e89b, 0x3f72dce8],
        );
    }

    #[test]
    fn embedding_l2_matches_llama_subnormal_underflow_to_zero() {
        let mut values = [f32::from_bits(1)];
        l2_normalize_embedding(&mut values).unwrap();
        assert_eq!(values, [0.0]);
    }

    #[test]
    fn embedding_l2_rejects_non_finite_values() {
        for value in [f32::INFINITY, f32::NAN] {
            let mut values = [value];
            assert!(l2_normalize_embedding(&mut values).is_err());
        }
    }

    #[test]
    #[ignore = "requires QWEN3_EMBEDDING_MODEL"]
    fn qwen3_embedding_tokens_match_pinned_llama_cpp() {
        let model = std::env::var("QWEN3_EMBEDDING_MODEL").unwrap();
        let source = open_model_source(Path::new(&model), ComponentRole::Llm).unwrap();
        let tokenizer =
            BPETokenizer::from_gguf_metadata(|key| source.metadata(key).cloned()).unwrap();

        for &(text, expected) in EMBEDDING_TOKEN_CASES {
            assert_eq!(
                tokenizer.encode(
                    text,
                    EncodeOptions {
                        add_special: true,
                        parse_special: true,
                    },
                ),
                expected,
                "{text:?}",
            );
        }
    }

    fn asr_cli_options() -> CliOptions {
        CliOptions {
            model: "missing.gguf".into(),
            audio: Some("missing.wav".into()),
            ..CliOptions::default()
        }
    }

    #[test]
    fn asr_cli_rejects_conflicting_modes_before_model_load() {
        let mut options = asr_cli_options();
        options.image = Some("missing.png".into());
        assert!(validate_cli_options(&options).unwrap_err().contains("--image"));

        let mut options = asr_cli_options();
        options.embedding = true;
        assert!(validate_cli_options(&options).unwrap_err().contains("--embedding"));

        let mut options = asr_cli_options();
        options.dump_logits = true;
        assert!(validate_cli_options(&options).unwrap_err().contains("--dump-logits"));

        let mut options = asr_cli_options();
        options.bench = true;
        assert!(validate_cli_options(&options).unwrap_err().contains("--bench"));

        let mut options = asr_cli_options();
        options.profile = true;
        assert!(validate_cli_options(&options).unwrap_err().contains("--profile"));

        let mut options = asr_cli_options();
        options.temperature = Some(0.1);
        assert!(validate_cli_options(&options).unwrap_err().contains("--temp"));

        let mut options = asr_cli_options();
        options.max_tokens = Some(0);
        assert!(validate_cli_options(&options)
            .unwrap_err()
            .contains("--max-tokens"));

        let mut options = asr_cli_options();
        options.audio = None;
        options.language = Some("English".into());
        assert!(validate_cli_options(&options).unwrap_err().contains("--language"));

        let mut options = asr_cli_options();
        options.prompt = Some("domain context".into());
        assert!(validate_cli_options(&options).is_ok());

        let args = ["rmi".to_string(), "--audio".to_string()];
        assert!(parse_cli_options(&args).unwrap_err().contains("--audio"));
    }

    #[test]
    fn asr_cli_rejects_empty_and_flag_shaped_values() {
        for args in [
            vec!["rmi", "--audio", ""],
            vec!["rmi", "--audio", "--image", "missing.png"],
            vec!["rmi", "--audio", "--language", "English"],
        ] {
            let args: Vec<String> = args.into_iter().map(str::to_string).collect();
            assert!(parse_cli_options(&args).unwrap_err().contains("--audio"));
        }

        let args: Vec<String> = ["rmi", "--audio", "missing.wav", "--language", "--prompt"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert!(parse_cli_options(&args)
            .unwrap_err()
            .contains("--language"));

        let args: Vec<String> = ["rmi", "--audio", "-recording.wav", "--language", "English"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let options = parse_cli_options(&args).unwrap();
        assert_eq!(options.audio.as_deref(), Some(Path::new("-recording.wav")));
        assert_eq!(options.language.as_deref(), Some("English"));

        let args: Vec<String> = ["rmi", "--audio", "missing.wav", "--language", ""]
            .into_iter()
            .map(str::to_string)
            .collect();
        let options = parse_cli_options(&args).unwrap();
        assert!(validate_cli_options(&options).is_ok());
        assert!(normalize_language(
            transcription_options(&options).language.as_deref()
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn asr_cli_defaults_are_greedy_and_256_tokens() {
        let mut options = asr_cli_options();
        options.language = Some("auto".into());
        options.prompt = Some("domain context".into());

        let (max_tokens, temperature) = resolve_cli_generation_options(&options);
        assert_eq!(max_tokens, 256);
        assert_eq!(temperature, 0.0);
        let transcription = transcription_options(&options);
        assert_eq!(transcription.language, None);
        assert_eq!(transcription.prompt.as_deref(), Some("domain context"));
        assert_eq!(transcription.max_new_tokens, 256);
        assert!(normalize_language(Some("auto")).is_err());

        let args = [
            "rmi".to_string(),
            "--audio".to_string(),
            "missing.wav".to_string(),
            "--n-gen".to_string(),
            "7".to_string(),
        ];
        assert_eq!(parse_cli_options(&args).unwrap().max_tokens, Some(7));

        let args = [
            "rmi".to_string(),
            "--unknown".to_string(),
            "--prompt".to_string(),
            "hello".to_string(),
        ];
        let text = parse_cli_options(&args).unwrap();
        assert_eq!(text.prompt.as_deref(), Some("hello"));
        assert_eq!(resolve_cli_generation_options(&text), (128, 0.6));
    }

    #[test]
    fn legacy_cli_parser_and_dispatch_semantics_are_preserved() {
        type Check = fn(&CliOptions) -> bool;
        let parse = |args: &[&str]| {
            let args: Vec<String> = args.iter().map(ToString::to_string).collect();
            parse_cli_options(&args).unwrap()
        };
        let cases: &[(&[&str], &str, Check)] = &[
            (&["rmi", "--embedding", "--prompt", "x"], "embedding", |o| o.embedding && o.prompt.as_deref() == Some("x")),
            (&["rmi", "--image", "image.png"], "image", |o| o.image.as_deref() == Some(Path::new("image.png"))),
            (&["rmi", "--mmproj", "projector.gguf"], "mmproj", |o| o.mmproj.as_deref() == Some(Path::new("projector.gguf"))),
            (&["rmi", "--model", "model.gguf"], "interactive", |o| o.prompt.is_none() && o.image.is_none() && o.mmproj.is_none()),
            (&["rmi", "positional", "--prompt", "x"], "unknown/positional", |o| o.prompt.as_deref() == Some("x")),
            (&["rmi"], "text defaults", |o| resolve_cli_generation_options(o) == (128, 0.6)),
            (&["rmi", "--max-tokens", "bad"], "malformed max", |o| o.max_tokens == Some(128)),
            (&["rmi", "--n-gen", "bad"], "malformed n-gen", |o| o.max_tokens == Some(128)),
            (&["rmi", "--temp", "bad"], "malformed temp", |o| o.temperature == Some(0.6)),
            (&["rmi", "--threads", "bad"], "malformed threads", |o| o.threads == 0),
            (&["rmi", "--kv-cache", "f32"], "F32 KV", |o| o.kv_format == KvFormat::F32),
            (&["rmi", "--kv-cache", "bad"], "fallback F16 KV", |o| o.kv_format == KvFormat::F16),
            (&["rmi", "--model", "", "--prompt", "", "--max-tokens", "", "--temp", "", "--threads", "", "--kv-cache", "", "--mmproj", "", "--image", ""], "empty legacy values", |o| o.model.as_os_str().is_empty() && o.prompt.as_deref() == Some("") && o.max_tokens == Some(128) && o.temperature == Some(0.6) && o.threads == 0 && o.kv_format == KvFormat::F16 && o.mmproj.as_deref() == Some(Path::new("")) && o.image.as_deref() == Some(Path::new(""))),
        ];
        for (args, name, check) in cases {
            assert!(check(&parse(args)), "{name}");
        }

        let absent: &[(&str, Check)] = &[
            ("--model", |o| o.model.as_os_str().is_empty()),
            ("--prompt", |o| o.prompt.is_none()),
            ("--max-tokens", |o| o.max_tokens.is_none()),
            ("--n-gen", |o| o.max_tokens.is_none()),
            ("--temp", |o| o.temperature.is_none()),
            ("--threads", |o| o.threads == 0),
            ("--kv-cache", |o| o.kv_format == KvFormat::F16),
            ("--mmproj", |o| o.mmproj.is_none()),
            ("--image", |o| o.image.is_none()),
        ];
        for (flag, check) in absent {
            assert!(check(&parse(&["rmi", flag])), "absent {flag}");
        }
        assert!(parse_cli_options(&["rmi".into(), "--embedding-output".into()]).is_err());

        for (value, expected) in [
            ("0", 0.0),
            ("-1", -1.0),
            ("NaN", f32::NAN),
            ("inf", f32::INFINITY),
            ("-inf", f32::NEG_INFINITY),
        ] {
            let options = parse(&["rmi", "--temp", value]);
            let actual = options.temperature.unwrap();
            if expected.is_nan() {
                assert!(actual.is_nan());
            } else {
                assert_eq!(actual, expected);
            }
            assert!(validate_cli_options(&options).is_ok());
        }
    }
}

fn parse_cli_options(args: &[String]) -> Result<CliOptions, String> {
    let mut options = CliOptions::default();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--model" => {
                if i + 1 < args.len() {
                    options.model = args[i + 1].as_str().into();
                    i += 1;
                }
            }
            "--prompt" => {
                if i + 1 < args.len() {
                    options.prompt = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--max-tokens" | "--n-gen" => {
                if i + 1 < args.len() {
                    options.max_tokens = Some(args[i + 1].parse().unwrap_or(128));
                    i += 1;
                }
            }
            "--temp" => {
                if i + 1 < args.len() {
                    options.temperature = Some(args[i + 1].parse().unwrap_or(0.6));
                    i += 1;
                }
            }
            "--threads" => {
                if i + 1 < args.len() {
                    options.threads = args[i + 1].parse().unwrap_or(0);
                    i += 1;
                }
            }
            "--dump-logits" => options.dump_logits = true,
            "--embedding" => options.embedding = true,
            "--embedding-output" => {
                options.embedding_output =
                    parse_embedding_output(args.get(i + 1).map(String::as_str))?;
                i += 1;
            }
            "--bench" => options.bench = true,
            "--thinking" => options.thinking = true,
            "--profile" => options.profile = true,
            "--gpu" => options.gpu = true,
            "--kv-cache" => {
                if i + 1 < args.len() {
                    options.kv_format = match args[i + 1].as_str() {
                        "f32" => KvFormat::F32,
                        _ => KvFormat::F16,
                    };
                    i += 1;
                }
            }
            "--mmproj" => {
                if i + 1 < args.len() {
                    options.mmproj = Some(args[i + 1].as_str().into());
                    i += 1;
                }
            }
            "--image" => {
                if i + 1 < args.len() {
                    options.image = Some(args[i + 1].as_str().into());
                    i += 1;
                }
            }
            "--audio" => {
                let value = args
                    .get(i + 1)
                    .filter(|value| !value.is_empty() && !value.starts_with("--"))
                    .ok_or("Missing value for --audio")?;
                options.audio = Some(value.as_str().into());
                i += 1;
            }
            "--language" => {
                let value = args
                    .get(i + 1)
                    .filter(|value| !value.starts_with("--"))
                    .ok_or("Missing value for --language")?;
                options.language = Some(value.clone());
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    Ok(options)
}

fn validate_cli_options(options: &CliOptions) -> Result<(), String> {
    if options.audio.is_none() {
        return if options.language.is_some() {
            Err("--language requires --audio".into())
        } else {
            Ok(())
        };
    }
    let conflict = if options.image.is_some() {
        Some("--image")
    } else if options.embedding {
        Some("--embedding")
    } else if options.dump_logits {
        Some("--dump-logits")
    } else if options.bench {
        Some("--bench")
    } else if options.profile {
        Some("--profile")
    } else {
        None
    };
    if let Some(conflict) = conflict {
        return Err(format!("--audio cannot be used with {conflict}"));
    }
    if options.temperature.is_some_and(|temperature| temperature != 0.0) {
        return Err("--audio requires greedy decoding; --temp must be 0".into());
    }
    if options.max_tokens == Some(0) {
        return Err("--audio requires --max-tokens greater than 0".into());
    }
    Ok(())
}

fn resolve_cli_generation_options(options: &CliOptions) -> (usize, f32) {
    (
        options
            .max_tokens
            .unwrap_or(if options.audio.is_some() { 256 } else { 128 }),
        options
            .temperature
            .unwrap_or(if options.audio.is_some() { 0.0 } else { 0.6 }),
    )
}

fn transcription_options(options: &CliOptions) -> TranscriptionOptions {
    let language = options
        .language
        .as_ref()
        .filter(|language| !language.eq_ignore_ascii_case("auto"))
        .cloned();
    TranscriptionOptions {
        language,
        prompt: options.prompt.clone(),
        max_new_tokens: resolve_cli_generation_options(options).0,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let options = parse_cli_options(&args).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    validate_cli_options(&options).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });

    #[cfg(feature = "gpu")]
    if options.gpu {
        let _ = compute::init();
        compute::set_device_ratio(compute::DeviceKind::Gpu(0), 100);
        eprintln!("GPU enabled");
    }

    if options.model.as_os_str().is_empty() {
        run_self_test();
        return;
    }

    if options.audio.is_some() {
        run_or_exit(run_asr_cli(&options));
        return;
    }

    let (max_tokens, temperature) = resolve_cli_generation_options(&options);
    let prompt = options.prompt.as_deref().unwrap_or_default();

    let model_path = options.model.as_path();
    let source: std::sync::Arc<dyn TensorSource> =
        std::sync::Arc::from(open_or_exit(model_path, ComponentRole::Llm));
    let arch = source
        .metadata("general.architecture")
        .and_then(MetaValue::to_string_val)
        .unwrap_or_default();
    let explicit_mmproj = options
        .mmproj
        .as_deref()
        .filter(|path| !path.as_os_str().is_empty());
    let image = options
        .image
        .as_deref()
        .filter(|path| !path.as_os_str().is_empty());

    if explicit_mmproj.is_some() || image.is_some() {
        run_or_exit(run_multimodal(
            source.as_ref(),
            model_path,
            explicit_mmproj,
            image,
            prompt,
            max_tokens,
            temperature,
            options.threads,
        ));
    } else if !prompt.is_empty() {
        if arch == "qwen35" {
            run_or_exit(run_multimodal(
                source.as_ref(),
                model_path,
                None,
                None,
                prompt,
                max_tokens,
                temperature,
                options.threads,
            ));
        } else if options.embedding {
            run_embedding(
                source.as_ref(),
                prompt,
                options.threads,
                options.kv_format,
                options.embedding_output,
            );
        } else if arch == "qwen3vl" {
            run_or_exit(validate_qwen3vl_decoder_mode(
                &arch,
                options.dump_logits,
                options.bench,
                options.profile,
                options.kv_format,
                false,
            ));
            run_or_exit(run_shared_inference(
                std::sync::Arc::clone(&source),
                prompt,
                max_tokens,
                temperature,
                options.threads,
                options.thinking,
            ));
        } else if options.dump_logits {
            run_or_exit(run_dump_logits(
                source.as_ref(),
                prompt,
                max_tokens,
                options.threads,
                options.kv_format,
            ));
        } else if options.bench || options.profile || options.kv_format == KvFormat::F32 {
            run_or_exit(run_inference(
                source.as_ref(),
                prompt,
                max_tokens,
                temperature,
                options.threads,
                options.thinking,
                options.bench,
                options.profile,
                options.kv_format,
            ));
        } else {
            run_or_exit(run_shared_inference(
                std::sync::Arc::clone(&source),
                prompt,
                max_tokens,
                temperature,
                options.threads,
                options.thinking,
            ));
        }
    } else {
        run_or_exit(validate_qwen3vl_decoder_mode(
            &arch,
            options.dump_logits,
            options.bench,
            options.profile,
            options.kv_format,
            true,
        ));
        run_or_exit(run_interactive(
            source.as_ref(),
            max_tokens,
            temperature,
            options.threads,
        ));
    }
}

fn run_asr_cli(options: &CliOptions) -> Result<(), String> {
    let started = Instant::now();
    eprintln!("Loading ASR decoder from {}", options.model.display());
    let llm_source: Arc<dyn TensorSource> = Arc::from(
        open_model_source(&options.model, ComponentRole::Llm).map_err(|error| error.to_string())?,
    );
    let tokenizer = Arc::new(BPETokenizer::from_gguf_metadata(|key| {
        llm_source.metadata(key).cloned()
    })?);
    let available = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1);
    let pool = Arc::new(thread_pool::ComputePool::new(resolve_thread_count(
        options.threads,
        available,
    )));
    let decoder = Arc::new(Qwen3Model::from_source(llm_source, tokenizer, pool)?);
    if decoder.config().architecture != "qwen3vl" {
        return Err("--audio requires a qwen3vl decoder".into());
    }
    let audio_source: Arc<dyn TensorSource> = match options.mmproj.as_deref() {
        Some(path) => Arc::from(
            open_model_source(path, ComponentRole::Mmproj).map_err(|error| error.to_string())?,
        ),
        None => open_bundled_audio_source(&options.model)?
            .ok_or("raw GGUF ASR requires --mmproj")?,
    };
    let runtime = AsrRuntime::new(decoder, audio_source).map_err(|error| error.to_string())?;
    let audio = options.audio.as_ref().expect("validated audio option");
    let wav = std::fs::read(audio)
        .map_err(|error| format!("Failed to read {}: {error}", audio.display()))?;
    let result = runtime
        .transcribe_wav(&wav, &transcription_options(options))
        .map_err(|error| error.to_string())?;
    eprintln!(
        "ASR: {} prompt tokens, {} audio tokens, {} output tokens in {:.3}s",
        result.prompt_tokens,
        result.audio_tokens,
        result.token_ids.len(),
        started.elapsed().as_secs_f64(),
    );
    println!("{}", result.text);
    Ok(())
}

fn open_or_exit(path: &Path, role: ComponentRole) -> Box<dyn TensorSource> {
    open_model_source(path, role).unwrap_or_else(|error| {
        eprintln!(
            "Failed to load {} component from {}: {error}",
            match role {
                ComponentRole::Llm => "LLM",
                ComponentRole::Mmproj => "mmproj",
            },
            path.display(),
        );
        std::process::exit(1);
    })
}

fn run_or_exit(result: Result<(), String>) {
    if let Err(error) = result {
        eprintln!("Inference error: {error}");
        std::process::exit(1);
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

#[derive(Debug)]
struct EmbeddingWeight<'a> {
    bytes: &'a [u8],
    ggml_type: GGMLType,
    n_cols: usize,
    n_rows: usize,
}

struct EmbeddingActivationScratch {
    f16: Vec<u16>,
    q8: Vec<u8>,
    scales: Vec<f32>,
}

struct EmbeddingActivation<'a> {
    ggml_type: GGMLType,
    n_cols: usize,
    f16: &'a [u16],
    q8: &'a [u8],
    scales: &'a [f32],
}

impl EmbeddingActivationScratch {
    fn new(max_cols: usize) -> Self {
        Self {
            f16: vec![0; max_cols],
            q8: vec![0; max_cols],
            scales: vec![0.0; max_cols.div_ceil(32)],
        }
    }

    fn prepare<'a>(
        &'a mut self,
        weight: &EmbeddingWeight<'_>,
        input: &[f32],
    ) -> Result<EmbeddingActivation<'a>, String> {
        if input.len() != weight.n_cols || input.len() > self.f16.len() {
            return Err(format!(
                "Embedding activation has {} values; expected {} (scratch capacity {})",
                input.len(),
                weight.n_cols,
                self.f16.len()
            ));
        }
        match weight.ggml_type {
            GGMLType::F16 => f32_slice_to_f16(input, &mut self.f16[..input.len()]),
            GGMLType::Q8_0 => quantize_q8_0_into(
                input,
                input.len(),
                &mut self.q8[..input.len()],
                &mut self.scales[..input.len() / 32],
            ),
            _ => unreachable!("EmbeddingWeight validates its type"),
        }
        Ok(EmbeddingActivation {
            ggml_type: weight.ggml_type,
            n_cols: input.len(),
            f16: &self.f16[..input.len()],
            q8: &self.q8[..input.len()],
            scales: &self.scales[..input.len() / 32],
        })
    }
}

impl<'a> EmbeddingWeight<'a> {
    fn load(
        source: &'a dyn TensorSource,
        name: &str,
        n_cols: usize,
        n_rows: usize,
    ) -> Result<Self, String> {
        let info = source
            .tensor_info(name)
            .ok_or_else(|| format!("Embedding tensor {name} not found"))?;
        let expected_dims = [n_cols as u64, n_rows as u64];
        if info.dims != expected_dims {
            return Err(format!(
                "Embedding tensor {name} has shape {:?}; expected {:?}",
                info.dims, expected_dims
            ));
        }
        if !matches!(info.ggml_type, GGMLType::F16 | GGMLType::Q8_0) {
            return Err(format!(
                "Embedding tensor {name} has unsupported type {:?}; expected F16 or Q8_0",
                info.ggml_type
            ));
        }
        if info.ggml_type == GGMLType::Q8_0 && n_cols % 32 != 0 {
            return Err(format!(
                "Embedding tensor {name} has Q8_0 columns {n_cols}; expected a multiple of 32"
            ));
        }
        let n_elements = n_cols
            .checked_mul(n_rows)
            .ok_or_else(|| format!("Embedding tensor {name} shape overflows"))?;
        let expected_bytes = info.ggml_type.nbytes(n_elements);
        let bytes = source
            .tensor_slice(name)
            .ok_or_else(|| format!("Embedding tensor {name} data not found"))?;
        if bytes.len() != expected_bytes {
            return Err(format!(
                "Embedding tensor {name} has {} bytes; expected {expected_bytes}",
                bytes.len()
            ));
        }
        Ok(Self {
            bytes,
            ggml_type: info.ggml_type,
            n_cols,
            n_rows,
        })
    }

    fn get_row(&self, row: usize, output: &mut [f32]) -> Result<(), String> {
        if row >= self.n_rows {
            return Err(format!(
                "Embedding row {row} is out of range for {} rows",
                self.n_rows
            ));
        }
        if output.len() != self.n_cols {
            return Err(format!(
                "Embedding row output has {} values; expected {}",
                output.len(), self.n_cols
            ));
        }
        match self.ggml_type {
            GGMLType::F16 => {
                let offset = row * self.n_cols * 2;
                for (value, bytes) in output
                    .iter_mut()
                    .zip(self.bytes[offset..offset + self.n_cols * 2].chunks_exact(2))
                {
                    *value = f16_to_f32(u16::from_le_bytes(bytes.try_into().unwrap()));
                }
            }
            GGMLType::Q8_0 => embedding_lookup_q8_0(self.bytes, row as u32, self.n_cols, output),
            _ => unreachable!("EmbeddingWeight validates its type"),
        }
        Ok(())
    }

    fn matmul_prepared(
        &self,
        activation: &EmbeddingActivation<'_>,
        output: &mut [f32],
    ) -> Result<(), String> {
        if activation.ggml_type != self.ggml_type
            || activation.n_cols != self.n_cols
            || output.len() != self.n_rows
        {
            return Err(format!(
                "Embedding matmul has activation/output type {:?} shape {}/{}; expected {:?} {}/{}",
                activation.ggml_type,
                activation.n_cols,
                output.len(),
                self.ggml_type,
                self.n_cols,
                self.n_rows
            ));
        }
        match self.ggml_type {
            GGMLType::F16 => {
                for (row, value) in output.iter_mut().enumerate() {
                    let offset = row * self.n_cols * 2;
                    *value = dot_f16_f16_bytes(
                        activation.f16,
                        &self.bytes[offset..offset + self.n_cols * 2],
                        self.n_cols,
                    );
                }
            }
            GGMLType::Q8_0 => matmul_q8_0_quantized(
                self.bytes,
                activation.q8,
                activation.scales,
                output,
                self.n_cols,
                self.n_rows,
            ),
            _ => unreachable!("EmbeddingWeight validates its type"),
        }
        Ok(())
    }
}

fn embedding_matmul_group(
    input: &[f32],
    projections: &mut [(&EmbeddingWeight<'_>, &mut [f32])],
    scratch: &mut EmbeddingActivationScratch,
) -> Result<(), String> {
    for ggml_type in [GGMLType::F16, GGMLType::Q8_0] {
        if let Some(index) = projections
            .iter()
            .position(|(weight, _)| weight.ggml_type == ggml_type)
        {
            let activation = scratch.prepare(projections[index].0, input)?;
            for (weight, output) in projections.iter_mut() {
                if weight.ggml_type == ggml_type {
                    weight.matmul_prepared(&activation, output)?;
                }
            }
        }
    }
    Ok(())
}

struct EmbeddingLayerWeights<'a> {
    attn_norm: Vec<f32>,
    ffn_norm: Vec<f32>,
    q_norm: Option<Vec<f32>>,
    k_norm: Option<Vec<f32>>,
    wq: EmbeddingWeight<'a>,
    wk: EmbeddingWeight<'a>,
    wv: EmbeddingWeight<'a>,
    wo: EmbeddingWeight<'a>,
    w_gate: EmbeddingWeight<'a>,
    w_up: EmbeddingWeight<'a>,
    w_down: EmbeddingWeight<'a>,
}

macro_rules! slice_from_mut {
    ($ptr:expr, $len:expr) => {
        unsafe { std::slice::from_raw_parts_mut($ptr, $len) }
    };
}

macro_rules! slice_from_ref {
    ($ptr:expr, $len:expr) => {
        unsafe { std::slice::from_raw_parts($ptr, $len) }
    };
}

macro_rules! raw_parts {
    ($ptr:expr, $len:expr) => {
        unsafe { std::slice::from_raw_parts($ptr, $len) }
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmbeddingPooling {
    Mean,
    Last,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EmbeddingConfig {
    causal_attn: bool,
    pooling: EmbeddingPooling,
}

fn embedding_config(
    arch: &str,
    get_meta: impl Fn(&str) -> Option<MetaValue>,
) -> Result<EmbeddingConfig, String> {
    let pooling_key = format!("{arch}.pooling_type");
    let pooling = match get_meta(&pooling_key).and_then(|value| value.to_u64()) {
        Some(1) => EmbeddingPooling::Mean,
        Some(3) => EmbeddingPooling::Last,
        Some(value) => {
            return Err(format!(
                "Unsupported {pooling_key}: {value}; expected 1=MEAN or 3=LAST"
            ));
        }
        None => return Err(format!("Missing or invalid metadata: {pooling_key}")),
    };

    let causal_key = format!("{arch}.attention.causal");
    let causal_attn = match get_meta(&causal_key) {
        None => true,
        Some(MetaValue::Bool(value)) => value,
        Some(value) => {
            return Err(format!(
                "Invalid metadata {causal_key}: expected bool, got {value:?}"
            ));
        }
    };

    Ok(EmbeddingConfig {
        causal_attn,
        pooling,
    })
}

fn encode_embedding_input(tokenizer: &BPETokenizer, prompt: &str) -> Vec<u32> {
    tokenizer.encode(
        prompt,
        EncodeOptions {
            add_special: true,
            parse_special: true,
        },
    )
}

fn embedding_positions(n_tokens: usize) -> std::ops::Range<usize> {
    0..n_tokens
}

fn attention_key_end(query: usize, n_tokens: usize, causal: bool) -> usize {
    if causal {
        (query + 1).min(n_tokens)
    } else {
        n_tokens
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_embedding_ffn_typed(
    hidden: &mut [f32],
    normed: &[f32],
    n_embd: usize,
    n_ff: usize,
    w_gate: &EmbeddingWeight<'_>,
    w_up: &EmbeddingWeight<'_>,
    w_down: &EmbeddingWeight<'_>,
    gate_buf: &mut [f32],
    up_buf: &mut [f32],
    down_buf: &mut [f32],
    activation_scratch: &mut EmbeddingActivationScratch,
) -> Result<(), String> {
    assert_eq!(hidden.len(), normed.len());
    assert!(n_embd > 0 && n_ff > 0);
    assert_eq!(hidden.len() % n_embd, 0);
    assert_eq!(gate_buf.len(), n_ff);
    assert_eq!(up_buf.len(), n_ff);
    assert_eq!(down_buf.len(), n_embd);

    for (input, residual) in normed
        .chunks_exact(n_embd)
        .zip(hidden.chunks_exact_mut(n_embd))
    {
        embedding_matmul_group(
            input,
            &mut [(w_gate, &mut *gate_buf), (w_up, &mut *up_buf)],
            activation_scratch,
        )?;

        silu_mul_inplace(gate_buf, up_buf);

        embedding_matmul_group(
            up_buf,
            &mut [(w_down, &mut *down_buf)],
            activation_scratch,
        )?;

        for index in 0..n_embd {
            residual[index] += down_buf[index];
        }
    }
    Ok(())
}

fn pool_embedding_rows(
    hidden: &[f32],
    n_tokens: usize,
    n_embd: usize,
    pooling: EmbeddingPooling,
) -> Result<Vec<f32>, String> {
    let expected = n_tokens
        .checked_mul(n_embd)
        .ok_or_else(|| "Embedding shape overflow".to_string())?;
    if n_tokens == 0 || n_embd == 0 || hidden.len() != expected {
        return Err(format!(
            "Invalid embedding shape: rows={n_tokens}, cols={n_embd}, values={}",
            hidden.len()
        ));
    }

    match pooling {
        EmbeddingPooling::Last => Ok(hidden[(n_tokens - 1) * n_embd..n_tokens * n_embd].to_vec()),
        EmbeddingPooling::Mean => {
            let mut pooled = vec![0.0f32; n_embd];
            for row in hidden.chunks_exact(n_embd) {
                for (output, value) in pooled.iter_mut().zip(row) {
                    *output += *value;
                }
            }
            let scale = 1.0 / n_tokens as f32;
            for value in &mut pooled {
                *value *= scale;
            }
            Ok(pooled)
        }
    }
}

fn l2_normalize_embedding(values: &mut [f32]) -> Result<(), String> {
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

    for value in values.iter_mut() {
        *value *= scale;
    }

    if values.iter().any(|value| !value.is_finite()) {
        return Err("Normalized embedding contains a non-finite value".into());
    }
    Ok(())
}

fn run_embedding(
    source: &dyn TensorSource,
    prompt: &str,
    n_threads_arg: usize,
    _kv_format: KvFormat,
    output: EmbeddingOutput,
) {
    let t0 = Instant::now();
    let config = model_config_from_source(source).expect("Failed to parse model config");

    let arch = source
        .metadata("general.architecture")
        .and_then(|v| v.to_string_val())
        .unwrap_or_default();
    let is_qwen3 = arch == "qwen3";

    let tokenizer = BPETokenizer::from_gguf_metadata(|k| source.metadata(k).cloned())
        .expect("Failed to init tokenizer");
    let embedding_cfg = embedding_config(&arch, |key| source.metadata(key).cloned())
        .unwrap_or_else(|error| {
            eprintln!("Embedding metadata error: {error}");
            std::process::exit(1);
        });

    let n_embd = config.n_embd;
    let n_layer = config.n_layer;
    let n_head = config.n_head;
    let n_head_kv = config.n_head_kv;
    let n_embd_head = config.n_embd_head;
    let n_embd_head_k = if let Some(v) = source.metadata(&format!("{}.attention.key_length", arch))
    {
        v.to_u64().unwrap_or(n_embd_head as u64) as usize
    } else {
        n_embd_head
    };
    let n_embd_head_v =
        if let Some(v) = source.metadata(&format!("{}.attention.value_length", arch)) {
            v.to_u64().unwrap_or(n_embd_head as u64) as usize
        } else {
            n_embd_head
        };
    let n_embd_q = n_head * n_embd_head_k;
    let n_embd_gqa = n_head_kv * n_embd_head_v;
    let n_ff = config.n_ff;
    let eps = config.norm_eps;
    let freq_base = config.rope_freq_base;

    let output_norm = get_f32_tensor(source, "output_norm.weight", n_embd);
    let embd_weight = EmbeddingWeight::load(source, "token_embd.weight", n_embd, tokenizer.vocab_size())
        .unwrap_or_else(|error| panic!("Failed to load embedding token weights: {error}"));

    let layers: Vec<EmbeddingLayerWeights> = (0..n_layer)
        .map(|l| EmbeddingLayerWeights {
            attn_norm: get_f32_tensor(source, &format!("blk.{}.attn_norm.weight", l), n_embd),
            ffn_norm: get_f32_tensor(source, &format!("blk.{}.ffn_norm.weight", l), n_embd),
            q_norm: if is_qwen3 {
                Some(get_f32_tensor(
                    source,
                    &format!("blk.{}.attn_q_norm.weight", l),
                    n_embd_head_k,
                ))
            } else {
                None
            },
            k_norm: if is_qwen3 {
                Some(get_f32_tensor(
                    source,
                    &format!("blk.{}.attn_k_norm.weight", l),
                    n_embd_head_k,
                ))
            } else {
                None
            },
            wq: EmbeddingWeight::load(source, &format!("blk.{l}.attn_q.weight"), n_embd, n_embd_q)
                .unwrap_or_else(|error| panic!("Failed to load embedding Q weights: {error}")),
            wk: EmbeddingWeight::load(source, &format!("blk.{l}.attn_k.weight"), n_embd, n_embd_gqa)
                .unwrap_or_else(|error| panic!("Failed to load embedding K weights: {error}")),
            wv: EmbeddingWeight::load(source, &format!("blk.{l}.attn_v.weight"), n_embd, n_embd_gqa)
                .unwrap_or_else(|error| panic!("Failed to load embedding V weights: {error}")),
            wo: EmbeddingWeight::load(source, &format!("blk.{l}.attn_output.weight"), n_embd_q, n_embd)
                .unwrap_or_else(|error| panic!("Failed to load embedding output weights: {error}")),
            w_gate: EmbeddingWeight::load(source, &format!("blk.{l}.ffn_gate.weight"), n_embd, n_ff)
                .unwrap_or_else(|error| panic!("Failed to load embedding gate weights: {error}")),
            w_up: EmbeddingWeight::load(source, &format!("blk.{l}.ffn_up.weight"), n_embd, n_ff)
                .unwrap_or_else(|error| panic!("Failed to load embedding up weights: {error}")),
            w_down: EmbeddingWeight::load(source, &format!("blk.{l}.ffn_down.weight"), n_ff, n_embd)
                .unwrap_or_else(|error| panic!("Failed to load embedding down weights: {error}")),
        })
        .collect();

    let load_ms = t0.elapsed().as_millis();
    if output == EmbeddingOutput::Summary {
        println!(
            "Model: {} | n_embd={} n_layer={} n_head={} n_head_kv={} n_ff={} | loaded in {}ms",
            arch, n_embd, n_layer, n_head, n_head_kv, n_ff, load_ms
        );
    }

    let prompt_tokens = encode_embedding_input(&tokenizer, prompt);
    #[cfg(feature = "parity-trace")]
    parity_trace::report(parity_trace::token_ids("embedding.tokens", &prompt_tokens));
    if prompt_tokens.is_empty() {
        eprintln!("Embedding input produced no tokens");
        std::process::exit(1);
    }
    let n_tokens = prompt_tokens.len();
    let available_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let n_threads = resolve_thread_count(n_threads_arg, available_threads);

    let pool = std::sync::Arc::new(thread_pool::ComputePool::new(n_threads));
    eprintln!("compute pool: {} threads", pool.n_threads());
    if output == EmbeddingOutput::Summary {
        println!("Prompt: {} ({} tokens)", prompt, n_tokens);
    }

    let kq_scale = 1.0f32 / (n_embd_head_k as f32).sqrt();
    let group_size = n_head / n_head_kv;

    let mut hidden = vec![0.0f32; n_tokens * n_embd];
    let mut q_buf = vec![0.0f32; n_tokens * n_embd_q];
    let mut k_buf = vec![0.0f32; n_tokens * n_embd_gqa];
    let mut v_buf = vec![0.0f32; n_tokens * n_embd_gqa];
    let mut attn_out = vec![0.0f32; n_tokens * n_embd_q];
    let mut attn_proj = vec![0.0f32; n_tokens * n_embd];
    let mut normed = vec![0.0f32; n_tokens * n_embd];
    let mut gate_buf = vec![0.0f32; n_ff];
    let mut up_buf = vec![0.0f32; n_ff];
    let mut down_buf = vec![0.0f32; n_embd];
    let mut activation_scratch =
        EmbeddingActivationScratch::new(n_embd.max(n_embd_q).max(n_ff));
    let max_n_padded = (n_tokens + 255) / 256 * 256;
    let mut scores = vec![0.0f32; max_n_padded];
    let mut values = vec![0.0f32; max_n_padded];

    for t in 0..n_tokens {
        let token_id = prompt_tokens[t];
        let x_slice = &mut hidden[t * n_embd..(t + 1) * n_embd];
        embd_weight
            .get_row(token_id as usize, x_slice)
            .unwrap_or_else(|error| panic!("Failed to read embedding token row: {error}"));
    }

    #[cfg(feature = "parity-trace")]
    parity_trace::report(parity_trace::checkpoint(
        "embedding.inp_embd",
        None,
        &[n_tokens, n_embd],
        &hidden,
    ));

    eprintln!(
        "DEBUG: initial embedding[0:8] = {:?}, n_embd={}, token_id={}",
        &hidden[..8],
        n_embd,
        prompt_tokens[0]
    );
    let t_embed = Instant::now();
    for layer in 0..n_layer {
        let lw = &layers[layer];

        for t in 0..n_tokens {
            rms_norm(
                &hidden[t * n_embd..(t + 1) * n_embd],
                &lw.attn_norm,
                &mut normed[t * n_embd..(t + 1) * n_embd],
                eps,
            );
        }

        for t in 0..n_tokens {
            let x = &normed[t * n_embd..(t + 1) * n_embd];
            let q = &mut q_buf[t * n_embd_q..(t + 1) * n_embd_q];
            let k = &mut k_buf[t * n_embd_gqa..(t + 1) * n_embd_gqa];
            let v = &mut v_buf[t * n_embd_gqa..(t + 1) * n_embd_gqa];

            embedding_matmul_group(
                x,
                &mut [(&lw.wq, q), (&lw.wk, k), (&lw.wv, v)],
                &mut activation_scratch,
            )
            .unwrap_or_else(|error| panic!("Embedding Q/K/V matmul failed: {error}"));
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

        for t in embedding_positions(n_tokens) {
            let q = &mut q_buf[t * n_embd_q..(t + 1) * n_embd_q];
            for h in 0..n_head {
                rope_neox(
                    &mut q[h * n_embd_head_k..(h + 1) * n_embd_head_k],
                    t,
                    n_embd_head_k,
                    freq_base,
                );
            }
        }
        for t in embedding_positions(n_tokens) {
            let k = &mut k_buf[t * n_embd_gqa..(t + 1) * n_embd_gqa];
            for h in 0..n_head_kv {
                rope_neox(
                    &mut k[h * n_embd_head_k..(h + 1) * n_embd_head_k],
                    t,
                    n_embd_head_k,
                    freq_base,
                );
            }
        }

        for t in 0..n_tokens {
            let q_row = &q_buf[t * n_embd_q..(t + 1) * n_embd_q];
            let attn_row = &mut attn_out[t * n_embd_q..(t + 1) * n_embd_q];

            for h in 0..n_head {
                let kv_h = h / group_size;
                let q_off = h * n_embd_head_k;
                let out_base = h * n_embd_head_v;
                let n_cached = attention_key_end(t, n_tokens, embedding_cfg.causal_attn);
                let n_padded = (n_cached + 255) / 256 * 256;
                for s in 0..n_cached {
                    let k_row = &k_buf[s * n_embd_gqa..(s + 1) * n_embd_gqa];
                    scores[s] = dot_f32(
                        &q_row[q_off..q_off + n_embd_head_k],
                        &k_row[kv_h * n_embd_head_v..kv_h * n_embd_head_v + n_embd_head_k],
                        n_embd_head_k,
                    ) * kq_scale;
                }
                scores[n_cached..n_padded].fill(f32::NEG_INFINITY);
                softmax(&mut scores[..n_padded]);
                for d in 0..n_embd_head_v {
                    for s in 0..n_cached {
                        values[s] = v_buf[s * n_embd_gqa + kv_h * n_embd_head_v + d];
                    }
                    values[n_cached..n_padded].fill(0.0);
                    attn_row[out_base + d] = attention_value_f32(
                        &values[..n_padded],
                        &scores[..n_padded],
                        n_cached,
                        n_padded,
                    );
                }
            }
        }

        for t in 0..n_tokens {
            let attn = &attn_out[t * n_embd_q..(t + 1) * n_embd_q];
            let proj = &mut attn_proj[t * n_embd..(t + 1) * n_embd];

            embedding_matmul_group(
                attn,
                &mut [(&lw.wo, proj)],
                &mut activation_scratch,
            )
            .unwrap_or_else(|error| panic!("Embedding output matmul failed: {error}"));
        }

        for t in 0..n_tokens {
            let x = &mut hidden[t * n_embd..(t + 1) * n_embd];
            let proj = &attn_proj[t * n_embd..(t + 1) * n_embd];
            for i in 0..n_embd {
                x[i] += proj[i];
            }
        }

        #[cfg(feature = "parity-trace")]
        parity_trace::report(parity_trace::checkpoint(
            "embedding.ffn_inp",
            Some(layer),
            &[n_tokens, n_embd],
            &hidden,
        ));

        for t in 0..n_tokens {
            rms_norm(
                &hidden[t * n_embd..(t + 1) * n_embd],
                &lw.ffn_norm,
                &mut normed[t * n_embd..(t + 1) * n_embd],
                eps,
            );
        }

        apply_embedding_ffn_typed(
            &mut hidden,
            &normed,
            n_embd,
            n_ff,
            &lw.w_gate,
            &lw.w_up,
            &lw.w_down,
            &mut gate_buf,
            &mut up_buf,
            &mut down_buf,
            &mut activation_scratch,
        )
        .unwrap_or_else(|error| panic!("Embedding FFN failed: {error}"));

        #[cfg(feature = "parity-trace")]
        parity_trace::report(parity_trace::checkpoint(
            "embedding.l_out",
            Some(layer),
            &[n_tokens, n_embd],
            &hidden,
        ));
    }

    for t in 0..n_tokens {
        let x = &mut hidden[t * n_embd..(t + 1) * n_embd];
        rms_norm(
            x,
            &output_norm,
            &mut normed[t * n_embd..(t + 1) * n_embd],
            eps,
        );
        x.copy_from_slice(&normed[t * n_embd..(t + 1) * n_embd]);
    }

    #[cfg(feature = "parity-trace")]
    parity_trace::report(parity_trace::checkpoint(
        "embedding.result_norm",
        None,
        &[n_tokens, n_embd],
        &hidden,
    ));

    let mut pooled = pool_embedding_rows(&hidden, n_tokens, n_embd, embedding_cfg.pooling)
        .unwrap_or_else(|error| {
            eprintln!("Embedding pooling error: {error}");
            std::process::exit(1);
        });
    #[cfg(feature = "parity-trace")]
    parity_trace::report(parity_trace::checkpoint(
        "embedding.pooled",
        None,
        &[n_embd],
        &pooled,
    ));

    l2_normalize_embedding(&mut pooled).unwrap_or_else(|error| {
        eprintln!("Embedding normalization error: {error}");
        std::process::exit(1);
    });

    #[cfg(feature = "parity-trace")]
    parity_trace::report(parity_trace::checkpoint(
        "embedding.final",
        None,
        &[n_embd],
        &pooled,
    ));

    let embed_ms = t_embed.elapsed().as_millis();
    match output {
        EmbeddingOutput::Summary => {
            println!(
                "Embedding ({} dims, {} layers, {}ms):",
                n_embd, n_layer, embed_ms
            );
            for value in pooled.iter().take(8) {
                print!("{value:+.6} ");
            }
            if n_embd > 8 {
                print!("... ");
                for value in &pooled[n_embd - 4..] {
                    print!("{value:+.6} ");
                }
            }
            println!();
        }
        EmbeddingOutput::Raw => {
            print!("embedding_raw:");
            for value in &pooled {
                print!(" {value:.9}");
            }
            println!();
        }
    }
}

fn run_dump_logits(
    source: &dyn TensorSource,
    prompt: &str,
    max_tokens: usize,
    n_threads_arg: usize,
    kv_format: KvFormat,
) -> Result<(), String> {
    let config = model_config_from_source(source)
        .map_err(|error| format!("Failed to parse model config: {error}"))?;

    let mut bin_out = std::fs::File::create("/tmp/rust_logits.bin")
        .map_err(|error| format!("Failed to create /tmp/rust_logits.bin: {error}"))?;

    let arch = source
        .metadata("general.architecture")
        .and_then(|v| v.to_string_val())
        .unwrap_or_default();
    let is_qwen3 = arch == "qwen3";

    let tokenizer = BPETokenizer::from_gguf_metadata(|k| source.metadata(k).cloned())
        .map_err(|error| format!("Failed to initialize tokenizer: {error}"))?;

    let max_ctx = 512usize.min(config.n_ctx);
    let n_embd = config.n_embd;
    let n_layer = config.n_layer;
    let n_head = config.n_head;
    let n_head_kv = config.n_head_kv;
    let n_embd_head = config.n_embd_head;
    let n_embd_head_k = if let Some(v) = source.metadata(&format!("{}.attention.key_length", arch))
    {
        v.to_u64().unwrap_or(n_embd_head as u64) as usize
    } else {
        n_embd_head
    };
    let n_embd_head_v =
        if let Some(v) = source.metadata(&format!("{}.attention.value_length", arch)) {
            v.to_u64().unwrap_or(n_embd_head as u64) as usize
        } else {
            n_embd_head
        };
    let n_embd_q = n_head * n_embd_head_k;
    let n_embd_gqa = n_head_kv * n_embd_head_v;
    let n_ff = config.n_ff;
    let eps = config.norm_eps;
    let freq_base = config.rope_freq_base;

    let output_norm = get_f32_tensor(source, "output_norm.weight", n_embd);
    let embd_weight = source.tensor_slice("token_embd.weight").expect("no embd");
    let output_weight = source.tensor_slice("output.weight").unwrap_or(embd_weight);

    let layers: Vec<LayerWeights> = (0..n_layer)
        .map(|l| LayerWeights {
            attn_norm: get_f32_tensor(source, &format!("blk.{}.attn_norm.weight", l), n_embd),
            ffn_norm: get_f32_tensor(source, &format!("blk.{}.ffn_norm.weight", l), n_embd),
            q_norm: if is_qwen3 {
                Some(get_f32_tensor(
                    source,
                    &format!("blk.{}.attn_q_norm.weight", l),
                    n_embd_head_k,
                ))
            } else {
                None
            },
            k_norm: if is_qwen3 {
                Some(get_f32_tensor(
                    source,
                    &format!("blk.{}.attn_k_norm.weight", l),
                    n_embd_head_k,
                ))
            } else {
                None
            },
            wq: source
                .tensor_slice(&format!("blk.{}.attn_q.weight", l))
                .unwrap(),
            wk: source
                .tensor_slice(&format!("blk.{}.attn_k.weight", l))
                .unwrap(),
            wv: source
                .tensor_slice(&format!("blk.{}.attn_v.weight", l))
                .unwrap(),
            wo: source
                .tensor_slice(&format!("blk.{}.attn_output.weight", l))
                .unwrap(),
            w_gate: source
                .tensor_slice(&format!("blk.{}.ffn_gate.weight", l))
                .unwrap(),
            w_up: source
                .tensor_slice(&format!("blk.{}.ffn_up.weight", l))
                .unwrap(),
            w_down: source
                .tensor_slice(&format!("blk.{}.ffn_down.weight", l))
                .unwrap(),
        })
        .collect();

    eprintln!(
        "Model: {} | n_embd={} n_layer={} n_head={} n_head_kv={} n_ff={}",
        arch, n_embd, n_layer, n_head, n_head_kv, n_ff
    );

    let prompt_tokens = tokenizer.encode(
        prompt,
        EncodeOptions {
            add_special: true,
            parse_special: true,
        },
    );
    eprintln!(
        "Tokenized to {} tokens: {:?}",
        prompt_tokens.len(),
        prompt_tokens
    );

    let vocab = tokenizer.vocab_size();

    let n_threads = if n_threads_arg > 0 { n_threads_arg } else { 1 };

    {
        use std::io::Write as IoWrite;
        let header: [i32; 3] = [vocab as i32, prompt_tokens.len() as i32, max_tokens as i32];
        bin_out
            .write_all(unsafe { std::slice::from_raw_parts(header.as_ptr() as *const u8, 12) })
            .unwrap();
        let pt: Vec<i32> = prompt_tokens.iter().map(|&t| t as i32).collect();
        bin_out
            .write_all(unsafe {
                std::slice::from_raw_parts(pt.as_ptr() as *const u8, pt.len() * 4)
            })
            .unwrap();
    }

    let kv_cache = match kv_format {
        KvFormat::F16 => KvCache::new_f16(n_layer, max_ctx, n_embd_gqa),
        KvFormat::F32 => KvCache::new_f32(n_layer, max_ctx, n_embd_gqa),
    };

    let mut scratch = ExecutionScratchpad::new(
        n_embd, n_embd_q, n_embd_gqa, n_ff, vocab, n_threads, max_ctx,
    );

    let input_tokens = prompt_tokens.clone();
    #[cfg(feature = "parity-trace")]
    parity_trace::report(parity_trace::token_ids("prompt_ids", &input_tokens));
    #[cfg(feature = "parity-trace")]
    let qwen3_positions: Vec<usize> = (0..input_tokens.len()).collect();
    #[cfg(feature = "parity-trace")]
    parity_trace::report(parity_trace::usize_values(
        "qwen3.positions",
        &[qwen3_positions.len()],
        &qwen3_positions,
    ));
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
        #[cfg(feature = "parity-trace")]
        parity_trace::report(parity_trace::checkpoint(
            "model.input_embed",
            None,
            &[1, n_embd],
            &scratch.x[..n_embd],
        ));

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
            let score_stride = scratch.score_stride;
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
            #[cfg(feature = "parity-trace")]
            if layer == 0 {
                parity_trace::report(parity_trace::checkpoint(
                    "attn_norm-0",
                    Some(0),
                    &[1, n_embd],
                    normed,
                ));
            }
            quantize_q8_0_into(
                normed,
                n_embd,
                &mut q8_buf[..n_embd],
                &mut scale_buf[..n_embd / 32],
            );

            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();

            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_embd);
                let sc = raw_parts!(sc, n_embd / 32);
                let q = slice_from_mut!(q_ptr, n_embd_q);
                let k_new = slice_from_mut!(k_ptr, n_embd_gqa);
                let v_new = slice_from_mut!(v_ptr, n_embd_gqa);

                matmul_q8_0_quantized_parallel_rows(lw.wq, q8, sc, q, n_embd, n_embd_q, ith, nth);
                matmul_q8_0_quantized_parallel_rows(
                    lw.wk, q8, sc, k_new, n_embd, n_embd_gqa, ith, nth,
                );
                matmul_q8_0_quantized_parallel_rows(
                    lw.wv, q8, sc, v_new, n_embd, n_embd_gqa, ith, nth,
                );
            });

            {
                let q = slice_from_mut!(q_ptr, n_embd_q);
                let k_new = slice_from_mut!(k_ptr, n_embd_gqa);
                let v_new = slice_from_mut!(v_ptr, n_embd_gqa);
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

                #[cfg(feature = "parity-trace")]
                if layer == 0 {
                    parity_trace::report(parity_trace::checkpoint(
                        "Qcur_normed-0",
                        Some(0),
                        &[n_head, n_embd_head_k],
                        q,
                    ));
                    parity_trace::report(parity_trace::checkpoint(
                        "Kcur_normed-0",
                        Some(0),
                        &[n_head_kv, n_embd_head_k],
                        k_new,
                    ));
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
                        n_embd_head_v,
                        freq_base,
                    );
                }
                #[cfg(feature = "parity-trace")]
                if layer == 0 {
                    parity_trace::report(parity_trace::checkpoint(
                        "Qcur-0",
                        Some(0),
                        &[n_head, n_embd_head_k],
                        q,
                    ));
                    parity_trace::report(parity_trace::checkpoint(
                        "Kcur-0",
                        Some(0),
                        &[n_head_kv, n_embd_head_k],
                        k_new,
                    ));
                }

                let kb = layer * max_ctx * n_embd_gqa;

                if kv_format == KvFormat::F16 {
                    let k_cache = slice_from_mut!(k_cache_f16_ptr, kv_cache_size);
                    let v_cache = slice_from_mut!(v_cache_f16_ptr, kv_cache_size);
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
                } else {
                    let k_cache = slice_from_mut!(k_cache_f32_ptr, kv_cache_size);
                    let v_cache = slice_from_mut!(v_cache_f32_ptr, kv_cache_size);
                    for h in 0..n_head_kv {
                        let off = h * n_embd_head_k;
                        k_cache[kb + pos * n_embd_gqa + off
                            ..kb + pos * n_embd_gqa + off + n_embd_head_k]
                            .copy_from_slice(&k_new[off..off + n_embd_head_k]);
                        v_cache[kb + pos * n_embd_gqa + off
                            ..kb + pos * n_embd_gqa + off + n_embd_head_v]
                            .copy_from_slice(&v_new[off..off + n_embd_head_v]);
                    }
                }
            }

            pool.compute(move |ith: usize, nth: usize| {
                let q = slice_from_ref!(q_ptr, n_embd_q);
                let attn_out = slice_from_mut!(attn_out_ptr, n_embd_q);
                let scores = slice_from_mut!(scores_ptr, n_threads * score_stride);
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
                        let s_off = ith * score_stride;
                        for t in 0..n_cached {
                            scores[s_off + t] = dot_f16_f32(
                                &q[q_off..q_off + n_embd_head_k],
                                &k_cache[kb + t * n_embd_gqa + kv_h * n_embd_head_v
                                    ..kb + t * n_embd_gqa + kv_h * n_embd_head_v + n_embd_head_k],
                                n_embd_head_k,
                            ) * kq_scale;
                        }
                        softmax(&mut scores[s_off..s_off + n_cached]);
                        for d in 0..n_embd_head_v {
                            let mut val = 0.0f32;
                            for t in 0..n_cached {
                                val += scores[s_off + t]
                                    * f16_to_f32(
                                        v_cache[kb + t * n_embd_gqa + kv_h * n_embd_head_v + d],
                                    );
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
                        let s_off = ith * score_stride;
                        for t in 0..n_cached {
                            scores[s_off + t] = dot_f32(
                                &q[q_off..q_off + n_embd_head_k],
                                &k_cache[kb + t * n_embd_gqa + kv_h * n_embd_head_v
                                    ..kb + t * n_embd_gqa + kv_h * n_embd_head_v + n_embd_head_k],
                                n_embd_head_k,
                            ) * kq_scale;
                        }
                        softmax(&mut scores[s_off..s_off + n_cached]);
                        for d in 0..n_embd_head_v {
                            let mut val = 0.0f32;
                            for t in 0..n_cached {
                                val += scores[s_off + t]
                                    * v_cache[kb + t * n_embd_gqa + kv_h * n_embd_head_v + d];
                            }
                            attn_out[h * n_embd_head_v + d] = val;
                        }
                    }
                }
            });

            let attn_out = slice_from_mut!(attn_out_ptr, n_embd_q);
            #[cfg(feature = "parity-trace")]
            if layer == 0 {
                parity_trace::report(parity_trace::checkpoint(
                    "kqv_out-0",
                    Some(0),
                    &[n_head, n_embd_head_v],
                    attn_out,
                ));
            }
            quantize_q8_0_into(
                attn_out,
                n_embd_q,
                &mut q8_buf[..n_embd_q],
                &mut scale_buf[..n_embd_q / 32],
            );

            let q8 = q8_buf[..n_embd_q].as_ptr();
            let sc = scale_buf[..n_embd_q / 32].as_ptr();
            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_embd_q);
                let sc = raw_parts!(sc, n_embd_q / 32);
                let attn_proj = slice_from_mut!(attn_proj_ptr, n_embd);
                matmul_q8_0_quantized_parallel_rows(
                    lw.wo, q8, sc, attn_proj, n_embd_q, n_embd, ith, nth,
                );
            });

            let attn_proj = slice_from_mut!(attn_proj_ptr, n_embd);
            let x = slice_from_mut!(x_ptr, n_embd);
            let normed = slice_from_mut!(normed_ptr, n_embd);
            for i in 0..n_embd {
                x[i] += attn_proj[i];
            }

            rms_norm(x, &lw.ffn_norm, normed, eps);
            quantize_q8_0_into(
                normed,
                n_embd,
                &mut q8_buf[..n_embd],
                &mut scale_buf[..n_embd / 32],
            );
            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();

            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_embd);
                let sc = raw_parts!(sc, n_embd / 32);
                let gate_buf = slice_from_mut!(gate_buf_ptr, n_ff);
                let up_buf = slice_from_mut!(up_buf_ptr, n_ff);
                matmul_q8_0_quantized_parallel_rows(
                    lw.w_gate, q8, sc, up_buf, n_embd, n_ff, ith, nth,
                );
                matmul_q8_0_quantized_parallel_rows(
                    lw.w_up, q8, sc, gate_buf, n_embd, n_ff, ith, nth,
                );

                let rows_per = n_ff / nth;
                let r_start = ith * rows_per;
                let r_end = if ith == nth - 1 {
                    n_ff
                } else {
                    r_start + rows_per
                };
                silu_mul_inplace(&up_buf[r_start..r_end], &mut gate_buf[r_start..r_end]);
            });

            let gate_buf = slice_from_mut!(gate_buf_ptr, n_ff);
            quantize_q8_0_into(
                gate_buf,
                n_ff,
                &mut q8_buf[..n_ff],
                &mut scale_buf[..n_ff / 32],
            );

            let q8 = q8_buf[..n_ff].as_ptr();
            let sc = scale_buf[..n_ff / 32].as_ptr();
            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_ff);
                let sc = raw_parts!(sc, n_ff / 32);
                let down_buf = slice_from_mut!(down_buf_ptr, n_embd);
                matmul_q8_0_quantized_parallel_rows(
                    lw.w_down, q8, sc, down_buf, n_ff, n_embd, ith, nth,
                );
            });

            let down_buf = slice_from_mut!(down_buf_ptr, n_embd);
            let x = slice_from_mut!(x_ptr, n_embd);
            #[cfg(feature = "parity-trace")]
            if layer == 0 {
                parity_trace::report(parity_trace::checkpoint(
                    "ffn_out-0",
                    Some(0),
                    &[1, n_embd],
                    down_buf,
                ));
            }
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

            rms_norm(x, &output_norm, normed, eps);
            #[cfg(feature = "parity-trace")]
            parity_trace::report(parity_trace::checkpoint(
                "result_norm",
                None,
                &[1, n_embd],
                normed,
            ));
            quantize_q8_0_into(
                normed,
                n_embd,
                &mut q8_buf[..n_embd],
                &mut scale_buf[..n_embd / 32],
            );

            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();
            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_embd);
                let sc = raw_parts!(sc, n_embd / 32);
                let logits = slice_from_mut!(logits_ptr, vocab);
                matmul_q8_0_quantized_parallel_rows(
                    output_weight,
                    q8,
                    sc,
                    logits,
                    n_embd,
                    vocab,
                    ith,
                    nth,
                );
            });
            #[cfg(feature = "parity-trace")]
            parity_trace::report(parity_trace::checkpoint(
                "result_output",
                None,
                &[vocab],
                &scratch.logits[..vocab],
            ));
        }

        if step < input_tokens.len() - 1 {
            continue;
        }

        let logits = &scratch.logits;

        let mut best_idx = 0usize;
        let mut best_val = logits[0];
        for (i, &v) in logits.iter().enumerate().skip(1) {
            if v > best_val {
                best_val = v;
                best_idx = i;
            }
        }

        println!("=== Step {} token={} ===", step, token_id);
        println!("  argmax={} logit={:.8}", best_idx, best_val);

        let mut indexed: Vec<(usize, f32)> =
            logits.iter().enumerate().map(|(i, &v)| (i, v)).collect();
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
        println!(
            "  stats: sum={:.6} mean={:.6} std={:.6} min={:.6} max={:.6}",
            sum, mean, std, mn, mx
        );

        {
            use std::io::Write as IoWrite;
            bin_out
                .write_all(unsafe {
                    std::slice::from_raw_parts(logits.as_ptr() as *const u8, vocab * 4)
                })
                .unwrap();
        }

        let chosen = best_idx as u32;
        if generated_tokens.len() >= max_tokens {
            break;
        }
        generated_tokens.push(chosen);
        all_tokens.push(chosen);
    }
    #[cfg(feature = "parity-trace")]
    parity_trace::report(parity_trace::token_ids(
        "generated_ids",
        &generated_tokens,
    ));
    Ok(())
}

fn run_shared_inference(
    source: std::sync::Arc<dyn TensorSource>,
    prompt: &str,
    max_tokens: usize,
    temperature: f32,
    n_threads_arg: usize,
    thinking: bool,
) -> Result<(), String> {
    let started = Instant::now();
    let tokenizer = std::sync::Arc::new(
        BPETokenizer::from_gguf_metadata(|key| source.metadata(key).cloned())
            .map_err(|error| format!("Failed to initialize tokenizer: {error}"))?,
    );
    let available_threads = std::thread::available_parallelism()
        .map(|threads| threads.get())
        .unwrap_or(4);
    let pool = std::sync::Arc::new(thread_pool::ComputePool::new(resolve_thread_count(
        n_threads_arg,
        available_threads,
    )));
    let model = Qwen3Model::from_source(source, std::sync::Arc::clone(&tokenizer), pool)?;
    let input_tokens = build_qwen_chat_prompt(
        &tokenizer,
        &[QwenMessage {
            role: "user",
            content: prompt,
        }],
        thinking,
    )?;
    let positions = qwen_text_positions(input_tokens.len());
    println!(
        "Model: {} | n_embd={} n_layer={} n_head={} n_head_kv={} n_ff={} | loaded in {}ms",
        model.config().architecture,
        model.config().n_embd,
        model.config().n_layer,
        model.config().n_head,
        model.config().n_head_kv,
        model.config().n_ff,
        started.elapsed().as_millis(),
    );
    eprintln!("compute pool: {} threads", model.pool().n_threads());
    println!("Prompt: {} ({} tokens)", prompt, input_tokens.len());
    print!("Output: ");
    io::stdout().flush().map_err(|error| error.to_string())?;
    let inference_started = Instant::now();
    let generation = model.generate(
        Qwen3Input {
            token_ids: &input_tokens,
            positions: &positions,
            embeddings: None,
        },
        Qwen3GenerateOptions {
            max_new_tokens: max_tokens,
            temperature,
        },
    )?;
    print!("{}", generation.text);
    io::stdout().flush().map_err(|error| error.to_string())?;
    let elapsed_ms = inference_started.elapsed().as_millis();
    let tokens_per_second = if elapsed_ms > 0 {
        generation.token_ids.len() as f64 / elapsed_ms as f64 * 1000.0
    } else {
        0.0
    };
    println!();
    println!(
        "[end-to-end: {} output tokens in {}ms | {:.1} tok/s]",
        generation.token_ids.len(),
        elapsed_ms,
        tokens_per_second,
    );
    Ok(())
}

fn run_inference(
    source: &dyn TensorSource,
    prompt: &str,
    max_tokens: usize,
    temperature: f32,
    n_threads_arg: usize,
    thinking: bool,
    bench: bool,
    profile: bool,
    kv_format: KvFormat,
) -> Result<(), String> {
    let t0 = Instant::now();
    let config = model_config_from_source(source)
        .map_err(|error| format!("Failed to parse model config: {error}"))?;

    let arch = source
        .metadata("general.architecture")
        .and_then(|v| v.to_string_val())
        .unwrap_or_default();
    let is_qwen3 = arch == "qwen3";

    let tokenizer = BPETokenizer::from_gguf_metadata(|k| source.metadata(k).cloned())
        .map_err(|error| format!("Failed to initialize tokenizer: {error}"))?;

    let max_ctx = 512usize.min(config.n_ctx);
    let n_embd = config.n_embd;
    let n_layer = config.n_layer;
    let n_head = config.n_head;
    let n_head_kv = config.n_head_kv;
    let n_embd_head = config.n_embd_head;
    let n_embd_head_k = if let Some(v) = source.metadata(&format!("{}.attention.key_length", arch))
    {
        v.to_u64().unwrap_or(n_embd_head as u64) as usize
    } else {
        n_embd_head
    };
    let n_embd_head_v =
        if let Some(v) = source.metadata(&format!("{}.attention.value_length", arch)) {
            v.to_u64().unwrap_or(n_embd_head as u64) as usize
        } else {
            n_embd_head
        };
    let n_embd_q = n_head * n_embd_head_k;
    let n_embd_gqa = n_head_kv * n_embd_head_v;
    let n_ff = config.n_ff;
    let eps = config.norm_eps;
    let freq_base = config.rope_freq_base;

    let output_norm = get_f32_tensor(source, "output_norm.weight", n_embd);
    let embd_weight = source.tensor_slice("token_embd.weight").expect("no embd");
    let output_weight = source.tensor_slice("output.weight").unwrap_or(embd_weight);

    let layers: Vec<LayerWeights> = (0..n_layer)
        .map(|l| LayerWeights {
            attn_norm: get_f32_tensor(source, &format!("blk.{}.attn_norm.weight", l), n_embd),
            ffn_norm: get_f32_tensor(source, &format!("blk.{}.ffn_norm.weight", l), n_embd),
            q_norm: if is_qwen3 {
                Some(get_f32_tensor(
                    source,
                    &format!("blk.{}.attn_q_norm.weight", l),
                    n_embd_head_k,
                ))
            } else {
                None
            },
            k_norm: if is_qwen3 {
                Some(get_f32_tensor(
                    source,
                    &format!("blk.{}.attn_k_norm.weight", l),
                    n_embd_head_k,
                ))
            } else {
                None
            },
            wq: source
                .tensor_slice(&format!("blk.{}.attn_q.weight", l))
                .unwrap(),
            wk: source
                .tensor_slice(&format!("blk.{}.attn_k.weight", l))
                .unwrap(),
            wv: source
                .tensor_slice(&format!("blk.{}.attn_v.weight", l))
                .unwrap(),
            wo: source
                .tensor_slice(&format!("blk.{}.attn_output.weight", l))
                .unwrap(),
            w_gate: source
                .tensor_slice(&format!("blk.{}.ffn_gate.weight", l))
                .unwrap(),
            w_up: source
                .tensor_slice(&format!("blk.{}.ffn_up.weight", l))
                .unwrap(),
            w_down: source
                .tensor_slice(&format!("blk.{}.ffn_down.weight", l))
                .unwrap(),
        })
        .collect();

    let load_ms = t0.elapsed().as_millis();
    println!(
        "Model: {} | n_embd={} n_layer={} n_head={} n_head_kv={} n_ff={} | loaded in {}ms",
        arch, n_embd, n_layer, n_head, n_head_kv, n_ff, load_ms
    );

    let kv_cache = match kv_format {
        KvFormat::F16 => KvCache::new_f16(n_layer, max_ctx, n_embd_gqa),
        KvFormat::F32 => KvCache::new_f32(n_layer, max_ctx, n_embd_gqa),
    };

    let vocab = tokenizer.vocab_size();
    let input_tokens = if bench {
        tokenizer.encode(
            prompt,
            EncodeOptions {
                add_special: true,
                parse_special: true,
            },
        )
    } else {
        build_qwen_chat_prompt(
            &tokenizer,
            &[QwenMessage {
                role: "user",
                content: prompt,
            }],
            thinking,
        )?
    };
    #[cfg(feature = "parity-trace")]
    parity_trace::report(parity_trace::token_ids("prompt_ids", &input_tokens));
    #[cfg(feature = "parity-trace")]
    let qwen3_positions: Vec<usize> = (0..input_tokens.len()).collect();
    #[cfg(feature = "parity-trace")]
    parity_trace::report(parity_trace::usize_values(
        "qwen3.positions",
        &[qwen3_positions.len()],
        &qwen3_positions,
    ));
    let available_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let n_threads = resolve_thread_count(n_threads_arg, available_threads);

    let mut scratch = ExecutionScratchpad::new(
        n_embd, n_embd_q, n_embd_gqa, n_ff, vocab, n_threads, max_ctx,
    );
    let pool = std::sync::Arc::new(thread_pool::ComputePool::new(n_threads));
    eprintln!("compute pool: {} threads", pool.n_threads());
    println!("Prompt: {} ({} tokens)", prompt, input_tokens.len());

    let eos_id = tokenizer.eos_id();
    let im_end_id = tokenizer.special_token_id("im_end");
    let mut generated_tokens: Vec<u32> = Vec::new();
    let mut all_tokens: Vec<u32> = input_tokens.clone();
    let mut decoder = tokenizer.streaming_decoder(false);

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
        #[cfg(feature = "parity-trace")]
        parity_trace::report(parity_trace::checkpoint(
            "model.input_embed",
            None,
            &[1, n_embd],
            &scratch.x[..n_embd],
        ));

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
            let score_stride = scratch.score_stride;
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
            #[cfg(feature = "parity-trace")]
            if layer == 0 {
                parity_trace::report(parity_trace::checkpoint(
                    "attn_norm-0",
                    Some(0),
                    &[1, n_embd],
                    normed,
                ));
            }
            quantize_q8_0_into(
                normed,
                n_embd,
                &mut q8_buf[..n_embd],
                &mut scale_buf[..n_embd / 32],
            );
            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();

            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_embd);
                let sc = raw_parts!(sc, n_embd / 32);
                let q = slice_from_mut!(q_ptr, n_embd_q);
                let k_new = slice_from_mut!(k_ptr, n_embd_gqa);
                let v_new = slice_from_mut!(v_ptr, n_embd_gqa);

                matmul_q8_0_quantized_parallel_rows(lw.wq, q8, sc, q, n_embd, n_embd_q, ith, nth);
                matmul_q8_0_quantized_parallel_rows(
                    lw.wk, q8, sc, k_new, n_embd, n_embd_gqa, ith, nth,
                );
                matmul_q8_0_quantized_parallel_rows(
                    lw.wv, q8, sc, v_new, n_embd, n_embd_gqa, ith, nth,
                );
            });

            {
                let q = slice_from_mut!(q_ptr, n_embd_q);
                let k_new = slice_from_mut!(k_ptr, n_embd_gqa);
                let v_new = slice_from_mut!(v_ptr, n_embd_gqa);
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

                #[cfg(feature = "parity-trace")]
                if layer == 0 {
                    parity_trace::report(parity_trace::checkpoint(
                        "Qcur_normed-0",
                        Some(0),
                        &[n_head, n_embd_head_k],
                        q,
                    ));
                    parity_trace::report(parity_trace::checkpoint(
                        "Kcur_normed-0",
                        Some(0),
                        &[n_head_kv, n_embd_head_k],
                        k_new,
                    ));
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
                        n_embd_head_v,
                        freq_base,
                    );
                }
                #[cfg(feature = "parity-trace")]
                if layer == 0 {
                    parity_trace::report(parity_trace::checkpoint(
                        "Qcur-0",
                        Some(0),
                        &[n_head, n_embd_head_k],
                        q,
                    ));
                    parity_trace::report(parity_trace::checkpoint(
                        "Kcur-0",
                        Some(0),
                        &[n_head_kv, n_embd_head_k],
                        k_new,
                    ));
                }

                let kb = layer * max_ctx * n_embd_gqa;

                if kv_format == KvFormat::F16 {
                    let k_cache = slice_from_mut!(k_cache_f16_ptr, kv_cache_size);
                    let v_cache = slice_from_mut!(v_cache_f16_ptr, kv_cache_size);
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
                } else {
                    let k_cache = slice_from_mut!(k_cache_f32_ptr, kv_cache_size);
                    let v_cache = slice_from_mut!(v_cache_f32_ptr, kv_cache_size);
                    for h in 0..n_head_kv {
                        let off = h * n_embd_head_k;
                        k_cache[kb + pos * n_embd_gqa + off
                            ..kb + pos * n_embd_gqa + off + n_embd_head_k]
                            .copy_from_slice(&k_new[off..off + n_embd_head_k]);
                        v_cache[kb + pos * n_embd_gqa + off
                            ..kb + pos * n_embd_gqa + off + n_embd_head_v]
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
                } else {
                    let k_cache = slice_from_ref!(k_cache_f32_ptr, kv_cache_size);
                    let v_cache = slice_from_ref!(v_cache_f32_ptr, kv_cache_size);
                    let scores = slice_from_mut!(scores_ptr, n_threads * score_stride);
                    for h in h_start..h_end {
                        let kv_h = h / group_size;
                        let q_off = h * n_embd_head_k;
                        let n_cached = pos + 1;
                        let n_padded = (n_cached + 255) / 256 * 256;
                        let out_base = h * n_embd_head_v;
                        let s_off = ith * score_stride;
                        for t in 0..n_cached {
                            scores[s_off + t] = dot_f32(
                                &q[q_off..q_off + n_embd_head_k],
                                &k_cache[kb + t * n_embd_gqa + kv_h * n_embd_head_v
                                    ..kb + t * n_embd_gqa + kv_h * n_embd_head_v + n_embd_head_k],
                                n_embd_head_k,
                            ) * kq_scale;
                        }
                        scores[s_off + n_cached..s_off + n_padded].fill(f32::NEG_INFINITY);
                        softmax(&mut scores[s_off..s_off + n_padded]);
                        let mut values = [0.0f32; 512];
                        for d in 0..n_embd_head_v {
                            for t in 0..n_cached {
                                values[t] = v_cache[
                                    kb + t * n_embd_gqa + kv_h * n_embd_head_v + d
                                ];
                            }
                            attn_out[out_base + d] = attention_value_f32(
                                &values[..n_padded],
                                &scores[s_off..s_off + n_padded],
                                n_cached,
                                n_padded,
                            );
                        }
                    }
                }
            });
            t_qkv += t0.elapsed().as_secs_f64();

            let attn_out = slice_from_mut!(attn_out_ptr, n_embd_q);
            #[cfg(feature = "parity-trace")]
            if layer == 0 {
                parity_trace::report(parity_trace::checkpoint(
                    "kqv_out-0",
                    Some(0),
                    &[n_head, n_embd_head_v],
                    attn_out,
                ));
            }
            let q8_buf = slice_from_mut!(q8_buf_ptr, max_n_in);
            let scale_buf = slice_from_mut!(scale_buf_ptr, max_n_in / 32);
            let t0 = Instant::now();
            quantize_q8_0_into(
                attn_out,
                n_embd_q,
                &mut q8_buf[..n_embd_q],
                &mut scale_buf[..n_embd_q / 32],
            );
            let q8 = q8_buf[..n_embd_q].as_ptr();
            let sc = scale_buf[..n_embd_q / 32].as_ptr();
            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_embd_q);
                let sc = raw_parts!(sc, n_embd_q / 32);
                let attn_proj = slice_from_mut!(attn_proj_ptr, n_embd);
                matmul_q8_0_quantized_parallel_rows(
                    lw.wo, q8, sc, attn_proj, n_embd_q, n_embd, ith, nth,
                );
            });
            t_wo += t0.elapsed().as_secs_f64();

            let attn_proj = slice_from_mut!(attn_proj_ptr, n_embd);
            let x = slice_from_mut!(x_ptr, n_embd);
            let normed = slice_from_mut!(normed_ptr, n_embd);
            for i in 0..n_embd {
                x[i] += attn_proj[i];
            }

            let t0 = Instant::now();
            rms_norm(x, &lw.ffn_norm, normed, eps);
            quantize_q8_0_into(
                normed,
                n_embd,
                &mut q8_buf[..n_embd],
                &mut scale_buf[..n_embd / 32],
            );
            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();

            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_embd);
                let sc = raw_parts!(sc, n_embd / 32);
                let gate_buf = slice_from_mut!(gate_buf_ptr, n_ff);
                let up_buf = slice_from_mut!(up_buf_ptr, n_ff);
                matmul_q8_0_quantized_parallel_rows(
                    lw.w_gate, q8, sc, up_buf, n_embd, n_ff, ith, nth,
                );
                matmul_q8_0_quantized_parallel_rows(
                    lw.w_up, q8, sc, gate_buf, n_embd, n_ff, ith, nth,
                );

                let rows_per = n_ff / nth;
                let r_start = ith * rows_per;
                let r_end = if ith == nth - 1 {
                    n_ff
                } else {
                    r_start + rows_per
                };
                silu_mul_inplace(&up_buf[r_start..r_end], &mut gate_buf[r_start..r_end]);
            });

            {
                let gate_buf = slice_from_mut!(gate_buf_ptr, n_ff);
                let q8_buf = slice_from_mut!(q8_buf_ptr, max_n_in);
                let scale_buf = slice_from_mut!(scale_buf_ptr, max_n_in / 32);
                quantize_q8_0_into(
                    gate_buf,
                    n_ff,
                    &mut q8_buf[..n_ff],
                    &mut scale_buf[..n_ff / 32],
                );
            }

            let q8 = q8_buf[..n_ff].as_ptr();
            let sc = scale_buf[..n_ff / 32].as_ptr();
            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_ff);
                let sc = raw_parts!(sc, n_ff / 32);
                let down_buf = slice_from_mut!(down_buf_ptr, n_embd);
                matmul_q8_0_quantized_parallel_rows(
                    lw.w_down, q8, sc, down_buf, n_ff, n_embd, ith, nth,
                );
            });
            t_ffn1 += t0.elapsed().as_secs_f64();

            let down_buf = slice_from_mut!(down_buf_ptr, n_embd);
            let x = slice_from_mut!(x_ptr, n_embd);
            #[cfg(feature = "parity-trace")]
            if layer == 0 {
                parity_trace::report(parity_trace::checkpoint(
                    "ffn_out-0",
                    Some(0),
                    &[1, n_embd],
                    down_buf,
                ));
            }
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

            let t0 = Instant::now();
            rms_norm(x, &output_norm, normed, eps);
            t_norm += t0.elapsed().as_secs_f64();
            #[cfg(feature = "parity-trace")]
            parity_trace::report(parity_trace::checkpoint(
                "result_norm",
                None,
                &[1, n_embd],
                normed,
            ));

            let t0 = Instant::now();
            quantize_q8_0_into(
                normed,
                n_embd,
                &mut q8_buf[..n_embd],
                &mut scale_buf[..n_embd / 32],
            );
            let q8 = q8_buf[..n_embd].as_ptr();
            let sc = scale_buf[..n_embd / 32].as_ptr();
            pool.compute(move |ith: usize, nth: usize| {
                let q8 = raw_parts!(q8, n_embd);
                let sc = raw_parts!(sc, n_embd / 32);
                let logits = slice_from_mut!(logits_ptr, vocab);
                matmul_q8_0_quantized_parallel_rows(
                    output_weight,
                    q8,
                    sc,
                    logits,
                    n_embd,
                    vocab,
                    ith,
                    nth,
                );
            });
            t_logits += t0.elapsed().as_secs_f64();
            #[cfg(feature = "parity-trace")]
            parity_trace::report(parity_trace::checkpoint(
                "result_output",
                None,
                &[vocab],
                &scratch.logits[..vocab],
            ));
        }

        let eval_elapsed = eval_started.elapsed();
        if step < input_tokens.len() {
            prefill_evals += 1;
            prefill_time += eval_elapsed;
        } else {
            decode_evals += 1;
            decode_time += eval_elapsed;
        }

        if step < input_tokens.len() - 1 {
            continue;
        }

        let logits = &mut scratch.logits;
        let chosen = if temperature <= 0.0 {
            logits
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                .map(|(i, _)| i)
                .unwrap_or(0)
        } else {
            for l in logits.iter_mut() {
                *l /= temperature;
            }
            let top = sample_top_k(logits, 40);
            let mut rng = 0u64;
            for &t in &all_tokens {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(t as u64);
            }
            let r = ((rng >> 33) as f32) / (1u64 << 31) as f32;
            let mut cum = 0.0f32;
            let mut chosen = top[0].0;
            for &(idx, prob) in &top {
                cum += prob;
                if cum >= r {
                    chosen = idx;
                    break;
                }
            }
            chosen
        };

        let chosen_id = chosen as u32;
        if !bench && (eos_id == Some(chosen_id) || im_end_id == Some(chosen_id)) {
            break;
        }
        if generated_tokens.len() >= max_tokens {
            break;
        }

        generated_tokens.push(chosen_id);
        all_tokens.push(chosen_id);

        let text = decoder.push(chosen_id);
        print!("{}", text);
        io::stdout().flush().unwrap();

        if generated_tokens.len() == 1 {
            eprintln!();
        }
    }
    #[cfg(feature = "parity-trace")]
    parity_trace::report(parity_trace::token_ids(
        "generated_ids",
        &generated_tokens,
    ));

    let tail = decoder.finish();
    if !tail.is_empty() {
        print!("{}", tail);
        io::stdout().flush().unwrap();
    }

    let infer_ms = t_infer.elapsed().as_millis();
    let tok_s = if infer_ms > 0 {
        generated_tokens.len() as f64 / infer_ms as f64 * 1000.0
    } else {
        0.0
    };
    let total = t_norm + _t_quant + t_qkv + t_wo + t_ffn1 + t_logits;
    if bench || profile {
        eprintln!();
    }
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
        eprintln!(
            "PROFILE: norm={:.1}% quant={:.1}% qkv+attn={:.1}% wo={:.1}% ffn={:.1}% logits={:.1}%",
            t_norm / total * 100.0,
            _t_quant / total * 100.0,
            t_qkv / total * 100.0,
            t_wo / total * 100.0,
            t_ffn1 / total * 100.0,
            t_logits / total * 100.0
        );
        eprintln!(
            "PROFILE: norm={:.3}s quant={:.3}s qkv+attn={:.3}s wo={:.3}s ffn={:.3}s logits={:.3}s",
            t_norm, _t_quant, t_qkv, t_wo, t_ffn1, t_logits
        );
    }
    println!();
    println!(
        "[end-to-end: {} output tokens in {}ms | {:.1} tok/s]",
        generated_tokens.len(),
        infer_ms,
        tok_s
    );
    Ok(())
}

fn get_f32_tensor<S: TensorSource + ?Sized>(
    source: &S,
    name: &str,
    expected_len: usize,
) -> Vec<f32> {
    let info = source
        .tensor_info(name)
        .unwrap_or_else(|| panic!("tensor {name} not found"));
    let bytes = source
        .tensor_slice(name)
        .unwrap_or_else(|| panic!("slice {name} not found"));
    let mut output = vec![0.0; expected_len];
    if info.ggml_type == GGMLType::F32 {
        for (value, chunk) in output.iter_mut().zip(bytes.chunks_exact(4)) {
            *value = f32::from_le_bytes(chunk.try_into().unwrap());
        }
    }
    output
}

fn run_interactive(
    source: &dyn TensorSource,
    max_tokens: usize,
    temperature: f32,
    n_threads_arg: usize,
) -> Result<(), String> {
    println!("=== RustModelInference Interactive Mode ===");
    println!("Type your prompt and press Enter. Ctrl+C to exit.\n");

    loop {
        print!("> ");
        io::stdout()
            .flush()
            .map_err(|error| format!("Failed to flush prompt: {error}"))?;
        let mut line = String::new();
        if io::stdin()
            .read_line(&mut line)
            .map_err(|error| format!("Failed to read prompt: {error}"))?
            == 0
        {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        run_inference(
            source,
            line,
            max_tokens,
            temperature,
            n_threads_arg,
            false,
            false,
            false,
            KvFormat::F16,
        )?;
        println!();
    }
    Ok(())
}

fn run_self_test() {
    println!("=== RustModelInference MVP Self-Test ===\n");
    let config = ModelConfig::qwen2_0_6b();
    println!(
        "[Config] Qwen2-0.6B: n_embd={}, n_layer={}, n_head={}, n_ff={}",
        config.n_embd, config.n_layer, config.n_head, config.n_ff
    );

    let mut alloc = BlockAllocator::new(64);
    let b0 = alloc.alloc().unwrap();
    let b1 = alloc.alloc().unwrap();
    alloc.free(b1);
    let b3 = alloc.alloc().unwrap();
    println!(
        "BlockAllocator: alloc {},{}, free {}, re-alloc {} [OK]",
        b0, b1, b1, b3
    );

    let mut arena = MemoryArena::new(1024, 1024);
    let ptr = arena.scratch_slice().as_ptr() as usize;
    arena.scratch_slice()[0] = 42.0;
    assert_eq!(arena.scratch_slice().as_ptr() as usize, ptr);
    println!("MemoryArena: ptr stable [OK]");

    println!("\nUsage: cargo run -- --model <path.gguf> --prompt \"hello\"");
    println!("       cargo run -- --model <path.gguf>  (interactive mode)");
    println!("       cargo run -- --model <llm.gguf> --mmproj <mmproj.gguf> --image <image.png> --prompt \"describe\"");
    println!(
        "       cargo run --release --bin rust-model-inference -- --model models/qwen3-asr-0.6b/Qwen3-ASR-0.6B-Q8_0.gguf --mmproj models/qwen3-asr-0.6b/mmproj-Qwen3-ASR-0.6B-Q8_0.gguf --audio sample.wav --language English --max-tokens 256 --threads 8"
    );
}

fn inject_vision_embeddings(
    llm: &Qwen35Model,
    tokens: &[i32],
    image_token_id: Option<i32>,
    vis_embd: &[f32],
    n_vis_tokens: usize,
    proj_dim: usize,
) -> Vec<f32> {
    let n_embd = llm.config.n_embd;
    let n_tokens = tokens.len();
    let mut embeddings = vec![0.0f32; n_tokens * n_embd];

    let mut vis_idx = 0;

    for t in 0..n_tokens {
        if image_token_id == Some(tokens[t]) && vis_idx < n_vis_tokens {
            let embd_off = t * n_embd;
            let vis_off = vis_idx * proj_dim;
            if proj_dim == n_embd {
                embeddings[embd_off..embd_off + n_embd]
                    .copy_from_slice(&vis_embd[vis_off..vis_off + n_embd]);
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

    let r = 0.5f32;
    let mut cumsum = 0.0f32;
    for (i, p) in probs.iter().enumerate() {
        cumsum += p;
        if cumsum >= r {
            return i as i32;
        }
    }
    (logits.len() - 1) as i32
}

fn decode_image(path: &Path) -> Result<image::DynamicImage, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("Failed to read image {}: {error}", path.display()))?;
    image::load_from_memory(&bytes)
        .map_err(|error| format!("Failed to decode image {}: {error}", path.display()))
}

fn normalize_resized_image(
    image: &image::DynamicImage,
    target_w: usize,
    target_h: usize,
    mean: &[f32; 3],
    std: &[f32; 3],
) -> Result<Vec<f32>, String> {
    if std.iter().any(|value| *value == 0.0) {
        return Err("Vision normalization std must be nonzero".into());
    }
    let width = u32::try_from(target_w).map_err(|_| "Vision width exceeds u32")?;
    let height = u32::try_from(target_h).map_err(|_| "Vision height exceeds u32")?;
    let resized = image
        .resize_exact(width, height, image::imageops::FilterType::Lanczos3)
        .to_rgb8();
    let output_len = target_w
        .checked_mul(target_h)
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or("Normalized image length overflow")?;
    let mut output = vec![0.0f32; output_len];
    for y in 0..target_h {
        for x in 0..target_w {
            let pixel = resized.get_pixel(x as u32, y as u32);
            let offset = (y * target_w + x) * 3;
            for channel in 0..3 {
                output[offset + channel] =
                    (f32::from(pixel[channel]) / 255.0 - mean[channel]) / std[channel];
            }
        }
    }
    Ok(output)
}

fn run_multimodal(
    llm_source: &dyn TensorSource,
    model_path: &Path,
    mmproj_path: Option<&Path>,
    image_path: Option<&Path>,
    prompt: &str,
    max_tokens: usize,
    temperature: f32,
    n_threads_arg: usize,
) -> Result<(), String> {
    let arch = llm_source
        .metadata("general.architecture")
        .and_then(|v| v.to_string_val())
        .unwrap_or_default();
    println!("LLM arch: {}", arch);
    if arch != "qwen35" {
        return Err(format!(
            "Only qwen35 architecture is supported for multimodal, got: {arch}"
        ));
    }

    #[cfg(feature = "parity-trace")]
    let mut vision_original_size = None;
    let (image_grid, vis_embeddings_vec) = if let Some(image_path) = image_path {
        let projector_path = mmproj_path.unwrap_or(model_path);
        println!("Loading mmproj {} ...", projector_path.display());
        let mmproj_source =
            open_model_source(projector_path, ComponentRole::Mmproj).map_err(|error| {
                if mmproj_path.is_none() {
                    format!(
                        "Model {} has no bundled mmproj; pass --mmproj: {error}",
                        model_path.display()
                    )
                } else {
                    format!(
                        "Failed to load mmproj {}: {error}",
                        projector_path.display()
                    )
                }
            })?;
        let mut encoder = VisionEncoder::from_source(mmproj_source.as_ref())
            .map_err(|error| format!("Failed to parse vision encoder: {error}"))?;
        encoder.precompute();
        println!(
            "Vision encoder loaded: {} layers, n_embd={}, image_size={}, patch_size={}, merge={}",
            encoder.config.n_layer,
            encoder.config.n_embd,
            encoder.config.image_size,
            encoder.config.patch_size,
            encoder.config.spatial_merge_size
        );
        let image = decode_image(image_path)?;
        let original_w = usize::try_from(image.width())
            .map_err(|_| "Original image width does not fit usize")?;
        let original_h = usize::try_from(image.height())
            .map_err(|_| "Original image height does not fit usize")?;
        #[cfg(feature = "parity-trace")]
        {
            vision_original_size = Some([original_w, original_h]);
        }
        let grid = qwen_smart_resize(original_w, original_h, &encoder.config)?;
        let pixels = normalize_resized_image(
            &image,
            grid.image_width(),
            grid.image_height(),
            &encoder.config.image_mean,
            &encoder.config.image_std,
        )?;
        println!(
            "Image resized to {}x{} ({} vision tokens)",
            grid.image_width(),
            grid.image_height(),
            grid.token_count()
        );
        let projection_dim = encoder.config.projection_dim;
        let mut scratch = VisionScratchpad::new(&encoder.config);
        println!("Encoding image...");
        let encoded_grid = encoder.encode_image(
            &pixels,
            grid.image_width(),
            grid.image_height(),
            &mut scratch,
        )?;
        if encoded_grid != grid {
            return Err(format!(
                "Vision grid mismatch: preprocess={grid:?}, encoder={encoded_grid:?}"
            ));
        }
        let projected_len = grid
            .token_count()
            .checked_mul(projection_dim)
            .ok_or("Projected vision length overflow")?;
        if scratch.projected.len() != projected_len {
            return Err(format!(
                "Projected vision length mismatch: expected {projected_len}, got {}",
                scratch.projected.len()
            ));
        }
        println!(
            "Vision tokens: {} (dim={})",
            grid.token_count(),
            projection_dim
        );
        (Some(grid), scratch.projected[..projected_len].to_vec())
    } else {
        (None, Vec::new())
    };
    let n_vis_tokens = image_grid.map(VisionGrid::token_count).unwrap_or(0);
    let vis_embeddings = &vis_embeddings_vec[..];
    if image_grid.is_some() {
        println!(
            "First 5 vision embedding values: {:?}",
            &vis_embeddings[..5.min(vis_embeddings.len())]
        );
    }

    let llm = Qwen35Model::from_source(llm_source)
        .map_err(|error| format!("Failed to parse Qwen3.5 model: {error}"))?;
    println!("Qwen3.5 model loaded: {} layers, n_embd={}, n_head={}, n_ff={}, rope_freq_base={}, rope_sections={:?}, rope_dim_count={}", llm.config.n_layer, llm.config.n_embd, llm.config.n_head, llm.config.n_ff, llm.config.rope_freq_base, llm.config.rope_dimension_sections, llm.config.rope_dimension_count);
    // llm.precompute_f32();

    let tokenizer = BPETokenizer::from_gguf_metadata(|k| llm_source.metadata(k).cloned())
        .map_err(|error| format!("Failed to initialize tokenizer: {error}"))?;
    let image_token_id = if image_grid.is_some() {
        Some(
            tokenizer
                .special_token_id("image_pad")
                .ok_or("Required token missing: <|image_pad|>")?,
        )
    } else {
        None
    };

    let mut content_tokens = Vec::new();
    if let Some(image_token_id) = image_token_id {
        content_tokens.push(
            tokenizer
                .special_token_id("vision_start")
                .ok_or("Required token missing: <|vision_start|>")?,
        );
        content_tokens.extend(std::iter::repeat(image_token_id).take(n_vis_tokens));
        content_tokens.push(
            tokenizer
                .special_token_id("vision_end")
                .ok_or("Required token missing: <|vision_end|>")?,
        );
    }
    content_tokens.extend(tokenizer.encode(
        prompt,
        EncodeOptions {
            add_special: false,
            parse_special: false,
        },
    ));

    let mut prompt_ids = Vec::new();
    append_qwen_message_tokens(&mut prompt_ids, &tokenizer, "user", &content_tokens)?;
    append_qwen_assistant_prefix(&mut prompt_ids, &tokenizer, false)?;
    let image_grids: Vec<VisionGrid> = image_grid.iter().copied().collect();
    let (prompt_positions, mut next_text_position) =
        build_qwen35_positions(&prompt_ids, image_token_id, &image_grids)?;
    let prompt_tokens: Vec<i32> = prompt_ids
        .iter()
        .copied()
        .map(|id| i32::try_from(id).map_err(|_| format!("Token ID {id} exceeds i32")))
        .collect::<Result<_, _>>()?;

    let projected_count = if vis_embeddings.is_empty() {
        0
    } else {
        let projection_dim = llm.config.n_embd;
        if vis_embeddings.len() % projection_dim != 0 {
            return Err("Projected vision embeddings are not row aligned".into());
        }
        vis_embeddings.len() / projection_dim
    };
    if projected_count != n_vis_tokens || prompt_positions.len() != prompt_tokens.len() {
        return Err(format!(
            "Vision/position count mismatch: placeholders={n_vis_tokens}, projected={projected_count}, positions={}, tokens={}",
            prompt_positions.len(),
            prompt_tokens.len()
        ));
    }
    #[cfg(feature = "parity-trace")]
    if let (Some(grid), Some([original_w, original_h])) = (image_grid, vision_original_size) {
        let projection_dim = llm.config.n_embd;
        parity_trace::report(parity_trace::usize_values(
            "vision.original_size",
            &[2],
            &[original_w, original_h],
        ));
        parity_trace::report(parity_trace::usize_values(
            "vision.grid",
            &[5],
            &[
                grid.grid_t,
                grid.grid_h,
                grid.grid_w,
                grid.patch_size,
                grid.merge_size,
            ],
        ));
        parity_trace::report(parity_trace::usize_values(
            "vision.token_counts",
            &[3],
            &[n_vis_tokens, projected_count, grid.token_count()],
        ));
        parity_trace::report(parity_trace::checkpoint(
            "vision.first_projected_embedding",
            None,
            &[projection_dim],
            &vis_embeddings[..projection_dim],
        ));
    }
    #[cfg(feature = "parity-trace")]
    parity_trace::report(parity_trace::token_ids("prompt_ids", &prompt_ids));
    #[cfg(feature = "parity-trace")]
    let qwen35_positions_flat: Vec<usize> = prompt_positions
        .iter()
        .flat_map(|position| position.iter().copied())
        .collect();
    #[cfg(feature = "parity-trace")]
    parity_trace::report(parity_trace::usize_values(
        "qwen35.positions",
        &[prompt_positions.len(), 4],
        &qwen35_positions_flat,
    ));
    #[cfg(feature = "parity-trace")]
    parity_trace::report(parity_trace::bool_values(
        "qwen35.is_recurrent",
        &llm.config.is_recurrent,
    ));
    let image_token_id = image_token_id
        .map(|id| i32::try_from(id).map_err(|_| format!("Token ID {id} exceeds i32")))
        .transpose()?;

    println!(
        "Prompt tokens: {} (including {} vision placeholders)",
        prompt_tokens.len(),
        n_vis_tokens
    );

    let max_seq = llm.config.n_ctx;
    let mut kv_cache = crate::scratchpad::KvCache::new_f32(
        llm.config.n_layer,
        max_seq,
        llm.config.n_embd_head() * llm.config.n_head_kv,
    );
    let mut llm_scratch =
        crate::qwen35::Qwen35Scratchpad::new(&llm.config, prompt_tokens.len().max(max_tokens));

    let prompt_embd = inject_vision_embeddings(
        &llm,
        &prompt_tokens,
        image_token_id,
        vis_embeddings,
        n_vis_tokens,
        llm.config.n_embd,
    );

    let n_prompt = prompt_tokens.len();
    let mut all_tokens = prompt_tokens.clone();

    let n_threads = if n_threads_arg > 0 { n_threads_arg } else { 8 };
    let pool = std::sync::Arc::new(crate::thread_pool::ComputePool::new(n_threads));
    eprintln!("compute pool: {} threads", pool.n_threads());

    let mut generated = String::new();
    let mut decoder = tokenizer.streaming_decoder(false);
    println!("\n--- Generation ---");
    let t_gen_start = std::time::Instant::now();

    for step in 0..max_tokens {
        let tokens = if step == 0 {
            &prompt_tokens
        } else {
            &all_tokens[all_tokens.len() - 1..all_tokens.len() - 1 + 1]
        };
        let n_tok = tokens.len();

        if step == 0 {
            for t in 0..n_prompt {
                let embd_off = t * llm.config.n_embd;
                llm_scratch.x[embd_off..embd_off + llm.config.n_embd]
                    .copy_from_slice(&prompt_embd[embd_off..embd_off + llm.config.n_embd]);
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
        let logits = llm.forward(n_tok, &mut kv_cache, &mut llm_scratch, &pool, positions)?;
        if step > 0 {
            next_text_position = next_text_position
                .checked_add(1)
                .ok_or("Qwen3.5 decode position overflow")?;
        }

        let next_token = if temperature <= 0.0 {
            logits
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                .map(|(i, _)| i as i32)
                .unwrap_or(0)
        } else {
            sample_token(&logits, temperature)
        };

        if next_token >= 0
            && (tokenizer.eos_id() == Some(next_token as u32)
                || tokenizer.special_token_id("im_end") == Some(next_token as u32))
        {
            break;
        }

        let token_str = decoder.push(next_token as u32);
        generated.push_str(&token_str);
        print!("{}", token_str);
        std::io::Write::flush(&mut std::io::stdout()).ok();

        all_tokens.push(next_token);
    }

    let tail = decoder.finish();
    generated.push_str(&tail);
    if !tail.is_empty() {
        print!("{}", tail);
        std::io::Write::flush(&mut std::io::stdout()).ok();
    }
    #[cfg(feature = "parity-trace")]
    let generated_tokens = &all_tokens[n_prompt..];
    #[cfg(feature = "parity-trace")]
    let generated_ids: Vec<u32> = generated_tokens
        .iter()
        .copied()
        .map(|id| u32::try_from(id).expect("generated token IDs were validated at sampling"))
        .collect();
    #[cfg(feature = "parity-trace")]
    parity_trace::report(parity_trace::token_ids("generated_ids", &generated_ids));

    let gen_ms = t_gen_start.elapsed().as_millis();
    let n_gen = all_tokens.len() - n_prompt;
    let tok_s = if gen_ms > 0 {
        n_gen as f64 / gen_ms as f64 * 1000.0
    } else {
        0.0
    };
    println!("\n--- End ---");
    eprintln!(
        "[{} gen tokens in {}ms | {:.1} tok/s]",
        n_gen, gen_ms, tok_s
    );
    Ok(())
}
