mod support;

#[test]
fn q8_fixture_exercises_signed_bytes_scales_batch_offset_and_tail() {
    let fixture = support::q8_fixture(2, 64, 129, 17..113);
    assert!(fixture
        .weight_blocks
        .iter()
        .any(|block| block.qs.iter().any(|q| *q < 0)));
    assert_ne!(
        fixture.weight_blocks[0].scale,
        fixture.weight_blocks[1].scale
    );
    assert_eq!(fixture.expected.len(), 2 * 96);
}

#[cfg(feature = "vulkan")]
#[test]
#[ignore = "requires a Vulkan 1.1 compute adapter"]
fn vulkan_q8_row_matches_cpu() {
    support::require_backend("vulkan");
    let fixture = support::q8_fixture(2, 64, 129, 17..113);
    let actual =
        support::run_q8_backend(rust_model_inference::BackendKind::Vulkan, &fixture).unwrap();
    support::assert_close(&actual, &fixture.expected, 1e-4, 1e-4);
    assert!(actual.iter().all(|value| value.is_finite()));
}

#[cfg(feature = "vulkan")]
#[test]
#[ignore = "requires a Vulkan 1.1 compute adapter"]
fn vulkan_q8_row_tail_guard_matches_cpu() {
    support::require_backend("vulkan");
    let fixture = support::q8_fixture(2, 64, 129, 17..112);
    assert_ne!(fixture.batch * fixture.rows.len() % 64, 0);
    let actual =
        support::run_q8_backend(rust_model_inference::BackendKind::Vulkan, &fixture).unwrap();
    support::assert_close(&actual, &fixture.expected, 1e-4, 1e-4);
    assert!(actual.iter().all(|value| value.is_finite()));
}

#[cfg(all(target_os = "macos", feature = "metal"))]
#[test]
#[ignore = "requires a Metal device"]
fn metal_q8_row_matches_cpu() {
    support::require_backend("metal");
    let fixture = support::q8_fixture(2, 64, 129, 17..113);
    let actual =
        support::run_q8_backend(rust_model_inference::BackendKind::Metal, &fixture).unwrap();
    support::assert_close(&actual, &fixture.expected, 1e-4, 1e-4);
    assert!(actual.iter().all(|value| value.is_finite()));
}

#[cfg(all(target_os = "macos", feature = "metal"))]
#[test]
#[ignore = "requires a Metal device"]
fn metal_q8_row_tail_guard_matches_cpu() {
    support::require_backend("metal");
    let fixture = support::q8_fixture(2, 64, 129, 17..112);
    assert_ne!(fixture.batch * fixture.rows.len() % 64, 0);
    let actual =
        support::run_q8_backend(rust_model_inference::BackendKind::Metal, &fixture).unwrap();
    support::assert_close(&actual, &fixture.expected, 1e-4, 1e-4);
    assert!(actual.iter().all(|value| value.is_finite()));
}

#[cfg(all(not(target_os = "macos"), feature = "metal"))]
#[test]
fn metal_feature_keeps_portable_public_contracts_available() {
    let provider = rust_model_inference::compute::CpuProvider::new(1);
    let _provider: &dyn rust_model_inference::DeviceProvider = &provider;
}
