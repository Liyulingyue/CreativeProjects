pub struct ExecutionScratchpad {
    pub x: Vec<f32>,
    pub normed: Vec<f32>,
    pub q: Vec<f32>,
    pub k_new: Vec<f32>,
    pub v_new: Vec<f32>,
    pub attn_out: Vec<f32>,
    pub attn_proj: Vec<f32>,
    pub down_buf: Vec<f32>,
    pub gate_buf: Vec<f32>,
    pub up_buf: Vec<f32>,
    pub logits: Vec<f32>,
    pub q8_buf: Vec<u8>,
    pub scale_buf: Vec<f32>,
    pub scores: Vec<f32>,
}

pub struct KvCacheF16 {
    pub k: Vec<u16>,
    pub v: Vec<u16>,
}

pub struct KvCacheF32 {
    pub k: Vec<f32>,
    pub v: Vec<f32>,
}

pub enum KvCache {
    F16(KvCacheF16),
    F32(KvCacheF32),
}

impl ExecutionScratchpad {
    pub fn new(
        n_embd: usize,
        n_embd_q: usize,
        n_embd_gqa: usize,
        n_ff: usize,
        vocab: usize,
        n_threads: usize,
        max_ctx: usize,
    ) -> Self {
        let max_n_in = n_embd_q.max(n_ff);
        Self {
            x: vec![0.0f32; n_embd],
            normed: vec![0.0f32; n_embd],
            q: vec![0.0f32; n_embd_q],
            k_new: vec![0.0f32; n_embd_gqa],
            v_new: vec![0.0f32; n_embd_gqa],
            attn_out: vec![0.0f32; n_embd_q],
            attn_proj: vec![0.0f32; n_embd],
            down_buf: vec![0.0f32; n_embd],
            gate_buf: vec![0.0f32; n_ff],
            up_buf: vec![0.0f32; n_ff],
            logits: vec![0.0f32; vocab],
            q8_buf: vec![0u8; max_n_in],
            scale_buf: vec![0.0f32; max_n_in / 32],
            scores: vec![0.0f32; n_threads * max_ctx],
        }
    }
}

impl KvCache {
    pub fn new_f16(n_layer: usize, max_ctx: usize, n_embd_gqa: usize) -> Self {
        let size = n_layer * max_ctx * n_embd_gqa;
        KvCache::F16(KvCacheF16 {
            k: vec![0u16; size],
            v: vec![0u16; size],
        })
    }

    pub fn new_f32(n_layer: usize, max_ctx: usize, n_embd_gqa: usize) -> Self {
        let size = n_layer * max_ctx * n_embd_gqa;
        KvCache::F32(KvCacheF32 {
            k: vec![0.0f32; size],
            v: vec![0.0f32; size],
        })
    }
}
