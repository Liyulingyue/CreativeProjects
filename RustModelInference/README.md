# RustModelInference

> 100% Pure Rust · mmap Zero-Copy · Qwen3-0.6B End-to-End Inference

## Overview

A from-scratch LLM inference engine that loads GGUF files via mmap and performs text generation. Built on five principles:

1. **Zero Heap Allocation in Hot Path** — `forward()` writes to pre-allocated `&mut [f32]`
2. **mmap Zero-Copy** — weights are `&'a [u8]` slices borrowed from `memmap2` regions
3. **Explicit Memory Lifetime** — all buffers are caller-provided
4. **Trait-Based Architecture** — operators and memory decoupled via traits
5. **No C/C++ FFI** — 100% pure Rust, including quantization kernels

**Working**: Qwen3-0.6B Q8_0 — full transformer forward pass with GQA attention, Q/K norm, SwiGLU MLP, BPE tokenizer, and temperature/top-k/top-p sampling.

## Quick Start

```bash
# Build
cargo build --release

# Inference
cargo run --release --bin rust-model-inference -- --model models/Qwen3-0.6B-Q8_0.gguf --prompt "The capital of France is" --max-tokens 30

# Interactive mode
cargo run --release --bin rust-model-inference -- --model models/Qwen3-0.6B-Q8_0.gguf
```

```bash
cargo run --release --bin rust-model-inference -- \
  --model Qwen3-Embedding-0.6B-Q8_0.gguf \
  --prompt "Hello, 世界! 123" --embedding --embedding-output raw --threads 1
```

### Apple Silicon (ARM64)

Apple Silicon builds natively and selects stable Rust NEON kernels automatically; Rosetta and external C/C++ libraries are not required. Scalar fallbacks remain available for operators without an ARM SIMD path.

```bash
cargo check --all-targets
cargo test --all-targets
cargo build --release --all-targets
cargo run --release --bin rust-model-inference -- --model models/Qwen3-0.6B-Q8_0.gguf --prompt "2 + 3 =" --max-tokens 4 --temp 0 --threads 8 --kv-cache f16 --bench
cargo run --release --bin micro-bench
```

For text inference and embeddings, `--threads` defaults to `min(available_parallelism, 8)` and can be set explicitly; dump-logits defaults to 1 and multimodal defaults to 8. The KV cache defaults to F16; fixed comparisons pass `--kv-cache f16` explicitly to match llama.cpp `-ctk f16 -ctv f16`. `--bench` reports prompt-processing (`BENCH: pp`) and token-generation (`BENCH: tg`) eval rates separately. For a fair CPU comparison, run llama.cpp with `llama-bench -ngl 0 -t 8`; `-ngl 99` uses the Metal backend and must be reported as a separate dataset. See [OPTIMIZATION.md](./OPTIMIZATION.md#rust-与-llamacpp-固定机器对比2026-08-10) for the pinned, self-contained llama.cpp setup.

On a fixed Apple Silicon performance machine, enforce the Q8_0 NEON gate explicitly:

```bash
cargo run --release --bin micro-bench -- --check
```

### CLI Options

| Flag | Default | Description |
|------|---------|-------------|
| `--model` | — | Path to GGUF file |
| `--prompt` | — | Input prompt (omit for interactive) |
| `--max-tokens` | 128 | Max tokens to generate |
| `--temp` | 0.6 | Sampling temperature |
| `--threads` | text/embedding: `min(available_parallelism, 8)`; dump-logits: 1; multimodal: 8 | Compute threads; explicitly configurable |
| `--kv-cache` | `f16` | KV cache type: `f16` or `f32` |
| `--bench` | off | Print separate `BENCH: pp` and `BENCH: tg` eval rates |
| `--embedding-output` | `summary` | Embedding display: `summary` or machine-readable `raw` |

### Debug Flags (env vars)

| Var | Description |
|-----|-------------|
| `VERBOSE=1` | Show top-10 tokens and logit stats |
| `DEBUG_LAYER=N` | Dump per-layer intermediate values for layer N |
| `DEBUG_POS=N` | Dump at position N |

## Example Output

```
$ cargo run --release --bin rust-model-inference -- --model models/Qwen3-0.6B-Q8_0.gguf --prompt "The capital of France is"
Output:  Paris. The capital of France is located in the southern part of France...

$ cargo run --release --bin rust-model-inference -- --model models/Qwen3-0.6B-Q8_0.gguf --prompt "2 + 3 ="
Output:  5, 3 + 4 = 
```

## Project Structure

```
src/
├── lib.rs        # Crate root, public re-exports
├── traits.rs     # Layer trait, ExecContext, ModelConfig
├── memory.rs     # PagedKVBlock, BlockAllocator, MemoryArena
├── quant.rs      # Q4_K_M block struct + dequantization kernel
├── model.rs      # GGUF V2/V3 mmap loader, QuantizedLinear<'a>, ModelGraph
├── ops.rs        # rms_norm, rope_neox, silu, softmax, matmul_q8_0, sampling
├── tokenizer.rs  # GPT-2 BPE tokenizer with byte-encoder/decoder
└── main.rs       # CLI + inference loop
```

## Qwen3-0.6B Parameters

| Parameter | Value |
|-----------|-------|
| Architecture | qwen3 |
| Embedding dim | 1024 |
| Layers | 28 |
| Attention heads (Q) | 16 |
| Attention heads (KV) | 8 (GQA) |
| Head dim (K/V) | 128 |
| Q dim | 2048 |
| FFN dim | 3072 |
| Context length | 40960 |
| Vocab size | 151,936 |
| RoPE freq base | 1,000,000 |
| Norm epsilon | 1e-6 |
| Q/K Norm | Yes (per-head RMSNorm) |

## Supported GGUF Features

- GGUF V2/V3 format parsing
- Q8_0 quantization (dequantize + matmul)
- Q4_K_M quantization (dequantize only)
- F32 tensors (norm weights, etc.)
- mmap zero-copy weight loading
- 310/310 tensor slices validated on Qwen3-0.6B-Q8_0

## Architecture

See [ARCHITECTURE.md](./ARCHITECTURE.md) for the full design document.

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `memmap2` | 0.9 | mmap zero-copy file loading |
| `half` | 2.4 | f16 for Q8_0 scale factor |

## Roadmap

- [ ] SIMD dequantization (AVX2 / NEON)
- [ ] Chat template support
- [ ] Quantized KV cache (f16)
- [ ] Continuous batching / multi-sequence
- [ ] More quant formats (Q4_K_M matmul, Q5_K, etc.)
- [ ] Per-layer numerical alignment tests vs llama.cpp

## License

MIT
