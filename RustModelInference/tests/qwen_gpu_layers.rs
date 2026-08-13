mod support;

use rust_model_inference::BackendKind;

#[test]
fn qwen3_layer_schedule_matches_cpu_for_two_tokens_and_carries_kv() {
    let fixture = support::tiny_qwen3();
    let expected = fixture.cpu_reference_two_tokens().unwrap();
    let actual = fixture.compiled_cpu_two_tokens().unwrap();
    support::assert_close(&actual.logits, &expected.logits, 1e-3, 1e-3);
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
    support::assert_close(&actual.logits, &expected.logits, 1e-3, 1e-3);
    assert_eq!(actual.tokens, expected.tokens);
    assert_eq!(actual.same_device_internal_host_waits, 0);
    assert_eq!(actual.kv_transfer_bytes, 0);
}
