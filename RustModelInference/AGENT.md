1. llamacpp的参考文件可以在 references 中找到
2. 模型文件在 models 目录下

## Project: RustModelInference
Rust-based LLM inference engine targeting precision and speed parity with llama.cpp, then surpassing it via Rust trait-based safety.

### Model Under Test
- `models/Qwen3-0.6B-Q8_0.gguf` (Q8_0 quantized, GGUF V3)
- Architecture: qwen3, n_embd=1024, n_layer=28, n_head=16, n_head_kv=8, n_ff=3072
- n_embd_head_k=n_embd_head_v=128 (read from GGUF metadata, NOT 64)
- n_embd_q=2048, n_embd_gqa=1024, freq_base=1e6, eps=1e-6
- GQA with group_size=2 (2 Q heads share 1 KV head)

### Precision Alignment (vs llama.cpp, Flash Attn disabled)
- argmax match: 100% across all test steps
- max_abs_diff: ~0.30 (comparable to llama.cpp's own Flash vs no-Flash diff of ~0.36)
- mean_abs_diff: ~0.05-0.06
- No logit diff > 1.0
- Top-5 tokens and ordering match

### Speed Benchmark (Qwen3-0.6B-Q8_0, 128 gen tokens, --bench mode no chat template)
| Threads | Rust (tok/s) | llama.cpp (tok/s) | Ratio |
|---------|-------------|-------------------|-------|
| 1       | ~9.5        | ~10               | ~95%  |
| 4       | 26.3        | 36.0              | 73%   |
| 6       | 30.7        | -                 | -     |
| 8       | 31.6        | -                 | 88%*  |

*8 threads vs llama.cpp 4 threads

### Profiling (8 threads, decode phase, 128 tokens)
- FFN = 42.1% (Gate+Up+SiLU + quantize + Down)
- QKV+attn = 25.6% (QKV matmul + single-threaded RoPE+KVwrite + attention)
- logits = 23.5% (151936×1024 output projection, memory-bound)
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

### Multi-threaded Architecture
- **ComputePool**: BSP model, epoch-based dispatch, `fence(SeqCst)`, no internal spin-barriers
- **7-step pipeline per layer**: (1) pool QKV → (2) main RoPE+KVwrite → (3) pool attn → (4) main quantize → (5) pool Wo+FFN1 → (6) main quantize → (7) pool Down
- **GQA correctness**: KV writes single-threaded on main thread to avoid write-write race
- **Epoch race fix**: worker `my_epoch = 0` hardcoded (not `epoch.load(Acquire)`)

### Build
- `cargo build --release` (opt-level=3, lto=fat, codegen-units=1)
- llama.cpp: cmake Release at `references/llama.cpp/build/`
- Run llama.cpp: `LD_LIBRARY_PATH=references/llama.cpp/build/bin references/llama.cpp/build/bin/llama-cli`
- CPU: Intel Core Ultra 5 125H (4P+8E+2LPE cores, AVX2+FMA, no AVX512)

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

### Key Files
- `src/main.rs`: inference loop, `run_inference` (7-step BSP), `run_dump_logits`, CLI flags
- `src/ops.rs`: SIMD ops — `matmul_q8_0_vs_q8_0_avx2`, `dot_f16_f32`, `vec_mad_f16_f32`, `f32_slice_to_f16`, `rms_norm`, `softmax`, `quantize_q8_0_into`, `rope_neox`, `silu`
- `src/model.rs`: GGUF parser, `QuantizedLinear`
- `src/tokenizer.rs`: BPETokenizer
- `src/thread_pool.rs`: `ComputePool` (BSP model, epoch-based dispatch)
- `src/traits.rs`: Layer trait, ModelConfig
- `src/memory.rs`: BlockAllocator, MemoryArena

### Design Spec (see ARCHITECTURE.md for full details)
- **Zero-heap allocation in hot path**: `forward()` takes `&mut [f32]` output, no `Vec`/`Box`
- **Physical zero-copy flat view**: `QuantizedTensorView<'a>` with `&'a [u8]` pointing to mmap
- **Unified f32 scratchpad arena**: `ExecutionScratchpad` for all hot-path buffers
- **Fine-grained hybrid quantization**: `MatMulOp` trait + `QuantizedLayer` enum for static dispatch
- **`.ggufrs` single-file heterogeneous scheduling**: Vision + LLM segments in one file
- **RAII drop for vision encoder resources**: Explicit scope-based lifetime for NPU/GPU handles

### Remaining Gap Analysis (88% of llama.cpp at 8 threads)
- Pool barrier overhead (~5-10%): 7 `pool.compute()` calls per layer × 28 layers = ~196 barriers/token
- logits matmul (23.5%): memory-bound at ~22 GB/s, near DDR bandwidth limit
- FFN (42.1%): two 1024→3072 matmuls + SiLU, compute-bound
- Possible improvements: merge pool.compute calls, f16 KV cache (done), quantized KV cache, NUMA-aware scheduling

### Previous Bug Fixes (see issue_track.md)
- #1: Q/K norm per-head (not per-tensor)
- #2: Double softmax
- #3: RoPE half-rotate
- #4: GQA write-write race (single-threaded KV writes)
- #5: FFN barrier race (removed internal spin-barriers)
- #6: ComputePool epoch race (my_epoch=0 hardcoded)
