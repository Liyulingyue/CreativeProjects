# Qwen3-ASR-0.6B Full Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在纯 Rust 中让本地 Qwen3-ASR-0.6B 主 GGUF 与 audio mmproj 通过同一核心链路支持 CLI、OpenAI 风格 HTTP 转写、raw GGUF/GGUFRS 和固定 llama.cpp 分阶段对齐。

**Architecture:** 新增 `qwen3.rs` 收敛 CLI/server 重复的 Qwen3-family decoder 与 session；新增 `qwen3a.rs` 实现严格 WAV、log-Mel 和当前 Qwen3A encoder/projector contract；新增 `asr.rs` 只编排语言、prompt、audio embedding 注入和输出协议。复用现有 `TensorSource`、tokenizer、Q8_0 kernels、`ComputePool`、KV cache、GGUFRS 与 parity trace，不建立通用 modality trait，也不复用 tensor contract 不同的 `VisionEncoder`。

**Tech Stack:** Rust 2021、现有 GGUF/GGUFRS loader、Axum 0.8 multipart、`rustfft`、现有 Q8_0/F16/NEON/AVX2 kernels、固定 llama.cpp `9558fa44c92746a58dd07ad1bf0c889715b938a6`。

## Global Constraints

- 固定 LLM：`models/qwen3-asr-0.6b/Qwen3-ASR-0.6B-Q8_0.gguf`，SHA-256 `bca259818b50ca7c4c05e9bdb35a5dc04fa039653a6d6f3f0f331f96f6aa1971`。
- 固定 mmproj：`models/qwen3-asr-0.6b/mmproj-Qwen3-ASR-0.6B-Q8_0.gguf`，SHA-256 `41a342b5e4c514e968cb756de6cd1b7be39eff43c44c57a2ef5fc6522e36603d`。
- 固定 parity WAV：`asr_en-pcm16-16khz.wav`，481718 bytes，SHA-256 `23775909b26f2ebb1ccf0b877e7590b2cc31700a94bccf2d4111b98e9595acd8`；文件通过环境变量引用，不进入 Git。
- 首版输入只接受 RIFF/WAVE、little-endian PCM16、mono、16000 Hz；不做下混、重采样、格式转换、静默截断或广播。
- 首版不实现 MP3/FLAC/M4A、流式 ASR、batch、timestamps、word alignment 或 VAD。
- ASR 生成始终 greedy，默认 256 个新 token，遇 `<|im_end|>`/EOS 停止；所有 token 与 audio embedding 均占 decoder context。
- `qwen3vl` 必须使用 Q/K RMSNorm 和 `rope.dimension_sections=[24,20,20,0]` 的 llama.cpp IMROPE；不得别名为普通 Qwen3 RoPE。
- `ComponentRole::Mmproj = 2`、GGUFRS version 1 与现有 segment 语义保持不变；audio/vision 只在 mmproj contract 校验处分支。
- 只有一个新增 direct dependency：`rustfft`；Axum 只开启已有 crate 的 `multipart` feature，不增加 WAV、HTTP 测试或通用音频依赖。
- server multipart body 上限 32 MiB；multipart 解析不持推理锁，一个 `std::sync::Mutex<()>` 覆盖整次 chat 或 ASR 模型调用。
- 所有公开输入长度、shape 乘法、offset、KV capacity 在分配前使用 checked arithmetic；NaN/Inf、空音频、协议错误和不匹配 shape 必须返回错误。
- 每个最多 800-frame 的 Mel window 独立跑 audio Transformer；scratch 复用，跨 window 只保留最终 1024 维 projected embeddings。
- 保留 x86_64 AVX2/FMA/F16C、aarch64 NEON 与 scalar fallback；不得以 ARM 修复换取 x86 回退。
- `.codegraph/` 是用户生成的本地索引，不 stage、不提交；模型、WAV、llama.cpp checkout 和 parity dump 也不提交。

## File Structure

| File | Responsibility |
| --- | --- |
| `src/qwen3.rs` | Qwen3/Qwen3VL metadata、权重、token embedding、共享 decoder/session、采样与 stop 条件 |
| `src/qwen3a.rs` | Qwen3A 固定 contract、WAV 解码、log-Mel、Conv2D、audio Transformer、projector |
| `src/asr.rs` | 语言归一化、ASR prompt、embedding 替换、context 校验、`transcribe_wav` 与输出解析 |
| `src/model.rs` | `qwen3vl` 通用模型 metadata 前缀支持 |
| `src/tokenizer.rs` | audio/ASR semantic token 映射 |
| `src/ops.rs` | llama.cpp-compatible IMROPE 修正与固定向量测试 |
| `src/ggufrs.rs` | `qwen3vl` LLM export 与 audio/vision mmproj contract 分支、raw/GGUFRS 等价测试 |
| `src/main.rs` | ASR CLI 参数校验、模型/mmproj 解析和共享核心调用；删除本文件中的重复 Qwen3 decoder |
| `src/bin/server.rs` | multipart endpoint、错误映射、可选 audio runtime 和全推理锁；删除重复 Qwen3 decoder |
| `src/lib.rs` | 导出 `qwen3`、`qwen3a`、`asr` 的最小公共接口 |
| `Cargo.toml`, `Cargo.lock` | `rustfft` 和 Axum multipart feature |

环境变量统一为：

```text
QWEN3_ASR_MODEL        主 GGUF 路径
QWEN3_ASR_MMPROJ       audio mmproj GGUF 路径
QWEN3_ASR_WAV          固定 PCM16/16k WAV 路径
QWEN3_ASR_LLAMA_TRACE  llama.cpp oracle trace JSONL 路径
RMI_PARITY_TRACE       Rust trace JSONL 路径
```

---

### Task 1: Accept the Qwen3VL and Qwen3A storage contracts

**Files:**
- Create: `src/qwen3a.rs`
- Modify: `src/lib.rs:1-42`
- Modify: `src/model.rs:600-656`
- Modify: `src/tokenizer.rs:64-72, 873-1028`
- Modify: `src/ggufrs.rs:388-501, 507-618, 3202-3295`

**Interfaces:**
- Consumes: existing `TensorSource`, `TensorInfo`, `MetaValue`, `GGMLType`, `ComponentRole::Mmproj`.
- Produces: `Qwen3AudioConfig::from_source(&dyn TensorSource) -> Result<Qwen3AudioConfig, String>` and `pub(crate) fn validate_qwen3a_source(&dyn TensorSource) -> Result<Qwen3AudioConfig, String>` for Task 5 and GGUFRS export validation.

- [ ] **Step 1: Add failing metadata and semantic-token tests**

Add tests named `qwen3vl_uses_its_own_metadata_prefix` in `src/model.rs` and `asr_semantic_literals_resolve_to_token_ids` in `src/tokenizer.rs`. The model fixture must expose these exact values and assert every parsed field:

```rust
#[derive(Default)]
struct MapTensorSource {
    metadata: HashMap<String, MetaValue>,
    tensors: HashMap<String, TensorInfo>,
}

impl TensorSource for MapTensorSource {
    fn metadata(&self, key: &str) -> Option<&MetaValue> { self.metadata.get(key) }
    fn tensor_info(&self, name: &str) -> Option<&TensorInfo> { self.tensors.get(name) }
    fn tensor_slice(&self, _name: &str) -> Option<&[u8]> { None }
}

let metadata = HashMap::from([
    ("general.architecture".into(), MetaValue::String("qwen3vl".into())),
    ("qwen3vl.embedding_length".into(), MetaValue::Uint32(1024)),
    ("qwen3vl.block_count".into(), MetaValue::Uint32(28)),
    ("qwen3vl.attention.head_count".into(), MetaValue::Uint32(16)),
    ("qwen3vl.attention.head_count_kv".into(), MetaValue::Uint32(8)),
    ("qwen3vl.feed_forward_length".into(), MetaValue::Uint32(3072)),
    ("qwen3vl.context_length".into(), MetaValue::Uint32(65536)),
    ("qwen3vl.rope.freq_base".into(), MetaValue::Float32(1_000_000.0)),
    ("qwen3vl.attention.layer_norm_rms_epsilon".into(), MetaValue::Float32(1e-6)),
    ("qwen3vl.vocab_size".into(), MetaValue::Uint32(151_936)),
]);
let config = model_config_from_source(&MapTensorSource { metadata, tensors: HashMap::new() }).unwrap();
assert_eq!((config.n_embd, config.n_layer, config.n_head, config.n_head_kv), (1024, 28, 16, 8));
assert_eq!((config.n_ff, config.n_ctx), (3072, 65536));
assert_eq!(config.rope_freq_base, 1_000_000.0);
assert_eq!(config.norm_eps, 1e-6);
```

Extend the tokenizer fixture with the four literals and assert semantic names, not hard-coded production IDs:

```rust
assert_eq!(tokenizer.special_token_id("audio_start"), Some(audio_start_id));
assert_eq!(tokenizer.special_token_id("audio_pad"), Some(audio_pad_id));
assert_eq!(tokenizer.special_token_id("audio_end"), Some(audio_end_id));
assert_eq!(tokenizer.special_token_id("asr_text"), Some(asr_text_id));
```

- [ ] **Step 2: Run the focused tests and confirm the contract is rejected**

Run:

```bash
cargo test model::tests::qwen3vl_uses_its_own_metadata_prefix -- --exact
cargo test tokenizer::tests::asr_semantic_literals_resolve_to_token_ids -- --exact
```

Expected: the first test fails with `Unsupported architecture: qwen3vl`; the second fails because the four semantic names return `None`.

- [ ] **Step 3: Add the minimal Qwen3VL/tokenizer dispatch**

Change only the accepted prefix and semantic literal table:

```rust
let prefix = match arch {
    "qwen2" | "qwen3" | "qwen3vl" | "qwen35" | "llama" => arch,
    _ => return Err(format!("Unsupported architecture: {arch}")),
};

const QWEN_SEMANTIC_TOKENS: &[(&str, &str)] = &[
    ("<|im_start|>", "im_start"),
    ("<|im_end|>", "im_end"),
    ("<|image_pad|>", "image_pad"),
    ("<|vision_pad|>", "vision_pad"),
    ("<|vision_start|>", "vision_start"),
    ("<|vision_end|>", "vision_end"),
    ("<|audio_start|>", "audio_start"),
    ("<|audio_pad|>", "audio_pad"),
    ("<|audio_end|>", "audio_end"),
    ("<asr_text>", "asr_text"),
    ("<|endoftext|>", "endoftext"),
];
```

- [ ] **Step 4: Add failing strict Qwen3A contract tests**

In `src/qwen3a.rs`, define a test-only map-backed source and one `valid_qwen3a_source()` helper. Populate exact metadata:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Qwen3AudioConfig {
    pub hidden: usize,
    pub ffn: usize,
    pub layers: usize,
    pub heads: usize,
    pub mel_bins: usize,
    pub projection: usize,
    pub epsilon: f32,
}

let expected = Qwen3AudioConfig {
    hidden: 896,
    ffn: 3584,
    layers: 18,
    heads: 14,
    mel_bins: 128,
    projection: 1024,
    epsilon: 1e-5,
};
```

The fixture must contain `general.architecture=clip`, `general.type=mmproj`, `clip.has_audio_encoder=true`, `clip.audio.projector_type=qwen3a`, and these exact numeric metadata values:

```text
clip.audio.embedding_length                  Uint32(896)
clip.audio.feed_forward_length               Uint32(3584)
clip.audio.block_count                       Uint32(18)
clip.audio.attention.head_count              Uint32(14)
clip.audio.num_mel_bins                      Uint32(128)
clip.audio.projection_dim                    Uint32(1024)
clip.audio.attention.layer_norm_epsilon      Float32(1e-5)
```

Generate all 18 block tensor records in a loop and use these exact per-layer names, shapes and types:

```text
a.blk.{i}.attn_q.weight       [896,896] Q8_0
a.blk.{i}.attn_q.bias         [896] F32
a.blk.{i}.attn_k.weight       [896,896] Q8_0
a.blk.{i}.attn_k.bias         [896] F32
a.blk.{i}.attn_v.weight       [896,896] Q8_0
a.blk.{i}.attn_v.bias         [896] F32
a.blk.{i}.attn_out.weight     [896,896] Q8_0
a.blk.{i}.attn_out.bias       [896] F32
a.blk.{i}.ln1.weight          [896] F32
a.blk.{i}.ln1.bias            [896] F32
a.blk.{i}.ln2.weight          [896] F32
a.blk.{i}.ln2.bias            [896] F32
a.blk.{i}.ffn_up.weight       [896,3584] Q8_0
a.blk.{i}.ffn_up.bias         [3584] F32
a.blk.{i}.ffn_down.weight     [3584,896] Q8_0
a.blk.{i}.ffn_down.bias       [896] F32
```

Add the shared tensors with exact records:

```text
a.position_embd.weight [896,1500] F32
a.conv2d.1.weight      [3,3,1,480] F16
a.conv2d.1.bias        [1,1,480] F32
a.conv2d.2.weight      [3,3,480,480] F16
a.conv2d.2.bias        [1,1,480] F32
a.conv2d.3.weight      [3,3,480,480] F16
a.conv2d.3.bias        [1,1,480] F32
a.conv_out.weight      [7680,896] F16
a.post_ln.weight       [896] F32
a.post_ln.bias         [896] F32
mm.a.mlp.1.weight      [896,896] Q8_0
mm.a.mlp.1.bias        [896] F32
mm.a.mlp.2.weight      [896,1024] Q8_0
mm.a.mlp.2.bias        [1024] F32
```

Tests must independently mutate a valid fixture to cover missing metadata, wrong projector type, missing tensor, wrong shape, wrong type and projection `!= 1024`.

- [ ] **Step 5: Run the Qwen3A contract tests and confirm the validator is absent**

Run:

```bash
cargo test qwen3a::tests::qwen3a_contract_accepts_only_the_fixed_model -- --exact
cargo test qwen3a::tests::qwen3a_contract_rejects_metadata_shape_and_type_drift -- --exact
```

Expected: compilation fails because `Qwen3AudioConfig::from_source` and `validate_qwen3a_source` do not exist.

- [ ] **Step 6: Implement one strict contract validator**

Use checked integer conversion and a compact local tensor check; both runtime loading and GGUFRS export must call this function:

```rust
fn require_tensor(
    source: &dyn TensorSource,
    name: &str,
    dims: &[u64],
    ggml_type: GGMLType,
) -> Result<(), String> {
    let info = source.tensor_info(name).ok_or_else(|| format!("Missing Qwen3A tensor: {name}"))?;
    if info.dims != dims || info.ggml_type != ggml_type {
        return Err(format!(
            "Invalid Qwen3A tensor {name}: shape {:?} type {:?}; expected {:?} {:?}",
            info.dims, info.ggml_type, dims, ggml_type
        ));
    }
    Ok(())
}

impl Qwen3AudioConfig {
    pub fn from_source(source: &dyn TensorSource) -> Result<Self, String> {
        validate_qwen3a_source(source)
    }
}
```

Reject any value that differs from the fixed config. This is intentionally model-specific: do not add a modality registry, factory or configurable tensor schema.

- [ ] **Step 7: Branch GGUFRS validation and retain existing role/version semantics**

In `load_export_source`, add `qwen3vl` to the LLM whitelist. For `ComponentRole::Mmproj`, a true `clip.has_audio_encoder` must also have `clip.audio.projector_type == "qwen3a"`, then call `validate_qwen3a_source(&source.loader)`; a missing/false audio flag runs the existing vision checks unchanged. A true audio flag with a missing/other projector type is an error naming `clip.audio.projector_type`, not a fallback to vision. Do not alter `plan_export`: audio tensors remain one `SegmentKind::Component` segment owned by role 2. Refactor the existing fixture constructor to `test_gguf_pair_with_arch(architecture: &str)` and keep `test_gguf_pair()` as a `qwen3` wrapper so existing tests do not change.

Add tests:

```rust
#[test]
fn qwen3vl_llm_uses_existing_shared_and_layer_segments() {
    let inputs = test_support::test_gguf_pair_with_arch("qwen3vl");
    let output = inputs.dir.join("qwen3vl.ggufrs");
    export_ggufrs(&output, &inputs.llm, Some(&inputs.mmproj), ExportOptions::default()).unwrap();
    let package = GgufrsFile::open(output).unwrap();
    let component = package.component_id(ComponentRole::Llm).unwrap();
    let segments = package.index.segments.iter()
        .filter(|segment| segment.component_id == component)
        .map(|segment| (segment.kind, segment.layer))
        .collect::<Vec<_>>();
    assert_eq!(segments, vec![
        (SegmentKind::Shared, None),
        (SegmentKind::Layer, Some(0)),
        (SegmentKind::Layer, Some(1)),
    ]);
}

#[test]
fn vision_mmproj_validation_is_unchanged() {
    let inputs = test_support::test_gguf_pair();
    let output = inputs.dir.join("vision.ggufrs");
    export_ggufrs(&output, &inputs.llm, Some(&inputs.mmproj), ExportOptions::default()).unwrap();
    GgufrsFile::open(output).unwrap().verify_all().unwrap();
}
```

The first test fixture only changes the existing test LLM architecture/prefix from `qwen3` to `qwen3vl`; its expected segment list stays `[Shared, Layer(0), Layer(1)]`.

Also add `qwen3a_mmproj_uses_the_audio_validation_branch`: write a raw mmproj fixture from the same exact metadata/tensor records used by `valid_qwen3a_source()` with the existing `test_support::SourceTensor`/`write_test_gguf`, export it beside a `qwen3vl` LLM fixture, and require `verify_all()` to pass. Rewrite only `clip.audio.projector_type` to `other` in a second fixture and require `export_ggufrs` to return a `SourceGguf` error naming that key. This is the cheap non-ignored proof that the exporter reaches audio validation; Task 7 retains the real-weight byte/transcript gate.

- [ ] **Step 8: Run focused and module tests**

Run:

```bash
cargo test model::tests::qwen3vl_uses_its_own_metadata_prefix -- --exact
cargo test tokenizer::tests::asr_semantic_literals_resolve_to_token_ids -- --exact
cargo test qwen3a::tests
cargo test ggufrs::tests
cargo test ggufrs::tests::qwen3a_mmproj_uses_the_audio_validation_branch -- --exact
```

Expected: all pass, including all pre-existing vision GGUFRS tests.

- [ ] **Step 9: Commit the storage contract**

```bash
git add src/lib.rs src/model.rs src/tokenizer.rs src/qwen3a.rs src/ggufrs.rs
git commit -m "feat: accept qwen3 ASR model contracts"
```

### Task 2: Correct IMROPE and extract the shared Qwen3 decoder

**Files:**
- Create: `src/qwen3.rs`
- Modify: `src/lib.rs:1-42`
- Modify: `src/ops.rs:211-317, 1802-end`
- Modify: `src/main.rs:1660-2208, 2210-2960`
- Modify: `src/bin/server.rs:19-80, 174-202, 366-792, 980-1152`

**Interfaces:**
- Consumes: `TensorSource`, `BPETokenizer`, `ExecutionScratchpad`, `KvCache`, `ComputePool`, Q8_0 ops and corrected `rope_mrope_interleaved`.
- Produces: `Qwen3Model`, `Qwen3Session`, `Qwen3Input`, `Qwen3GenerateOptions`, `Qwen3Generation`, `qwen_text_positions` and `Qwen3Model::embed_tokens` for Tasks 6, 8 and 9.

- [ ] **Step 1: Add a fixed-vector llama.cpp IMROPE regression test**

Add `imrope_matches_pinned_llama_cpp_qwen3vl_vector` to `src/ops.rs`. Use one 128-d head, positions `[1,2,3,4]`, sections `[24,20,20,0]`, base `1_000_000`, and input `(index - 64) * 0.03125`. Assert the selected f32 bits:

```rust
let expected = [
    (0, 0xbf8a_5140), (1, 0x3d49_bc65), (2, 0x3f27_e158),
    (20, 0xbfb3_0f0c), (21, 0xbfac_e489),
    (40, 0xbf40_1d22), (41, 0xbf38_2418),
    (59, 0xbe20_0444), (60, 0xbe00_012a),
    (61, 0xbdc0_07a4), (62, 0xbd80_0642), (63, 0xbd00_0290),
    (64, 0xbfd7_6aa4), (65, 0xbffb_f3f1), (66, 0xbfe9_7fe5),
    (84, 0x3f11_cb34), (85, 0x3f24_4b31),
    (104, 0x3f9f_f741), (105, 0x3fa3_f5df),
    (123, 0x3feb_fff4), (124, 0x3fef_fffe),
    (125, 0x3ff3_fffa), (126, 0x3ff7_fffd), (127, 0x3ffc_0000),
];
for (index, bits) in expected {
    assert_eq!(values[index].to_bits(), bits, "index={index}");
}
```

- [ ] **Step 2: Run the vector test and verify the old implementation fails**

Run:

```bash
cargo test ops::tests::imrope_matches_pinned_llama_cpp_qwen3vl_vector -- --exact
```

Expected: FAIL at the first checked index because the old function iterates dimensions instead of rotary pairs and uses `1/freq_base` as its recurrence.

- [ ] **Step 3: Implement the pinned llama.cpp IMROPE recurrence**

Replace `rope_mrope_interleaved` with pair iteration and NeoX half pairing:

```rust
pub fn rope_mrope_interleaved(
    x: &mut [f32],
    positions: [usize; 4],
    sections: [i32; 4],
    head_dim: usize,
    freq_base: f32,
    n_rope_dims: usize,
) {
    assert!(n_rope_dims <= head_dim && n_rope_dims % 2 == 0);
    let pair_count = n_rope_dims / 2;
    let section_pairs: usize = sections.iter().map(|&value| value as usize).sum();
    let theta_scale = freq_base.powf(-2.0 / n_rope_dims as f32);
    for head in x.chunks_exact_mut(head_dim) {
        let mut theta = positions.map(|value| value as f32);
        for pair in 0..pair_count {
            let sector = pair % section_pairs;
            let axis = if sector % 3 == 1 && sector < 3 * sections[1] as usize {
                1
            } else if sector % 3 == 2 && sector < 3 * sections[2] as usize {
                2
            } else if sector % 3 == 0 && sector < 3 * sections[0] as usize {
                0
            } else {
                3
            };
            let (sin, cos) = theta[axis].sin_cos();
            let x0 = head[pair];
            let x1 = head[pair + pair_count];
            head[pair] = x0.mul_add(cos, -(x1 * sin));
            head[pair + pair_count] = x0.mul_add(sin, x1 * cos);
            for value in &mut theta {
                *value *= theta_scale;
            }
        }
    }
}
```

Keep the existing `rope_mrope` and `rope_neox` functions unchanged.

- [ ] **Step 4: Add failing shared-decoder interface tests**

Create `src/qwen3.rs` tests for configuration and allocation boundaries:

```rust
#[test]
fn qwen3vl_requires_qk_norm_and_fixed_imrope_sections() {
    let config = Qwen3Config::from_source(&qwen3vl_metadata_source(), 151_936).unwrap();
    assert!(config.has_qk_norm);
    assert_eq!(config.rope, Qwen3Rope::Interleaved {
        sections: [24, 20, 20, 0],
        n_dims: 128,
    });
}

#[test]
fn session_capacity_is_prompt_plus_generation_not_model_context() {
    assert_eq!(checked_session_capacity(23, 17, 65_536).unwrap(), 40);
    assert!(checked_session_capacity(65_500, 37, 65_536).is_err());
    assert!(checked_session_capacity(usize::MAX, 1, 65_536).is_err());
}

#[test]
fn decoder_input_rejects_position_and_embedding_shape_mismatch() {
    assert!(validate_input_shapes(3, 1024, 2, None).is_err());
    assert!(validate_input_shapes(3, 1024, 3, Some(3 * 1024 - 1)).is_err());
}

#[test]
fn greedy_ties_choose_the_lowest_token_id() {
    assert_eq!(greedy_token(&[1.0, 2.0, 2.0]).unwrap(), 1);
    assert!(greedy_token(&[1.0, f32::NAN]).is_err());
}
```

The checked helpers used by these tests have these private signatures:

```rust
fn checked_session_capacity(prompt: usize, generation: usize, context: usize) -> Result<usize, String>;
fn validate_input_shapes(
    token_count: usize,
    embedding_dim: usize,
    position_count: usize,
    embedding_values: Option<usize>,
) -> Result<(), String>;
fn greedy_token(logits: &[f32]) -> Result<u32, String>;
```

- [ ] **Step 5: Define the exact shared decoder API**

Use these public types; keep weight structs private:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Qwen3Rope {
    Neox,
    Interleaved { sections: [i32; 4], n_dims: usize },
}

pub struct Qwen3Config {
    pub architecture: String,
    pub n_embd: usize,
    pub n_layer: usize,
    pub n_head: usize,
    pub n_head_kv: usize,
    pub n_embd_head_k: usize,
    pub n_embd_head_v: usize,
    pub n_ff: usize,
    pub vocab: usize,
    pub n_ctx: usize,
    pub eps: f32,
    pub freq_base: f32,
    pub has_qk_norm: bool,
    pub rope: Qwen3Rope,
}

impl Qwen3Config {
    fn from_source(source: &dyn TensorSource, tokenizer_vocab: usize) -> Result<Self, String>;
}

pub struct Qwen3Input<'a> {
    pub token_ids: &'a [u32],
    pub positions: &'a [[usize; 4]],
    pub embeddings: Option<&'a [f32]>,
}

#[derive(Debug, Clone, Copy)]
pub struct Qwen3GenerateOptions {
    pub max_new_tokens: usize,
    pub temperature: f32,
}

pub struct Qwen3Generation {
    pub text: String,
    pub rendered_tokens: Vec<String>,
    pub token_ids: Vec<u32>,
    pub prompt_tokens: usize,
}

pub struct Qwen3Model {
    source: Arc<dyn TensorSource>,
    tokenizer: Arc<BPETokenizer>,
    pool: Arc<ComputePool>,
    config: Qwen3Config,
    layers: Vec<Qwen3LayerWeights>,
    output_norm: Vec<f32>,
    token_embedding: &'static [u8],
    output: &'static [u8],
}

struct Qwen3LayerWeights {
    attn_norm: Vec<f32>,
    ffn_norm: Vec<f32>,
    q_norm: Option<Vec<f32>>,
    k_norm: Option<Vec<f32>>,
    wq: &'static [u8],
    wk: &'static [u8],
    wv: &'static [u8],
    wo: &'static [u8],
    w_gate: &'static [u8],
    w_up: &'static [u8],
    w_down: &'static [u8],
}

pub struct Qwen3Session<'model> {
    model: &'model Qwen3Model,
    kv_cache: KvCache,
    scratch: ExecutionScratchpad,
    capacity: usize,
}
```

`Qwen3Generation::token_ids` contains generated non-EOS token IDs only; the terminal EOS/`<|im_end|>` sample is a stop condition and is neither rendered nor counted. The llama.cpp oracle in Task 10 must write `asr.generated_ids` with the same terminal token excluded before exact comparison.

Methods and helpers must have these signatures:

```rust
impl Qwen3Model {
    pub fn from_source(
        source: Arc<dyn TensorSource>,
        tokenizer: Arc<BPETokenizer>,
        pool: Arc<ComputePool>,
    ) -> Result<Self, String>;
    pub fn config(&self) -> &Qwen3Config;
    pub fn tokenizer(&self) -> &BPETokenizer;
    pub fn pool(&self) -> Arc<ComputePool>;
    pub fn embed_tokens(&self, token_ids: &[u32]) -> Result<Vec<f32>, String>;
    pub fn generate(
        &self,
        input: Qwen3Input<'_>,
        options: Qwen3GenerateOptions,
    ) -> Result<Qwen3Generation, String>;
}

impl<'model> Qwen3Session<'model> {
    pub fn new(model: &'model Qwen3Model, capacity: usize) -> Result<Self, String>;
    pub fn generate(
        &mut self,
        input: Qwen3Input<'_>,
        options: Qwen3GenerateOptions,
    ) -> Result<Qwen3Generation, String>;
}

pub fn qwen_text_positions(n_tokens: usize) -> Vec<[usize; 4]> {
    (0..n_tokens).map(|position| [position; 4]).collect()
}
```

- [ ] **Step 6: Move the decoder once and add Qwen3VL branches**

Move the server's owned model loading and Qwen3 loop into `qwen3.rs`, retaining the source-owning `Arc` before every transmuted tensor slice and documenting that invariant at the single unsafe helper. Merge the CLI parity checkpoints into that loop instead of keeping a second traced implementation.

The configuration parser must use the architecture prefix, retain existing `qwen2`, `qwen3` and `llama` behavior, add `qwen3vl`, read optional `attention.key_length`/`value_length`, and verify metadata vocab equals `tokenizer_vocab`. `Qwen3Model::from_source` then verifies token-embedding/output row counts equal that vocab. In the Task 2 test module, repeat Task 1's three-method map-backed `TensorSource` and build `qwen3vl_metadata_source()` with the Task 1 main-model metadata plus `key_length=128`, `value_length=128` and the four-element Int32 `rope.dimension_sections` array. For `qwen3vl`, require:

```rust
has_qk_norm = true;
rope = Qwen3Rope::Interleaved {
    sections: read_i32_array(source, "qwen3vl.rope.dimension_sections")?,
    n_dims: n_embd_head_k,
};
if sections != [24, 20, 20, 0] {
    return Err(format!("Unsupported qwen3vl.rope.dimension_sections: {sections:?}"));
}
```

Also require `(n_embd,n_layer,n_head,n_head_kv,n_embd_head_k,n_embd_head_v,n_ff,n_ctx,eps,freq_base)` to equal `(1024,28,16,8,128,128,3072,65536,1e-6,1_000_000.0)` for `qwen3vl`. Validate every main-model F32 norm and Q8_0 matrix shape/type in `from_source`; return a named error instead of indexing or unwrapping a missing tensor.

`greedy_token` rejects empty or non-finite logits and scans in token-ID order, replacing the current best only on a strictly larger logit so ties choose the lowest ID like llama.cpp. Positive-temperature sampling may retain the current algorithm after validating its probability sum is finite and positive.

At each Q/K head, dispatch exactly once:

```rust
match config.rope {
    Qwen3Rope::Neox => rope_neox(head, position[0], config.n_embd_head_k, config.freq_base),
    Qwen3Rope::Interleaved { sections, n_dims } => {
        rope_mrope_interleaved(head, position, sections, config.n_embd_head_k, config.freq_base, n_dims)
    }
}
```

For prompt rows, use supplied embeddings when present and token lookup otherwise. Generated rows always use token lookup and continue with `[next_position; 4]`. Before creating KV/scratch, validate `prompt_len > 0`, `max_new_tokens > 0`, `temperature.is_finite() && temperature >= 0.0`, all shapes, and `capacity = prompt_len + max_new_tokens <= model.context_length`. In `Qwen3Session::new`, checked-multiply `n_layer * capacity * n_embd_gqa` and every scratch length before allocation; allocate KV and scratch with `capacity`, never 512, 4096 or the full 65536 context.

- [ ] **Step 7: Replace both callers and delete duplicate Qwen3 code**

In `src/main.rs`, instantiate `Qwen3Model` and call `generate` for the ordinary Qwen2/Qwen3 path. Keep embedding, profiling and Qwen3.5 paths intact. In `src/bin/server.rs`, replace `Qwen3State`, `Qwen3Config`, `LayerWeightsOwned` and `generate_qwen3` with `ModelBackend::Qwen3(Arc<Qwen3Model>)`; adapt the existing response fields from `Qwen3Generation`.

The server match becomes:

```rust
match model {
    ModelBackend::Qwen3(model) => {
        let token_ids = server_prompt_tokens(model.tokenizer(), messages)?;
        let positions = qwen_text_positions(token_ids.len());
        model.generate(
            Qwen3Input { token_ids: &token_ids, positions: &positions, embeddings: None },
            Qwen3GenerateOptions { max_new_tokens: max_tokens, temperature },
        )
    }
    ModelBackend::Qwen35(state) => generate_qwen35(state, messages, max_tokens, temperature),
}
```

- [ ] **Step 8: Run decoder-focused and full regression tests**

Run:

```bash
cargo fmt --all -- --check
cargo test ops::tests::imrope_matches_pinned_llama_cpp_qwen3vl_vector -- --exact
cargo test qwen3::tests
cargo test --all-targets
```

Expected: all pass; `rg -n "fn generate_qwen3|struct Qwen3State|struct Qwen3Config" src/main.rs src/bin/server.rs` returns no duplicate definitions.

- [ ] **Step 9: Commit the shared decoder**

```bash
git add src/lib.rs src/ops.rs src/qwen3.rs src/main.rs src/bin/server.rs
git commit -m "refactor: share the qwen3 decoder"
```

### Task 3: Implement strict WAV decoding and pinned log-Mel preprocessing

**Files:**
- Modify: `Cargo.toml:12-25`
- Modify: `Cargo.lock`
- Modify: `src/qwen3a.rs`

**Interfaces:**
- Consumes: standard-library byte parsing and `rustfft::FftPlanner<f32>`.
- Produces: `decode_pcm16_wav(&[u8]) -> Result<Vec<f32>, AsrAudioError>` and `log_mel_windows(&[f32]) -> Result<Vec<MelWindow>, AsrAudioError>` for Task 5/6.

- [ ] **Step 1: Add failing RIFF boundary tests**

Build WAV bytes inside the test module with a small `pcm16_wav(samples, extra_chunks)` helper. Add tests covering: valid one-sample WAV (`-32768 -> -1.0`), unknown chunk skipping, odd unknown-chunk padding, truncated header/chunk, duplicate `fmt `, duplicate `data`, non-PCM tag, channels `!=1`, sample rate `!=16000`, bits `!=16`, block align `!=2`, byte rate `!=32000`, odd data length and zero samples.

The valid byte fixture starts with:

```rust
let bytes = pcm16_wav(&[-32768, 0, 32767], &[(b"JUNK", vec![7])]);
let samples = decode_pcm16_wav(&bytes).unwrap();
assert_eq!(samples, vec![-1.0, 0.0, 32767.0 / 32768.0]);
```

- [ ] **Step 2: Run WAV tests and confirm the decoder is absent**

Run:

```bash
cargo test qwen3a::tests::wav_
```

Expected: compilation fails because `decode_pcm16_wav` is not defined.

- [ ] **Step 3: Implement checked RIFF/WAVE parsing**

Add a small structured audio error and no WAV abstraction layer:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsrAudioError {
    Unsupported(String),
    Invalid(String),
}
```

Classify a wrong container/codec/channel/rate/bit-depth/block-align/byte-rate contract as `Unsupported`; classify a structurally corrupt RIFF (truncation, duplicate required chunk, odd PCM byte count), empty PCM, checked-arithmetic failure or non-finite DSP result as `Invalid`. This distinction is consumed unchanged by the HTTP 415/422 mapping in Tasks 6 and 9.

Parser algorithm:

```rust
if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
    return Err(AsrAudioError::Unsupported("expected RIFF/WAVE".into()));
}
let riff_len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
let riff_end = 8usize.checked_add(riff_len).ok_or_else(overflow)?;
if riff_end > bytes.len() { return Err(truncated("RIFF")); }

let mut offset = 12usize;
while offset < riff_end {
    let id_end = offset.checked_add(4).ok_or_else(overflow)?;
    let header_end = offset.checked_add(8).ok_or_else(overflow)?;
    let id = bytes.get(offset..id_end).ok_or_else(|| truncated("chunk header"))?;
    let len = read_u32(bytes, id_end)? as usize;
    let data_end = header_end.checked_add(len).ok_or_else(overflow)?;
    let padded_end = data_end.checked_add(len & 1).ok_or_else(overflow)?;
    let chunk = bytes.get(header_end..data_end).ok_or_else(|| truncated("chunk data"))?;
    match id {
        b"fmt " => parse_fmt_once(chunk, &mut format)?,
        b"data" => store_data_once(chunk, &mut pcm)?,
        _ => {}
    }
    if padded_end > riff_end { return Err(truncated("chunk padding")); }
    offset = padded_end;
}
```

Require all exact format fields before converting each `i16::from_le_bytes` to `sample as f32 / 32768.0`.

- [ ] **Step 4: Add failing DSP tests independent of FFT implementation**

Add tests named `silence_impulse_and_440hz_have_pinned_mel_shapes`, `short_audio_reflection_padding_uses_the_reference_zero_fallback` and `mel_windows_use_800_100_boundaries_and_zero_padding`. The first test computes one reference frame with an O(`400^2`) direct DFT in test code and compares the production raw log10 Mel frame within `1e-5`; it also asserts all production values are finite. The short-audio test uses one sample and 199 samples, asserting `samples[pad-i]`/`samples[n-2-i]` are used only when those indices exist and all other pad cells are exactly zero. Use exactly:

```rust
let silence = vec![0.0; 16_000];
let impulse = {
    let mut values = vec![0.0; 16_000];
    values[8_000] = 1.0;
    values
};
let tone: Vec<f32> = (0..16_000)
    .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 16_000.0).sin())
    .collect();
```

The second test feeds synthetic normalized Mel with effective frame counts `1`, `100`, `101`, `800`, and `801`, and asserts padded frame counts `100`, `100`, `200`, `800`, and `[800,100]`; every padded value must be exactly `0.0`.

- [ ] **Step 5: Add `rustfft` and implement the pinned preprocessing**

Change dependencies only as follows:

```toml
rustfft = "6.2"
```

Use these internal layouts and constants:

```rust
const SAMPLE_RATE: usize = 16_000;
const FFT_SIZE: usize = 400;
const HOP: usize = 160;
const MEL_BINS: usize = 128;
const WINDOW_FRAMES: usize = 800;
const CHUNK_FRAMES: usize = 100;

pub(crate) struct MelWindow {
    pub values: Vec<f32>, // [mel][padded_frame]
    pub frames: usize,
    pub valid_frames: usize,
}

struct LogMel {
    raw: Vec<f32>,        // [mel][frame]
    normalized: Vec<f32>, // [mel][frame]
    frames: usize,
}

pub fn decode_pcm16_wav(bytes: &[u8]) -> Result<Vec<f32>, AsrAudioError>;
pub(crate) fn log_mel_windows(samples: &[f32]) -> Result<Vec<MelWindow>, AsrAudioError>;
fn compute_log_mel(samples: &[f32]) -> Result<LogMel, AsrAudioError>;
```

Implement periodic Hann `0.5 * (1 - cos(2*pi*i/400))`, 200-sample reflection padding, 201-bin power spectrum, Slaney Mel conversion, area-normalized triangular filters, `max(power_sum, 5.960464477539063e-8).log10()`, one global maximum over the complete audio, then `(max(value, global_max - 8) + 4) / 4`. Match the pinned short-audio reference exactly: start padding reads original indices `200..1` where present and otherwise zero; end padding reads `n-2..n-201` where nonnegative and otherwise zero. Keep a private `compute_log_mel` result containing both raw and normalized arrays so the direct-DFT unit test and parity trace inspect the pre-normalization checkpoint without expanding the public API.

Set effective frames to:

```rust
let effective = stft_frames.min(samples.len() / HOP + 1);
```

Split at 800 frames and pad each window to `ceil(valid/100)*100`. Reject empty samples, checked-size overflow and any non-finite intermediate value. Under `parity-trace`, emit `asr.pcm`, `asr.raw_log_mel`, `asr.normalized_mel` and one `asr.padded_mel` checkpoint per window.

- [ ] **Step 6: Run DSP and dependency checks**

Run:

```bash
cargo test qwen3a::tests::wav_
cargo test qwen3a::tests::silence_impulse_and_440hz_have_pinned_mel_shapes -- --exact
cargo test qwen3a::tests::short_audio_reflection_padding_uses_the_reference_zero_fallback -- --exact
cargo test qwen3a::tests::mel_windows_use_800_100_boundaries_and_zero_padding -- --exact
cargo check --all-targets
```

Expected: all pass; `cargo tree -i rustfft` shows only the intended DSP dependency path.

- [ ] **Step 7: Commit preprocessing**

```bash
git add Cargo.toml Cargo.lock src/qwen3a.rs
git commit -m "feat: add qwen3 audio preprocessing"
```

### Task 4: Implement Qwen3A Conv2D and conv projection

**Files:**
- Modify: `src/qwen3a.rs`

**Interfaces:**
- Consumes: validated Conv F16/F32 tensors, `MelWindow`, existing half conversion helpers.
- Produces: private `Qwen3AudioModel::encode_convolution(&MelWindow) -> Result<AudioHidden, String>` and reusable audio linear loader used by Task 5.

- [ ] **Step 1: Add a hand-computable Conv2D layout test**

Test the internal stride-2, padding-1 kernel with one channel, one all-one 3x3 filter, input `1..=9` in NCHW `[1,1,3,3]`, and bias `0.5`. Assert the output shape `[1,1,2,2]` and pre-activation values:

```rust
assert_eq!(output, vec![12.5, 16.5, 24.5, 28.5]);
```

Add a second test that labels the final synthetic conv buffer by `(time, mel, channel)` and asserts the flattening formula exactly:

```rust
let feature = channel * 16 + mel;
assert_eq!(flattened[time * 7680 + feature], source[nchw_index(0, channel, mel, time)]);
```

- [ ] **Step 2: Run the Conv2D tests and verify they fail**

Run:

```bash
cargo test qwen3a::tests::conv2d_stride2_padding_and_layout_are_exact -- --exact
cargo test qwen3a::tests::conv_output_flattens_channel_then_mel_per_time -- --exact
```

Expected: compilation fails because the convolution helpers do not exist.

- [ ] **Step 3: Load typed audio weights without a generic layer framework**

Add private fixed-purpose structs:

```rust
struct F16Tensor {
    bytes: &'static [u8],
    dims: Vec<u64>,
}

struct AudioLinear {
    weight: &'static [u8],
    kind: GGMLType,
    input: usize,
    output: usize,
    bias: Vec<f32>,
}

struct Conv2dWeights {
    weight: F16Tensor,
    bias: Vec<f32>,
    input_channels: usize,
    output_channels: usize,
}

struct AudioHidden {
    values: Vec<f32>, // [token][896]
    tokens: usize,
}
```

The audio model must own `Arc<dyn TensorSource>` before its `'static` slices, using the same single unsafe lifetime helper/invariant as `Qwen3Model`. `AudioLinear` accepts only the F16 `a.conv_out.weight` or Q8_0 Transformer/projector matrices specified in Task 1; no extra GGML types.

- [ ] **Step 4: Implement the three fixed convolution stages**

For each 100-frame chunk, view Mel as NCHW `[1,1,128,100]`, run three kernel-3/stride-2/padding-1 convolutions and GELU-erf after bias. Enforce these stage shapes:

```text
[1,1,128,100] -> [1,480,64,50]
               -> [1,480,32,25]
               -> [1,480,16,13]
```

Flatten to 13 rows of 7680 with `feature=channel*16+mel`, multiply F16 `a.conv_out.weight [7680,896]` without bias, and concatenate chunks in time order. Under `parity-trace`, emit `asr.after_conv_blocks` before flattening and `asr.after_conv_out` after projection.

Use checked multiplication for every allocation and reject a `MelWindow.frames` value not divisible by 100.

- [ ] **Step 5: Add and pass a shape/finite-value integration test**

With test weights filled by small finite F16 values, pass one zero `[128,100]` window and assert:

```rust
let hidden = model.encode_convolution(&window).unwrap();
assert_eq!(hidden.tokens, 13);
assert_eq!(hidden.values.len(), 13 * 896);
assert!(hidden.values.iter().all(|value| value.is_finite()));
```

Run:

```bash
cargo test qwen3a::tests::conv2d_stride2_padding_and_layout_are_exact -- --exact
cargo test qwen3a::tests::conv_output_flattens_channel_then_mel_per_time -- --exact
cargo test qwen3a::tests::one_mel_chunk_produces_thirteen_hidden_rows -- --exact
```

Expected: all pass.

- [ ] **Step 6: Commit convolution support**

```bash
git add src/qwen3a.rs
git commit -m "feat: add qwen3 audio convolution"
```

### Task 5: Implement the audio Transformer and projector

**Files:**
- Modify: `src/qwen3a.rs`

**Interfaces:**
- Consumes: `AudioHidden`, 18 validated audio blocks, positional embedding and Q8_0/F32 projector tensors.
- Produces: `Qwen3AudioModel::from_source`, `Qwen3AudioModel::encode` and `AudioEmbeddings` for Task 6.

- [ ] **Step 1: Add small numeric tests for every new primitive**

Add these independent checks:

```rust
#[test]
fn layer_norm_uses_weight_bias_and_population_variance() {
    let mut output = [0.0; 2];
    layer_norm(&[1.0, 3.0], &[2.0, 0.5], &[0.25, -0.25], 0.0, &mut output).unwrap();
    assert_eq!(output, [-1.75, 0.25]);
}

#[test]
fn gelu_erf_keeps_zero_and_matches_fixed_values() {
    assert_eq!(gelu_erf(0.0), 0.0);
    assert!((gelu_erf(1.0) - 0.841_344_7).abs() < 1e-6);
    assert!((gelu_erf(-1.0) + 0.158_655_26).abs() < 1e-6);
}

#[test]
fn full_attention_is_bidirectional() {
    let output = full_attention(
        &[1.0, 0.0, 0.0, 1.0],
        &[1.0, 0.0, 0.0, 1.0],
        &[1.0, 2.0, 3.0, 4.0],
        2, 1, 2,
    ).unwrap();
    assert!((output[0] - 1.660_476_9).abs() < 1e-5);
    assert!((output[1] - 2.660_477).abs() < 1e-5);
    assert!((output[2] - 2.339_523).abs() < 1e-5);
    assert!((output[3] - 3.339_523).abs() < 1e-5);
}
```

Also add `audio_ffn_is_up_gelu_down_not_gated` using a 32-wide Q8_0 identity-like fixture; assert there is exactly one up activation and no elementwise gate multiplication.

- [ ] **Step 2: Run primitive tests and confirm missing behavior**

Run:

```bash
cargo test qwen3a::tests::layer_norm_uses_weight_bias_and_population_variance -- --exact
cargo test qwen3a::tests::gelu_erf_keeps_zero_and_matches_fixed_values -- --exact
cargo test qwen3a::tests::full_attention_is_bidirectional -- --exact
cargo test qwen3a::tests::audio_ffn_is_up_gelu_down_not_gated -- --exact
```

Expected: compilation fails for the new helpers.

- [ ] **Step 3: Define the full audio model API and load all weights strictly**

Add:

```rust
pub(crate) struct AudioEmbeddings {
    pub values: Vec<f32>, // [token][projection]
    pub tokens: usize,
    pub dim: usize,
}

struct LayerNormWeights {
    weight: Vec<f32>,
    bias: Vec<f32>,
}

struct AudioTransformerLayer {
    ln1: LayerNormWeights,
    q: AudioLinear,
    k: AudioLinear,
    v: AudioLinear,
    output: AudioLinear,
    ln2: LayerNormWeights,
    up: AudioLinear,
    down: AudioLinear,
}

pub struct Qwen3AudioModel {
    source: Arc<dyn TensorSource>,
    config: Qwen3AudioConfig,
    pool: Arc<ComputePool>,
    conv: [Conv2dWeights; 3],
    conv_out: AudioLinear,
    positions: Vec<f32>,
    layers: Vec<AudioTransformerLayer>,
    post_ln: LayerNormWeights,
    projector_1: AudioLinear,
    projector_2: AudioLinear,
}

impl Qwen3AudioModel {
    pub fn from_source(
        source: Arc<dyn TensorSource>,
        pool: Arc<ComputePool>,
    ) -> Result<Self, String>;

    pub(crate) fn encode(&self, windows: &[MelWindow]) -> Result<AudioEmbeddings, String>;

    pub fn config(&self) -> Qwen3AudioConfig;
}
```

The numeric helpers used above have these private contracts:

```rust
fn layer_norm(
    input: &[f32],
    weight: &[f32],
    bias: &[f32],
    epsilon: f32,
    output: &mut [f32],
) -> Result<(), String>;

fn full_attention(
    query: &[f32],
    key: &[f32],
    value: &[f32],
    tokens: usize,
    heads: usize,
    head_dim: usize,
) -> Result<Vec<f32>, String>;
```

Both reject shape mismatch and non-finite inputs/results. `full_attention` applies `1/sqrt(head_dim)` and a stable per-query softmax over all `tokens`; it does not accept or construct a causal mask.

`from_source` first calls `Qwen3AudioConfig::from_source`, then loads every Task 1 tensor with exact byte-length checks. F32 vectors must decode little-endian and reject non-finite values. Keep biases as `Vec<f32>`; keep matrix bytes mapped.

Implement GELU-erf without another dependency using the Abramowitz-Stegun 7.1.26 `erf` approximation (maximum absolute error below `1.5e-7`), then `0.5*x*(1+erf(x/sqrt(2)))`:

```rust
fn erf_approx(value: f32) -> f32 {
    let sign = if value < 0.0 { -1.0 } else { 1.0 };
    let x = value.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let polynomial = (((((1.061_405_4 * t - 1.453_152_1) * t) + 1.421_413_8) * t
        - 0.284_496_72) * t + 0.254_829_6) * t;
    sign * (1.0 - polynomial * (-x * x).exp())
}

fn gelu_erf(value: f32) -> f32 {
    0.5 * value * (1.0 + erf_approx(value * std::f32::consts::FRAC_1_SQRT_2))
}
```

The fixed llama.cpp parity thresholds, rather than bit equality, govern accumulated activation differences.

- [ ] **Step 4: Implement one-window bidirectional Transformer execution**

For each window independently:

1. Call `encode_convolution`.
2. Add `a.position_embd.weight[position]` to every row with `position = token_index % 13`.
3. For each of 18 layers, apply LN1, Q/K/V+bias, reshape `[token,14,64]`, full scaled dot-product attention over all tokens, O+bias, residual, LN2, up+bias, GELU-erf, down+bias, residual.
4. Apply `a.post_ln`.
5. Apply `mm.a.mlp.1 + bias`, GELU-erf, then `mm.a.mlp.2 + bias`.
6. Append projected rows in window order.

Use the existing `ComputePool` for output-row partitions of Q8_0 matrices. Do not parallelize separate calls to the same pool. Mark the server-wide serialization ceiling only where the mutex is introduced in Task 9.

Under `parity-trace`, emit `asr.transformer_layer_0`, `asr.after_transformer` and `asr.projected` with `[tokens, hidden]` or `[tokens,1024]` shapes.

- [ ] **Step 5: Add window/token/reset tests**

Add a synthetic model test whose position rows encode their row index. For two 100-frame chunks, assert 26 output rows and position application `0..12,0..12`. For 900 effective frames, assert the encoder is invoked as two independent windows producing `104 + 13 = 117` tokens and never constructs attention over 117 rows.

Expose an internal test counter around `encode_window` only under `#[cfg(test)]`; production API remains unchanged.

- [ ] **Step 6: Run audio encoder tests**

Run:

```bash
cargo test qwen3a::tests
cargo check --all-targets --features parity-trace
```

Expected: all tests pass and trace-enabled code compiles.

- [ ] **Step 7: Commit the encoder/projector**

```bash
git add src/qwen3a.rs
git commit -m "feat: add qwen3 audio transformer"
```

### Task 6: Build the shared ASR orchestration contract

**Files:**
- Create: `src/asr.rs`
- Modify: `src/lib.rs:1-45`
- Modify: `src/qwen3.rs`
- Modify: `src/tokenizer.rs`

**Interfaces:**
- Consumes: `Qwen3Model`, `Qwen3AudioModel`, strict WAV/Mel functions and tokenizer semantic IDs.
- Produces: `AsrRuntime`, `TranscriptionOptions`, `Transcription`, `AsrError`, `normalize_language` and `transcribe_wav` used unchanged by CLI/server.

- [ ] **Step 1: Add exhaustive language normalization tests**

Use one static table containing exactly these 30 pairs:

```rust
const LANGUAGES: &[(&str, &str)] = &[
    ("Chinese", "zh"), ("English", "en"), ("Cantonese", "yue"),
    ("Arabic", "ar"), ("German", "de"), ("French", "fr"),
    ("Spanish", "es"), ("Portuguese", "pt"), ("Indonesian", "id"),
    ("Italian", "it"), ("Korean", "ko"), ("Russian", "ru"),
    ("Thai", "th"), ("Vietnamese", "vi"), ("Japanese", "ja"),
    ("Turkish", "tr"), ("Hindi", "hi"), ("Malay", "ms"),
    ("Dutch", "nl"), ("Swedish", "sv"), ("Danish", "da"),
    ("Finnish", "fi"), ("Polish", "pl"), ("Czech", "cs"),
    ("Filipino", "fil"), ("Persian", "fa"), ("Greek", "el"),
    ("Romanian", "ro"), ("Hungarian", "hu"), ("Macedonian", "mk"),
];
```

For every row assert trim and ASCII-case-insensitive canonical/code inputs return the canonical static string. Assert `None`, `""`, and whitespace return auto (`None`). Assert `auto`, `en-US`, `zh-Hans`, `cmn`, `tl`, `cn`, `jp`, `Hebrew`, and every detected-only dialect are rejected by the core normalizer.

- [ ] **Step 2: Add failing prompt, injection and output-protocol tests**

Create a small tokenizer fixture containing ChatML and all audio literals. Tests must assert:

- empty system framing is retained;
- optional system prompt appears only between `system\n` and `<|im_end|>`;
- a prompt containing any tokenizer special control literal is rejected;
- exactly N `<|audio_pad|>` token IDs are generated between audio start/end;
- forced language appends `language English<asr_text>` to assistant prefill;
- every returned position equals `[index;4]`;
- audio replacement changes only the N pad rows and rejects count/dimension mismatches;
- auto output parses `language English<asr_text>Hello`;
- auto output permits whitespace/newlines between the detected language and `<asr_text>`;
- `language None<asr_text>` is accepted only with an empty transcript;
- missing `language `, missing `<asr_text>`, unknown detected language or nonempty `None` output is rejected;
- forced output only trims framing/outer whitespace and does not rewrite repeated words.

- [ ] **Step 3: Define the exact public ASR API**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsrErrorKind {
    UnsupportedAudio,
    Unprocessable,
    Internal,
}

#[derive(Debug)]
pub struct AsrError {
    pub kind: AsrErrorKind,
    pub message: String,
}

impl std::fmt::Display for AsrError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AsrError {}

#[derive(Debug, Clone)]
pub struct TranscriptionOptions {
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub max_new_tokens: usize,
}

impl Default for TranscriptionOptions {
    fn default() -> Self {
        Self { language: None, prompt: None, max_new_tokens: 256 }
    }
}

pub struct Transcription {
    pub text: String,
    pub language: Option<String>,
    pub token_ids: Vec<u32>,
    pub prompt_tokens: usize,
    pub audio_tokens: usize,
}

pub struct AsrRuntime {
    decoder: Arc<Qwen3Model>,
    audio: Qwen3AudioModel,
}

impl AsrRuntime {
    pub fn new(
        decoder: Arc<Qwen3Model>,
        audio_source: Arc<dyn TensorSource>,
    ) -> Result<Self, AsrError>;

    pub fn transcribe_wav(
        &self,
        wav: &[u8],
        options: &TranscriptionOptions,
    ) -> Result<Transcription, AsrError>;
}

pub fn normalize_language(value: Option<&str>) -> Result<Option<&'static str>, AsrError>;
```

Keep orchestration helpers private with these exact contracts so the unit tests do not need real weights:

```rust
struct AsrPrompt {
    token_ids: Vec<u32>,
    positions: Vec<[usize; 4]>,
}

fn build_asr_prompt(
    tokenizer: &BPETokenizer,
    decoder_context: usize,
    audio_tokens: usize,
    system_prompt: Option<&str>,
    forced_language: Option<&'static str>,
) -> Result<AsrPrompt, AsrError>;

fn replace_audio_embeddings(
    decoder: &Qwen3Model,
    prompt: &AsrPrompt,
    audio: &AudioEmbeddings,
) -> Result<Vec<f32>, AsrError>;

fn parse_model_output(
    output: &str,
    forced_language: Option<&'static str>,
) -> Result<(String, Option<String>), AsrError>;
```

Add `BPETokenizer::contains_special_literal(&self, text: &str) -> bool`, implemented as one scan over its already-built `special_tokens`, and use it to reject system prompts that could break framing. Do not maintain a second literal list in `asr.rs`.

The auto-output parser accepts the 30 canonical names plus exactly these detected-only labels:

```rust
const DETECTED_ONLY_LANGUAGES: &[&str] = &[
    "Anhui", "Dongbei", "Fujian", "Gansu", "Guizhou", "Hebei", "Henan",
    "Hubei", "Hunan", "Jiangxi", "Ningxia", "Shandong", "Shaanxi", "Shanxi",
    "Sichuan", "Tianjin", "Yunnan", "Zhejiang",
    "Cantonese (Hong Kong accent)", "Cantonese (Guangdong accent)",
    "Wu language", "Minnan language",
];
```

- [ ] **Step 4: Implement prompt construction and strict embedding replacement**

Reject `audio_tokens >= decoder.config().n_ctx` before allocating the repeated placeholder string. Then construct this exact string before tokenization with `parse_special=true`:

```rust
let prompt_text = format!(
    "<|im_start|>system\n{}<|im_end|>\n\
     <|im_start|>user\n<|audio_start|>{}<|audio_end|><|im_end|>\n\
     <|im_start|>assistant\n{}",
    system_prompt,
    "<|audio_pad|>".repeat(audio_tokens),
    forced_language.map(|name| format!("language {name}<asr_text>")).unwrap_or_default(),
);
```

Resolve all six required semantic IDs through `special_token_id`; do not embed production token numbers. Tokenize with `EncodeOptions { add_special: false, parse_special: true }`, because the complete ChatML framing is already present. Call `decoder.embed_tokens`, collect the indices whose token ID is `audio_pad`, require exactly `audio.tokens`, require `audio.dim == decoder.config().n_embd`, and copy one `[1024]` projected row into each selected embedding row. Audio start/end keep ordinary token embeddings.

- [ ] **Step 5: Implement `transcribe_wav` as the only ASR orchestration path**

Execution order must be:

```text
normalize language and validate max_new_tokens/prompt
decode_pcm16_wav
log_mel_windows
audio.encode
build/tokenize prompt
validate prompt + generation <= decoder context
embed tokens and replace only audio_pad rows
decoder.generate with temperature 0.0
parse auto-language protocol or use forced canonical language
return Transcription
```

Map `AsrAudioError::Unsupported` to `AsrErrorKind::UnsupportedAudio`, `AsrAudioError::Invalid` plus invalid language/prompt/context to `Unprocessable`, and model weight/inference/non-finite/model-output-protocol failures to `Internal`. Context rejection must happen before `Qwen3Session::new`.

Under `parity-trace`, emit exact `asr.prompt_ids`, flattened `[n_tokens,4]` `asr.positions`, `asr.decoder_first_logits` and `asr.generated_ids` checkpoints through the shared decoder.

- [ ] **Step 6: Run all orchestration tests**

Run:

```bash
cargo test asr::tests
cargo test qwen3::tests
cargo check --all-targets --features parity-trace
```

Expected: all pass, including 30-language exhaustive cases and every prompt/shape/context error.

- [ ] **Step 7: Commit the shared ASR core**

```bash
git add src/lib.rs src/qwen3.rs src/tokenizer.rs src/asr.rs
git commit -m "feat: add shared qwen3 ASR core"
```

### Task 7: Prove raw GGUF and GGUFRS source equivalence

**Files:**
- Modify: `src/asr.rs`
- Modify: `src/ggufrs.rs`

**Interfaces:**
- Consumes: `GgufrsFile`, `open_model_source`, `ComponentRole::{Llm,Mmproj}`, `AsrRuntime`.
- Produces: `open_bundled_audio_source` for CLI/server and ignored real-model equivalence tests.

- [ ] **Step 1: Add source-resolution tests**

Define:

```rust
pub fn open_bundled_audio_source(
    model_path: &Path,
) -> Result<Option<Arc<dyn TensorSource>>, String>;
```

Tests must assert: raw GGUF returns `Ok(None)`; GGUFRS without role 2 returns `Ok(None)`; GGUFRS with vision role 2 returns `Ok(None)`; GGUFRS with `clip.has_audio_encoder=true` and `qwen3a` returns the `Mmproj` component; `clip.has_audio_encoder=true` with a missing/wrong projector type returns an error; malformed GGUFRS returns an error. Explicit `--mmproj` paths are intentionally handled by `open_model_source(path, Mmproj)` in callers and are not hidden by this optional helper.

- [ ] **Step 2: Implement bundled source lookup without changing raw loader semantics**

Read exactly the first eight bytes. If they are not `b"GGUFRS\0\0"`, return `Ok(None)`; otherwise open the package with `GgufrsFile::open`, inspect `component_id(ComponentRole::Mmproj)`, and load it once. Missing/false `clip.has_audio_encoder` is a vision component and returns `Ok(None)`; when that flag is true, require `clip.audio.projector_type=qwen3a` and `validate_qwen3a_source` or return an error. Do not use file extensions. Once the GGUFRS magic matched, keep every parse/checksum/contract error as an error rather than treating corruption as raw GGUF.

- [ ] **Step 3: Add an ignored real-model byte/transcription equivalence test**

Add `qwen3_asr_raw_and_ggufrs_are_byte_and_transcript_equivalent` inside `src/ggufrs.rs`'s test module, marked ignored and requiring all three Qwen env vars. Keeping it in this module allows comparison through the private `LoadedComponent::tensor_infos` map without adding a production iterator. Guard the one exact temporary output with:

```rust
struct RemoveOnDrop(PathBuf);
impl Drop for RemoveOnDrop {
    fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
}
```

The test must:

1. verify both model hashes and WAV hash before work;
2. export a temporary `.ggufrs` with both fixed GGUFs;
3. `verify_all()` the package;
4. compare sorted metadata entries, every tensor name/shape/type and every tensor byte slice for LLM and mmproj;
5. create raw and packaged `AsrRuntime` with one thread;
6. transcribe the same WAV with forced `English`, empty prompt and 256 tokens;
7. assert prompt count, audio token count, generated IDs, language and text are exact-equal;
8. remove only the test-created temporary package through its fixture drop guard.

- [ ] **Step 4: Run cheap source tests and compile the ignored gate**

Run:

```bash
cargo test asr::tests::bundled_audio_source_resolution_is_strict -- --exact
cargo test --no-run ggufrs::tests::qwen3_asr_raw_and_ggufrs_are_byte_and_transcript_equivalent
```

Expected: pass/compile without requiring local models.

- [ ] **Step 5: Run the real equivalence gate**

Run:

```bash
QWEN3_ASR_MODEL=/absolute/path/Qwen3-ASR-0.6B-Q8_0.gguf \
QWEN3_ASR_MMPROJ=/absolute/path/mmproj-Qwen3-ASR-0.6B-Q8_0.gguf \
QWEN3_ASR_WAV=/absolute/path/asr_en-pcm16-16khz.wav \
cargo test --release ggufrs::tests::qwen3_asr_raw_and_ggufrs_are_byte_and_transcript_equivalent -- --ignored --exact --nocapture
```

Expected: PASS; retain elapsed time and peak RSS in the execution report, not in a hard-coded assertion.

- [ ] **Step 6: Commit source equivalence support**

```bash
git add src/asr.rs src/ggufrs.rs
git commit -m "test: verify qwen3 ASR model sources"
```

### Task 8: Expose full ASR through the CLI

**Files:**
- Modify: `src/main.rs:567-760, 2974-3042`

**Interfaces:**
- Consumes: `Qwen3Model`, `AsrRuntime`, `open_bundled_audio_source`, `TranscriptionOptions`.
- Produces: `--audio`, `--language`, mode validation and transcript-only stdout.

- [ ] **Step 1: Extract parseable CLI options and add failing combination tests**

Replace mode booleans only where necessary with a small parsed struct:

```rust
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
    embedding: bool,
    embedding_output: EmbeddingOutput,
    dump_logits: bool,
    bench: bool,
    profile: bool,
    kv_format: KvFormat,
}
```

Keep `--n-gen` as an alias that writes the same `max_tokens` field, and keep the existing text/embedding/image behavior for every non-audio invocation.

Add `validate_cli_options(&CliOptions) -> Result<(), String>` tests for all argument-only invalid pairs: audio with image, embedding, dump-logits, bench or profile; audio with explicit nonzero temp; audio max tokens `0`; `--language` without audio; and a missing audio path value. Assert prompt remains allowed. Assert language `auto` is converted to `None` only by the CLI before building `TranscriptionOptions`; the core still rejects literal `auto`. Whether a model path is raw or bundled is checked by Task 7's eight-byte source resolver after argument validation, because it cannot be inferred from options alone.

- [ ] **Step 2: Run CLI validation tests and confirm flags are unknown**

Run:

```bash
cargo test cli_tests::asr_cli_rejects_conflicting_modes_before_model_load -- --exact
cargo test cli_tests::asr_cli_defaults_are_greedy_and_256_tokens -- --exact
```

Expected: FAIL because audio/language options and validation are absent.

- [ ] **Step 3: Parse and validate ASR arguments before opening model files**

Add `--audio <path>` and `--language <value>` to the existing manual parser. Track `max_tokens` and `temperature` as `Option` so ASR can distinguish the text defaults from explicit values. Resolve defaults after mode validation:

```rust
let max_tokens = options.max_tokens.unwrap_or(if options.audio.is_some() { 256 } else { 128 });
let temperature = options.temperature.unwrap_or(if options.audio.is_some() { 0.0 } else { 0.6 });
```

Reject every invalid combination with exit code 2 before `open_model_source` or reading the WAV.

- [ ] **Step 4: Add one CLI ASR call path**

Implement `run_asr_cli(options: &CliOptions) -> Result<(), String>`:

```rust
let llm_source: Arc<dyn TensorSource> = Arc::from(
    open_model_source(&options.model, ComponentRole::Llm).map_err(|error| error.to_string())?
);
let tokenizer = Arc::new(BPETokenizer::from_gguf_metadata(|key| llm_source.metadata(key).cloned())?);
let available = std::thread::available_parallelism()
    .map(std::num::NonZeroUsize::get)
    .unwrap_or(1);
let pool = Arc::new(ComputePool::new(resolve_thread_count(options.threads, available)));
let decoder = Arc::new(Qwen3Model::from_source(llm_source, tokenizer, pool)?);
if decoder.config().architecture != "qwen3vl" {
    return Err("--audio requires a qwen3vl decoder".into());
}
let audio_source = match options.mmproj.as_deref() {
    Some(path) => Arc::from(
        open_model_source(path, ComponentRole::Mmproj).map_err(|error| error.to_string())?
    ),
    None => open_bundled_audio_source(&options.model)?
        .ok_or("raw GGUF ASR requires --mmproj")?,
};
let runtime = AsrRuntime::new(decoder, audio_source).map_err(|error| error.to_string())?;
let wav = std::fs::read(options.audio.as_ref().unwrap()).map_err(|error| error.to_string())?;
let result = runtime
    .transcribe_wav(&wav, &transcription_options)
    .map_err(|error| error.to_string())?;
println!("{}", result.text);
```

Use the public read-only `Qwen3Config::architecture` field defined in Task 2. Send model-loading, elapsed/token count and diagnostics to stderr; stdout receives only the final transcript plus newline.

- [ ] **Step 5: Update help and run CLI tests**

Add the exact usage example from the design to `run_self_test`/usage output. Run:

```bash
cargo test cli_tests::asr_cli_rejects_conflicting_modes_before_model_load -- --exact
cargo test cli_tests::asr_cli_defaults_are_greedy_and_256_tokens -- --exact
cargo test --all-targets
```

Expected: all pass.

- [ ] **Step 6: Run the fixed real CLI smoke**

Run:

```bash
cargo run --release --bin rust-model-inference -- \
  --model /absolute/path/Qwen3-ASR-0.6B-Q8_0.gguf \
  --mmproj /absolute/path/mmproj-Qwen3-ASR-0.6B-Q8_0.gguf \
  --audio /absolute/path/asr_en-pcm16-16khz.wav \
  --language English \
  --max-tokens 256 \
  --threads 1 \
  > /tmp/rmi-asr-cli.txt
```

Expected: exit 0; `/tmp/rmi-asr-cli.txt` contains exactly one transcript line and no diagnostics.

- [ ] **Step 7: Commit CLI support**

```bash
git add src/main.rs
git commit -m "feat: expose qwen3 ASR in the CLI"
```

### Task 9: Add the multipart transcription endpoint and global inference lock

**Files:**
- Modify: `Cargo.toml:17`
- Modify: `Cargo.lock`
- Modify: `src/bin/server.rs`

**Interfaces:**
- Consumes: `Arc<Qwen3Model>`, optional `Arc<AsrRuntime>`, `TranscriptionOptions`.
- Produces: `POST /v1/audio/transcriptions`, 32 MiB boundary, OpenAI JSON and one lock shared with chat.

- [ ] **Step 1: Add pure request-field and error-mapping tests**

Define an internal request accumulator:

```rust
#[derive(Default)]
struct TranscriptionFields {
    file: Option<Vec<u8>>,
    model: Option<String>,
    language: Option<String>,
    prompt: Option<String>,
    max_tokens: Option<usize>,
    response_format: Option<String>,
    stream: Option<bool>,
}
```

Test missing/duplicate file, duplicate scalar field, unknown field, invalid UTF-8, `max_tokens=0`, wrong model name, response format other than `json`, stream other than `false`, unsupported WAV mapping to 415, user/context error mapping to 422, missing runtime mapping to 503 and internal inference mapping to 500. Serialize the success struct and assert exact JSON `{"text":"hello"}`.

- [ ] **Step 2: Add a concurrency test for one shared lock**

Add one private `AppState::run_inference<T>(&self, call: impl FnOnce() -> T) -> T` method that acquires `inference_lock` with poison recovery, runs `call`, then releases the guard. Both chat and ASR blocking closures must call this exact method. In the test, spawn one simulated chat and one simulated ASR call through `run_inference`, use `AtomicUsize` active/peak counters inside the supplied closures, synchronize starts with a barrier, and assert `peak == 1`; parsing/setup increments a separate counter before `run_inference` to prove it is outside the critical section. This tests the production lock path rather than a standalone mutex example.

- [ ] **Step 3: Run server tests and confirm multipart/runtime fields are absent**

Run:

```bash
cargo test --bin server transcription_
cargo test --bin server chat_and_asr_share_one_inference_lock -- --exact
```

Expected: compilation fails for the new request parser and state.

- [ ] **Step 4: Enable Axum multipart and extend state**

Change only:

```toml
axum = { version = "0.8", features = ["ws", "multipart"] }
```

Use:

```rust
#[derive(Clone)]
struct AppState {
    model: Arc<ModelBackend>,
    asr: Option<Arc<AsrRuntime>>,
    model_name: String,
    inference_lock: Arc<std::sync::Mutex<()>>,
}

#[derive(Serialize)]
struct TranscriptionResponse {
    text: String,
}
```

Parse server `--mmproj`; explicit mmproj failure stops startup. For a Qwen3VL model, explicit audio mmproj takes precedence over bundled role 2. A bundled vision mmproj or a raw model without `--mmproj` leaves `state.asr=None`, so chat remains available and ASR returns 503.

- [ ] **Step 5: Parse multipart completely before inference**

Add handler signature:

```rust
async fn audio_transcriptions(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> impl IntoResponse;
```

Loop through fields, reject unknown or repeated names, call `field.bytes().await` for `file` and `field.text().await` for scalars, then validate the complete accumulator. Scope the upload limit to this route:

```rust
.route(
    "/v1/audio/transcriptions",
    post(audio_transcriptions).layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024)),
)
```

Do not write temporary files or log multipart/file bytes.

- [ ] **Step 6: Hold the same lock for complete chat and ASR model calls**

In every `spawn_blocking` closure, invoke the shared helper:

```rust
state.run_inference(|| generate(/* chat */))
state.run_inference(|| runtime.transcribe_wav(/* ASR */))
```

`run_inference` acquires immediately before `generate`/`transcribe_wav` and releases only after that call returns. Streaming chat response setup and all multipart parsing stay outside. Add a Ponytail ceiling comment at the state field:

```rust
// ponytail: one global model lock; split per-runtime only if measured concurrent throughput requires it.
```

- [ ] **Step 7: Map responses to the exact status contract**

Return:

```text
400 malformed/missing/duplicate/unknown multipart field
413 body limit rejection (Axum layer)
415 AsrErrorKind::UnsupportedAudio
422 invalid language/model/format/stream/audio content/context
503 no audio runtime
500 AsrErrorKind::Internal or blocking worker join failure
200 Json(TranscriptionResponse { text })
```

When `model` is present, compare it to the loaded model path's `file_stem`, not `general.name`.

- [ ] **Step 8: Run server and full tests**

Run:

```bash
cargo test --bin server transcription_
cargo test --bin server chat_and_asr_share_one_inference_lock -- --exact
cargo test --all-targets
```

Expected: all pass.

- [ ] **Step 9: Run a real CLI/server equality smoke**

Start the server with the fixed models and one thread, then call:

```bash
curl --fail-with-body http://127.0.0.1:8080/v1/audio/transcriptions \
  -F file=@/absolute/path/asr_en-pcm16-16khz.wav \
  -F model=Qwen3-ASR-0.6B-Q8_0 \
  -F language=English \
  -F response_format=json \
  -F stream=false \
  -F max_tokens=256 \
  > /tmp/rmi-asr-server.json
```

Extract `.text` with the standard library test helper or `jq -r .text` if installed and compare byte-for-byte to `/tmp/rmi-asr-cli.txt` after removing only the final newline from each transport representation.

- [ ] **Step 10: Commit server support**

```bash
git add Cargo.toml Cargo.lock src/bin/server.rs
git commit -m "feat: add ASR transcription endpoint"
```

### Task 10: Establish fixed llama.cpp stage parity and final gates

**Files:**
- Modify: `src/asr.rs`
- Modify: `src/qwen3.rs`
- Modify: `src/qwen3a.rs`
- Modify: `src/parity_trace.rs`

**Interfaces:**
- Consumes: all named Rust checkpoints, fixed llama.cpp checkout and fixed model/audio env vars.
- Produces: one ignored numeric parity test plus exact token/text gate and final evidence.

- [ ] **Step 1: Add metric unit tests before the comparator**

Under `#[cfg(all(test, feature = "parity-trace"))]`, add pure helpers and known-vector tests for:

```rust
fn nrmse(got: &[f32], reference: &[f32]) -> Result<f64, String>;
fn cosine(got: &[f32], reference: &[f32]) -> Result<f64, String>;
fn p99_abs(got: &[f32], reference: &[f32]) -> Result<f32, String>;
fn p99_scaled_abs(got: &[f32], reference: &[f32]) -> Result<f64, String>;
fn row_cosines(got: &[f32], reference: &[f32], columns: usize) -> Result<Vec<f64>, String>;
fn top_k(values: &[f32], k: usize) -> Result<Vec<usize>, String>;
```

Use `[1.0,2.0,3.0]` vs `[1.0,2.0,4.0]` to assert hand-calculated norms/cosine, and tied logits to assert deterministic ascending-token-ID tie breaking. Every helper rejects empty, non-finite, length/shape mismatch, zero `columns`, or invalid `k` before calculation.

- [ ] **Step 2: Add a failing ignored trace comparator**

Add `qwen3_asr_matches_pinned_llama_cpp_trace`, ignored, requiring all five environment variables. It must load the reference JSONL/binary sidecars, run Rust into a fresh trace path, and apply exact checkpoint names:

```text
asr.pcm
asr.raw_log_mel
asr.normalized_mel
asr.padded_mel
asr.after_conv_blocks
asr.after_conv_out
asr.transformer_layer_0
asr.after_transformer
asr.projected
asr.prompt_ids
asr.positions
asr.decoder_first_logits
asr.generated_ids
```

Before comparison, assert model/WAV SHA-256, shapes, chunk count, 100/800 boundaries, zero padding, token count/order, chunk position resets and embedding-slot indices exactly.

- [ ] **Step 3: Apply the fixed numeric hard gates**

The comparator must use these limits verbatim:

```text
PCM: sample count and F32 bytes exact
raw log10 Mel: p99_abs <= 3e-4, max_abs <= 1e-3
normalized Mel: p99_abs <= 1e-4, max_abs <= 5e-4
after_conv_blocks: cosine >= 0.9999, NRMSE <= 1.5e-2, p99_scaled_abs <= 2e-2
after_conv_out: cosine >= 0.9999, NRMSE <= 2e-2, p99_scaled_abs <= 2e-2
Transformer layer 0: cosine >= 0.9999
final Transformer/projected: global cosine >= 0.999, NRMSE <= 4e-2, nonzero-row p01 cosine >= 0.99
centered logits: cosine >= 0.9995, NRMSE <= 3e-2, p99_abs <= 0.10, max_abs <= 0.30, top-10 overlap >= 9/10
```

For first-token ranking, require exact argmax when reference top1-top2 margin is at least `0.10`; otherwise require Rust argmax in reference top-3. The fixed acceptance WAV must ultimately have exact generated token IDs and transcript; if its margin is unstable, it cannot serve as the acceptance fixture.

- [ ] **Step 4: Build a deterministic local llama.cpp oracle at the pinned SHA**

Verify, do not update, the checkout:

```bash
test "$(git -C /tmp/rmi-asr-yA0sai/llama.cpp rev-parse HEAD)" = 9558fa44c92746a58dd07ad1bf0c889715b938a6
cmake -S /tmp/rmi-asr-yA0sai/llama.cpp -B /tmp/rmi-asr-yA0sai/llama.cpp/build \
  -DGGML_METAL=OFF -DLLAMA_CURL=OFF -DCMAKE_BUILD_TYPE=Release
cmake --build /tmp/rmi-asr-yA0sai/llama.cpp/build --target llama-mtmd-cli -j
```

In that temporary checkout only, gate dump writes on `LLAMA_ASR_PARITY_TRACE`. Use the existing Qwen3A graph callback names for Mel/Conv/Transformer/projected, dump decoded PCM before preprocessing, prompt token IDs and four-axis positions before decode, and first-step pre-sampling logits/generated IDs in the Qwen3VL path. Floating checkpoints write little-endian F32 sidecars and JSONL records with the existing Rust fields `name`, `layer`, `shape`, `len`, `finite`, `sum`, `min`, `max`, `head`, `tail`, `occurrence`, and `binary_path`. Integer checkpoints use the existing no-sidecar schemas exactly: `{"name":"asr.prompt_ids","token_ids":[...]}`, `{"name":"asr.positions","shape":[N,4],"usize_values":[...]}`, and `{"name":"asr.generated_ids","token_ids":[...]}`. Write generated IDs without the terminal EOG token to match `Qwen3Generation::token_ids`. Do not change tensor values, graph execution or sampling.

The stock CLI treats command-line audio without a nonempty `-p` as interactive, and `-p '<__media__>'` alone omits the approved empty system turn. Add one oracle-only branch, gated by `LLAMA_ASR_PARITY_MODE=1`, immediately before the normal single-turn user evaluation: when no system prompt was supplied, call the existing `eval_message` once with `common_chat_msg { role="system", content="" }`. Then the normal user message `-p '<__media__>'` produces the exact approved auto-language/empty-prompt framing:

```text
<|im_start|>system
<|im_end|>
<|im_start|>user
<|audio_start|>[audio embeddings]<|audio_end|><|im_end|>
<|im_start|>assistant
```

The Rust parity test must likewise use auto language, an empty prompt and `max_new_tokens=256`. Keep this branch and all dump code outside the project checkout; `LLAMA_ASR_PARITY_MODE` changes only the oracle CLI framing.

- [ ] **Step 5: Prove the oracle is deterministic before comparing Rust**

Run the same binary twice with CPU-only controls:

```bash
LLAMA_ASR_PARITY_MODE=1 \
LLAMA_ASR_PARITY_TRACE=/tmp/llama-asr-run1.jsonl \
/tmp/rmi-asr-yA0sai/llama.cpp/build/bin/llama-mtmd-cli \
  -m "$QWEN3_ASR_MODEL" --mmproj "$QWEN3_ASR_MMPROJ" \
  --audio "$QWEN3_ASR_WAV" -p '<__media__>' --jinja \
  -ngl 0 --threads 1 --threads-batch 1 --flash-attn off \
  --no-mmproj-offload --no-warmup --seed 0 --temp 0 --simple-io -n 256

LLAMA_ASR_PARITY_MODE=1 \
LLAMA_ASR_PARITY_TRACE=/tmp/llama-asr-run2.jsonl \
/tmp/rmi-asr-yA0sai/llama.cpp/build/bin/llama-mtmd-cli \
  -m "$QWEN3_ASR_MODEL" --mmproj "$QWEN3_ASR_MMPROJ" \
  --audio "$QWEN3_ASR_WAV" -p '<__media__>' --jinja \
  -ngl 0 --threads 1 --threads-batch 1 --flash-attn off \
  --no-mmproj-offload --no-warmup --seed 0 --temp 0 --simple-io -n 256
```

Compare JSONL after normalizing only sidecar path strings, then `cmp` every corresponding sidecar. All pre-sampling dumps must be bitwise equal. The Mel code internally uses four frame workers; do not describe the entire oracle as single-threaded.

- [ ] **Step 6: Run and tighten the Rust parity gate**

Run:

```bash
QWEN3_ASR_MODEL="$QWEN3_ASR_MODEL" \
QWEN3_ASR_MMPROJ="$QWEN3_ASR_MMPROJ" \
QWEN3_ASR_WAV="$QWEN3_ASR_WAV" \
QWEN3_ASR_LLAMA_TRACE=/tmp/llama-asr-run1.jsonl \
RMI_PARITY_TRACE=/tmp/rust-asr.jsonl \
cargo test --release --features parity-trace \
  asr::tests::qwen3_asr_matches_pinned_llama_cpp_trace -- --ignored --exact --nocapture
```

Expected: PASS under every hard gate, with exact final IDs/text. If a stage fails, inspect the first diverging named checkpoint and fix that stage before rerunning later gates; do not widen thresholds beyond the table.

- [ ] **Step 7: Run the complete repository gates**

Run exactly:

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo check --all-targets --features parity-trace
RUSTFLAGS='' cargo check --target x86_64-apple-darwin --all-targets
cargo run --release --bin micro-bench -- --check
```

Expected: all pass. Report a missing x86 target or unavailable environment separately from code regressions; do not claim an unrun cross-check.

- [ ] **Step 8: Record the real-model resource evidence**

Run the fixed CLI once with `/usr/bin/time -l` on macOS (or the platform's equivalent peak-RSS tool), record WAV duration, Mel windows, audio tokens, prompt tokens, generated tokens, elapsed time and peak RSS. Treat these as observations for this exact hardware/model/input, not performance guarantees.

- [ ] **Step 9: Commit parity support**

```bash
git add src/asr.rs src/qwen3.rs src/qwen3a.rs src/parity_trace.rs
git commit -m "test: align qwen3 ASR with llama.cpp"
```

- [ ] **Step 10: Audit final scope without pushing**

Run:

```bash
git status --short
git diff --check 033b41e..HEAD
git diff --name-only 033b41e..HEAD
git log --oneline 033b41e..HEAD
```

Expected tracked scope: `Cargo.toml`, `Cargo.lock`, the listed `src/` files, and this implementation plan/spec history only. `.codegraph/`, models, WAVs, oracle checkout and dumps remain untracked/outside Git. Do not push without a separate explicit request.
