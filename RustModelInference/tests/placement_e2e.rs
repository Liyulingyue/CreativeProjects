mod support;

use std::path::Path;

use rust_model_inference::{
    build_qwen35_positions, compile_model, load_model_sources, BPETokenizer, ComponentId,
    EncodeOptions, ExecutionOptions, QwenRunner,
};

struct ShortPromptRun {
    logits: Vec<f32>,
    tokens: Vec<u32>,
}

fn highest_logit_token(logits: &[f32]) -> u32 {
    logits
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.total_cmp(right.1))
        .map(|(index, _)| index as u32)
        .expect("model has a vocabulary")
}

fn run_short_prompt(path: &Path, placement: String) -> Result<ShortPromptRun, String> {
    let sources = load_model_sources(path, None).map_err(|error| error.to_string())?;
    let llm = sources
        .iter()
        .find(|(component, _)| *component == ComponentId::Llm)
        .ok_or("model source did not contain an LLM")?
        .1
        .clone();
    let tokenizer = BPETokenizer::from_gguf_metadata(|key| llm.metadata(key).cloned())?;
    let prompt = tokenizer.encode("2 + 3 =", EncodeOptions::default());
    let (compiled, runner) = compile_model(
        sources,
        &ExecutionOptions {
            placements: vec![placement],
            thread_count: 1,
            max_batch_tokens: 1,
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?;
    let mut run = compiled.start_run().map_err(|error| error.to_string())?;
    let uploads_before: u64 = run.stats().values().map(|stats| stats.weight_uploads).sum();
    let (logits, tokens) = match runner {
        QwenRunner::Qwen3(model) => {
            let mut logits = vec![0.0; model.config.vocab];
            for (position, token) in prompt.iter().enumerate() {
                let position = position as u32;
                model
                    .forward(
                        &mut run,
                        &[*token],
                        &[[position, position, position, 0]],
                        &mut logits,
                    )
                    .map_err(|error| error.to_string())?;
            }
            let first = highest_logit_token(&logits);
            let position = prompt.len() as u32;
            model
                .forward(
                    &mut run,
                    &[first],
                    &[[position, position, position, 0]],
                    &mut logits,
                )
                .map_err(|error| error.to_string())?;
            let second = highest_logit_token(&logits);
            (logits, vec![first, second])
        }
        QwenRunner::Qwen35(model) => {
            let (positions, next_position) = build_qwen35_positions(&prompt, None, &[])?;
            let mut logits = vec![0.0; model.config.vocab_size];
            for (token, position) in prompt.iter().zip(positions) {
                model
                    .forward_compiled(
                        &mut run,
                        &[*token],
                        &[[
                            position[0] as u32,
                            position[1] as u32,
                            position[2] as u32,
                            position[3] as u32,
                        ]],
                        &mut logits,
                    )
                    .map_err(|error| error.to_string())?;
            }
            let first = highest_logit_token(&logits);
            let position = u32::try_from(next_position).map_err(|_| "position overflow")?;
            model
                .forward_compiled(
                    &mut run,
                    &[first],
                    &[[position, position, position, 0]],
                    &mut logits,
                )
                .map_err(|error| error.to_string())?;
            let second = highest_logit_token(&logits);
            (logits, vec![first, second])
        }
    };
    let uploads_after: u64 = run.stats().values().map(|stats| stats.weight_uploads).sum();
    if uploads_after != uploads_before {
        return Err(format!(
            "inference uploaded {} additional weight buffers",
            uploads_after - uploads_before
        ));
    }
    Ok(ShortPromptRun { logits, tokens })
}

#[test]
#[ignore = "requires four local real-model files and RMI_REQUIRE_BACKEND"]
fn explicit_layer_backend_matches_cpu_for_all_model_sources() {
    let backend =
        std::env::var("RMI_REQUIRE_BACKEND").expect("RMI_REQUIRE_BACKEND=vulkan|metal is required");
    assert!(matches!(backend.as_str(), "vulkan" | "metal"));
    for variable in [
        "RMI_QWEN3_GGUF",
        "RMI_QWEN3_GGUFRS",
        "RMI_QWEN35_GGUF",
        "RMI_QWEN35_GGUFRS",
    ] {
        let path = std::env::var(variable).unwrap_or_else(|_| panic!("{variable} is required"));
        let cpu = run_short_prompt(Path::new(&path), "llm:layer=cpu0@1".into()).unwrap();
        let accelerated =
            run_short_prompt(Path::new(&path), format!("llm:layer={backend}0@1")).unwrap();
        assert_eq!(accelerated.tokens, cpu.tokens, "{variable}");
        support::assert_close(&accelerated.logits, &cpu.logits, 1e-3, 1e-3);
    }
}

#[test]
#[cfg(feature = "gpu")]
fn default_cpu_compilation_never_discovers_or_opens_gpu_providers() {
    let probes = support::ProviderProbes::default();
    support::compile_fixture_with_probes(&support::tiny_qwen3(), &[], &probes).unwrap();
    support::assert_gpu_probes(&probes, (0, 0, 0, 0));
}

#[test]
fn explicit_vision_gpu_and_unavailable_backends_fail_before_open() {
    let probes = support::ProviderProbes::default();
    for placement in [
        "vision:layer=vulkan0@1",
        "vision:layer=metal0@1",
        "vision:layer=npu0@1",
    ] {
        let error =
            support::compile_fixture_with_probes(&support::tiny_qwen3(), &[placement], &probes)
                .unwrap_err();
        assert!(error.contains("unsupported component") || error.contains("unavailable"));
    }
    assert_eq!(
        probes.vulkan_open.load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    assert_eq!(
        probes.metal_open.load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

#[test]
fn raw_gguf_and_ggufrs_execute_the_same_qwen_logits_and_tokens() {
    for fixture in [
        support::tiny_qwen3_sources(),
        support::tiny_qwen35_sources(),
    ] {
        let raw = support::run_fixture_prompt(&fixture.gguf).unwrap();
        let packaged = support::run_fixture_prompt(&fixture.ggufrs).unwrap();
        support::assert_close(&raw.logits, &packaged.logits, 1e-3, 1e-3);
        assert_eq!(raw.tokens, packaged.tokens);
    }
}

#[test]
fn shared_compile_defaults_to_cpu_layer_plan() {
    let fixture = support::tiny_qwen3();
    let (compiled, _) = rust_model_inference::compile_model(
        vec![(rust_model_inference::ComponentId::Llm, fixture.llm_source())],
        &rust_model_inference::ExecutionOptions {
            thread_count: 1,
            max_batch_tokens: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        compiled.plan().components[&rust_model_inference::ComponentId::Llm]
            .primary
            .as_str(),
        "cpu0"
    );
}

#[test]
fn compile_model_rejects_missing_and_unknown_architecture_metadata() {
    for architecture in [None, Some("not-qwen")] {
        let result = rust_model_inference::compile_model(
            vec![(
                rust_model_inference::ComponentId::Llm,
                support::tiny_qwen3_source_with_architecture(architecture),
            )],
            &rust_model_inference::ExecutionOptions {
                thread_count: 1,
                max_batch_tokens: 1,
                ..Default::default()
            },
        );
        let error = match result {
            Ok(_) => panic!("{architecture:?} architecture metadata unexpectedly compiled"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("architecture"));
    }
}

#[test]
fn bundled_or_standalone_vision_defaults_to_cpu_before_opening_sessions() {
    let fixture = support::tiny_qwen3();
    let (compiled, _) = rust_model_inference::compile_model(
        vec![
            (rust_model_inference::ComponentId::Llm, fixture.llm_source()),
            (
                rust_model_inference::ComponentId::Vision,
                support::empty_vision_source(),
            ),
        ],
        &rust_model_inference::ExecutionOptions {
            thread_count: 1,
            max_batch_tokens: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        compiled.plan().components[&rust_model_inference::ComponentId::Vision]
            .primary
            .as_str(),
        "cpu0"
    );
}

use std::collections::BTreeSet;

use rust_model_inference::{
    ComponentWorkload, GGMLType, LayerOp, MetaValue, ProgramKind, Qwen3EmbeddingConfig,
    Qwen3EmbeddingPooling,
};

#[test]
fn qwen3_and_qwen35_q8_tensors_all_use_compiled_programs() {
    let fixtures = [support::tiny_qwen3(), support::tiny_qwen35_hybrid()];
    for fixture in fixtures {
        let expected_q8_matrices = fixture
            .catalog()
            .entries()
            .iter()
            .filter(|entry| {
                entry.component == ComponentId::Llm && entry.ggml_type == GGMLType::Q8_0
            })
            .filter(|entry| {
                entry.id != fixture.token_embedding_id() || entry.id == fixture.output_id()
            })
            .map(|entry| entry.id)
            .collect::<BTreeSet<_>>();
        let trace = fixture.run_recording_forward_two_tokens().unwrap();
        assert_eq!(trace.q8_matrix_tensor_ids, expected_q8_matrices);
        assert_eq!(
            trace.embedding_tensor_ids,
            BTreeSet::from([fixture.token_embedding_id()]),
        );
    }
}

#[test]
fn layer_execution_submits_embedding_spans_and_finalization_without_row_q8() {
    for fixture in [support::tiny_qwen3(), support::tiny_qwen35_hybrid()] {
        let (result, trace) = fixture
            .run_recording_forward("llm:layer=cpu0@1", &[1], &[[0, 0, 0, 0]])
            .unwrap();
        result.unwrap();
        assert!(matches!(
            trace.program_kinds.as_slice(),
            [
                ProgramKind::EmbeddingRows { tensor, .. },
                ProgramKind::LayerSegment { .. },
                ProgramKind::FinalNormQ8Logits { output, .. },
            ] if *tensor == fixture.token_embedding_id() && *output == fixture.output_id()
        ));
        assert!(trace
            .program_kinds
            .iter()
            .all(|kind| !matches!(kind, ProgramKind::Q8Rows { .. })));
    }
}

#[test]
fn qwen35_recurrent_layer_convolves_full_qkv_before_slicing() {
    let fixture = support::tiny_qwen35_hybrid();
    let (_, trace) = fixture
        .run_recording_forward("llm:layer=cpu0@1", &[1], &[[0, 0, 0, 0]])
        .unwrap();
    let qkv = fixture
        .catalog()
        .find(ComponentId::Llm, "blk.1.attn_qkv.weight")
        .unwrap();
    let qkv_index = trace
        .layer_ops
        .iter()
        .position(|op| matches!(op, LayerOp::Q8Matmul { weight, .. } if *weight == qkv))
        .unwrap();
    let qkv_slot = match trace.layer_ops[qkv_index] {
        LayerOp::Q8Matmul { output, .. } => output,
        _ => unreachable!(),
    };
    let conv_index = trace.layer_ops[qkv_index..]
        .iter()
        .position(|op| matches!(op, LayerOp::DepthwiseCausalConv { .. }))
        .map(|index| qkv_index + index)
        .unwrap();
    let (input, state, output) = match trace.layer_ops[conv_index] {
        LayerOp::DepthwiseCausalConv {
            input,
            state,
            output,
            ..
        } => (input, state, output),
        _ => unreachable!(),
    };
    assert_eq!((input, output), (qkv_slot, qkv_slot));
    assert!(trace.layer_ops[qkv_index + 1..conv_index]
        .iter()
        .all(|op| !matches!(op, LayerOp::Slice { input, .. } if *input == qkv_slot)));
    assert_eq!(trace.slot_byte_lengths[&state], 4 * 4 * 96);
}

#[test]
fn qwen35_recurrent_projections_precede_stateful_convolution() {
    let fixture = support::tiny_qwen35_hybrid();
    let (_, trace) = fixture
        .run_recording_forward("llm:layer=cpu0@1", &[1], &[[0, 0, 0, 0]])
        .unwrap();
    let conv_index = trace
        .layer_ops
        .iter()
        .position(|op| matches!(op, LayerOp::DepthwiseCausalConv { .. }))
        .unwrap();

    for name in [
        "blk.1.attn_qkv.weight",
        "blk.1.attn_gate.weight",
        "blk.1.ssm_beta.weight",
        "blk.1.ssm_alpha.weight",
    ] {
        let weight = fixture.catalog().find(ComponentId::Llm, name).unwrap();
        let projection_index = trace
            .layer_ops
            .iter()
            .position(|op| matches!(op, LayerOp::Q8Matmul { weight: candidate, .. } if *candidate == weight))
            .unwrap();
        assert!(
            projection_index < conv_index,
            "{name} projection must precede the stateful convolution"
        );
    }
}

#[test]
fn model_requirements_use_catalog_metadata() {
    for (fixture, expected_context) in [
        (support::tiny_qwen3(), 17),
        (support::tiny_qwen35_hybrid(), 19),
    ] {
        let ComponentWorkload::Llm(requirements) = fixture.requirements().workload else {
            panic!("fixture must compile an LLM workload");
        };
        assert_eq!(requirements.context_length, expected_context);
    }
}

#[test]
fn qwen35_recurrent_contract_is_rejected_during_catalog_model_construction() {
    for (case, expected) in [
        ("metadata", "Qwen3.5 recurrent dimensions"),
        ("qkv-shape", "blk.1.attn_qkv.weight"),
        ("gate-shape", "blk.1.attn_gate.weight"),
        ("beta-shape", "blk.1.ssm_beta.weight"),
        ("alpha-shape", "blk.1.ssm_alpha.weight"),
        ("conv-shape", "blk.1.ssm_conv1d.weight"),
        ("dt-bias-shape", "blk.1.ssm_dt.bias"),
        ("ssm-a-shape", "blk.1.ssm_a"),
        ("ssm-norm-shape", "blk.1.ssm_norm.weight"),
        ("ssm-output-shape", "blk.1.ssm_out.weight"),
    ] {
        let error = support::qwen35_recurrent_contract_error(case)
            .unwrap_or_else(|| panic!("{case} was accepted"));
        assert!(error.contains(expected), "{case}: {error}");
    }
}

#[test]
fn qwen35_non_q8_recurrent_projections_run_in_row_and_fail_layer_compile() {
    let fixture = support::tiny_qwen35_f32_recurrent();
    let logits = fixture.run_cpu_forward_two_tokens().unwrap();
    assert!(logits.iter().all(|value| value.is_finite()));

    let error = fixture
        .run_recording_forward("llm:layer=cpu0@1", &[1], &[[0, 0, 0, 0]])
        .unwrap_err();
    assert!(error.contains("unsupported tensor"), "{error}");
}

#[test]
fn qwen3_row_logits_are_derived_from_embeddings() {
    let logits = support::tiny_qwen3().run_cpu_forward_two_tokens().unwrap();
    assert!(logits.iter().any(|logit| *logit != 0.0));
}

#[test]
fn qwen3_tied_embedding_output_uses_the_compiled_q8_program() {
    let fixture = support::tiny_qwen3_tied();
    assert_eq!(fixture.token_embedding_id(), fixture.output_id());
    let trace = fixture.run_recording_forward_two_tokens().unwrap();
    assert!(trace.q8_matrix_tensor_ids.contains(&fixture.output_id()));
    let logits = fixture.run_cpu_forward_two_tokens().unwrap();
    assert!(logits.iter().any(|logit| *logit != 0.0));
}

#[test]
fn qwen3_embedding_runs_full_sequence_metadata_and_compiled_q8() {
    assert_eq!(
        Qwen3EmbeddingConfig::from_metadata(|key| match key {
            "qwen3.pooling_type" => Some(MetaValue::Uint32(3)),
            "qwen3.attention.causal" => Some(MetaValue::Bool(false)),
            _ => None,
        })
        .unwrap(),
        Qwen3EmbeddingConfig {
            causal: false,
            pooling: Qwen3EmbeddingPooling::Last,
        }
    );
    assert!(Qwen3EmbeddingConfig::from_metadata(|_| None)
        .unwrap_err()
        .contains("qwen3.pooling_type"));

    let fixture = support::tiny_qwen3();
    let tokens = [1, 2];
    let config = |causal, pooling| Qwen3EmbeddingConfig { causal, pooling };
    let mean = fixture
        .run_cpu_qwen3_embedding(&tokens, config(true, Qwen3EmbeddingPooling::Mean))
        .unwrap();
    let noncausal = fixture
        .run_cpu_qwen3_embedding(&tokens, config(false, Qwen3EmbeddingPooling::Mean))
        .unwrap();
    let last = fixture
        .run_cpu_qwen3_embedding(&tokens, config(true, Qwen3EmbeddingPooling::Last))
        .unwrap();
    assert_ne!(
        mean,
        [0.70710677, 0.70710677]
            .into_iter()
            .chain(std::iter::repeat(0.0))
            .take(64)
            .collect::<Vec<_>>()
    );
    assert_ne!(
        mean, noncausal,
        "future token must affect noncausal attention"
    );
    assert_ne!(mean, last, "metadata pooling must select MEAN versus LAST");

    let (result, trace) = fixture
        .run_recording_qwen3_embedding(
            "llm:row=cpu0@1",
            &tokens,
            config(true, Qwen3EmbeddingPooling::Mean),
        )
        .unwrap();
    result.unwrap();
    let expected = fixture
        .catalog()
        .entries()
        .iter()
        .filter(|entry| entry.component == ComponentId::Llm && entry.ggml_type == GGMLType::Q8_0)
        .filter(|entry| entry.id != fixture.token_embedding_id() && entry.id != fixture.output_id())
        .map(|entry| entry.id)
        .collect::<BTreeSet<_>>();
    assert_eq!(trace.q8_matrix_tensor_ids, expected);
}

#[test]
fn qwen3_embedding_rejects_layer_placement_before_submitting_work() {
    let fixture = support::tiny_qwen3();
    let (result, trace) = fixture
        .run_recording_qwen3_embedding(
            "llm:layer=cpu0@1",
            &[1, 2],
            Qwen3EmbeddingConfig {
                causal: true,
                pooling: Qwen3EmbeddingPooling::Mean,
            },
        )
        .unwrap();
    assert!(result.unwrap_err().contains("Row placement"));
    assert_eq!(trace, support::PlacementTrace::default());
}

#[test]
fn qwen35_row_logits_are_derived_from_embeddings() {
    let logits = support::tiny_qwen35_hybrid()
        .run_cpu_forward_two_tokens()
        .unwrap();
    assert!(logits.iter().any(|logit| *logit != 0.0));
}

#[test]
fn public_row_forward_accepts_multiple_tokens() {
    for fixture in [support::tiny_qwen3(), support::tiny_qwen35_hybrid()] {
        let logits = fixture.run_cpu_forward_two_tokens_in_one_call().unwrap();
        assert!(logits.iter().any(|logit| *logit != 0.0));
    }
}

#[test]
fn qwen35_f32_and_f16_cpu_row_matrices_produce_logits() {
    for fixture in [
        support::tiny_qwen35_f32_dense(),
        support::tiny_qwen35_f16_dense(),
    ] {
        let logits = fixture.run_cpu_forward_two_tokens().unwrap();
        assert!(logits.iter().all(|logit| logit.is_finite()));
        assert!(logits.iter().any(|logit| *logit != 0.0));
    }
}

#[test]
fn qwen35_dense_row_uses_the_doubled_q_attention_gate() {
    let open_gate = support::tiny_qwen35_f32_dense_with_attention_gate(20.0)
        .run_cpu_forward_two_tokens()
        .unwrap();
    let closed_gate = support::tiny_qwen35_f32_dense_with_attention_gate(-20.0)
        .run_cpu_forward_two_tokens()
        .unwrap();
    assert_ne!(open_gate, closed_gate);
}

#[test]
fn catalog_models_reject_metadata_tensor_layout_mismatches() {
    assert!(support::qwen3_metadata_layer_count_mismatch()
        .contains("metadata/tensor layer count mismatch"));
    assert!(support::qwen35_metadata_layer_selection_mismatch().contains("blk.0.attn_qkv.weight"));
}

#[test]
fn invalid_model_inputs_fail_before_recording_backend_submit_except_supported_qwen35_layer_batch() {
    for (name, fixture) in [
        ("qwen3", support::tiny_qwen3()),
        ("qwen35", support::tiny_qwen35_hybrid()),
    ] {
        for (placement, tokens, positions) in [
            ("llm:row=cpu0@1", vec![64], vec![[0, 0, 0, 0]]),
            ("llm:row=cpu0@1", vec![1], vec![[1, 1, 1, 0]]),
            ("llm:row=cpu0@1", vec![1], vec![[8, 8, 8, 0]]),
            ("llm:layer=cpu0@1", vec![64], vec![[0, 0, 0, 0]]),
            ("llm:layer=cpu0@1", vec![1], vec![[8, 8, 8, 0]]),
            (
                "llm:layer=cpu0@1",
                vec![1, 2],
                vec![[0, 0, 0, 0], [1, 1, 1, 0]],
            ),
        ] {
            let (result, trace) = fixture
                .run_recording_forward(placement, &tokens, &positions)
                .unwrap();
            if name == "qwen35" && placement == "llm:layer=cpu0@1" && tokens == [1, 2] {
                assert!(result.is_ok(), "{name} rejected supported layer batch");
                assert_ne!(
                    trace,
                    support::PlacementTrace::default(),
                    "{name} {placement}"
                );
                continue;
            }
            assert!(
                result.is_err(),
                "{name} accepted {placement} {tokens:?} {positions:?}"
            );
            assert_eq!(
                trace,
                support::PlacementTrace::default(),
                "{name} {placement}"
            );
        }
    }
}

#[test]
fn qwen35_quantized_cpu_row_fallback_matrices_produce_logits() {
    for matrix_type in [GGMLType::Q4K, GGMLType::Q5K, GGMLType::Q6K] {
        let logits = support::tiny_qwen35_quantized_dense(matrix_type)
            .run_cpu_forward_two_tokens()
            .unwrap();
        assert!(
            logits.iter().all(|logit| logit.is_finite()),
            "{matrix_type:?}"
        );
        assert!(logits.iter().any(|logit| *logit != 0.0), "{matrix_type:?}");
    }
}
