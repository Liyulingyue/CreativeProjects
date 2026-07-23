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
cargo run --release -- --model models/Qwen3-0.6B-Q8_0.gguf --prompt "The capital of France is" --max-tokens 30

# Interactive mode
cargo run --release -- --model models/Qwen3-0.6B-Q8_0.gguf
```

### CLI Options

| Flag | Default | Description |
|------|---------|-------------|
| `--model` | — | Path to GGUF file |
| `--prompt` | — | Input prompt (omit for interactive) |
| `--max-tokens` | 128 | Max tokens to generate |
| `--temp` | 0.6 | Sampling temperature |

### Debug Flags (env vars)

| Var | Description |
|-----|-------------|
| `VERBOSE=1` | Show top-10 tokens and logit stats |
| `DEBUG_LAYER=N` | Dump per-layer intermediate values for layer N |
| `DEBUG_POS=N` | Dump at position N |

## Example Output

```
$ cargo run --release -- --model models/Qwen3-0.6B-Q8_0.gguf --prompt "The capital of France is"
Output:  Paris. The capital of France is located in the southern part of France...

$ cargo run --release -- --model models/Qwen3-0.6B-Q8_0.gguf --prompt "2 + 3 ="
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
