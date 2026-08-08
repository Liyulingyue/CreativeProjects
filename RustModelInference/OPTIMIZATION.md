# Rust LLM Inference Engine — 优化记录

## 当前性能基线（2025-07-31 更新，llama-bench 校正）

| 模型 | 线程 | Rust | llama.cpp | 差距 |
|------|------|------|-----------|------|
| Qwen3-0.6B Q8_0 | T1 | ~16 tok/s | ~21 tok/s | 1.3x |
| Qwen3-0.6B Q8_0 | T4 | ~38 tok/s | ~44 tok/s | 1.16x |
| MiniCPM5-1B Q8_0 | T1 | 9.5 tok/s | ~32 tok/s | 3.4x |
| MiniCPM5-1B Q8_0 | T4 | 27.9 tok/s | ~32 tok/s | 1.15x |

> 注：之前 llama.cpp T1=44 tok/s 的数据有误（可能是多线程测量）。使用 `llama-bench -t 1` 校正后为 ~21 tok/s。

测试条件：40 decode tokens，纯 decode（无 prompt），`--bench` 模式。

> 2025-07-31：移除 prefetch + 优化 f16 加载后，Qwen3 T1 +32%, T4 +15-20%

## 已修复的 Bug

1. **Q/K Norm 计算错误** — Qwen3 特有的 per-head RMS Norm 实现有误
2. **Softmax 重复执行** — double softmax
3. **Sampling 索引错误** — top-k 返回后处理逻辑错误
4. **BPE Byte Decode** — 特殊 byte token 未正确解码
5. **Chat Template** — `<|im_start|>/<|im_end|>` 特殊 token 处理
6. **`sample_top_k` O(n log n) → O(n*k)** — 151936 token 全排序改为增量维护 top-k，T1: 6.0→10.1 tok/s
7. **`vocab_size` fallback** — Qwen3 等模型没有显式 `vocab_size` key，从 tokenizer array 推断

## 已验证无效的方向

### 1. OnceLock 缓存 CPU Feature Detection
- **做法**：用 `std::sync::OnceLock` 替代 `AtomicBool::load()`
- **结果**：反而变慢（9.6→6.7 tok/s）
- **原因**：`swap()` 比 `load()` 重（带 lock 前缀），`get().map().unwrap_or()` 分支多
- **结论**：atomic load 编译成单条 mov，已是 zero-overhead，无需缓存

### 2. 合并 FFN compute 调用
- **做法**：把 FFN gate/up/down 3 个 matmul 合并到 1 个 `compute()`
- **结果**：输出乱码 + 变慢（9.6→6.6 tok/s）
- **原因**：silu activation 需要在 gate/up matmul 完成后才能执行，合并改变了数据流
- **结论**：需更仔细的依赖分析

### 3. Persistent Workers 架构（2025-07-31）
- **做法**：重写 `ComputePool`，让 worker 线程永不退出，在 `worker_loop` 中遍历所有 ops。用 `work_ready` flag + exit_barrier + reenter_barrier 三阶段同步。
- **结果**：Qwen3 T4 从 31.1 → 23.3 tok/s（**更慢**），输出正确
- **原因**：每步推理多了 `work_ready` spin-wait + 额外 barrier 开销。Fork-join 的 barrier 已经是 minimal overhead 了。
- **结论**：Persistent workers 需要完全重新设计（消除 epoch-based wakeup，改用 work-stealing queue），不能简单叠加在现有模型上

### 4. `select_nth_unstable_by` 替代 O(n*k) 扫描
- **做法**：用 `Vec::select_nth_unstable_by` 替代手写增量扫描
- **结果**：变慢（9.7→8.5 tok/s）
- **原因**：需要分配 151936 元素的 `Vec<(usize,f32)>`，堆分配开销超过算法改进收益

## Profiling 结果（MiniCPM5-1B T1）

```
matmul 合计:  74.4% (3.09s)
  - QKV matmul:  11.5% (0.479s)
  - WO matmul:     7.3% (0.305s)
  - FFN matmul:   55.6% (2.308s)
logits:         24.5% (1.017s)
rope+Kv+attn:   1.0%  (0.043s)
```

## Micro-benchmark（单核 matmul kernel）

| 操作 | n_in x n_out | Rust GFLOPS | Rust GB/s |
|------|--------------|-------------|-----------|
| Qwen3 wq | 1024x2048 | 49.86 | 26.55 |
| Qwen3 ffn_gate | 1024x3072 | 23.51 | 12.51 |
| Qwen3 ffn_down | 3072x1024 | 40.04 | 21.37 |
| Qwen3 logits | 2048x151936 | 24.31 | 12.91 |
| MiniCPM5 wq | 1536x2048 | 44.57 | 23.73 |
| MiniCPM5 ffn_gate | 1536x4608 | 20.23 | 10.76 |
| MiniCPM5 ffn_down | 4608x1536 | 30.80 | 16.41 |

llama.cpp FFN 约 50 GB/s，差距 4-5x。

## 后续优化方向（按优先级）

### 🔴 高优先级

#### 1. Persistent Workers 架构
**目标**：消除每层 5 次 fork-join 的调度开销

llama.cpp 的 workers 是 persistent 的：线程进入 `worker_loop` 后，遍历 **所有 compute 操作**（barrier 模型），无需每次都 spawn/join。

实现方式：
- Worker 线程在 `worker_loop` 中维护一个 "当前 op" 指针
- 每个 compute 操作携带：op 类型、权重指针、数据指针、操作函数
- 主线程遍历 ops 并分发给 workers（类似 `tokio` 的 task）
- Worker 遍历所有 op，做完才进入下一轮

这是 llama.cpp 50 GB/s 的关键来源。

#### 2. Matmul Kernel 优化（✅ 已实施，2025-07-31）
**改动**：
- 移除了 inner loop 中的 prefetch（`_mm_prefetch`）— 顺序访问本身已被 prefetcher 覆盖
- 将 byte-by-byte f16 加载改为 `std::ptr::read_unaligned`（编译器可生成单一 16-bit load）

**结果**：
- Qwen3 T1: 12.6 → 16.9 tok/s（**+34%**）
- Qwen3 T4: 31.8 → 37-39 tok/s（**+18-22%**）
- MiniCPM5 T4: 26.3 → 27.9 tok/s（**+6%**）

> 实测 2-row tiling 性能与 4-row tiling 相同，无额外收益

**分析**：移除不必要的 prefetch 减少了指令数和 L1 cache 压力。Prefetch 在顺序访问模式下会增加开销而不带来收益。

### 🟡 中优先级

#### 3. Logits 层优化
Logits 占 24.5% 时间，vocab=130560-151936：
- 当前 `sample_top_k` 是 O(n*k) 扫描，可进一步用 `select_nth_unstable_by`（需避免大 Vec 分配）
- 可以预分配固定大小的 `Vec<(usize,f32)>` 并复用，避免每次 `push`

#### 4. 减少 compute() 调用
每层当前 5 次 compute：
- QKV（3 个 matmul）
- WO（1 个 matmul）
- FFN gate/up（2 个 matmul + silu）
- FFN down（1 个 matmul）
- Logits（1 个 matmul）

WO 可以和 QKV 合并（需要研究依赖）；FFN gate/up/silu/down 可以在 persistent worker 模型下自然流水线化。

### 🟢 低优先级

#### 5. Multi-model 测试
- Qwen2.5-0.5B Q4_K_M（验证 Q4_K_M 量化）
- Gemma-3-1B Q4_K_M（验证 gemma3 架构）

#### 6. Chat Template 对齐
MiniCPM5 在 chat 模式下输出乱码（`--bench` 正常），chat template 未对齐。

## llama.cpp 关键实现参考

### 调度器（ggml-cpu.c）
- `ggml_compute_forward_mul_mat`: lines 1254-1451
- Chunk 分配：`atomic_fetch_add(&current_chunk, 1)` + `barrier`
- `current_chunk` 每 matmul 重置为 `nth`
- 2D tiling: `blck_0=16, blck_1=16`

### Q8_0 Kernel（arch/x86/quants.c）
- `ggml_vec_dot_q8_0_q8_0`: lines 1308-1374
- `mul_sum_i8_pairs_float`: lines 122-134
- 优先使用 VNNI（`_mm256_dpbssd_epi32`），其次 `_mm256_sign + _mm256_maddubs_epi16`

### 文件位置
- `references/ggml/src/ggml-cpu/ggml-cpu.c` — 调度器
- `references/ggml/src/ggml-cpu/arch/x86/quants.c` — Q8_0 AVX2 内核
- `references/llama.cpp/src/llama-context.cpp` — llama 上下文和线程管理
