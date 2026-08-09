# ARM Mac 原生与 NEON 适配设计

## 目标

让 `rust-model-inference` 在 `aarch64-apple-darwin` 上原生编译、测试并使用现有 Qwen3-0.6B Q8_0 模型完成推理，同时：

- 保留所有非 SIMD 目标可用的标量回退；
- 保持现有 x86_64 AVX2/FMA/F16C 内核及其调度行为不变；
- 为 Apple Silicon 的主要推理热路径增加 NEON 实现；
- 增加可复现的 ARM 微基准、显式性能门禁和完整模型推理基准记录。

## 当前问题

在 Apple Silicon 主机上执行 `cargo check --all-targets` 会产生 23 个编译错误。阻塞点包括：

- `is_x86_feature_detected!` 在非 x86 目标上无条件展开；
- `std::arch::x86_64`、`__m256` 和 AVX2 `target_feature` 未全部受 `target_arch = "x86_64"` 保护；
- `ops.rs` 的部分通用调度函数在 ARM 构建中仍直接引用 AVX2 内核；
- `quant.rs` 和 `qwen35.rs` 在函数内部定义无架构保护的 AVX2 包装函数。

项目已经为多数算子保留标量实现，因此适配应修复统一调度点，而不是在每个调用者中增加 ARM 特判。

## 方案选择

采用目标相关静态派发并只移植实际热路径：

1. `x86_64` 保持当前 AVX2/FMA/F16C 路径；
2. `aarch64` 优先使用 NEON，Apple Silicon 支持时使用 FP16 和 DotProd 子路径；
3. 其他目标或不可用 CPU 特性使用现有标量实现。

不引入 SIMD trait、backend 工厂或第三方 SIMD 依赖。现有公开推理 API 不变。

## 架构设计

### CPU 特性与调度

`ops.rs` 继续作为公共算子入口。特性探测本身按目标编译：

- 只有 `x86_64` 构建会展开 `is_x86_feature_detected!`；
- 只有 `aarch64` 构建会展开 `is_aarch64_feature_detected!`；
- 非对应目标的查询函数直接返回 `false`；
- 公共算子按 `x86 SIMD -> ARM SIMD -> scalar` 的顺序选择实现。

ARM 内核处理完整 SIMD 块，剩余尾部元素交给相同算子的标量循环。所有裸指针加载都以调用入口已验证的切片长度为边界，不改变现有数据布局。

### NEON 覆盖范围

NEON 覆盖当前 Qwen3 Q8_0 推理和视觉编码中的主要计算热点：

- F32 点积、缩放、乘加、逐元素乘加；
- F16/F32 批量转换、F16-F32 点积和乘加；
- Q8_0 激活量化；
- Q8_0 权重与 Q8_0 激活的矩阵乘，包括 QKV、FFN 和 logits 共用的 range 路径；
- RMSNorm/LayerNorm 所需的求和、平方和与缩放；
- Vision Attention 的 QK 点积和加权 V 累加。

Q4_K、Q5_K、Q6_K 在 ARM 上先使用现有标量实现。它们必须能够编译并保持数值正确，但不属于本次 NEON 性能门禁；当前仓库可用的端到端验证模型是 Q8_0。

### x86_64 防退化约束

- AVX2/FMA/F16C 内核函数体不做算法修改；
- x86_64 的特性探测条件和调用顺序不变；
- ARM 内核通过 `cfg(target_arch = "aarch64")` 隔离，不进入 x86_64 产物；
- 在 ARM 主机上对 `x86_64-apple-darwin` 执行交叉编译检查。

ARM 主机无法提供可信的 x86 原生吞吐量测量，因此本次不宣称完成了 x86 硬件性能实测。保持 x86 内核主体和调度路径不变是本次防退化边界。

## 正确性验证

### 单元测试

使用固定输入分别调用标量参考实现和 ARM SIMD 实现，覆盖：

- 正数、负数、零值和量化边界值；
- 一个完整 SIMD 块和带标量尾部的向量长度；
- Q8_0 量化结果及 scale；
- Q8_0 矩阵乘的单行、多行和非整齐 row range；
- F32/F16 向量算子；
- Vision Attention 的 QK 与加权累加。

量化整数和 F16 转换在定义相同舍入规则时要求精确一致；浮点归约允许的误差为：

```text
abs(neon - scalar) <= 1e-4 + 1e-4 * abs(scalar)
```

### 构建与测试门禁

ARM Mac 上必须通过：

```bash
cargo check --all-targets
cargo test --all-targets
cargo run --release --
```

安装 x86_64 macOS Rust target 后必须通过：

```bash
cargo check --target x86_64-apple-darwin --all-targets
```

### 端到端推理冒烟

使用仓库本地模型执行确定性单 token 推理：

```bash
cargo run --release -- \
  --model models/Qwen3-0.6B-Q8_0.gguf \
  --prompt "2 + 3 =" \
  --max-tokens 1 \
  --temp 0 \
  --bench
```

命令必须正常退出，加载原生 ARM Release 二进制，并生成文档基线中的首 token ` 5`。

## ARM 性能基准

### 微基准

扩展现有 `micro-bench`，保留当前 Qwen3/MiniCPM 形状报告，并增加 ARM 的标量与自动派发对比：

- 使用固定种子生成输入；
- 为 Q8_0 权重生成合法 F16 scale 和 i8 数据，不再随机生成任意 scale 字节；
- 每个被测实现先预热；
- 标量与 NEON 交替执行 15 轮；
- 每轮包含固定次数的矩阵乘并使用 `black_box` 消除无效优化；
- 报告两者中位数、GFLOPS、GB/s 和加速比。

普通运行只输出数据：

```bash
cargo run --release --bin micro-bench
```

固定 Apple Silicon 性能机显式启用硬门禁：

```bash
cargo run --release --bin micro-bench -- --check
```

`--check` 使用 Qwen3 FFN 的 `1024 x 3072` 代表形状，要求：

```text
median_scalar / median_neon >= 1.10
```

不满足时返回非零退出码。非 `aarch64-apple-darwin` 目标使用 `--check` 时返回清晰错误；常规构建和测试不运行性能门禁。

### 完整模型基准

使用现有推理入口运行固定 prompt、线程数和生成 token 数，记录：

- Apple 芯片型号；
- macOS、Rust 和 LLVM 版本；
- Release 编译参数；
- 线程数、生成 token 数、总耗时和 tok/s；
- NEON 微基准相对标量的中位数加速比。

完整模型文件未纳入 Git，因此该基准作为本机验收和 `OPTIMIZATION.md` 的实测记录，不进入普通 `cargo test`。

## 文档更新

`README.md` 增加 Apple Silicon 原生构建、测试、推理和基准命令。`OPTIMIZATION.md` 增加本次实际运行得到的 ARM 环境与结果；未执行的 x86 硬件性能验证必须明确标注。

## 非目标

- 不引入 Metal、Accelerate、BLAS 或 C/C++ FFI；
- 不重构现有线程池或推理流水线；
- 不为 Q4_K/Q5_K/Q6_K 编写 NEON 内核；
- 不使用 Rosetta 结果代表 x86 原生性能；
- 不新增 CI 平台或托管性能机器。

## 验收标准

1. Apple Silicon 上所有 target 原生编译且现有测试通过；
2. 新增 NEON/标量数值一致性测试通过；
3. Qwen3-0.6B Q8_0 确定性推理冒烟通过；
4. `x86_64-apple-darwin` 交叉编译通过，原 AVX2/FMA/F16C 内核主体未发生算法变化；
5. ARM 微基准可重复输出标量、NEON 与加速比；
6. 固定 Apple Silicon 机器上的显式性能门禁达到 1.10 倍；
7. README 和优化记录准确反映实际执行结果与未验证边界。
