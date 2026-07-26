# Qwen3.5 性能优化计划

## 现状

| 模型 | 路径 | 线程 | SIMD | 速度 | llama.cpp | 差距 |
|------|------|------|------|------|-----------|------|
| Qwen3-0.6B | `run_inference` (ops.rs) | 8线程+ComputePool | AVX2 Q8_0×Q8_0 | 31.2 tok/s | 37 tok/s | 1.2x |
| Qwen3.5-0.8B | `run_multimodal` (qwen35.rs) | 单线程 | 无 | 2.4 tok/s | 36.4 tok/s | 15x |

## 根因

`qwen35.rs` 的 `QWeight::matmul()` 是纯标量循环，`forward()` 忽略 `_n_threads` 参数。

具体问题：
1. **Q4K/Q5K/Q6K**: 先全量反量化到 FP32，再逐元素标量乘加 — 双重浪费
2. **Q8_0**: 也是先反量化再标量乘加，没走整数 SIMD
3. **F16**: 逐元素 f16_to_f32 + 标量乘加
4. **F32**: 最简单的标量循环，也没用 SIMD
5. **attention**: 逐元素标量 dot product
6. **无并行**: forward() 的 _n_threads 参数未使用

## 优化步骤

### Phase 1: Q8_0 路径 — 复用已有 AVX2+并行基础设施

Qwen3.5 Q4_K_M 模型中，大部分权重是 Q4K/Q5K/Q6K 格式，但部分小张量可能是 Q8_0 或 F16。
先把 Q8_0 路径接上，因为 ops.rs 已有完整的 Q8_0 量化→Q8_0×Q8_0 AVX2 matmul 流水线。

**改动**:
- `QWeight::Q8_0` 分支: 不再反量化，改为输入量化到 Q8_0，调用 `matmul_q8_0_quantized`
- 需要在 qwen35 中引入 `ExecutionScratchpad` 或至少 Q8_0 量化缓冲区

### Phase 2: Q4K/Q5K/Q6K 专用 AVX2 kernel

这些是 Qwen3.5 的主力格式。llama.cpp 的做法是：每个 block 内用整数 SIMD 解量化+点积，
一块一块处理，不需要全量反量化。

**改动**:
- `QWeight::Q4K` 分支: 写 `matmul_q4k_q8k_avx2`，内部对每个 block 用 SIMD 解量化+整数点积
- `QWeight::Q5K` 分支: 写 `matmul_q5k_q8k_avx2`
- `QWeight::Q6K` 分支: 写 `matmul_q6k_q8k_avx2`
- 输入统一量化到 Q8K，复用 Q8K 的整数 SIMD 点积

**优先级**: Q6K > Q4K > Q5K（按模型中占比排序）

### Phase 3: F16/F32 路径 AVX2

**改动**:
- `QWeight::F16` 分支: 用 `dot_f16_f32_avx2` 做 on-the-fly 转换+累加
- `QWeight::F32` 分支: 用 `dot_f32_avx2`

### Phase 4: 多线程并行

**改动**:
- `forward()` 中每个 matmul 调用改为 `pool.compute()` 并行
- 参照 `run_inference` 的模式：量化→并行matmul→汇合
- 需要在 `Qwen35Model` 或 `forward()` 中持有 `ComputePool`

### Phase 5: Attention AVX2

**改动**:
- Q×K dot product: 用 `dot_f32_avx2` 替代标量循环
- Score×V 累加: 用 `vec_mad_f32_avx2` 替代标量循环
- Online softmax: 用 `vec_scale_f32_avx2`

## 预期效果

| Phase | 预期累计速度 | 说明 |
|-------|------------|------|
| 初始 | 2.4 tok/s | - |
| Phase 1 | ~5 tok/s | Q8_0 路径走 AVX2 |
| Phase 2 | ~15-20 tok/s | 主力格式走 AVX2，消除反量化开销 |
| Phase 4 | ~25-35 tok/s | 多线程并行 |
| Phase 5 | ~30-36 tok/s | Attention 也走 AVX2 |

## 实施顺序

**先做 Phase 2 + Phase 4** — 这两个是最大瓶颈。Q4K/Q6K 占模型权重 90%+，
多线程提供 4x 线性加速。Phase 1/3/5 是锦上添花。

实际操作：
1. 先让 `QWeight::matmul()` 支持 AVX2 dispatch（内部调用 ops.rs 的函数）
2. 给 `forward()` 加 ComputePool 并行
3. 写 Q4K/Q6K 专用 AVX2 kernel
