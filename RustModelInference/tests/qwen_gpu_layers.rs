mod support;

use rust_model_inference::BackendKind;

#[test]
fn qwen3_layer_schedule_matches_cpu_for_two_tokens_and_carries_kv() {
    let fixture = support::tiny_qwen3();
    let expected = fixture.cpu_reference_two_tokens().unwrap();
    let actual = fixture.compiled_cpu_two_tokens().unwrap();
    support::assert_close(
        &actual.first_token_logits,
        &expected.first_token_logits,
        1e-3,
        1e-3,
    );
    support::assert_close(
        &actual.second_token_logits,
        &expected.second_token_logits,
        1e-3,
        1e-3,
    );
    assert_ne!(actual.first_token_logits, actual.second_token_logits);
    assert_eq!(actual.same_device_internal_host_waits, 0);
    assert_eq!(actual.kv_transfer_bytes, 0);
}

#[test]
#[ignore = "requires selected GPU backend"]
fn gpu_qwen3_layer_matches_cpu() {
    let backend = match std::env::var("RMI_REQUIRE_BACKEND").as_deref() {
        Ok("metal") => BackendKind::Metal,
        Ok("vulkan") => BackendKind::Vulkan,
        other => panic!("RMI_REQUIRE_BACKEND must be metal or vulkan, got {other:?}"),
    };
    let fixture = support::tiny_qwen3();
    let expected = fixture.compiled_cpu_two_tokens().unwrap();
    let actual = fixture.compiled_backend_two_tokens(backend).unwrap();
    support::assert_close(
        &actual.first_token_logits,
        &expected.first_token_logits,
        1e-3,
        1e-3,
    );
    support::assert_close(
        &actual.second_token_logits,
        &expected.second_token_logits,
        1e-3,
        1e-3,
    );
    assert_eq!(actual.tokens, expected.tokens);
    assert_eq!(actual.same_device_internal_host_waits, 0);
    assert_eq!(actual.kv_transfer_bytes, 0);
}

#[test]
fn qwen35_dense_layer_uses_metadata_mrope_and_matches_cpu() {
    let fixture = support::tiny_qwen35_q8_dense();
    let expected = fixture.cpu_reference_two_tokens().unwrap();
    let actual = fixture.compiled_cpu_two_tokens().unwrap();
    support::assert_close(
        &actual.first_token_logits,
        &expected.first_token_logits,
        1e-3,
        1e-3,
    );
    support::assert_close(
        &actual.second_token_logits,
        &expected.second_token_logits,
        1e-3,
        1e-3,
    );
    let (result, trace) = fixture
        .run_recording_forward("llm:layer=cpu0@1", &[1], &[[0, 0, 0, 0]])
        .expect("Qwen3.5 dense layer schedule must compile");
    result.expect("Qwen3.5 dense layer schedule must execute");
    assert!(trace.layer_ops.iter().any(|op| matches!(
        op,
        rust_model_inference::LayerOp::MRope { sections, .. } if *sections == [4, 4, 4, 4]
    )));
    assert!(trace
        .layer_ops
        .iter()
        .any(|op| matches!(op, rust_model_inference::LayerOp::SigmoidMul { .. })));
}

#[test]
fn qwen35_dense_two_token_batch_matches_cpu_row() {
    let fixture = support::tiny_qwen35_q8_dense();
    let expected = fixture.cpu_reference_two_tokens().unwrap();
    let actual = fixture
        .compiled_backend_two_token_batch(BackendKind::Cpu)
        .unwrap();
    support::assert_close(&actual, &expected.second_token_logits, 1e-3, 1e-3);
}

#[test]
#[ignore = "requires selected GPU backend"]
fn gpu_qwen35_dense_layer_matches_cpu() {
    let backend = match std::env::var("RMI_REQUIRE_BACKEND").as_deref() {
        Ok("metal") => BackendKind::Metal,
        Ok("vulkan") => BackendKind::Vulkan,
        other => panic!("RMI_REQUIRE_BACKEND must be metal or vulkan, got {other:?}"),
    };
    let fixture = support::tiny_qwen35_q8_dense();
    let expected = fixture.compiled_cpu_two_tokens().unwrap();
    let actual = fixture.compiled_backend_two_tokens(backend).unwrap();
    support::assert_close(&actual.logits, &expected.logits, 1e-3, 1e-3);
    assert_eq!(actual.tokens, expected.tokens);
    assert_eq!(actual.kv_transfer_bytes, 0);
}

#[test]
#[ignore = "requires selected GPU backend"]
fn gpu_qwen35_dense_two_token_batch_matches_cpu_row() {
    let backend = match std::env::var("RMI_REQUIRE_BACKEND").as_deref() {
        Ok("metal") => BackendKind::Metal,
        Ok("vulkan") => BackendKind::Vulkan,
        other => panic!("RMI_REQUIRE_BACKEND must be metal or vulkan, got {other:?}"),
    };
    let fixture = support::tiny_qwen35_q8_dense();
    let expected = fixture.cpu_reference_two_tokens().unwrap();
    let actual = fixture.compiled_backend_two_token_batch(backend).unwrap();
    support::assert_close(&actual, &expected.second_token_logits, 1e-3, 1e-3);
}

#[test]
fn qwen35_recurrent_layer_carries_and_resets_conv_and_ssm_state() {
    let fixture = support::tiny_qwen35_hybrid();
    let expected = fixture.cpu_reference_recurrent_two_tokens().unwrap();
    let actual = fixture
        .compiled_backend_recurrent_two_tokens(BackendKind::Cpu)
        .unwrap();
    support::assert_close(
        &actual.first_token_logits,
        &expected.first_token_logits,
        1e-3,
        1e-3,
    );
    support::assert_close(
        &actual.second_token_logits,
        &expected.second_token_logits,
        1e-3,
        1e-3,
    );
    assert_ne!(actual.first_token_logits, actual.second_token_logits);
    support::assert_close(
        &actual.first_after_reset_logits,
        &actual.first_token_logits,
        1e-3,
        1e-3,
    );
    assert_eq!(actual.tokens, expected.tokens);
    assert_eq!(actual.conv_state_bytes, 4 * 96 * 4);
    assert_eq!(actual.ssm_state_bytes, 32 * 32 * 4);
    assert_eq!(actual.recurrent_transfer_bytes, 0);
}

#[test]
#[ignore = "requires selected GPU backend"]
fn gpu_qwen35_recurrent_layer_matches_cpu_and_carries_state() {
    let backend = match std::env::var("RMI_REQUIRE_BACKEND").as_deref() {
        Ok("metal") => BackendKind::Metal,
        Ok("vulkan") => BackendKind::Vulkan,
        other => panic!("RMI_REQUIRE_BACKEND must be metal or vulkan, got {other:?}"),
    };
    let fixture = support::tiny_qwen35_hybrid();
    let expected = fixture
        .compiled_backend_recurrent_two_tokens(BackendKind::Cpu)
        .unwrap();
    let actual = fixture
        .compiled_backend_recurrent_two_tokens(backend)
        .unwrap();
    support::assert_close(
        &actual.first_token_logits,
        &expected.first_token_logits,
        1e-3,
        1e-3,
    );
    support::assert_close(
        &actual.second_token_logits,
        &expected.second_token_logits,
        1e-3,
        1e-3,
    );
    support::assert_close(
        &actual.first_after_reset_logits,
        &expected.first_after_reset_logits,
        1e-3,
        1e-3,
    );
    assert_ne!(actual.first_token_logits, actual.second_token_logits);
    assert_eq!(actual.tokens, expected.tokens);
    assert_eq!(actual.conv_state_bytes, expected.conv_state_bytes);
    assert_eq!(actual.ssm_state_bytes, expected.ssm_state_bytes);
    assert_eq!(actual.recurrent_transfer_bytes, 0);
}
