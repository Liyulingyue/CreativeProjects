use crate::model::{MetaValue, TensorSource};

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
    pub image_min_pixels: usize,
    pub image_max_pixels: usize,
    pub eps: f32,
    pub use_gelu: bool,
    pub image_mean: [f32; 3],
    pub image_std: [f32; 3],
    pub has_deepstack_layers: Vec<bool>,
}

impl ClipVisionConfig {
    pub fn from_source<S: TensorSource + ?Sized>(source: &S) -> Result<Self, String> {
        let get_u32 = |key: &str| -> Result<u32, String> {
            source.metadata(key)
                .and_then(|v| v.to_u64())
                .map(|v| v as u32)
                .ok_or_else(|| format!("Missing clip metadata: {}", key))
        };

        let get_f32 = |key: &str| -> Result<f32, String> {
            source.metadata(key)
                .and_then(|v| v.to_f64())
                .map(|v| v as f32)
                .ok_or_else(|| format!("Missing clip metadata: {}", key))
        };

        let get_bool = |key: &str| -> bool {
            source.metadata(key)
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
        let spatial_merge_size = source.metadata("clip.vision.spatial_merge_size")
            .and_then(|v| v.to_u64())
            .map(usize::try_from)
            .transpose()
            .map_err(|_| "clip.vision.spatial_merge_size does not fit usize")?
            .unwrap_or(2);
        let factor = patch_size
            .checked_mul(spatial_merge_size)
            .ok_or("clip patch/merge factor overflow")?;
        let factor_pixels = factor
            .checked_mul(factor)
            .ok_or("clip patch/merge pixel factor overflow")?;
        let default_min = factor_pixels
            .checked_mul(8)
            .ok_or("clip minimum pixel count overflow")?;
        let default_max = factor_pixels
            .checked_mul(4096)
            .ok_or("clip maximum pixel count overflow")?;
        let image_min_pixels = source
            .metadata("clip.vision.image_min_pixels")
            .and_then(MetaValue::to_u64)
            .map(usize::try_from)
            .transpose()
            .map_err(|_| "clip.vision.image_min_pixels does not fit usize")?
            .unwrap_or(default_min);
        let image_max_pixels = source
            .metadata("clip.vision.image_max_pixels")
            .and_then(MetaValue::to_u64)
            .map(usize::try_from)
            .transpose()
            .map_err(|_| "clip.vision.image_max_pixels does not fit usize")?
            .unwrap_or(default_max);
        if image_min_pixels == 0 || image_min_pixels > image_max_pixels {
            return Err("Invalid clip vision pixel limits".into());
        }
        let eps = get_f32("clip.vision.attention.layer_norm_epsilon")?;
        let use_gelu = get_bool("clip.use_gelu");

        let image_mean = match source.metadata("clip.vision.image_mean") {
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

        let image_std = match source.metadata("clip.vision.image_std") {
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

        let has_deepstack_layers = match source.metadata("clip.vision.is_deepstack_layers") {
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
            image_min_pixels,
            image_max_pixels,
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
    pub recurrent: Qwen35RecurrentDimensions,
    pub full_attention_interval: usize,
    pub is_recurrent: Vec<bool>,
    pub key_length: usize,
    pub value_length: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Qwen35RecurrentDimensions {
    pub conv_width: usize,
    pub state_size: usize,
    pub group_count: usize,
    pub dt_rank: usize,
    pub inner_size: usize,
    pub key_dim: usize,
    pub value_dim: usize,
    pub conv_dim: usize,
    pub head_width: usize,
}

impl Qwen35RecurrentDimensions {
    fn new(
        conv_width: usize,
        state_size: usize,
        group_count: usize,
        dt_rank: usize,
        inner_size: usize,
    ) -> Result<Self, String> {
        if [conv_width, state_size, group_count, dt_rank, inner_size].contains(&0) {
            return Err("Invalid Qwen3.5 recurrent dimensions: values must be nonzero".into());
        }
        let key_dim = state_size
            .checked_mul(group_count)
            .ok_or("Invalid Qwen3.5 recurrent dimensions: key width overflow")?;
        let value_dim = state_size
            .checked_mul(dt_rank)
            .ok_or("Invalid Qwen3.5 recurrent dimensions: value width overflow")?;
        if inner_size != value_dim {
            return Err(format!(
                "Invalid Qwen3.5 recurrent dimensions: inner_size {inner_size} does not equal state_size * time_step_rank {value_dim}"
            ));
        }
        let head_width = inner_size.checked_div(dt_rank)
            .filter(|_| inner_size % dt_rank == 0)
            .ok_or("Invalid Qwen3.5 recurrent dimensions: inner_size is not divisible by time_step_rank")?;
        let conv_dim = key_dim
            .checked_mul(2)
            .and_then(|width| width.checked_add(value_dim))
            .ok_or("Invalid Qwen3.5 recurrent dimensions: convolution width overflow")?;
        Ok(Self {
            conv_width,
            state_size,
            group_count,
            dt_rank,
            inner_size,
            key_dim,
            value_dim,
            conv_dim,
            head_width,
        })
    }
}

fn unsigned_u64(value: &MetaValue) -> Option<u64> {
    match value {
        MetaValue::Uint8(value) => Some(*value as u64),
        MetaValue::Uint16(value) => Some(*value as u64),
        MetaValue::Uint32(value) => Some(*value as u64),
        MetaValue::Uint64(value) => Some(*value),
        _ => None,
    }
}

fn full_attention_interval(value: Option<&MetaValue>) -> Result<Option<u64>, String> {
    value
        .map(|value| {
            unsigned_u64(value)
                .ok_or_else(|| {
                    "Invalid qwen35.full_attention_interval: expected unsigned integer".into()
                })
        })
        .transpose()
}

fn recurrent_layer_mask(
    n_layer: usize,
    recurrent_layers: Option<&MetaValue>,
    full_attention_interval: Option<u64>,
) -> Result<Vec<bool>, String> {
    if let Some(value) = recurrent_layers {
        let MetaValue::Array(_, values) = value else {
            return Err("Invalid qwen35.attention.recurrent_layers: expected array".into());
        };
        if values.len() != n_layer {
            return Err(format!(
                "Invalid qwen35.attention.recurrent_layers length: expected {n_layer}, got {}",
                values.len()
            ));
        }
        return values
            .iter()
            .enumerate()
            .map(|(index, value)| match value {
                MetaValue::Bool(value) => Ok(*value),
                value => match unsigned_u64(value) {
                    Some(0) => Ok(false),
                    Some(1) => Ok(true),
                    _ => Err(format!(
                        "Invalid recurrent selector at layer {index}; expected 0, 1, or bool"
                    )),
                },
            })
            .collect();
    }

    let interval = full_attention_interval.unwrap_or(4);
    if interval == 0 {
        return Err("qwen35.full_attention_interval must be greater than zero".into());
    }
    Ok((0..n_layer)
        .map(|layer| ((layer as u64 + 1) % interval) != 0)
        .collect())
}

impl Qwen35Config {
    pub fn from_source<S: TensorSource + ?Sized>(source: &S) -> Result<Self, String> {
        let get_u32 = |key: &str| -> Result<u32, String> {
            let value = source
                .metadata(key)
                .and_then(|v| v.to_u64())
                .ok_or_else(|| format!("Missing qwen35 metadata: {key}"))?;
            u32::try_from(value).map_err(|_| format!("Qwen3.5 metadata does not fit u32: {key}"))
        };

        let get_f32 = |key: &str| -> Result<f32, String> {
            source.metadata(key)
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

        let key_length = source.metadata("qwen35.attention.key_length").and_then(|v| v.to_u64()).unwrap_or(n_embd as u64 / n_head as u64) as usize;
        let value_length = source.metadata("qwen35.attention.value_length").and_then(|v| v.to_u64()).unwrap_or(n_embd as u64 / n_head as u64) as usize;

        let vocab_size = match source.metadata("tokenizer.ggml.tokens") {
            Some(MetaValue::Array(_, vals)) => vals.len(),
            _ => 151936,
        };

        let rope_freq_base = source.metadata("qwen35.rope.freq_base")
            .and_then(|v| v.to_f64())
            .unwrap_or(1_000_000.0) as f32;
        let norm_eps = get_f32("qwen35.attention.layer_norm_rms_epsilon")?;

        let rope_dimension_count = source.metadata("qwen35.rope.dimension_count")
            .and_then(|v| v.to_u64())
            .unwrap_or(64) as usize;

        let rope_dimension_sections = match source.metadata("qwen35.rope.dimension_sections") {
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

        let recurrent = Qwen35RecurrentDimensions::new(
            get_u32("qwen35.ssm.conv_kernel")? as usize,
            get_u32("qwen35.ssm.state_size")? as usize,
            get_u32("qwen35.ssm.group_count")? as usize,
            get_u32("qwen35.ssm.time_step_rank")? as usize,
            get_u32("qwen35.ssm.inner_size")? as usize,
        )?;
        let full_attention_interval_raw =
            full_attention_interval(source.metadata("qwen35.full_attention_interval"))?;
        let is_recurrent = recurrent_layer_mask(
            n_layer,
            source.metadata("qwen35.attention.recurrent_layers"),
            full_attention_interval_raw,
        )?;
        let full_attention_interval = usize::try_from(full_attention_interval_raw.unwrap_or(4))
            .map_err(|_| "qwen35.full_attention_interval does not fit usize")?;

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
            recurrent,
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
        self.recurrent.key_dim
    }

    pub fn value_dim(&self) -> usize {
        self.recurrent.value_dim
    }

    pub fn conv_dim(&self) -> usize {
        self.recurrent.conv_dim
    }

    pub fn head_v_dim(&self) -> usize {
        self.recurrent.head_width
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MetaValueType;

    #[test]
    fn qwen35_recurrent_layers_metadata_is_authoritative() {
        let values = MetaValue::Array(
            MetaValueType::Uint32,
            [1, 0, 1, 0]
                .into_iter()
                .map(MetaValue::Uint32)
                .collect(),
        );
        assert_eq!(
            recurrent_layer_mask(4, Some(&values), Some(3)).unwrap(),
            vec![true, false, true, false],
        );
    }

    #[test]
    fn qwen35_recurrent_layers_reject_malformed_arrays() {
        let wrong_length = MetaValue::Array(
            MetaValueType::Uint32,
            vec![MetaValue::Uint32(1)],
        );
        assert!(recurrent_layer_mask(2, Some(&wrong_length), Some(4)).is_err());

        let invalid_selector = MetaValue::Array(
            MetaValueType::Uint32,
            vec![MetaValue::Uint32(1), MetaValue::Uint32(2)],
        );
        assert!(recurrent_layer_mask(2, Some(&invalid_selector), Some(4)).is_err());
    }

    #[test]
    fn qwen35_recurrent_layers_reject_float_selectors() {
        let float_selector = MetaValue::Array(
            MetaValueType::Float32,
            vec![MetaValue::Float32(0.5), MetaValue::Uint32(1)],
        );
        assert!(recurrent_layer_mask(2, Some(&float_selector), Some(4)).is_err());
    }

    #[test]
    fn qwen35_rejects_float_interval_with_recurrent_layers() {
        let recurrent_layers = MetaValue::Array(
            MetaValueType::Uint32,
            vec![MetaValue::Uint32(1), MetaValue::Uint32(0)],
        );
        let float_interval = MetaValue::Float32(4.0);
        assert!(full_attention_interval(Some(&float_interval))
            .and_then(|interval| recurrent_layer_mask(2, Some(&recurrent_layers), interval))
            .is_err());
    }

    #[test]
    fn qwen35_zero_interval_is_an_error_not_a_panic() {
        assert!(recurrent_layer_mask(4, None, Some(0)).is_err());
    }

    #[test]
    fn qwen35_interval_fallback_matches_llama() {
        assert_eq!(
            recurrent_layer_mask(8, None, Some(4)).unwrap(),
            vec![true, true, true, false, true, true, true, false],
        );
    }

    #[test]
    #[ignore = "requires RMI_QWEN35_MODEL"]
    fn qwen35_config_uses_real_layer_selection_metadata() {
        let path = std::env::var("RMI_QWEN35_MODEL").unwrap();
        let source = crate::open_model_source(
            std::path::Path::new(&path),
            crate::ComponentRole::Llm,
        )
        .unwrap();
        let config = Qwen35Config::from_source(source.as_ref()).unwrap();
        let expected = recurrent_layer_mask(
            config.n_layer,
            source.metadata("qwen35.attention.recurrent_layers"),
            full_attention_interval(source.metadata("qwen35.full_attention_interval")).unwrap(),
        )
        .unwrap();
        assert_eq!(config.is_recurrent, expected);
    }
}
