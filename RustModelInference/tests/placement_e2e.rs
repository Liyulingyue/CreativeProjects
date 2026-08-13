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
fn qwen35_cpu_row_fallback_matrix_types_remain_supported() {
    let source = include_str!("../src/qwen35.rs");
    for matrix_type in ["F32", "F16", "Q4K", "Q5K", "Q6K"] {
        assert!(
            source.contains(&format!("CpuRowMatrix::{matrix_type}")),
            "missing Qwen3.5 CPU Row fallback for {matrix_type}",
        );
    }
    assert!(source.contains("primary.descriptor.backend != crate::BackendKind::Cpu"));
}

#[test]
fn qwen35_f32_cpu_row_matrices_produce_logits() {
    let logits = support::tiny_qwen35_f32_dense()
        .run_cpu_forward_two_tokens()
        .unwrap();
    assert!(logits.iter().any(|logit| *logit != 0.0));
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
