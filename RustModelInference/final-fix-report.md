# Final Fix Report

Branch: `codex/mac-metal-backend`
Base: `84f70f4` (`fix: exercise shared compile core in gpu probes`)

## Resolved findings

### Important: validate the Qwen3.5 recurrent contract before planning or execution

The recurrent row path and layer planner previously derived related dimensions independently and accepted inconsistent model metadata/tensor layouts until later execution.

The fix introduces one checked `Qwen35RecurrentDimensions` value during configuration loading. It rejects zero values and arithmetic overflow, enforces `inner_size == state_size * time_step_rank`, and makes the derived key, value, convolution, and per-head widths authoritative for both row and layer placement. `Qwen35Model::from_catalog` now rejects recurrent QKV, gate, alpha, beta, convolution, time-step bias, SSM A/norm, and output tensors whose exact shape or storage format violates that contract.

Regression evidence:

- RED before the fix: `cargo test --test placement_e2e qwen35_recurrent_contract_is_rejected_during_catalog_model_construction -- --nocapture` exited 101 because malformed metadata was accepted.
- GREEN after the fix: the same focused test passed, and it is also covered by both full all-target runs below.

### Important: reject invalid public Q8 arguments before backend side effects

`ExecutionRun::execute_q8` previously validated output length but did not validate the input width or batch against the compiled program before writing backend buffers.

The fix derives the input width and batch capacity from the immutable `Q8Rows` program and its F32 input slot. It cross-checks every shard's tensor, row range, program/input/output binding, and contract; then requires a nonzero in-capacity batch plus exact checked input/output lengths before the first backend write. These pre-submit validation errors leave the run healthy, while errors after backend work begins retain the existing poisoned-run behavior.

Regression evidence:

- RED before the fix: `q8_argument_validation_is_side_effect_free_and_run_remains_healthy` exited 101 because a short input was accepted.
- GREEN after the fix: short input, long input, and over-capacity batch all return before write/submit/wait/read, and a valid call on the same run succeeds. The test is included in both full all-target runs below.

### Minor: remove the unused placement helper

The unused `parse_requested_placements` implementation, crate-root export, and dedicated unit test were removed. A repository audit found no remaining non-documentation references to it or to the superseded recurrent dimension fields.

## Verification

- `cargo test --all-targets`: exit 0; 184 passed, 0 failed, 15 conditionally ignored.
- `cargo test --all-targets --features gpu`: exit 0; 214 passed, 0 failed, 22 conditionally ignored. This compiles the Metal and Vulkan implementations and runs their non-hardware contract tests.
- Focused suites observed during the fix: `execution_lifecycle` 11/11; `placement_e2e` 22 passed with 1 environment-dependent test ignored; `qwen_gpu_layers` 4 passed with 4 explicitly selected hardware tests ignored; recurrent planner state test passed.
- Explicit rustfmt checks pass for every fully formatted changed source/test file. `clip_config.rs` retains its pre-existing repository-wide formatting baseline; every newly changed hunk matches rustfmt output.
- `git diff --check`: exit 0.

The full runs retain pre-existing compiler warnings in untouched vision, GGUF, tokenizer, server, and test-support code. Tests requiring local model files, a pinned llama.cpp binary, or explicit `RMI_REQUIRE_BACKEND` selection remain ignored. No Metal shader, Vulkan shader/SPIR-V, or other GPU ABI file changed, so no shader regeneration check applies to this fix.

## Changed scope

- Recurrent metadata and tensor validation: `src/clip_config.rs`, `src/qwen35.rs`, `src/load_plan.rs`, `tests/placement_e2e.rs`, `tests/support/mod.rs`
- Q8 runtime argument validation: `src/compute/session.rs`, `tests/execution_lifecycle.rs`
- Unused helper cleanup: `src/placement.rs`, `src/lib.rs`

The untracked `.codegraph/` directory is user-owned workspace state and is intentionally excluded from the fix.
