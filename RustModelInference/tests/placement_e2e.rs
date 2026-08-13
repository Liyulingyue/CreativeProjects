mod support;

use std::collections::BTreeSet;

use rust_model_inference::{ComponentId, ComponentWorkload, GGMLType};

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
fn qwen35_q8_dispatch_has_no_direct_cpu_escape() {
    let source = include_str!("../src/qwen35.rs");
    assert!(!source.contains("fn quantize_and_matmul_with_scratch"));
    assert!(!source.contains("cpu_matmul_with_scratch"));
    assert!(!source.contains("forward_cpu_legacy"));
}

#[test]
fn cli_has_no_legacy_q8_execution_path() {
    let source = include_str!("../src/main.rs");
    assert!(!source.contains("fn legacy_run_inference"));
    assert!(!source.contains("fn legacy_run_dump_logits"));
    assert!(!source.contains("Qwen3.5 CLI execution is not yet supported"));
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
