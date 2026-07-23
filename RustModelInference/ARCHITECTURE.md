# Architecture Planning: RustModelInference

> 100% Pure Rust, Zero Heap Allocation in Hot Path, mmap Zero-Copy LLM Inference Engine
> MVP Target: Qwen2-0.6B (GGUF Q4_K_M)

---

## 0. Problem Analysis: Why Not llama.cpp?

### 0.1 手写偏移 (Hardcoded Offsets)
llama.cpp 的 GGUF tensor 加载依赖 `ggml_get_tensor_offset()` 返回的偏移量，但这些偏移量的计算散落在 `gguf_init_from_file` 的解析逻辑中，与文件版本、alignment、padding 规则深度耦合。任何 GGUF spec 的微小变更（如 V2→V3 的 alignment 变化）都需要深入 C 代码修改。

**我们的解法**: 使用 Rust `gguf` crate 进行声明式解析，所有 offset 由 crate 根据 GGUF spec 自动计算。我们只消费 `&'a [u8]` 切片。

### 0.2 mmproj 与 gguf 需要分开 (Split Model Files)
llama.cpp 的多模态实现中，ViT 权重（clip model）和 LLM 权重通常存储在不同的 GGUF 文件中，需要分别加载、分别构建计算上下文。这导致：
- 两次 mmap，两套内存管理
- 跨 Context 的特征搬运（CPU memcpy）
- 无法共享 Scratch Buffer

**我们的解法**: 统一的 `ModelArena` 持有所有 mmap region，ViT 和 LLM 的权重切片共存于同一生命周期 `'a` 下，通过零拷贝切片直接传递。

### 0.3 视觉编码计算效率差 (Compute-Bound ViT on Memory-Bound LLM Skeleton)
llama.cpp 的 GGML 计算图是为 Decode 阶段（1 token/batch, Memory Bound）优化的。ViT 的 Prefill 是 Compute Bound（大 Batch 矩阵乘法），GGML 的 graph 构建、调度、线程同步开销远超专用引擎。

**我们的解法**: 
- ViT 作为独立的 `VisionEncoder` trait 实现，拥有自己的 `PrefillScheduler`
- ViT output slice 直接作为 LLM input slice（零拷贝衔接）
- 未来可接入 compute backend trait（SIMD / BLAS / NPU）

---

## 1. Core Architectural Principles

| # | Principle | Enforcement |
|---|-----------|-------------|
| P1 | **Zero Heap Allocation in Hot Path** | `forward()` 签名强制 `&mut [f32]` output，编译期拒绝 `Vec`/`Box` |
| P2 | **mmap Zero-Copy Weight Binding** | 权重字段类型为 `&'a [u8]`，lifetime 与 mmap region 绑定 |
| P3 | **Explicit Memory Lifetime** | 所有 Buffer 由调用方显式传入，无隐式分配 |
| P4 | **Trait-Based Architecture** | `Layer` trait 解耦算子与内存，模型层可组合 |
| P5 | **No C/C++ FFI** | 100% Rust，包括量化反量化算子 |

---

## 2. Module Architecture

```
src/
├── lib.rs              # Crate root, re-exports
├── traits.rs           # Layer trait, ExecContext, ModelConfig
├── memory.rs           # PagedKVBlock, BlockAllocator, MemoryArena
├── quant.rs            # Q4_K_M block struct + dequantization
├── model.rs            # GGUF loader, QuantizedLinear, ModelGraph
├── ops.rs              # RMSNorm, RoPE, SiLU, Softmax (future)
├── scheduler.rs        # Prefill/Decode scheduler (future)
└── main.rs             # Integration test / CLI entry
```

---

## 3. Data Flow: End-to-End Inference Pipeline

```
┌─────────────┐     mmap       ┌──────────────────┐
│  .gguf file │ ────────────── │  MmapRegion<'a>  │
└─────────────┘                │  (memmap2 crate) │
                               └────────┬─────────┘
                                        │ &'a [u8] slices
                               ┌────────▼─────────┐
                               │  GGUF Parser     │
                               │  (gguf crate)    │
                               └────────┬─────────┘
                                        │ tensor offsets
                        ┌───────────────▼───────────────┐
                        │       ModelGraph<'a>          │
                        │  ┌─────────────────────────┐  │
                        │  │ QuantizedLinear<'a>     │  │
                        │  │   weight: &'a [u8]      │  │
                        │  │   bias:   Option<&'a[u8]>│  │
                        │  └─────────────────────────┘  │
                        └───────────────┬───────────────┘
                                        │
                        ┌───────────────▼───────────────┐
                        │     forward(input, output, ctx)│
                        │                               │
                        │  input:  &[f32]  (borrowed)   │
                        │  output: &mut [f32] (pre-alloc)│
                        │  ctx:    &mut ExecContext      │
                        │    ├── scratch: &mut [f32]     │
                        │    └── kv_cache: &mut KVCache  │
                        └───────────────────────────────┘
```

---

## 4. Trait System Design

### 4.1 Core Trait: `Layer`

```rust
pub trait Layer {
    /// Forward pass: read input, write output, borrow scratchpad + kv cache.
    /// INVARIANT: output.len() >= self.output_dim()
    /// INVARIANT: No heap allocation occurs within this call.
    fn forward(&self, input: &[f32], output: &mut [f32], ctx: &mut ExecContext);

    /// Dimensionality queries (const or computed from config)
    fn input_dim(&self) -> usize;
    fn output_dim(&self) -> usize;
    fn name(&self) -> &str;
}
```

### 4.2 Execution Context

```rust
pub struct ExecContext<'a> {
    /// Scratchpad for intermediate computations (matmul temp, etc.)
    pub scratch: &'a mut [f32],
    /// Paged KV Cache reference
    pub kv_cache: &'a mut KVCacheView,
    /// Current token position in the sequence
    pub pos: u32,
    /// Current layer index
    pub layer_idx: u32,
}
```

### 4.3 Future Extension: VisionEncoder Trait

```rust
pub trait VisionEncoder {
    /// Encode image patches into vision tokens.
    /// Output slice is written directly, zero-copy to LLM input.
    fn encode<'a>(
        &self,
        image: &[u8],          // raw image bytes
        vision_tokens: &'a mut [f32], // pre-allocated output
        ctx: &mut ExecContext,
    ) -> usize; // returns number of vision tokens produced
}
```

---

## 5. Memory Architecture

### 5.1 Paged KV Cache

**Design Rationale**: 固定大小的物理页（Block）消除碎片化，数组索引管理消除链表开销。

```
┌─────────────────────────────────────────────┐
│              BlockAllocator                  │
│  blocks: [PagedKVBlock; N]  ← 固定数组      │
│  free_list: [u16; N]        ← 空闲页索引栈   │
│  free_top: u16              ← 栈顶指针       │
└─────────────────────────────────────────────┘

┌─────────────────────────────────────────────┐
│  PagedKVBlock (BLOCK_SIZE = 16 tokens)      │
│  k: [[f32; HEAD_DIM]; BLOCK_SIZE]           │
│  v: [[f32; HEAD_DIM]; BLOCK_SIZE]           │
│  ref_count: u16                              │
│  seq_id: u16                                 │
└─────────────────────────────────────────────┘
```

**Key Invariants**:
- `free_top` 永远不会触发堆分配（纯栈操作）
- Block 分配/释放是 O(1)
- 物理页在内存中连续，cache-line 友好

### 5.2 MemoryArena

全局静态内存池，为 scratch buffer 和 KV cache 提供预分配存储：

```rust
pub struct MemoryArena {
    data: Box<[u8]>,     // single contiguous allocation at init
    scratch_offset: usize,
    kv_offset: usize,
    total_size: usize,
}
```

**Layout**:
```
┌──────────────────────────────────────────────────┐
│                  MemoryArena                      │
│  ┌────────────────────┬─────────────────────────┐│
│  │   Scratch Buffer   │      KV Cache Pool      ││
│  │   (matmul temp,    │   (PagedKVBlock × N)    ││
│  │    layer intermediates)                       ││
│  └────────────────────┴─────────────────────────┘│
└──────────────────────────────────────────────────┘
```

---

## 6. Q4_K_M Quantization Format

### 6.1 Block Structure (from llama.cpp ggml-common.h)

```rust
#[repr(C, packed)]
pub struct BlockQ4K {
    pub d: f16,              // super-block scale (2 bytes)
    pub dmin: f16,           // super-block min scale (2 bytes)
    pub scales: [u8; 12],    // packed scale/min factors (12 bytes)
    pub qs: [u8; 128],       // 4-bit quantized values (128 bytes)
}
// Total: 144 bytes per 256 f32 values
// Effective: 4.5 bits/weight
```

### 6.2 Dequantization Formula

```
For each sub-block pair (is, is+1), processing 64 values:
  sc, m = get_scale_min_k4(is, scales)
  d_eff = d * sc
  m_eff = dmin * m

  y[l]      = d_eff * (qs[l] & 0xF) - m_eff    (low nibble,  l = 0..31)
  y[l + 32] = d_eff * (qs[l] >> 4)  - m_eff    (high nibble, l = 0..31)
```

### 6.3 Scale/Min Decoding

```rust
fn get_scale_min_k4(j: usize, q: &[u8; 12]) -> (u8, u8) {
    if j < 4 {
        (q[j] & 63, q[j + 4] & 63)
    } else {
        let sc = (q[j + 4] & 0xF) | ((q[j - 4] >> 6) << 4);
        let mn = (q[j + 4] >> 4)   | ((q[j]     >> 6) << 4);
        (sc, mn)
    }
}
```

---

## 7. GGUF File Format

### 7.1 Header Layout

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | magic | `"GGUF"` |
| 4 | 4 | version | `uint32` = 3 |
| 8 | 8 | n_tensors | `uint64` |
| 16 | 8 | n_kv | `uint64` |
| 24+ | var | kv_pairs | metadata key-value pairs |
| ... | var | tensor_info | tensor descriptors |
| ... | var | data | aligned weight data |

### 7.2 Tensor Info Record

| Field | Type | Description |
|-------|------|-------------|
| name | string | tensor name (uint64 len + chars) |
| n_dims | uint32 | number of dimensions |
| dims | int64[n_dims] | dimension sizes |
| type | int32 | ggml_type (Q4_K = 12) |
| offset | uint64 | offset within data section |

### 7.3 Key Metadata for Qwen2-0.6B

```
general.architecture = "qwen2"
qwen2.context_length = 32768
qwen2.embedding_length = 1024
qwen2.feed_forward_length = 2816
qwen2.block_count = 24
qwen2.attention.head_count = 16
qwen2.attention.head_count_kv = 16
qwen2.attention.layer_norm_rms_epsilon = 1e-6
qwen2.rope.freq_base = 1000000.0
tokenizer.ggml.model = "gpt2"
tokenizer.ggml.tokens = [...] (151936 entries)
```

---

## 8. Qwen2-0.6B Model Parameters

| Parameter | Value | Description |
|-----------|-------|-------------|
| n_embd | 1024 | Embedding dimension |
| n_layer | 24 | Transformer layers |
| n_head | 16 | Attention heads |
| n_head_kv | 16 | KV heads (GQA ratio = 1) |
| n_embd_head | 64 | Head dimension (1024/16) |
| n_ff | 2816 | Feed-forward dimension |
| n_ctx_train | 32768 | Training context length |
| rope_freq_base | 1,000,000 | RoPE frequency base |
| vocab_size | 151,936 | Vocabulary size |
| norm_eps | 1e-6 | RMSNorm epsilon |

### Q4_K_M Memory Estimates

- Per layer: ~7.1 MB (Q/K/V/O + gate/up/down projections)
- Full model: ~170-180 MB
- KV Cache (F16, ctx=2048): ~192 MB (24 layers × 2 × 4 MB)

---

## 9. MVP Scope (Phase 1)

### In Scope
- [x] `Layer` trait + `ExecContext` definition
- [x] `PagedKVBlock` + `BlockAllocator` (static array-based)
- [x] `MemoryArena` (global pre-allocated buffer)
- [x] GGUF mmap loading via `memmap2` + `gguf` crate
- [x] `QuantizedLinear<'a>` with `&'a [u8]` weight binding
- [x] Q4_K_M dequantization kernel
- [x] Integration test: load GGUF → init arena → forward → verify no realloc

### Out of Scope (Phase 2+)
- [ ] Full Qwen2 model graph (RMSNorm, RoPE, Attention, SiLU/MLP)
- [ ] Tokenizer (BPE)
- [ ] Sampling (temperature, top-k, top-p)
- [ ] VisionEncoder trait + ViT implementation
- [ ] SIMD-accelerated dequantization (AVX2/NEON)
- [ ] Multi-threaded matmul
- [ ] Continuous batching / multi-sequence scheduling
- [ ] Quantized KV cache

---

## 10. Dependency Strategy

| Crate | Version | Purpose | Heap in Hot Path? |
|-------|---------|---------|-------------------|
| `memmap2` | 0.9 | mmap zero-copy weight loading | No (init only) |
| `gguf` | 0.4 | GGUF header/tensor parsing | No (init only) |
| `half` | 2.4 | f16 type for Q4_K scales | No (stack only) |

**Notably absent**:
- No `rayon` (thread pool allocates) — future custom work-stealing
- No `ndarray`/`tensor` — we use raw slices for zero-alloc
- No `libc`/`bindgen` — no C FFI

---

## 11. Future Vision: Multimodal Zero-Copy Pipeline

```
┌──────────┐    mmap     ┌──────────────┐
│ viT.gguf │ ────────── │ MmapRegion'a │──┐
└──────────┘            └──────────────┘  │
┌──────────┐    mmap     ┌──────────────┐  │  &'a [u8] slices
│ llm.gguf │ ────────── │ MmapRegion'a │──┤
└──────────┘            └──────────────┘  │
                                           ▼
                        ┌──────────────────────────┐
                        │    Unified ModelArena    │
                        │  ┌────────────────────┐  │
                        │  │ VisionEncoder<'a>  │  │
                        │  │  → vision_tokens   │  │
                        │  │    &'a mut [f32]   │──┤ zero-copy
                        │  └────────────────────┘  │ handoff
                        │  ┌────────────────────┐  │
                        │  │ LLM Decoder<'a>    │◄─┤
                        │  │  ← input slice    │  │
                        │  └────────────────────┘  │
                        └──────────────────────────┘
```

**Key Insight**: ViT output 和 LLM input 共享同一个 `&'a mut [f32]` 切片。Vision tokens 生成后直接作为 LLM 的 Prefill input，无需任何内存拷贝。这通过 Rust 的 borrow checker 在编译期强制保证。

---

## 12. Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| `gguf` crate API 不稳定 | Medium | 封装 adapter layer，必要时 fork |
| Q4_K_M 反量化精度 | Low | 对比 llama.cpp 参考实现逐 block 验证 |
| MemoryArena 尺寸不足 | Medium | 运行时 assert + 配置化容量 |
| 量化类型扩展（Q5_K/Q6_K） | Low | Trait 化构天然支持扩展 |
| 多模态动态分辨率 | High | 固定 Block 大小 + 动态 Block 数量分配 |
