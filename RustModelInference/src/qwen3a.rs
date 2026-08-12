use crate::model::{GGMLType, MetaValue, TensorSource};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Qwen3AudioConfig {
    pub hidden: usize,
    pub ffn: usize,
    pub layers: usize,
    pub heads: usize,
    pub mel_bins: usize,
    pub projection: usize,
    pub epsilon: f32,
}

impl Qwen3AudioConfig {
    pub fn from_source(source: &dyn TensorSource) -> Result<Self, String> {
        validate_qwen3a_source(source)
    }
}

fn require_string(source: &dyn TensorSource, key: &str, expected: &str) -> Result<(), String> {
    match source.metadata(key) {
        Some(MetaValue::String(value)) if value == expected => Ok(()),
        _ => Err(format!(
            "Invalid Qwen3A metadata {key}: expected {expected}"
        )),
    }
}

fn require_bool(source: &dyn TensorSource, key: &str, expected: bool) -> Result<(), String> {
    match source.metadata(key) {
        Some(MetaValue::Bool(value)) if *value == expected => Ok(()),
        _ => Err(format!(
            "Invalid Qwen3A metadata {key}: expected {expected}"
        )),
    }
}

fn require_u32(source: &dyn TensorSource, key: &str, expected: u32) -> Result<u32, String> {
    match source.metadata(key) {
        Some(MetaValue::Uint32(value)) if *value == expected => Ok(*value),
        _ => Err(format!(
            "Invalid Qwen3A metadata {key}: expected {expected}"
        )),
    }
}

fn require_f32(source: &dyn TensorSource, key: &str, expected: f32) -> Result<(), String> {
    match source.metadata(key) {
        Some(MetaValue::Float32(value)) if *value == expected => Ok(()),
        _ => Err(format!(
            "Invalid Qwen3A metadata {key}: expected {expected}"
        )),
    }
}

fn require_tensor(
    source: &dyn TensorSource,
    name: &str,
    dims: &[u64],
    ggml_type: GGMLType,
) -> Result<(), String> {
    let info = source
        .tensor_info(name)
        .ok_or_else(|| format!("Missing Qwen3A tensor: {name}"))?;
    if info.dims != dims || info.ggml_type != ggml_type {
        return Err(format!(
            "Invalid Qwen3A tensor {name}: shape {:?} type {:?}; expected {:?} {:?}",
            info.dims, info.ggml_type, dims, ggml_type
        ));
    }
    Ok(())
}

pub(crate) fn validate_qwen3a_source(
    source: &dyn TensorSource,
) -> Result<Qwen3AudioConfig, String> {
    require_string(source, "general.architecture", "clip")?;
    require_string(source, "general.type", "mmproj")?;
    require_bool(source, "clip.has_audio_encoder", true)?;
    require_string(source, "clip.audio.projector_type", "qwen3a")?;
    let hidden = usize::try_from(require_u32(source, "clip.audio.embedding_length", 896)?)
        .map_err(|_| "clip.audio.embedding_length does not fit usize")?;
    let ffn = usize::try_from(require_u32(source, "clip.audio.feed_forward_length", 3584)?)
        .map_err(|_| "clip.audio.feed_forward_length does not fit usize")?;
    let layers = usize::try_from(require_u32(source, "clip.audio.block_count", 18)?)
        .map_err(|_| "clip.audio.block_count does not fit usize")?;
    let heads = usize::try_from(require_u32(source, "clip.audio.attention.head_count", 14)?)
        .map_err(|_| "clip.audio.attention.head_count does not fit usize")?;
    let mel_bins = usize::try_from(require_u32(source, "clip.audio.num_mel_bins", 128)?)
        .map_err(|_| "clip.audio.num_mel_bins does not fit usize")?;
    let projection = usize::try_from(require_u32(source, "clip.audio.projection_dim", 1024)?)
        .map_err(|_| "clip.audio.projection_dim does not fit usize")?;
    require_f32(source, "clip.audio.attention.layer_norm_epsilon", 1e-5)?;

    for i in 0..18 {
        let prefix = format!("a.blk.{i}");
        for name in ["attn_q", "attn_k", "attn_v", "attn_out"] {
            require_tensor(
                source,
                &format!("{prefix}.{name}.weight"),
                &[896, 896],
                GGMLType::Q8_0,
            )?;
            require_tensor(
                source,
                &format!("{prefix}.{name}.bias"),
                &[896],
                GGMLType::F32,
            )?;
        }
        for name in ["ln1", "ln2"] {
            require_tensor(
                source,
                &format!("{prefix}.{name}.weight"),
                &[896],
                GGMLType::F32,
            )?;
            require_tensor(
                source,
                &format!("{prefix}.{name}.bias"),
                &[896],
                GGMLType::F32,
            )?;
        }
        require_tensor(
            source,
            &format!("{prefix}.ffn_up.weight"),
            &[896, 3584],
            GGMLType::Q8_0,
        )?;
        require_tensor(
            source,
            &format!("{prefix}.ffn_up.bias"),
            &[3584],
            GGMLType::F32,
        )?;
        require_tensor(
            source,
            &format!("{prefix}.ffn_down.weight"),
            &[3584, 896],
            GGMLType::Q8_0,
        )?;
        require_tensor(
            source,
            &format!("{prefix}.ffn_down.bias"),
            &[896],
            GGMLType::F32,
        )?;
    }
    for (name, dims, ggml_type) in [
        ("a.position_embd.weight", &[896, 1500][..], GGMLType::F32),
        ("a.conv2d.1.weight", &[3, 3, 1, 480][..], GGMLType::F16),
        ("a.conv2d.1.bias", &[1, 1, 480][..], GGMLType::F32),
        ("a.conv2d.2.weight", &[3, 3, 480, 480][..], GGMLType::F16),
        ("a.conv2d.2.bias", &[1, 1, 480][..], GGMLType::F32),
        ("a.conv2d.3.weight", &[3, 3, 480, 480][..], GGMLType::F16),
        ("a.conv2d.3.bias", &[1, 1, 480][..], GGMLType::F32),
        ("a.conv_out.weight", &[7680, 896][..], GGMLType::F16),
        ("a.post_ln.weight", &[896][..], GGMLType::F32),
        ("a.post_ln.bias", &[896][..], GGMLType::F32),
        ("mm.a.mlp.1.weight", &[896, 896][..], GGMLType::Q8_0),
        ("mm.a.mlp.1.bias", &[896][..], GGMLType::F32),
        ("mm.a.mlp.2.weight", &[896, 1024][..], GGMLType::Q8_0),
        ("mm.a.mlp.2.bias", &[1024][..], GGMLType::F32),
    ] {
        require_tensor(source, name, dims, ggml_type)?;
    }

    Ok(Qwen3AudioConfig {
        hidden,
        ffn,
        layers,
        heads,
        mel_bins,
        projection,
        epsilon: 1e-5,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GGMLType, MetaValue, TensorInfo, TensorSource};
    use std::collections::HashMap;

    #[derive(Default)]
    struct MapTensorSource {
        metadata: HashMap<String, MetaValue>,
        tensors: HashMap<String, TensorInfo>,
    }

    impl TensorSource for MapTensorSource {
        fn metadata(&self, key: &str) -> Option<&MetaValue> {
            self.metadata.get(key)
        }

        fn tensor_info(&self, name: &str) -> Option<&TensorInfo> {
            self.tensors.get(name)
        }

        fn tensor_slice(&self, _name: &str) -> Option<&[u8]> {
            None
        }
    }

    fn add_tensor(
        source: &mut MapTensorSource,
        name: impl Into<String>,
        dims: &[u64],
        ggml_type: GGMLType,
    ) {
        let name = name.into();
        source.tensors.insert(
            name.clone(),
            TensorInfo {
                name,
                dims: dims.to_vec(),
                ggml_type,
                offset: 0,
            },
        );
    }

    fn valid_qwen3a_source() -> MapTensorSource {
        let mut source = MapTensorSource {
            metadata: HashMap::from([
                (
                    "general.architecture".into(),
                    MetaValue::String("clip".into()),
                ),
                ("general.type".into(), MetaValue::String("mmproj".into())),
                ("clip.has_audio_encoder".into(), MetaValue::Bool(true)),
                (
                    "clip.audio.projector_type".into(),
                    MetaValue::String("qwen3a".into()),
                ),
                ("clip.audio.embedding_length".into(), MetaValue::Uint32(896)),
                (
                    "clip.audio.feed_forward_length".into(),
                    MetaValue::Uint32(3584),
                ),
                ("clip.audio.block_count".into(), MetaValue::Uint32(18)),
                (
                    "clip.audio.attention.head_count".into(),
                    MetaValue::Uint32(14),
                ),
                ("clip.audio.num_mel_bins".into(), MetaValue::Uint32(128)),
                ("clip.audio.projection_dim".into(), MetaValue::Uint32(1024)),
                (
                    "clip.audio.attention.layer_norm_epsilon".into(),
                    MetaValue::Float32(1e-5),
                ),
            ]),
            tensors: HashMap::new(),
        };
        for i in 0..18 {
            let prefix = format!("a.blk.{i}");
            for name in ["attn_q", "attn_k", "attn_v", "attn_out"] {
                add_tensor(
                    &mut source,
                    format!("{prefix}.{name}.weight"),
                    &[896, 896],
                    GGMLType::Q8_0,
                );
                add_tensor(
                    &mut source,
                    format!("{prefix}.{name}.bias"),
                    &[896],
                    GGMLType::F32,
                );
            }
            for name in ["ln1", "ln2"] {
                add_tensor(
                    &mut source,
                    format!("{prefix}.{name}.weight"),
                    &[896],
                    GGMLType::F32,
                );
                add_tensor(
                    &mut source,
                    format!("{prefix}.{name}.bias"),
                    &[896],
                    GGMLType::F32,
                );
            }
            add_tensor(
                &mut source,
                format!("{prefix}.ffn_up.weight"),
                &[896, 3584],
                GGMLType::Q8_0,
            );
            add_tensor(
                &mut source,
                format!("{prefix}.ffn_up.bias"),
                &[3584],
                GGMLType::F32,
            );
            add_tensor(
                &mut source,
                format!("{prefix}.ffn_down.weight"),
                &[3584, 896],
                GGMLType::Q8_0,
            );
            add_tensor(
                &mut source,
                format!("{prefix}.ffn_down.bias"),
                &[896],
                GGMLType::F32,
            );
        }
        for (name, dims, ggml_type) in [
            ("a.position_embd.weight", &[896, 1500][..], GGMLType::F32),
            ("a.conv2d.1.weight", &[3, 3, 1, 480][..], GGMLType::F16),
            ("a.conv2d.1.bias", &[1, 1, 480][..], GGMLType::F32),
            ("a.conv2d.2.weight", &[3, 3, 480, 480][..], GGMLType::F16),
            ("a.conv2d.2.bias", &[1, 1, 480][..], GGMLType::F32),
            ("a.conv2d.3.weight", &[3, 3, 480, 480][..], GGMLType::F16),
            ("a.conv2d.3.bias", &[1, 1, 480][..], GGMLType::F32),
            ("a.conv_out.weight", &[7680, 896][..], GGMLType::F16),
            ("a.post_ln.weight", &[896][..], GGMLType::F32),
            ("a.post_ln.bias", &[896][..], GGMLType::F32),
            ("mm.a.mlp.1.weight", &[896, 896][..], GGMLType::Q8_0),
            ("mm.a.mlp.1.bias", &[896][..], GGMLType::F32),
            ("mm.a.mlp.2.weight", &[896, 1024][..], GGMLType::Q8_0),
            ("mm.a.mlp.2.bias", &[1024][..], GGMLType::F32),
        ] {
            add_tensor(&mut source, name, dims, ggml_type);
        }
        source
    }

    #[test]
    fn qwen3a_contract_accepts_only_the_fixed_model() {
        let expected = Qwen3AudioConfig {
            hidden: 896,
            ffn: 3584,
            layers: 18,
            heads: 14,
            mel_bins: 128,
            projection: 1024,
            epsilon: 1e-5,
        };
        assert_eq!(
            Qwen3AudioConfig::from_source(&valid_qwen3a_source()).unwrap(),
            expected
        );
    }

    #[test]
    fn qwen3a_contract_rejects_metadata_shape_and_type_drift() {
        let mut missing_metadata = valid_qwen3a_source();
        missing_metadata
            .metadata
            .remove("clip.audio.embedding_length");
        assert!(validate_qwen3a_source(&missing_metadata)
            .unwrap_err()
            .contains("clip.audio.embedding_length"));

        let mut wrong_projector = valid_qwen3a_source();
        wrong_projector.metadata.insert(
            "clip.audio.projector_type".into(),
            MetaValue::String("other".into()),
        );
        assert!(validate_qwen3a_source(&wrong_projector)
            .unwrap_err()
            .contains("clip.audio.projector_type"));

        let mut missing_tensor = valid_qwen3a_source();
        missing_tensor.tensors.remove("a.blk.0.attn_q.weight");
        assert!(validate_qwen3a_source(&missing_tensor)
            .unwrap_err()
            .contains("a.blk.0.attn_q.weight"));

        let mut wrong_shape = valid_qwen3a_source();
        wrong_shape
            .tensors
            .get_mut("a.conv_out.weight")
            .unwrap()
            .dims = vec![896, 7680];
        assert!(validate_qwen3a_source(&wrong_shape)
            .unwrap_err()
            .contains("a.conv_out.weight"));

        let mut wrong_type = valid_qwen3a_source();
        wrong_type
            .tensors
            .get_mut("a.post_ln.weight")
            .unwrap()
            .ggml_type = GGMLType::F16;
        assert!(validate_qwen3a_source(&wrong_type)
            .unwrap_err()
            .contains("a.post_ln.weight"));

        let mut wrong_projection = valid_qwen3a_source();
        wrong_projection
            .metadata
            .insert("clip.audio.projection_dim".into(), MetaValue::Uint32(512));
        assert!(validate_qwen3a_source(&wrong_projection)
            .unwrap_err()
            .contains("clip.audio.projection_dim"));
    }
}
