# Task 2 report: Correct IMROPE and extract the shared Qwen3 decoder

Commit: `5271ee7` (`refactor: share the qwen3 decoder`)

## Implementation

- Replaced `rope_mrope_interleaved` with the pinned llama.cpp pair recurrence, NeoX half pairing, and fixed Qwen3VL section-axis dispatch.
- Added `src/qwen3.rs` with the exact public `Qwen3Model`, `Qwen3Session`, `Qwen3Input`, `Qwen3GenerateOptions`, `Qwen3Generation`, `Qwen3Rope`, and `qwen_text_positions` contracts.
- Added architecture-prefix configuration parsing for qwen2, qwen3, qwen3vl, and llama; optional key/value head lengths; tokenizer/model vocab equality; fixed Qwen3VL metadata; Q/K RMSNorm; and exact `[24,20,20,0]` IMROPE.
- Main-model loading now validates every named F32 norm and Q8_0 matrix shape/type/data length. The sole unsafe tensor lifetime helper documents and enforces the source-owning `Arc` invariant.
- Session capacity is checked prompt plus generation, with checked KV/scratch arithmetic before capacity-sized allocation. Inputs reject empty prompts, zero generation, invalid temperature, non-finite embeddings/logits, token/position/embedding mismatches, and out-of-range token IDs.
- Greedy ties retain the lowest token ID; terminal EOS/`im_end` is excluded from rendered/generated IDs. A focused test prevents evaluation of the last generated token.
- Server Qwen2/Qwen3 loading and generation now use `Qwen3Model`; duplicate server state/config/loop were removed.
- Ordinary CLI Qwen2/Qwen3 generation now uses `Qwen3Model`. Existing embedding, dump-logits, benchmark/profiling, F32-KV, Qwen3.5, multimodal, and interactive paths remain unchanged. Shared generation carries the existing parity checkpoints.

## Files changed

- `src/lib.rs`
- `src/ops.rs`
- `src/qwen3.rs` (new)
- `src/main.rs`
- `src/bin/server.rs`

## TDD RED/GREEN evidence

1. IMROPE vector RED:
   - Command: `cargo test ops::tests::imrope_matches_pinned_llama_cpp_qwen3vl_vector -- --exact`
   - Initial compile-only mistake was corrected by specifying `[f32; 128]`.
   - Correct RED then failed at `index=1`: left bits `3220963328`, expected `1028242533`, proving the old recurrence/iteration mismatch.
   - After the minimal recurrence replacement, the exact vector passed.
2. Shared interface RED:
   - Command: `cargo test qwen3::tests`
   - Failed with missing `Qwen3Config`, `Qwen3Rope`, `checked_session_capacity`, `validate_input_shapes`, and `greedy_token`.
   - Minimal configuration and validation helpers made all four required tests pass.
3. Generated-step regression RED:
   - Command: `cargo test qwen3::tests::decoder_does_not_evaluate_the_last_generated_token -- --exact`
   - Failed because `checked_decoder_steps` did not exist.
   - Minimal `capacity - 1` checked helper passed and is used by the shared loop.

## Verification

- `cargo test ops::tests::imrope_matches_pinned_llama_cpp_qwen3vl_vector -- --exact`: PASS, 1 passed.
- `cargo test qwen3::tests`: PASS, 5 passed.
- `cargo test --all-targets`: PASS, library 134 passed / 5 ignored; binaries and integration tests passed, with environment-dependent tests ignored.
- `cargo check --features parity-trace --all-targets`: PASS.
- `git diff --check`: PASS.
- `rustfmt --check src/qwen3.rs`: PASS.
- `cargo fmt --all -- --check`: FAIL on pre-existing repository-wide formatting drift. Confirmed independently by running rustfmt check against the pristine HEAD version of `src/bin/micro_bench.rs`, which already fails. No unrelated formatting was changed.
- Existing qwen35, vision, tokenizer, and server warnings remain; no new dependency was added.

## Duplicate-definition check

- `rg -n "fn generate_qwen3|struct Qwen3State|struct Qwen3Config" src/main.rs src/bin/server.rs`
- Result: no output, exit 1 (empty match set).

## Self-review

- API signatures match the brief.
- Qwen3VL uses Q/K RMSNorm and the exact fixed IMROPE sections; it is not aliased to NeoX.
- Prompt embeddings are used only for supplied prompt rows; generated rows always use token lookup with `[next_position; 4]`.
- Token embedding/output rows are checked against tokenizer metadata vocab, and every named tensor load returns an error instead of indexing/unwrapping.
- The transmuted slices have one documented unsafe helper and are retained by the same model that owns the source `Arc`.
- KV/scratch allocations use the requested session capacity and checked arithmetic; no 512/4096/full-context allocation is present in the shared decoder.
- Duplicate server Qwen3 decoder/config/state are gone. Existing specialized CLI paths were intentionally preserved to avoid unrelated behavioral regressions.

## Concerns

- No real Qwen2/Qwen3/Qwen3VL model file was available for an end-to-end inference smoke test; verification is compile-, unit-, and regression-suite-based.
- `cargo fmt --all -- --check` remains red solely because broad baseline files are not rustfmt-clean; formatting all of them would violate task scope and create unrelated changes.

## Fix round 1

### Changes

- Added a Qwen3VL-specific CLI guard so `--dump-logits`, `--bench`, `--profile`, `--kv-cache f32`, and interactive mode cannot enter the legacy NeoX decoder. Default F16 prompt generation still uses `Qwen3Model`; Qwen2/Qwen3 diagnostics and interactive behavior, embedding, Qwen3.5, and multimodal routing are unchanged.
- Generated decoder positions now start at the checked successor of the last supplied prompt text coordinate and increment from there, instead of using the decoder loop index. All four generated axes use that text coordinate.

### TDD RED/GREEN evidence

1. Qwen3VL legacy routing RED:
   - Command: `cargo test qwen3vl_rejects_legacy_decoder_modes`
   - Initial RED failed to compile because `validate_qwen3vl_decoder_mode` did not exist. The same build also reported the separately missing generated-position helper because Cargo compiled all test targets.
   - After the focused guard and branch were added, the test passed: 1 passed in `src/main.rs`.
2. Generated-position continuation RED:
   - The initial command above also failed with `cannot find function checked_generated_position` at both custom-position assertions.
   - Command after implementation: `cargo test generated_positions_continue_from_prompt_text_positions`
   - GREEN: 1 passed in `qwen3::tests`; the test covers `[42, 100, 200, 300] -> [43; 4]`, the following `[44; 4]`, and checked overflow.
3. Interactive escape-hatch RED:
   - After structural review found `run_interactive -> run_inference`, the Qwen3VL routing test was extended with an interactive case.
   - Command: `cargo test qwen3vl_rejects_legacy_decoder_modes`
   - RED failed to compile because the guard had no interactive-mode input; after adding the narrow rejection at interactive entry, GREEN passed: 1 passed.

### Verification after final edits

- `cargo test qwen3::tests`: PASS, 6 passed.
- `cargo test --all-targets`: PASS, library 135 passed / 5 ignored; main binary 24 passed / 1 ignored; other binary and integration targets passed or reported environment-dependent ignores.
- `cargo check --features parity-trace --all-targets`: PASS with existing warnings.
- `rg -n "fn generate_qwen3|struct Qwen3State|struct Qwen3Config" src/main.rs src/bin/server.rs`: no matches, exit 1.
- `git diff --check`: PASS.
- `rustfmt --edition 2021 --check src/qwen3.rs`: PASS.
- `cargo fmt --all -- --check`: expected repository-wide baseline failure; its first diff remains the pre-existing `src/bin/micro_bench.rs`. A rustfmt-output diff filtered to the newly changed `main.rs` symbols returned no matches, so the fix-round edits are clean without formatting unrelated code.

### Remaining concern

- No real Qwen3VL model was available for an end-to-end CLI smoke test, so the routing and position fixes are verified by focused unit tests, structural control-flow inspection, the full test suite, and parity-feature compilation.
