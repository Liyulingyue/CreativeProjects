use crate::model::{GGUFLoader, MetaValue};

#[derive(Debug, Clone)]
pub struct ClipVisionConfig {
    pub projection_dim: usize,
    pub image_size: usize,
    pub patch_size: usize,
    pub n_embd: usize,
    pub n_ff: usize,
    pub n_layer: usize,
    pub n_head: usize,
    pub spatial_merge_size: usize,
    pub eps: f32,
    pub use_gelu: bool,
    pub image_mean: [f32; 3],
    pub image_std: [f32; 3],
    pub has_deepstack_layers: Vec<bool>,
}

impl ClipVisionConfig {
    pub fn from_gguf(loader: &GGUFLoader) -> Result<Self, String> {
        let get_u32 = |key: &str| -> Result<u32, String> {
            loader.metadata(key)
                .and_then(|v| v.to_u64())
                .map(|v| v as u32)
                .ok_or_else(|| format!("Missing clip metadata: {}", key))
        };

        let get_f32 = |key: &str| -> Result<f32, String> {
            loader.metadata(key)
                .and_then(|v| v.to_f64())
                .map(|v| v as f32)
                .ok_or_else(|| format!("Missing clip metadata: {}", key))
        };

        let get_bool = |key: &str| -> bool {
            loader.metadata(key)
                .and_then(|v| match v { MetaValue::Bool(b) => Some(*b), _ => None })
                .unwrap_or(false)
        };

        let projection_dim = get_u32("clip.vision.projection_dim")? as usize;
        let image_size = get_u32("clip.vision.image_size")? as usize;
        let patch_size = get_u32("clip.vision.patch_size")? as usize;
        let n_embd = get_u32("clip.vision.embedding_length")? as usize;
        let n_ff = get_u32("clip.vision.feed_forward_length")? as usize;
        let n_layer = get_u32("clip.vision.block_count")? as usize;
        let n_head = get_u32("clip.vision.attention.head_count")? as usize;
        let spatial_merge_size = loader.metadata("clip.vision.spatial_merge_size")
            .and_then(|v| v.to_u64())
            .unwrap_or(2) as usize;
        let eps = get_f32("clip.vision.attention.layer_norm_epsilon")?;
        let use_gelu = get_bool("clip.use_gelu");

        let image_mean = match loader.metadata("clip.vision.image_mean") {
            Some(MetaValue::Array(_, vals)) => {
                let m: Vec<f32> = vals.iter()
                    .filter_map(|v| v.to_f64().map(|x| x as f32))
                    .collect();
                if m.len() == 3 {
                    [m[0], m[1], m[2]]
                } else {
                    [0.48145466, 0.4578275, 0.40821073]
                }
            }
            _ => {
                [0.48145466, 0.4578275, 0.40821073]
            }
        };

        let image_std = match loader.metadata("clip.vision.image_std") {
            Some(MetaValue::Array(_, vals)) => {
                let s: Vec<f32> = vals.iter()
                    .filter_map(|v| v.to_f64().map(|x| x as f32))
                    .collect();
                if s.len() == 3 {
                    [s[0], s[1], s[2]]
                } else {
                    [0.26862954, 0.26130258, 0.27577711]
                }
            }
            _ => {
                [0.26862954, 0.26130258, 0.27577711]
            }
        };

        let has_deepstack_layers = match loader.metadata("clip.vision.is_deepstack_layers") {
            Some(MetaValue::Array(_, vals)) => {
                vals.iter()
                    .filter_map(|v| match v { MetaValue::Bool(b) => Some(*b), _ => None })
                    .collect()
            }
            _ => vec![false; n_layer],
        };

        Ok(Self {
            projection_dim,
            image_size,
            patch_size,
            n_embd,
            n_ff,
            n_layer,
            n_head,
            spatial_merge_size,
            eps,
            use_gelu,
            image_mean,
            image_std,
            has_deepstack_layers,
        })
    }

    pub fn d_head(&self) -> usize {
        self.n_embd / self.n_head
    }

    pub fn n_patches_per_side(&self) -> usize {
        self.image_size / self.patch_size
    }

    pub fn n_patches(&self) -> usize {
        let ps = self.n_patches_per_side();
        ps * ps
    }

    pub fn n_output_tokens(&self) -> usize {
        let merge = self.spatial_merge_size;
        let ps = self.n_patches_per_side();
        (ps / merge) * (ps / merge)
    }
}

#[derive(Debug, Clone)]
pub struct Qwen35Config {
    pub n_embd: usize,
    pub n_layer: usize,
    pub n_head: usize,
    pub n_head_kv: usize,
    pub n_ff: usize,
    pub n_ctx: usize,
    pub vocab_size: usize,
    pub rope_freq_base: f32,
    pub norm_eps: f32,
    pub rope_dimension_count: usize,
    pub rope_dimension_sections: [i32; 4],
    pub ssm_d_conv: usize,
    pub ssm_d_state: usize,
    pub ssm_n_group: usize,
    pub ssm_dt_rank: usize,
    pub ssm_d_inner: usize,
    pub full_attention_interval: usize,
    pub is_recurrent: Vec<bool>,
    pub key_length: usize,
    pub value_length: usize,
}

impl Qwen35Config {
    pub fn from_gguf(loader: &GGUFLoader) -> Result<Self, String> {
        let get_u32 = |key: &str| -> Result<u32, String> {
            loader.metadata(key)
                .and_then(|v| v.to_u64())
                .map(|v| v as u32)
                .ok_or_else(|| format!("Missing qwen35 metadata: {}", key))
        };

        let get_f32 = |key: &str| -> Result<f32, String> {
            loader.metadata(key)
                .and_then(|v| v.to_f64())
                .map(|v| v as f32)
                .ok_or_else(|| format!("Missing qwen35 metadata: {}", key))
        };

        let n_embd = get_u32("qwen35.embedding_length")? as usize;
        let n_layer = get_u32("qwen35.block_count")? as usize;
        let n_head = get_u32("qwen35.attention.head_count")? as usize;
        let n_head_kv = get_u32("qwen35.attention.head_count_kv")? as usize;
        let n_ff = get_u32("qwen35.feed_forward_length")? as usize;
        let n_ctx = get_u32("qwen35.context_length")? as usize;

        let key_length = loader.metadata("qwen35.attention.key_length").and_then(|v| v.to_u64()).unwrap_or(n_embd as u64 / n_head as u64) as usize;
        let value_length = loader.metadata("qwen35.attention.value_length").and_then(|v| v.to_u64()).unwrap_or(n_embd as u64 / n_head as u64) as usize;

        let vocab_size = match loader.metadata("tokenizer.ggml.tokens") {
            Some(MetaValue::Array(_, vals)) => vals.len(),
            _ => 151936,
        };

        let rope_freq_base = loader.metadata("qwen35.rope.freq_base")
            .and_then(|v| v.to_f64())
            .unwrap_or(1_000_000.0) as f32;
        let norm_eps = get_f32("qwen35.attention.layer_norm_rms_epsilon")?;

        let rope_dimension_count = loader.metadata("qwen35.rope.dimension_count")
            .and_then(|v| v.to_u64())
            .unwrap_or(64) as usize;

        let rope_dimension_sections = match loader.metadata("qwen35.rope.dimension_sections") {
            Some(MetaValue::Array(_, vals)) => {
                let s: Vec<i32> = vals.iter()
                    .filter_map(|v| v.to_u64().map(|x| x as i32))
                    .collect();
                [s.get(0).copied().unwrap_or(16),
                 s.get(1).copied().unwrap_or(16),
                 s.get(2).copied().unwrap_or(16),
                 s.get(3).copied().unwrap_or(16)]
            }
            _ => {
                let sec = rope_dimension_count as i32 / 4;
                [sec, sec, sec, sec]
            }
        };

        let ssm_d_conv = get_u32("qwen35.ssm.conv_kernel")? as usize;
        let ssm_d_state = get_u32("qwen35.ssm.state_size")? as usize;
        let ssm_n_group = get_u32("qwen35.ssm.group_count")? as usize;
        let ssm_dt_rank = get_u32("qwen35.ssm.time_step_rank")? as usize;
        let ssm_d_inner = get_u32("qwen35.ssm.inner_size")? as usize;
        let full_attention_interval = loader.metadata("qwen35.full_attention_interval")
            .and_then(|v| v.to_u64())
            .unwrap_or(4) as usize;

        let is_recurrent: Vec<bool> = (0..n_layer)
            .map(|i| (i + 1) % full_attention_interval != 0)
            .collect();

        Ok(Self {
            n_embd,
            n_layer,
            n_head,
            n_head_kv,
            n_ff,
            n_ctx,
            vocab_size,
            rope_freq_base,
            norm_eps,
            rope_dimension_count,
            rope_dimension_sections,
            ssm_d_conv,
            ssm_d_state,
            ssm_n_group,
            ssm_dt_rank,
            ssm_d_inner,
            full_attention_interval,
            is_recurrent,
            key_length,
            value_length,
        })
    }

    pub fn n_embd_head(&self) -> usize {
        self.key_length
    }

    pub fn n_embd_gqa(&self) -> usize {
        self.n_head_kv * self.n_embd_head()
    }

    pub fn key_dim(&self) -> usize {
        self.ssm_d_state * self.ssm_n_group
    }

    pub fn value_dim(&self) -> usize {
        self.ssm_d_state * self.ssm_dt_rank
    }

    pub fn conv_dim(&self) -> usize {
        self.key_dim() * 2 + self.value_dim()
    }

    pub fn head_v_dim(&self) -> usize {
        self.ssm_d_inner / self.ssm_dt_rank
    }
}
