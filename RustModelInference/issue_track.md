# Issue Track — RustModelInference Debugging Log

## Issue #1: Q/K Norm applied to entire vector instead of per-head (FIXED)

**Root Cause:** `attn_q_norm.weight` has shape `[128]` (head_dim), but `get_f32_tensor` was called with `n_embd_q=2048`, allocating a 2048-element Vec where only the first 128 are filled from tensor data. The remaining 1920 elements are 0.0.

**Effect:** `rms_norm_inplace(&mut q, &q_norm, eps)` with q.len()=2048, q_norm.len()=2048:
- `n = min(2048, 2048) = 2048`
- For i >= 128: `q[i] *= scale * q_norm[i] = scale * 0.0 = 0.0`
- **Q heads 1-15 zeroed out, K heads 1-7 zeroed out**

**Fix:** Apply Q/K norm per-head:
```rust
for h in 0..n_head {
    let off = h * n_embd_head_k;
    rms_norm_inplace(&mut q[off..off + n_embd_head_k], &q_norm, eps);
}
for h in 0..n_head_kv {
    let off = h * n_embd_head_k;
    rms_norm_inplace(&mut k_new[off..off + n_embd_head_k], &k_norm, eps);
}
```

**Lesson:** GGUF tensor shapes must be read from `TensorInfo.dims`, not assumed from model config. Per-head operations (norm, RoPE) must iterate heads explicitly.

---

## Issue #2: Double softmax in sampling pipeline (FIXED)

**Root Cause:** `sample_top_p()` calls `softmax()` internally, converting logits to probabilities. Then the outer code called `softmax()` again on the already-probability values.

**Effect:** Double softmax on probabilities: `softmax(softmax(logits))` produces a much more peaked distribution than intended, making sampling nearly greedy even with temperature > 0.

**Fix:** Rewrote sampling pipeline: softmax → top-k filter → top-p filter → renormalize → sample.

**Lesson:** Sampling functions should have clear contracts — either they consume logits and produce probabilities, or they work entirely in log-space. Mixing both is error-prone.

---

## Issue #3: sample_top_k / sample_top_p index confusion (FIXED)

**Root Cause:** `sample_top_k` sets non-top-k logits to -inf at their **original indices** (via `ids[]`), but `sample_top_p` then calls `softmax(logits[..n_valid])` which operates on **contiguous indices 0..n_valid**. These are NOT the same set.

**Fix:** Rewrote sampling to work on original indices: softmax on full vocab, sort by probability, zero out non-top-k and non-top-p tokens at their original positions, renormalize, then sample.

---

## Issue #4: Degenerate output — model generates "!" repeatedly (FIXED)

**Root Cause:** Issue #1 (Q/K norm zeroing heads 1-15 of Q and 1-7 of K) caused completely broken attention patterns. With most Q/K heads zeroed, attention collapsed to a degenerate state producing token "!" with probability ~1.0.

**Fix:** Issue #1 fix resolved this. After fix, model correctly predicts "Paris" (logit=17.4) for "The capital of France is", and "5" for "2 + 3 =".

**Verification:**
- "The capital of France is" → " Paris. The capital of France is located in the southern part of France..."
- "2 + 3 =" → " 5, 3 + 4 ="
- "Write a short poem about rain:" → " the way it falls, how it changes the landscape... The rain, a gentle dance, begins its descent..."

---

## Issue #5: Tokenizer decode — BPE byte encoding not reversed (FIXED)

**Root Cause:** `decode()` used `token.as_bytes()` which produces UTF-8 bytes of the GPT-2 BPE character representation (e.g., "Ġ" → [0xC4, 0xA0]) instead of the original bytes (e.g., "Ġ" → [0x20] = space).

**Fix:** Added `byte_decoder: HashMap<char, u8>` (inverse of `byte_encoder`) to `BPETokenizer`. `decode()` now maps each character through the byte_decoder to recover original bytes.

**Effect:** "ĠParis" now decodes to " Paris" instead of "ĠParis". Chinese tokens partially decode correctly (UTF-8 multi-byte splits across tokens remain a known edge case).

---

## Remaining Known Issues

- [ ] UTF-8 multi-byte characters split across token boundaries may produce replacement characters (U+FFFD)
- [ ] No chat template support — `<|im_start|>` encodes to multiple tokens, not a single special token
- [ ] Per-layer numerical alignment with llama.cpp not yet verified (model produces correct output but intermediate values not compared)
- [ ] `get_f32_tensor` called every forward pass — should cache norm weights
- [ ] Matmul allocates per-row — should use pre-allocated buffers consistently
