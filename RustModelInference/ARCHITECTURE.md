# 技术设计规范：rust-model-inference

> **文档用途：** 本文档定义了 `rust-model-inference` 引擎的核心设计哲学、物理内存约束与 Rust 接口规范。任何参与重构或扩展此项目的 AI Agent / 开发者，必须严格遵守以下设计法则。

---

## 1. 核心设计初衷与目标

`rust-model-inference` 是一个针对端侧设备（如 Raspberry Pi, Cix P1, NUC 等）打造的**高吞吐、零堆分配（Zero-Allocation）、多模态混合量化**推理引擎。

### 关键工程痛点与破局策略

1. **摆脱 C++ 内存隐患：** 传统的 C++ 推理引擎在处理复杂多模态/混合量化管道时，极易因指针强转、悬垂引用或动态 `memcpy` 产生静默内存污染与段错误（Segfault）。
2. **面向 Agent 演进（Agent-Safe）：** 利用 Rust 严格的编译期 Borrow Checker 与生命周期（Lifetime）约束，赋予 AI Agent 安全重构代码的能力。只要代码通过编译，即数学证明其不存在数据竞争、野指针或越界。
3. **极致系统级性能：** 放弃通用堆分配与中间重排（Repack），通过**物理切片零拷贝 + 静态 Arena 缓冲区 + SIMD 静态派发**，将 Hot Path 数据流完全锁在 CPU L1/L2 Cache 中。

---

## 2. 三大核心设计法则

### 法则一：物理层无缝切片（Zero-Copy Flat View）

* **规则：** 无论是 GGUF 文件、`mmproj` 多模态投影权重，还是不同量化等级（FP16/Q8_0/Q4_K/Q3_K）的 Tensor，在加载与存储物理层**严禁解包或动态复制**。
* **实现：** 统一借用 `mmap` 的裸字节切片 `&'a [u8]`。利用 `zerocopy` 或安全结构体映射，实现 0 次动态内存申请。

### 法则二：交互层统一数据流（Unified `f32` In-Place Arena）

* **规则：** 算子与算子之间、Layer 与 Layer 之间、模态与模态（Projector -> LLM Layer）之间的特征向量（Hidden States）交互，**必须且仅能使用全局静态预分配的 `Scratchpad` 切片 `&'a mut [f32]**`。
* **物理合理性：** 虽然使用 `f32` 传递临时 Activation 在位宽上有轻微冗余，但它完全消除了层间"实时重新量化"的 CPU 计算开销，保障了数据 100% 在 Cache 中 In-Place 覆写接力，在端侧总线带宽与 CPU 指令周期上取得了最优解。

### 法则三：算子层粒度混搭（Fine-Grained Hybrid Quantization）

* **规则：** 允许在**模态级**（ViT FP16 / LLM Q4_K）、**层级**（Head/Tail 高精度 / Body 低精度）甚至**算子级**（Self-Attention Q8_0 / FFN Q3_K）自由混合不同量化精度。
* **实现：** 利用 Rust `Enum` 与 `Trait` 实现静态派发，消除虚函数表（vtable）寻址与类型转换开销。

---

## 3. 核心架构与代码规范范式

AI Agent 在生成逻辑时，必须参考并遵循以下三层核心结构：

### 3.1 物理视图层：统一零拷贝借用

```rust
/// 描述物理存储上的 Tensor 视图，纯借用，零堆分配
pub struct QuantizedTensorView<'a> {
    pub name: &'a str,
    pub shape: &'a [usize],
    pub quant_type: QuantType,
    pub raw_data: &'a [u8], // 直接指向 mmap 的物理地址，受 lifetime 'a 约束
}
```

### 3.2 算子抽象层：Trait 与零成本静态派发

```rust
/// 所有量化矩阵乘法算子必须实现的统一接口
pub trait MatMulOp {
    /// 算子内部负责解包自己的量化 Block，并与输入的 f32 激活值做 SIMD 点积，
    /// 结果直接写入 Scratchpad 提供的输出切片。
    fn forward(&self, input: &[f32], output: &mut [f32]);
}

/// 算子级混合量化枚举（LLVM 将其内联优化为跳转表，无虚函数开销）
pub enum QuantizedLayer<'a> {
    Fp16(Fp16MatMul<'a>),
    Q8_0(Q8_0MatMul<'a>),
    Q4_K(Q4KMatMul<'a>),
    Q3_K(Q3KMatMul<'a>),
}

impl<'a> MatMulOp for QuantizedLayer<'a> {
    #[inline(always)]
    fn forward(&self, input: &[f32], output: &mut [f32]) {
        match self {
            Self::Fp16(op) => op.forward(input, output),
            Self::Q8_0(op) => op.forward(input, output),
            Self::Q4_K(op) => op.forward(input, output),
            Self::Q3_K(op) => op.forward(input, output),
        }
    }
}
```

### 3.3 数据流与 Scratchpad 交互

```rust
/// 全局静态 Scratchpad，引擎 Hot Path 唯一的内存载体
pub struct ExecutionScratchpad {
    // 预分配的大块连续内存，严禁在推理过程中 resize 或 free
    pub hidden_states: [f32; 2048 * 4096],
}

impl ExecutionScratchpad {
    /// 划分安全切片，保证在 Layer 间 In-Place 覆写时不发生数据踩踏
    #[inline(always)]
    pub fn get_layer_buffers<'a>(&'a mut self, dim: usize) -> (&'a [f32], &'a mut [f32]) {
        let (input, output) = self.hidden_states.split_at_mut(dim);
        (&input[..dim], &mut output[..dim])
    }
}
```

---

## 4. 给 AI Agent 的约束指令（System Directives）

1. **零堆分配原则（Zero Heap Allocation）：** 严禁在 `forward` 或推理 Hot Path 中使用 `Vec::new()`、`Box::new()`、`format!`、`memcpy` 或任何触发内存分配的逻辑。所有临时空间必须向 `ExecutionScratchpad` 申请。
2. **严格生命周期关联：** 所有 Tensor 视图与借用必须带有显式生命周期参数 `'a`，且绑定至物理文件 `mmap` 的生命周期。
3. **安全第一：** 严禁在未经生命周期约束的逻辑中使用 `unsafe` 做裸指针转换。任何指针操作必须通过切片借用（Slice Borrowing）完成。
4. **保持显式强类型：** 量化 Block 的 Header 偏移解析必须按结构体强类型对齐处理，不得凭空假定字节偏移。

---

## 5. 单文件异构调度架构（.ggufrs Specification）

* **设计理念：** 将多模态 (mmproj) 与 LLM 主干权重打包于单一 `.ggufrs` 文件中，通过 Header 元数据实现 Segment 级的物理隔离与异构设备调度。
* **分发机制：**
  - **Vision Segment (NPU/GPU)：** 低延时高并发的视觉/音频特征提取器，直接借用物理切片提交至端侧 NPU/GPU。
  - **LLM Segment (CPU/GPU)：** 逻辑推演主干，由 CPU SIMD 或 GPU 离散/集成显卡接管。
* **异构桥梁：** 跨硬件计算的中间特征向量（Embeddings）严格限定在全局 `Scratchpad` (`&mut [f32]`) 中完成 DMA 复制与 In-Place 接力，确保异构调度不引入任何动态堆内存分配。

---

## 6. 异构资源即时释放规范（Explicit Scope & Drop Semantics）

* **痛点解决：** 针对传统 C++ 引擎 (如 llama.cpp) 中 ViT/mmproj 与 LLM 句柄强绑定导致 Prefill 结束后视觉显存无法释放的问题，`rust-model-inference` 采用**段级生命周期剥离**设计。
* **物理解耦：** `.ggufrs` 仅作为静态物理地址映射，`VisionRunner` (NPU/GPU) 与 `LlmRunner` (CPU/GPU) 拥有各自独立的 C-API 硬件驱动句柄与 RAII 生命周期。
* **RAII 立即回收：** 视觉 Encoder 计算完成后，利用 Rust 的 `Drop` 机制显式触发驱动级的 Unload/Free 接口，在进入自回归 (Decode) 阶段前**100% 回收视觉模块占用的 NPU/GPU 硬件显存**，将其无缝还给 LLM 的 KV Cache 扩展。

---

## 7. 当前实现状态（Phase 1 MVP）

### Model Under Test
- `models/Qwen3-0.6B-Q8_0.gguf` (Q8_0 quantized, GGUF V3)
- Architecture: `qwen3`, n_embd=1024, n_layer=28, n_head=16, n_head_kv=8, n_ff=3072
- n_embd_head_k=n_embd_head_v=128, n_embd_q=2048, n_embd_gqa=1024, freq_base=1e6, eps=1e-6
- GQA with group_size=2 (2 Q heads share 1 KV head)

### CPU Target
- Intel Core Ultra 5 125H — 4P cores (0-7 HT), 8E cores (8-15), 2LPE cores (16-17), AVX2+FMA, no AVX512

### Speed Benchmark (Qwen3-0.6B-Q8_0, 128 gen tokens, --bench mode no chat template)

| Threads | Rust (tok/s) | llama.cpp (tok/s) | Ratio |
|---------|-------------|-------------------|-------|
| 1       | ~9.5        | ~10               | ~95%  |
| 4       | 26.3        | 36.0              | 73%   |
| 6       | 30.7        | -                 | -     |
| 8       | 31.6        | -                 | 88%*  |
| 16      | 卡死/极慢    | -                 | -     |

*8线程 vs llama.cpp 4线程

### Profiling (8 threads, decode phase, 128 tokens)
- FFN = 42.1% (Gate+Up+SiLU + quantize + Down)
- QKV+attn = 25.6% (QKV matmul + single-threaded RoPE+KVwrite + attention)
- logits = 23.5% (151936×1024 output projection)
- Wo = 8.8%

### Optimizations Applied
1. RM=4 register blocking kernel (4 weight rows per tile, shared input q8 load)
2. Packed f16→f32 via `_mm_cvtph_ps` (batch convert 4 deltas, broadcast with `_mm256_shuffle_ps`)
3. Software prefetching (`_mm_prefetch T0`) for next block's weight data
4. Raw pointer access to eliminate bounds checks in hot loop
5. Online softmax + SIMD `vec_mad_f32`/`vec_scale_f32` for attention V accumulation
6. f16 KV cache with AVX2 SIMD ops, runtime KV format selection (`--kv-cache f16|f32`)
7. Q8_0 quantized input for all matmuls (avoid f32 dequant overhead)
8. Clean 7-step BSP pipeline per layer with `ComputePool` (no internal spin-barriers)
9. Fixed `ComputePool` epoch race bug (worker threads could miss epochs with 16+ threads)

### Build
- `cargo build --release` (opt-level=3, lto=fat, codegen-units=1)
- llama.cpp: cmake Release at `references/llama.cpp/build/`
- Run: `LD_LIBRARY_PATH=references/llama.cpp/build/bin references/llama.cpp/build/bin/llama-cli`

### Key Files
- `src/main.rs`: inference loop, `run_inference` (7-step BSP), `run_dump_logits`, CLI flags
- `src/ops.rs`: SIMD ops — `matmul_q8_0_vs_q8_0_avx2`, `dot_f16_f32`, `vec_mad_f16_f32`, `f32_slice_to_f16`, `rms_norm`, `softmax`, `quantize_q8_0_into`, `rope_neox`, `silu`
- `src/model.rs`: GGUF parser, `QuantizedLinear`
- `src/tokenizer.rs`: BPETokenizer
- `src/thread_pool.rs`: `ComputePool` (BSP model, epoch-based dispatch, `fence(SeqCst)`)
- `src/traits.rs`: Layer trait, ModelConfig
- `src/memory.rs`: BlockAllocator, MemoryArena

### CLI Flags
- `--model <path.gguf>`: model file
- `--prompt "text"`: input prompt
- `--threads N`: thread count (default: available parallelism)
- `--max-tokens N`: generation length (default: 128)
- `--temp F`: sampling temperature (default: 0.6)
- `--bench`: skip chat template, raw token generation
- `--profile`: print timing breakdown after inference
- `--kv-cache f16|f32`: KV cache format (default: f16)
- `--dump-logits`: write logits to `/tmp/rust_logits.bin` for precision verification

---

## 8. 多线程架构设计（ComputePool）

### 8.1 BSP 执行模型

每个推理步骤（token 生成）使用 **Bulk Synchronous Parallel** 模型：每层内多个 `pool.compute()` 调用间通过 pool 的自然屏障同步，**不使用任何内部自旋屏障**。

### 8.2 当前 7 步流水线（每层）

| Step | 执行者 | 操作 |
|------|--------|------|
| 1 | `pool.compute()` | QKV matmul (3 个并行矩阵乘法) |
| 2 | 主线程 | RoPE + Q/K norm + KV cache 写入 (单线程，避免 GQA 写-写竞争) |
| 3 | `pool.compute()` | Attention (online softmax, per-head 并行) |
| 4 | 主线程 | quantize attn_out → Q8_0 |
| 5 | `pool.compute()` | Wo 投影 + 残差加 + FFN norm + quantize → Gate+Up+SiLU |
| 6 | 主线程 | quantize gate_buf → Q8_0 |
| 7 | `pool.compute()` | Down 投影 |

### 8.3 GQA 正确性约束

- **KV 写入必须单线程化：** 当 `n_head_kv` 不能被线程数整除时，基于 Q head 范围推导的 KV head 范围会产生重叠，导致多线程写-写竞争。解决：RoPE + KV 写入始终在主线程单线程执行。
- **Attention 只读 KV cache：** Attention 阶段各线程只读取自己负责的 Q head 对应的 KV cache 条目，不存在写竞争。

### 8.4 ComputePool 的 Epoch 竞态修复

Worker 线程的 `my_epoch` 必须从 0 开始（而非 `epoch.load(Acquire)`），否则在 start_barrier 之后到进入 spin-loop 之间的延迟可能导致 worker 读到已递增的 epoch 值，跳过当前计算轮次，造成死锁。

```rust
fn worker_loop(tid: usize, n_threads: usize, inner: &Inner) {
    let mut my_epoch: u32 = 0; // 必须从 0 开始，不能读当前值
    loop {
        while inner.epoch.load(Ordering::Acquire) == my_epoch {
            if inner.shutdown.load(Ordering::Acquire) { return; }
            std::hint::spin_loop();
        }
        my_epoch = inner.epoch.load(Ordering::Acquire);
        // ...
    }
}
```

---

## 9. 已知问题与下一步

### 已修复
- GQA 写-写竞争（单线程化 KV 写入）
- FFN barrier 非确定性（移除内部自旋屏障）
- ComputePool epoch 竞态（worker `my_epoch` 从 0 初始化）
- Q/K norm 逐 head 处理（非逐 tensor）
- Double softmax / RoPE 半旋转错误

### 下一步优化方向
1. **logits 优化（23.5%）：** 151936×1024 的输出投影，内存带宽受限；可考虑 top-k sparse 输出或分块计算
2. **FFN 优化（42.1%）：** 融合 Gate+Up+SiLU+quantize+Down 减少 pool.compute() 调用次数
3. **Q8_0 matmul 内核优化：** 研究 llama.cpp 的 `ggml` 内核实现差异
4. **多模态扩展：** 实现 `VisionEncoder` trait + `.ggufrs` 统一打包格式
5. **混合量化支持：** 扩展 `QuantizedLayer` 枚举支持 Q4_K/Q3_K/Q5_K/Q6_K

---

## 10. GGUF 文件格式参考

### Header Layout

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | magic | `"GGUF"` |
| 4 | 4 | version | `uint32` = 3 |
| 8 | 8 | n_tensors | `uint64` |
| 16 | 8 | n_kv | `uint64` |
| 24+ | var | kv_pairs | metadata key-value pairs |
| ... | var | tensor_info | tensor descriptors |
| ... | var | data | aligned weight data |

### Key Metadata for Qwen3-0.6B

```
general.architecture = "qwen3"
qwen3.context_length = 32768
qwen3.embedding_length = 1024
qwen3.feed_forward_length = 3072
qwen3.block_count = 28
qwen3.attention.head_count = 16
qwen3.attention.head_count_kv = 8
qwen3.attention.key_length = 128
qwen3.attention.value_length = 128
qwen3.attention.layer_norm_rms_epsilon = 1e-6
qwen3.rope.freq_base = 1000000.0
tokenizer.ggml.model = "gpt2"
tokenizer.ggml.tokens = [...] (151936 entries)
```

---

## 11. Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| `gguf` crate API 不稳定 | Medium | 封装 adapter layer，必要时 fork |
| Q8_0 反量化精度 | Low | 对比 llama.cpp 参考实现逐 block 验证 |
| MemoryArena 尺寸不足 | Medium | 运行时 assert + 配置化容量 |
| 量化类型扩展（Q4_K/Q3_K） | Low | Trait/Enum 化构天然支持扩展 |
| 多模态动态分辨率 | High | 固定 Block 大小 + 动态 Block 数量分配 |
| ComputePool 大线程数死锁 | High | 已修复：epoch 从 0 初始化 |
| GQA 多线程 KV 写竞争 | High | 已修复：单线程化 KV 写入 |
