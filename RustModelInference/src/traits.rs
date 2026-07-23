use crate::memory::KVCacheView;

#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub n_embd: usize,
    pub n_layer: usize,
    pub n_head: usize,
    pub n_head_kv: usize,
    pub n_embd_head: usize,
    pub n_ff: usize,
    pub n_ctx: usize,
    pub vocab_size: usize,
    pub rope_freq_base: f32,
    pub norm_eps: f32,
}

impl ModelConfig {
    pub fn qwen2_0_6b() -> Self {
        Self {
            n_embd: 1024,
            n_layer: 24,
            n_head: 16,
            n_head_kv: 16,
            n_embd_head: 64,
            n_ff: 2816,
            n_ctx: 32768,
            vocab_size: 151936,
            rope_freq_base: 1_000_000.0,
            norm_eps: 1e-6,
        }
    }

    pub fn n_embd_gqa(&self) -> usize {
        self.n_head_kv * self.n_embd_head
    }

    pub fn scratch_size_per_layer(&self) -> usize {
        let attn_out = self.n_embd;
        let qkv = self.n_embd + 2 * self.n_embd_gqa();
        let ffn_inter = self.n_ff;
        qkv.max(attn_out).max(ffn_inter)
    }
}

pub struct ExecContext<'a> {
    pub scratch: &'a mut [f32],
    pub kv_cache: &'a mut KVCacheView,
    pub pos: u32,
    pub layer_idx: u32,
}

impl<'a> ExecContext<'a> {
    pub fn new(
        scratch: &'a mut [f32],
        kv_cache: &'a mut KVCacheView,
        pos: u32,
        layer_idx: u32,
    ) -> Self {
        Self {
            scratch,
            kv_cache,
            pos,
            layer_idx,
        }
    }
}

pub trait Layer {
    fn forward(&self, input: &[f32], output: &mut [f32], ctx: &mut ExecContext);

    fn input_dim(&self) -> usize;
    fn output_dim(&self) -> usize;
    fn name(&self) -> &str;
}
