# Qwen3-ASR-0.6B 完整对齐设计

日期：2026-08-12

## 目标

在纯 Rust 推理引擎中完整支持本地 `models/qwen3-asr-0.6b` 的两份 GGUF：

- `Qwen3-ASR-0.6B-Q8_0.gguf`：Qwen3VL 文本 decoder。
- `mmproj-Qwen3-ASR-0.6B-Q8_0.gguf`：Qwen3A 音频 encoder 和 projector。

“完整支持”指同一条核心链路覆盖：

1. PCM16 WAV 解码。
2. Whisper 风格 log-Mel 前处理。
3. Qwen3A Conv2D、Transformer 和 projector。
4. ChatML audio placeholder 展开及 embedding 注入。
5. Qwen3VL interleaved mRoPE decoder greedy 生成。
6. CLI 转写。
7. OpenAI 风格非流式 `/v1/audio/transcriptions` 服务接口。

实现以固定 llama.cpp 参考为行为基线，并留下逐阶段可重复的 parity 检查。

## 固定输入与参考基线

本地模型固定为：

| 文件 | SHA-256 | 角色 |
| --- | --- | --- |
| `Qwen3-ASR-0.6B-Q8_0.gguf` | `bca259818b50ca7c4c05e9bdb35a5dc04fa039653a6d6f3f0f331f96f6aa1971` | LLM |
| `mmproj-Qwen3-ASR-0.6B-Q8_0.gguf` | `41a342b5e4c514e968cb756de6cd1b7be39eff43c44c57a2ef5fc6522e36603d` | audio mmproj |

权威运行时参考固定为 `ggml-org/llama.cpp` commit `9558fa44c92746a58dd07ad1bf0c889715b938a6`。主要参考路径：

- `tools/mtmd/mtmd-audio.cpp`：Qwen3A log-Mel 前处理。
- `tools/mtmd/models/qwen3a.cpp`：Conv2D、audio Transformer、projector。
- `tools/mtmd/mtmd.cpp` 和 `tools/mtmd/mtmd-helper.cpp`：音频输入、特殊 token 和 embedding 注入。
- `src/models/qwen3vl.cpp`：Qwen3VL decoder 和 interleaved mRoPE。

语言、prompt 和输出协议同时固定到官方模型 `Qwen/Qwen3-ASR-0.6B` revision `4ce9cc728b473a5aedbe7b6e1ea45646316824dc`；其 `support_languages`、chat template 和特殊 token 是该部分的权威来源。

模型目录不进入 Git；测试通过显式环境变量引用本地权重。

## 范围

### 首版包含

- RIFF/WAVE、little-endian PCM16、单声道、16 kHz。
- 非空合法 WAV，长度受 server 32 MiB 上传限制和 decoder context 上限约束。
- 自动语言识别，以及官方 30 个 canonical language name 与明确代码的强制语言映射。
- 可选 prompt。
- CLI 和 HTTP 共享同一个 `transcribe_wav` 核心。
- raw GGUF 与现有 GGUFRS 中 `Mmproj` 组件的等价加载。
- 单请求串行推理，避免共享 `ComputePool` 的调用状态竞态。

### 首版不包含

- MP3、FLAC、M4A 或其他容器。
- 多声道下混、自动重采样或非 PCM16 位深转换。
- 流式上传、流式转写、batch ASR。
- timestamps、word-level alignment、VAD。
- 任意音频模型框架或 modality trait。
- llama.cpp FFI 或子进程依赖。

不支持的格式必须明确报错，不做静默转换、截断或广播。

## 方案选择

采用纯 Rust 共享核心：

- 从当前 CLI 和 server 中抽出一份 Qwen3-family decoder，避免新增第三份推理循环。
- 新增只针对当前 GGUF contract 的 Qwen3A audio encoder。
- WAV 使用标准库解析；FFT 使用 `rustfft`；Axum 仅打开已有 crate 的 `multipart` feature。
- 复用现有 GGUF、GGUFRS、tokenizer、prompt、Q8_0、softmax、向量和线程池实现。
- 不泛化现有 `VisionEncoder`。它的 tensor contract、patch embedding 和位置编码都与 Qwen3A 不同。

## 组件边界

### `qwen3` 共享 decoder

共享 decoder 负责：

- 从 `TensorSource` 加载 Qwen3 与 Qwen3VL dense decoder 权重。
- 将 `qwen3vl` 识别为具有 Q/K RMSNorm 的 Qwen3-family 模型。
- 使用 GGUF architecture 前缀读取 metadata。
- 接收 token IDs 或调用者提供的 `[n_tokens, n_embd]` 输入 embedding。
- 接收每个 token 的四轴 position。
- 对 Qwen3VL 使用 `rope.dimension_sections=[24,20,20,0]` 的 interleaved mRoPE。
- prefill、KV cache、逐 token decode、greedy logits 选择和停止条件。

decoder 不读取 WAV，也不理解 audio token。它只消费 embedding、position 和生成选项。

### `asr` 核心

ASR 核心负责：

- 严格 WAV 解析。
- log-Mel 前处理。
- Qwen3A 权重和 shape 校验。
- 音频 encoder/projector 执行。
- audio prompt 构造和 embedding 严格替换。
- 调用共享 decoder。
- 解析模型的 `language ...<asr_text>...` 输出协议。

公共入口为等价于：

```text
transcribe_wav(runtime, wav_bytes, options) -> Transcription
```

CLI 和 server 不各自实现音频算法或 decoder 循环。

## 数据流

```text
WAV bytes
  -> PCM16 mono 16 kHz f32 samples
  -> centered STFT
  -> 128-bin normalized log-Mel
  -> 800-frame windows, padded to 100-frame chunks
  -> 3x Conv2D + conv_out
  -> learned audio positions
  -> 18-layer bidirectional audio Transformer
  -> 2-layer projector, 1024-d embeddings
  -> ChatML audio_pad slots replaced in place
  -> Qwen3VL interleaved mRoPE prefill
  -> greedy decode until im_end
  -> language and transcript parsing
```

## WAV 边界

解析器接受：

- `RIFF` 和 `WAVE` magic。
- `fmt ` 和 `data` chunk；允许并跳过未知 chunk。
- chunk 的 RIFF 偶数字节 padding。
- PCM format tag `1`。
- 1 channel、16000 Hz、16 bits per sample。
- `block_align=2`、`byte_rate=32000`。

所有 offset、长度和乘法使用 checked arithmetic。缺失 chunk、截断 chunk、任意重复的 `fmt ` 或 `data`、奇数 PCM 字节数、错误格式参数均返回结构化错误。

样本转换为 `i16 / 32768.0` 的 `f32`。首版不做重采样或下混。

## 音频前处理

与固定 llama.cpp Qwen3A 路径一致：

- sample rate：16000。
- FFT size：400。
- periodic Hann window：400。
- hop：160。
- center padding：首尾 reflection pad 200 samples。
- power spectrum。
- 128-bin Slaney Mel，`fmin=0`、`fmax=8000`。
- `log10`。
- 对完整音频先求全局最大值，再执行：

```text
normalized = (max(log_mel, global_max - 8) + 4) / 4
```

有效帧数为 `min(STFT 结果帧数, floor(n_samples / 160) + 1)`。

有效帧按最多 800 帧切 window；每个 window 补零到 100 帧的倍数。实现遵循 llama.cpp 的实际常量 `800/100`，不采用其过期注释中的 `3000/200`。

## Qwen3A encoder 与 projector

每个 100-frame、128-mel chunk 独立进入：

1. Conv2D `1 -> 480`，kernel 3、stride 2、padding 1、bias、GELU-erf。
2. Conv2D `480 -> 480`，同样参数。
3. Conv2D `480 -> 480`，同样参数。
4. 输出 shape 为 `[13,16,480]`。
5. 按 `feature = channel * 16 + mel` 重排为每个 time token 的 7680 维输入。
6. F16 `conv_out.weight` 执行 `7680 -> 896`，无 bias。

每个 100-frame chunk 产生 13 个 token。learned absolute position 在每个 chunk 内重置为 `0..12`，不跨 chunk 连续。

同一个 800-frame window 的全部 audio token 一起进入 18 层 Transformer：

- hidden 896。
- 14 heads，head dim 64。
- FFN 3584。
- normal LayerNorm，epsilon `1e-5`，带 bias。
- 分离 Q/K/V/O，带 bias。
- 双向 full self-attention，无 causal mask。
- pre-norm attention + residual。
- pre-norm GELU-erf FFN + residual。
- 最后执行 `a.post_ln`。

projector 为：

```text
Linear 896 -> 896 + bias
GELU-erf
Linear 896 -> 1024 + bias
```

每个 window 的 projected embeddings 按音频时间顺序拼接。

## Prompt、语言、embedding 注入与生成

从 tokenizer 按字面量解析，不硬编码 token ID：

- `<|audio_start|>`
- `<|audio_pad|>`
- `<|audio_end|>`
- `<|im_start|>`
- `<|im_end|>`
- `<asr_text>`

精确 prompt template 为：

```text
<|im_start|>system
{prompt-or-empty}<|im_end|>
<|im_start|>user
<|audio_start|><|audio_pad|> x N<|audio_end|><|im_end|>
<|im_start|>assistant
[language CanonicalName<asr_text>]
```

system 段即使为空也必须保留。`prompt` 是可选 system context，不放入 user/audio 段；为了不破坏 framing，其中出现 tokenizer 特殊控制 token 字面量时拒绝请求。方括号中的 assistant prefill 仅在强制语言时存在。

`N` 必须严格等于 projector 输出行数。先执行普通 token embedding lookup，再只将 `<|audio_pad|>` 对应的 N 行替换为 audio embeddings。数量或维度不一致直接报错。

`<|audio_start|>` 和 `<|audio_end|>` 保留普通 token embedding。所有 token 和 audio embeddings 都占用 decoder context position。

Qwen3VL audio position 退化为一维：每个位置的四轴值相同并单调递增。RoPE 仍必须使用 Qwen3VL interleaved section 规则，不能退回普通 Qwen3 NeoX RoPE。

生成固定为 greedy，temperature 等价于 0，遇模型 EOS `<|im_end|>` 停止。默认最大新 token 为 256，可由 CLI/API 覆盖为正整数，但必须受 context 上限约束。

强制语言的完整映射如下：

| Canonical | Code | Canonical | Code | Canonical | Code |
| --- | --- | --- | --- | --- | --- |
| Chinese | `zh` | English | `en` | Cantonese | `yue` |
| Arabic | `ar` | German | `de` | French | `fr` |
| Spanish | `es` | Portuguese | `pt` | Indonesian | `id` |
| Italian | `it` | Korean | `ko` | Russian | `ru` |
| Thai | `th` | Vietnamese | `vi` | Japanese | `ja` |
| Turkish | `tr` | Hindi | `hi` | Malay | `ms` |
| Dutch | `nl` | Swedish | `sv` | Danish | `da` |
| Finnish | `fi` | Polish | `pl` | Czech | `cs` |
| Filipino | `fil` | Persian | `fa` | Greek | `el` |
| Romanian | `ro` | Hungarian | `hu` | Macedonian | `mk` |

规则：

- 缺失、空字符串或纯空白表示自动识别；CLI 另外接受 `auto` 作为本地 sentinel，不传入模型。
- Canonical name 和表中 code 均 trim 后按 ASCII 大小写不敏感匹配，内部一律转成 canonical name。
- `yue` 是 ISO 639-3，`fil` 是 ISO 639-2/3；不把整张表误称为 ISO 639-1。
- 首版不接受 BCP-47 地区/脚本后缀、国家码或其他 alias；未知值不能静默退回 auto。
- 官方声明的 22 种中文方言不是可强制的 canonical label，只由自动识别路径输出：`Anhui`、`Dongbei`、`Fujian`、`Gansu`、`Guizhou`、`Hebei`、`Henan`、`Hubei`、`Hunan`、`Jiangxi`、`Ningxia`、`Shandong`、`Shaanxi`、`Shanxi`、`Sichuan`、`Tianjin`、`Yunnan`、`Zhejiang`、`Cantonese (Hong Kong accent)`、`Cantonese (Guangdong accent)`、`Wu language`、`Minnan language`。

未指定 language 时，解析：

```text
language <CanonicalLanguageName><asr_text><transcript>
```

元数据和 `<asr_text>` 之间允许换行与空白，但自动识别输出缺少 tag、缺少 `language ` 前缀，或语言不在 30 个 canonical name 和 22 个 detected-only dialect label 中，均视为模型协议错误。`language None<asr_text>` 且 transcript 为空时返回空转写和未知语言。

指定 language 时在 assistant 前缀后预填 `language NAME<asr_text>`，只生成 transcript，内部 language 直接使用已验证的 canonical name。对 transcript 只去除 framing 和首尾空白，不做重复文本等启发式改写。

对外只返回 transcript；内部结果同时保留检测到的 language、token IDs 和计数，供测试与调试使用。

## CLI 契约

示例：

```bash
cargo run --release --bin rust-model-inference -- \
  --model models/qwen3-asr-0.6b/Qwen3-ASR-0.6B-Q8_0.gguf \
  --mmproj models/qwen3-asr-0.6b/mmproj-Qwen3-ASR-0.6B-Q8_0.gguf \
  --audio sample.wav \
  --language English \
  --max-tokens 256 \
  --threads 8
```

规则：

- `--audio` 启用 ASR。
- raw GGUF 下 `--mmproj` 必填；GGUFRS 可从同一包读取 `Mmproj`。
- 显式 `--mmproj` 始终覆盖 GGUFRS 内置组件。
- `--prompt` 作为可选 system context。
- `--audio` 与 `--image`、`--embedding`、`--dump-logits`、`--bench`、`--profile` 互斥；ASR 忽略文本模式的默认 temperature 并强制 greedy，显式传入非零 `--temp` 则拒绝。
- stdout 最终输出 transcript；诊断与性能信息写 stderr。
- 非法组合在读取模型或执行推理前退出并返回非零状态。

## Server 契约

启动：

```bash
cargo run --release --bin server -- \
  --model model.gguf \
  --mmproj mmproj.gguf \
  --host 127.0.0.1 \
  --port 8080
```

端点：

```http
POST /v1/audio/transcriptions
Content-Type: multipart/form-data
```

字段：

- `file`：必填 WAV bytes。
- `model`：可选；提供时必须与已加载 model 路径的 file stem 相同。
- `language`：可选。
- `prompt`：可选。
- `max_tokens`：可选正整数，默认 256。
- `response_format`：可选，首版只接受 `json`。
- `stream`：可选，首版只接受 `false`。

成功响应：

```json
{"text":"transcript"}
```

错误状态：

- 400：缺字段、重复字段或 malformed multipart。
- 413：multipart body 超过 32 MiB。
- 415：不是受支持的 WAV/PCM contract。
- 422：语言、模型、response format、stream、音频内容或 context 不可处理。
- 503：server 未加载 audio mmproj。
- 500：内部推理错误或 blocking worker 失败。

server 不写临时文件。显式 `--mmproj` 覆盖 bundled `Mmproj`；未加载 mmproj 时 chat completion 仍可用，ASR 返回 503。chat completion 与 ASR 共享一个推理互斥锁，锁覆盖整次模型调用，避免共享 `ComputePool` 的 call state 被并发覆盖。HTTP 解析不持锁。

## GGUF 与 GGUFRS

现有 `ComponentRole::Mmproj` 同时承载 vision 或 audio mmproj。无需新增 role、无需升级 GGUFRS 版本、无需改变 segment 语义。

LLM 导出校验的 architecture 白名单增加 `qwen3vl`，仍使用现有 LLM shared/layer segment 规划。

mmproj 校验按 metadata 分支：

- vision：沿用现有 `clip.vision.*` 和 vision tensor contract。
- audio：要求 `general.architecture=clip`、`general.type=mmproj`、`clip.has_audio_encoder=true`、`clip.audio.projector_type=qwen3a`，并校验 Qwen3A 必需 metadata、tensor 名、shape 和支持的 GGML type。

导出仍使用现有 `--mmproj`。raw GGUF 和从 GGUFRS 加载的 `TensorSource` 必须产生相同 metadata、tensor bytes 和转写结果。

## 错误与资源边界

- 所有公开输入长度使用 checked arithmetic。
- 空音频、NaN/Inf 中间结果和模型输出协议错误均报错。
- audio embedding 数、placeholder 数、projection dim 和 decoder dim 必须严格相等。
- prompt + audio token + generation budget 超过 context 时，在分配 KV cache 前拒绝。
- server 上传上限为 32 MiB；CLI 仍受 WAV parser 和 context 上限约束。
- 每个 800-frame window 独立运行 encoder，避免音频 attention 形成全音频长度的二次方内存。
- encoder scratch 按一个 window 复用；projected embeddings 只保留 decoder 所需的最终 1024 维结果。
- 不记录 WAV 内容或 multipart bytes。

## 测试与 parity

### 单元测试

- WAV：合法最小 PCM16、未知 chunk、偶数 padding、截断、重复 chunk、错误采样率/声道/位深。
- DSP：固定 impulse、silence、440 Hz，校验 frame count、Mel shape、有限值和固定 checkpoint。
- Conv2D：小型可手算 input/weight/bias，覆盖 stride、padding和通道布局。
- LayerNorm、GELU-erf、audio attention、projector 的小型实值检查。
- fake `TensorSource`：metadata/tensor 缺失、shape/type 错误、projection dim 不匹配。
- prompt：空 system 段保留、特殊 token 展开、严格 embedding 替换、30 种语言映射、未知值拒绝、context 上限。
- decoder：`qwen3vl` family dispatch、Q/K norm、interleaved mRoPE positions。
- GGUFRS：vision 分支不回归、audio mmproj byte-exact round trip。
- server：缺文件、超限、坏 WAV、未配置 mmproj、成功 JSON、推理互斥。

### 固定 llama.cpp 分阶段 parity

最终验收 WAV 固定为官方 `asr_en.wav` 的严格输入版：

- 源文件：`https://qianwen-res.oss-cn-beijing.aliyuncs.com/Qwen3-ASR-Repo/asr_en.wav`，SHA-256 `f9b4440ac8393e47c14a6240e9739dea09b645bb1592b8f2dd48feb9666cea7f`，原始为 24-bit mono 48 kHz，不直接进入首版运行时。
- 使用 FFmpeg 8.1.2 执行 `ffmpeg -i asr_en.wav -map_metadata -1 -ac 1 -ar 16000 -c:a pcm_s16le asr_en-pcm16-16khz.wav`。
- 派生文件大小 481718 bytes，SHA-256 `23775909b26f2ebb1ccf0b877e7590b2cc31700a94bccf2d4111b98e9595acd8`。parity 仅接受该精确 bytes，测试通过环境变量引用，不在 Git 中提交音频。

在临时、固定 SHA 的 llama.cpp checkout 中使用 CPU、`--threads 1 --threads-batch 1`、`--flash-attn off`、`--no-mmproj-offload`，并只为 oracle 增加环境变量门控的二进制 dump。Qwen3A Mel 实现内部固定使用 4 个 frame worker，因此不声称全过程单线程；同 binary/硬件重复运行必须仍为 bitwise deterministic。项目工作区不 vendoring 或修改 llama.cpp。

固定同一 WAV 和模型，对比：

1. decoded PCM F32。
2. raw log10 Mel。
3. normalized Mel。
4. padded Mel window。
5. `after_conv_blocks`。
6. `after_conv_out`。
7. audio Transformer layer 0 和最终输出。
8. `projected` audio embeddings。
9. prompt token IDs 和四轴 positions。
10. decoder 首 token logits/argmax。
11. 最终 generated token IDs 和 transcript。

容差指标定义：

```text
NRMSE = ||got-ref||₂ / max(||ref||₂, 1e-12)
cos = dot(got, ref) / max(||got||₂ ||ref||₂, 1e-12)
p99_scaled_abs = p99(|got-ref|) / max(RMS(ref), 1e-6)
```

Rust 与 llama.cpp 的首版 hard gates：

| 阶段 | 容差 |
| --- | --- |
| PCM16 解码 | sample count 和 F32 bytes exact |
| raw log10 Mel | `p99_abs <= 3e-4`，`max_abs <= 1e-3` |
| normalized Mel | `p99_abs <= 1e-4`，`max_abs <= 5e-4` |
| `after_conv_blocks` | `cos >= 0.9999`，`NRMSE <= 1.5e-2`，`p99_scaled_abs <= 2e-2` |
| `after_conv_out` | `cos >= 0.9999`，`NRMSE <= 2e-2`，`p99_scaled_abs <= 2e-2` |
| Transformer layer 0 | `cos >= 0.9999` |
| final Transformer / `projected` | global `cos >= 0.999`，`NRMSE <= 4e-2`，非零行 `p01(row_cos) >= 0.99` |
| centered logits | `cos >= 0.9995`，`NRMSE <= 3e-2`，`p99_abs <= 0.10`，`max_abs <= 0.30`，top-10 overlap `>= 9/10` |

GGUF hash、tensor bytes/shape/layout、Mel frame/window/padding 计数、padding 零值、audio token 数与顺序、chunk position reset、attention mask 协议、prompt token IDs、四轴 positions 和 embedding 注入槽位必须 exact。浮点 conv/Transformer/projector/logits 不要求跨实现 bitwise 相等。

首 token 使用 margin-aware 诊断：参考 top-1 与 top-2 margin `>= 0.10` 时 argmax 必须相同，否则 Rust argmax 必须位于参考 top-3。用于最终验收的固定 WAV 必须先确认每步 margin 稳定，再对 generated token IDs 和 transcript 做 exact 门禁；近似并列的样本只用于数值诊断，不充当文本验收 fixture。

同一 llama.cpp binary、CPU/ISA、参数和输入的两次 oracle 运行必须在所有 pre-sampling dump 上 bitwise exact；否则先修复 oracle 稳定性，不放宽 Rust 容差。

### 验证门禁

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo check --all-targets --features parity-trace
RUSTFLAGS='' cargo check --target x86_64-apple-darwin --all-targets
cargo run --release --bin micro-bench -- --check
```

真实模型测试使用显式环境变量并标记 ignored，至少覆盖：

- CLI 与 server transcript/token IDs 一致。
- raw GGUF 与 GGUFRS 一致。
- 固定 llama.cpp baseline 的逐阶段 parity。
- 固定 WAV 的 audio token 数、运行时间和峰值 RSS 被记录，未证明的性能不写成结论。

## 验收标准

以下条件全部满足才称为完整对齐：

1. 两份固定 GGUF 均能被严格加载。
2. 合法 PCM16 mono 16 kHz WAV 可通过 CLI 完成转写。
3. 同一 WAV 可通过 `/v1/audio/transcriptions` 返回相同 transcript。
4. PCM、Mel、encoder/projector、positions、首 token 和最终 token/text 都有固定 llama.cpp 对比证据。
5. Qwen3VL 使用 Q/K RMSNorm 与 interleaved mRoPE，不以普通 Qwen3 alias 代替。
6. 错误格式、embedding 数量不一致、context 超限和未配置 mmproj 都安全失败。
7. raw GGUF 与 GGUFRS 行为一致。
8. 全量 Rust 门禁通过；任何 baseline 或环境失败被单独报告，不冒充回归。
