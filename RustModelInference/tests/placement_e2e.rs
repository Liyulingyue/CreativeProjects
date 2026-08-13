mod support;

use std::collections::BTreeSet;

use rust_model_inference::{
    ComponentId, ComponentWorkload, GGMLType, LayerOp, MetaValue, ProgramKind,
    Qwen3EmbeddingConfig, Qwen3EmbeddingPooling,
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
fn invalid_model_inputs_fail_before_recording_backend_submit() {
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
