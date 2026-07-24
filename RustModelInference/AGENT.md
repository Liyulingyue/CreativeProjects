1. llamacpp的参考文件可以在 references 中找到
2. 模型文件在 models 目录下

## Project: RustModelInference
Rust-based LLM inference engine targeting precision and speed parity with llama.cpp.

### Model Under Test
- `models/Qwen3-0.6B-Q8_0.gguf` (Q8_0 quantized, GGUF V3)
- Architecture: qwen3, n_embd=1024, n_layer=28, n_head=16, n_head_kv=8, n_ff=3072
- n_embd_head_k=n_embd_head_v=128 (read from GGUF metadata, NOT 64)
- n_embd_q=2048, n_embd_gqa=1024, freq_base=1e6, eps=1e-6

### Precision Alignment (vs llama.cpp, Flash Attn disabled)
- argmax match: 100% across all test steps
- max_abs_diff: ~0.30 (comparable to llama.cpp's own Flash vs no-Flash diff of ~0.36)
- mean_abs_diff: ~0.05-0.06
- No logit diff > 1.0
- Top-5 tokens and ordering match
- Key difference source: llama.cpp uses f16 KV cache, Rust uses f32 (Rust is actually more precise)

### Speed Benchmark (Qwen3-0.6B-Q8_0, 128 gen tokens, --bench mode no chat template)
| Threads | Rust (tok/s) | llama.cpp (tok/s) | Ratio |
|---------|-------------|-------------------|-------|
| 1       | 21.4        | 24.8              | 86%   |
| 4       | 36.6        | 43.5              | 84%   |

### Profiling (4 threads, decode phase)
- qkv: 19.1%, ffn1: 29.5%, down: 12.7%, logits: 25.0%, wo: 8.9%, attn: 4.8%
- Matmul is ~95% of runtime (qkv+ffn1+down+logits+wo)
- Attention improved from 7.9% to 4.8% via online softmax + SIMD vec_mad/vec_scale

### Optimizations Applied
1. RM=4 register blocking kernel (4 weight rows per tile, shared input q8 load)
2. Packed f16→f32 via _mm_cvtph_ps (batch convert 4 deltas, broadcast with _mm256_shuffle_ps)
3. Software prefetching (_mm_prefetch T0) for next block's weight data
4. Raw pointer access to eliminate bounds checks in hot loop
5. Online softmax + SIMD vec_mad_f32/vec_scale_f32 for attention V accumulation
6. --bench mode (no chat template) for fair comparison

### Build
- `cargo build --release` (opt-level=3, lto=fat, codegen-units=1)
- llama.cpp: cmake Release at `references/llama.cpp/build/`
- CPU: Intel Core Ultra 5 125H (4P+8E+2LPE cores, AVX2+FMA, no AVX512)

### Key Files
- `src/main.rs`: inference loop + run_dump_logits + run_inference (with --bench, --profile flags)
- `src/ops.rs`: SIMD ops (rms_norm, matmul_q8_0_vs_q8_0_avx2, rope_neox, vec_mad_f32, vec_scale_f32, softmax, silu, quantize_q8_0)
- `src/model.rs`: GGUF parser, QuantizedLinear
- `src/tokenizer.rs`: BPETokenizer
- `src/traits.rs`: Layer trait, ModelConfig
- `src/memory.rs`: KV cache (f32)
- `src/thread_pool.rs`: ComputePool

### Remaining Gap Analysis (84% of llama.cpp)
- Logits matmul (25%): memory-bound at ~22 GB/s, near DDR bandwidth limit
- ffn1 (29.5%): two 1024→3072 matmuls + SiLU, compute-bound
- qkv (19.1%): three matmuls sharing input, compute-bound
- Possible improvements: graph-based execution (reduce dispatch overhead), f16 KV cache, quantized KV cache, NUMA-aware scheduling

### Previous Bug Fixes (see issue_track.md)
- #1: Q/K norm per-head (not per-tensor)
- #2: Double softmax
- #3: RoPE half-rotate
