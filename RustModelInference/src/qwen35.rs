use crate::clip_config::Qwen35Config;
use crate::vision::VisionGrid;
use crate::{
    ComponentId, ComponentRequirements, ComponentWorkload, ExecutionRun, GGMLType, KvCacheType,
    LlmLayerSpec, LlmRequirements, PlacementMode, Qwen35DenseLayerSpec, Qwen35RecurrentLayerSpec,
    TensorCatalog, TensorId,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub fn build_qwen35_positions(
    token_ids: &[u32],
    image_token_id: Option<u32>,
    image_grids: &[VisionGrid],
) -> Result<(Vec<[usize; 4]>, usize), String> {
    let mut positions = Vec::with_capacity(token_ids.len());
    let mut next = 0_usize;
    let mut token = 0_usize;
    let mut grid_index = 0_usize;

    while token < token_ids.len() {
        if image_token_id == Some(token_ids[token]) {
            let grid = *image_grids
                .get(grid_index)
                .ok_or("Image placeholder has no matching vision grid")?;
            let count = grid.checked_token_count()?;
            let end = token
                .checked_add(count)
                .ok_or("Image placeholder range overflow")?;
            if end > token_ids.len()
                || token_ids[token..end]
                    .iter()
                    .any(|id| Some(*id) != image_token_id)
            {
                return Err(format!(
                    "Image grid {grid_index} requires {count} contiguous placeholders"
                ));
            }
            let base = next;
            for image_index in 0..count {
                let row = image_index / grid.grid_w;
                let column = image_index % grid.grid_w;
                positions.push([base, base + row, base + column, 0]);
            }
            next = next
                .checked_add(grid.position_span())
                .ok_or("Qwen3.5 logical position overflow")?;
            token = end;
            grid_index += 1;
        } else {
            positions.push([next, next, next, 0]);
            next = next
                .checked_add(1)
                .ok_or("Qwen3.5 logical position overflow")?;
            token += 1;
        }
    }

    if grid_index != image_grids.len() {
        return Err(format!(
            "Unused vision grids: consumed {grid_index}, provided {}",
            image_grids.len()
        ));
    }
    Ok((positions, next))
}

pub struct Qwen35Model {
    pub config: Qwen35Config,
    tensor_ids: Qwen35TensorIds,
    cpu_row_matrices: BTreeMap<TensorId, CpuRowMatrix>,
    row_weights: Qwen35RowWeights,
    row_state: Arc<Mutex<Qwen35RowState>>,
}

enum CpuRowMatrix {
    F32 {
        values: Vec<f32>,
        input_width: usize,
        rows: usize,
    },
    F16 {
        values: Vec<f32>,
        input_width: usize,
        rows: usize,
    },
    Q4K {
        values: Vec<u8>,
        input_width: usize,
        rows: usize,
    },
    Q5K {
        values: Vec<u8>,
        input_width: usize,
        rows: usize,
    },
    Q6K {
        values: Vec<u8>,
        input_width: usize,
        rows: usize,
    },
}

#[derive(Debug, Clone)]
struct Qwen35TensorIds {
    token_embedding: TensorId,
    output: TensorId,
    final_norm: TensorId,
    layers: Vec<Qwen35LayerTensorIds>,
    q8_input_widths: BTreeMap<TensorId, usize>,
}

#[derive(Debug, Clone)]
enum Qwen35LayerTensorIds {
    Dense {
        attn_norm: TensorId,
        post_attn_norm: TensorId,
        q_norm: TensorId,
        k_norm: TensorId,
        q: TensorId,
        k: TensorId,
        v: TensorId,
        o: TensorId,
        gate: TensorId,
        up: TensorId,
        down: TensorId,
    },
    Recurrent {
        attn_norm: TensorId,
        post_attn_norm: TensorId,
        qkv: TensorId,
        gate: TensorId,
        beta: TensorId,
        alpha: TensorId,
        conv_weight: TensorId,
        dt_bias: TensorId,
        ssm_a: TensorId,
        ssm_norm: TensorId,
        ssm_output: TensorId,
        ffn_gate: TensorId,
        ffn_up: TensorId,
        ffn_down: TensorId,
    },
}

#[derive(Debug, Clone)]
struct Qwen35RowWeights {
    final_norm: Vec<f32>,
    layers: Vec<Qwen35RowLayerWeights>,
}

#[derive(Debug, Clone)]
enum Qwen35RowLayerWeights {
    Dense {
        attn_norm: Vec<f32>,
        post_attn_norm: Vec<f32>,
        q_norm: Vec<f32>,
        k_norm: Vec<f32>,
    },
    Recurrent {
        attn_norm: Vec<f32>,
        post_attn_norm: Vec<f32>,
        conv_weight: Vec<f32>,
        dt_bias: Vec<f32>,
        ssm_a: Vec<f32>,
        ssm_norm: Vec<f32>,
    },
}

#[derive(Debug, Default)]
struct Qwen35RowState {
    next_position: usize,
    dense_keys: Vec<Vec<f32>>,
    dense_values: Vec<Vec<f32>>,
    conv: Vec<Vec<f32>>,
    ssm: Vec<Vec<f32>>,
}

impl Qwen35Model {
    pub fn from_catalog(catalog: &TensorCatalog) -> Result<Self, String> {
        let source = catalog
            .source(ComponentId::Llm)
            .ok_or("Missing Qwen3.5 LLM source")?;
        let config = Qwen35Config::from_source(source.as_ref())?;
        let find = |name: &str| {
            catalog
                .find(ComponentId::Llm, name)
                .ok_or_else(|| format!("Missing {name}"))
        };
        let token_embedding = find("token_embd.weight")?;
        let embedding = catalog
            .entry(token_embedding)
            .ok_or("Missing token embedding entry")?;
        let embedding_width = usize::try_from(
            *embedding
                .shape
                .first()
                .ok_or("Invalid token embedding shape")?,
        )
        .map_err(|_| "token embedding width does not fit usize")?;
        let embedding_rows = usize::try_from(
            *embedding
                .shape
                .get(1)
                .ok_or("Invalid token embedding shape")?,
        )
        .map_err(|_| "token embedding rows do not fit usize")?;
        if config.n_embd != embedding_width || config.vocab_size != embedding_rows {
            return Err("Qwen3.5 metadata does not match embedding tensor shape".into());
        }

        let output = catalog
            .find(ComponentId::Llm, "output.weight")
            .unwrap_or(token_embedding);
        let final_norm = find("output_norm.weight")?;
        let mut layers = Vec::with_capacity(config.n_layer);
        for layer in 0..config.n_layer {
            let layer = u32::try_from(layer).map_err(|_| "Qwen3.5 layer index does not fit u32")?;
            let prefix = format!("blk.{layer}");
            let attn_norm = find(&format!("{prefix}.attn_norm.weight"))?;
            let post_attn_norm = find(&format!("{prefix}.post_attention_norm.weight"))?;
            if config.is_recurrent[layer as usize] {
                layers.push(Qwen35LayerTensorIds::Recurrent {
                    attn_norm,
                    post_attn_norm,
                    qkv: find(&format!("{prefix}.attn_qkv.weight"))?,
                    gate: find(&format!("{prefix}.attn_gate.weight"))?,
                    beta: find(&format!("{prefix}.ssm_beta.weight"))?,
                    alpha: find(&format!("{prefix}.ssm_alpha.weight"))?,
                    conv_weight: find(&format!("{prefix}.ssm_conv1d.weight"))?,
                    dt_bias: find(&format!("{prefix}.ssm_dt.bias"))?,
                    ssm_a: find(&format!("{prefix}.ssm_a"))?,
                    ssm_norm: find(&format!("{prefix}.ssm_norm.weight"))?,
                    ssm_output: find(&format!("{prefix}.ssm_out.weight"))?,
                    ffn_gate: find(&format!("{prefix}.ffn_gate.weight"))?,
                    ffn_up: find(&format!("{prefix}.ffn_up.weight"))?,
                    ffn_down: find(&format!("{prefix}.ffn_down.weight"))?,
                });
            } else {
                layers.push(Qwen35LayerTensorIds::Dense {
                    attn_norm,
                    post_attn_norm,
                    q_norm: find(&format!("{prefix}.attn_q_norm.weight"))?,
                    k_norm: find(&format!("{prefix}.attn_k_norm.weight"))?,
                    q: find(&format!("{prefix}.attn_q.weight"))?,
                    k: find(&format!("{prefix}.attn_k.weight"))?,
                    v: find(&format!("{prefix}.attn_v.weight"))?,
                    o: find(&format!("{prefix}.attn_output.weight"))?,
                    gate: find(&format!("{prefix}.ffn_gate.weight"))?,
                    up: find(&format!("{prefix}.ffn_up.weight"))?,
                    down: find(&format!("{prefix}.ffn_down.weight"))?,
                });
            }
        }

        let q8_input_widths = catalog
            .entries()
            .iter()
            .filter(|entry| {
                entry.component == ComponentId::Llm && entry.ggml_type == GGMLType::Q8_0
            })
            .filter_map(|entry| {
                usize::try_from(*entry.shape.first()?)
                    .ok()
                    .map(|width| (entry.id, width))
            })
            .collect();
        let cpu_row_matrices = catalog
            .entries()
            .iter()
            .filter(|entry| entry.component == ComponentId::Llm && entry.shape.len() >= 2)
            .filter(|entry| entry.ggml_type != GGMLType::Q8_0)
            .filter(|entry| {
                matches!(
                    entry.ggml_type,
                    GGMLType::F32 | GGMLType::F16 | GGMLType::Q4K | GGMLType::Q5K | GGMLType::Q6K
                )
            })
            .map(|entry| {
                let input_width = usize::try_from(entry.shape[0]).map_err(|_| {
                    format!("Qwen3.5 input width does not fit usize for {}", entry.name)
                })?;
                let rows = usize::try_from(entry.row_count).map_err(|_| {
                    format!("Qwen3.5 row count does not fit usize for {}", entry.name)
                })?;
                let bytes = catalog.bytes(entry.id).map_err(|error| error.to_string())?;
                let matrix = match entry.ggml_type {
                    GGMLType::F32 => CpuRowMatrix::F32 {
                        values: bytes
                            .chunks_exact(4)
                            .map(|bytes| {
                                f32::from_le_bytes(
                                    bytes.try_into().expect("F32 chunks are four bytes"),
                                )
                            })
                            .collect(),
                        input_width,
                        rows,
                    },
                    GGMLType::F16 => CpuRowMatrix::F16 {
                        values: bytes
                            .chunks_exact(2)
                            .map(|bytes| {
                                crate::f16_to_f32(u16::from_le_bytes(
                                    bytes.try_into().expect("F16 chunks are two bytes"),
                                ))
                            })
                            .collect(),
                        input_width,
                        rows,
                    },
                    GGMLType::Q4K => CpuRowMatrix::Q4K {
                        values: bytes.to_vec(),
                        input_width,
                        rows,
                    },
                    GGMLType::Q5K => CpuRowMatrix::Q5K {
                        values: bytes.to_vec(),
                        input_width,
                        rows,
                    },
                    GGMLType::Q6K => CpuRowMatrix::Q6K {
                        values: bytes.to_vec(),
                        input_width,
                        rows,
                    },
                    other => {
                        return Err(format!(
                            "Unsupported Qwen3.5 CPU Row matrix type for {}: {other:?}",
                            entry.name
                        ));
                    }
                };
                Ok((entry.id, matrix))
            })
            .collect::<Result<_, String>>()?;

        let row_weights = Qwen35RowWeights {
            final_norm: load_row_f32(catalog, final_norm, config.n_embd)?,
            layers: layers
                .iter()
                .map(|layer| match layer {
                    Qwen35LayerTensorIds::Dense {
                        attn_norm,
                        post_attn_norm,
                        q_norm,
                        k_norm,
                        ..
                    } => Ok(Qwen35RowLayerWeights::Dense {
                        attn_norm: load_row_f32(catalog, *attn_norm, config.n_embd)?,
                        post_attn_norm: load_row_f32(catalog, *post_attn_norm, config.n_embd)?,
                        q_norm: load_row_f32(catalog, *q_norm, config.key_length)?,
                        k_norm: load_row_f32(catalog, *k_norm, config.key_length)?,
                    }),
                    Qwen35LayerTensorIds::Recurrent {
                        attn_norm,
                        post_attn_norm,
                        conv_weight,
                        dt_bias,
                        ssm_a,
                        ssm_norm,
                        ..
                    } => Ok(Qwen35RowLayerWeights::Recurrent {
                        attn_norm: load_row_f32(catalog, *attn_norm, config.n_embd)?,
                        post_attn_norm: load_row_f32(catalog, *post_attn_norm, config.n_embd)?,
                        conv_weight: load_row_f32(
                            catalog,
                            *conv_weight,
                            config
                                .conv_dim()
                                .checked_mul(config.ssm_d_conv)
                                .ok_or("Qwen3.5 convolution shape overflow")?,
                        )?,
                        dt_bias: load_row_f32(catalog, *dt_bias, config.ssm_dt_rank)?,
                        ssm_a: load_row_f32(catalog, *ssm_a, config.ssm_dt_rank)?,
                        ssm_norm: load_row_f32(catalog, *ssm_norm, config.head_v_dim())?,
                    }),
                })
                .collect::<Result<_, String>>()?,
        };

        Ok(Self {
            config,
            tensor_ids: Qwen35TensorIds {
                token_embedding,
                output,
                final_norm,
                layers,
                q8_input_widths,
            },
            cpu_row_matrices,
            row_weights,
            row_state: Arc::new(Mutex::new(Qwen35RowState::default())),
        })
    }

    pub fn requirements(&self) -> ComponentRequirements {
        ComponentRequirements {
            component: ComponentId::Llm,
            workload: ComponentWorkload::Llm(LlmRequirements {
                layers: self
                    .tensor_ids
                    .layers
                    .iter()
                    .enumerate()
                    .map(|(layer, tensors)| match tensors {
                        Qwen35LayerTensorIds::Dense {
                            attn_norm,
                            post_attn_norm,
                            q_norm,
                            k_norm,
                            q,
                            k,
                            v,
                            o,
                            gate,
                            up,
                            down,
                        } => LlmLayerSpec::Qwen35Dense(Qwen35DenseLayerSpec {
                            layer: u32::try_from(layer).expect("Qwen3.5 layer count was validated"),
                            attn_norm: *attn_norm,
                            post_attn_norm: *post_attn_norm,
                            q_norm: *q_norm,
                            k_norm: *k_norm,
                            q: *q,
                            k: *k,
                            v: *v,
                            o: *o,
                            ffn_gate: *gate,
                            ffn_up: *up,
                            ffn_down: *down,
                            head_count: u32::try_from(self.config.n_head)
                                .expect("Qwen3.5 head count was validated"),
                            kv_head_count: u32::try_from(self.config.n_head_kv)
                                .expect("Qwen3.5 KV head count was validated"),
                            key_head_dim: u32::try_from(self.config.key_length)
                                .expect("Qwen3.5 key width was validated"),
                            value_head_dim: u32::try_from(self.config.value_length)
                                .expect("Qwen3.5 value width was validated"),
                            rope_dims: u32::try_from(self.config.rope_dimension_count)
                                .expect("Qwen3.5 rope width was validated"),
                            rope_sections: self.config.rope_dimension_sections,
                            rope_freq_base_bits: self.config.rope_freq_base.to_bits(),
                            norm_epsilon_bits: self.config.norm_eps.to_bits(),
                        }),
                        Qwen35LayerTensorIds::Recurrent {
                            attn_norm,
                            post_attn_norm,
                            qkv,
                            gate,
                            beta,
                            alpha,
                            conv_weight,
                            dt_bias,
                            ssm_a,
                            ssm_norm,
                            ssm_output,
                            ffn_gate,
                            ffn_up,
                            ffn_down,
                        } => LlmLayerSpec::Qwen35Recurrent(Qwen35RecurrentLayerSpec {
                            layer: u32::try_from(layer).expect("Qwen3.5 layer count was validated"),
                            attn_norm: *attn_norm,
                            post_attn_norm: *post_attn_norm,
                            qkv: *qkv,
                            gate: *gate,
                            beta: *beta,
                            alpha: *alpha,
                            conv_weight: *conv_weight,
                            dt_bias: *dt_bias,
                            ssm_a: *ssm_a,
                            ssm_norm: *ssm_norm,
                            ssm_output: *ssm_output,
                            ffn_gate: *ffn_gate,
                            ffn_up: *ffn_up,
                            ffn_down: *ffn_down,
                            conv_width: u32::try_from(self.config.ssm_d_conv)
                                .expect("Qwen3.5 convolution width was validated"),
                            state_size: u32::try_from(self.config.ssm_d_state)
                                .expect("Qwen3.5 state size was validated"),
                            group_count: u32::try_from(self.config.ssm_n_group)
                                .expect("Qwen3.5 group count was validated"),
                            dt_rank: u32::try_from(self.config.ssm_dt_rank)
                                .expect("Qwen3.5 dt rank was validated"),
                            inner_size: u32::try_from(self.config.ssm_d_inner)
                                .expect("Qwen3.5 inner size was validated"),
                            norm_epsilon_bits: self.config.norm_eps.to_bits(),
                        }),
                    })
                    .collect(),
                hidden_size: u32::try_from(self.config.n_embd)
                    .expect("Qwen3.5 hidden size was validated"),
                context_length: u32::try_from(self.config.n_ctx)
                    .expect("Qwen3.5 context length was validated"),
                max_batch_tokens: 1,
                kv_cache: KvCacheType::F16,
                final_norm: self.tensor_ids.final_norm,
                output: self.tensor_ids.output,
                norm_epsilon_bits: self.config.norm_eps.to_bits(),
            }),
        }
    }

    pub fn forward_compiled(
        &self,
        run: &mut ExecutionRun,
        tokens: &[u32],
        positions: &[[u32; 4]],
        output: &mut [f32],
    ) -> Result<(), String> {
        let params = crate::qwen3::checked_params(tokens, positions)?;
        match run.plan().components[&ComponentId::Llm].mode {
            PlacementMode::Layer => {
                run.execute_embedding_into_layers(
                    ComponentId::Llm,
                    self.tensor_ids.token_embedding,
                    tokens,
                    &params,
                )
                .map_err(|error| error.to_string())?;
                run.execute_logits(ComponentId::Llm, &params, output)
                    .map_err(|error| error.to_string())
            }
            PlacementMode::Row => {
                for (token, position) in tokens.iter().zip(positions) {
                    let params = crate::qwen3::checked_params(
                        std::slice::from_ref(token),
                        std::slice::from_ref(position),
                    )?;
                    self.forward_rows(run, std::slice::from_ref(token), &params, output)?;
                }
                Ok(())
            }
        }
    }

    fn forward_rows(
        &self,
        run: &mut ExecutionRun,
        tokens: &[u32],
        params: &crate::RunParams<'_>,
        output: &mut [f32],
    ) -> Result<(), String> {
        let mut embeddings = vec![0.0; tokens.len() * self.config.n_embd];
        run.execute_embedding(
            ComponentId::Llm,
            self.tensor_ids.token_embedding,
            tokens,
            &mut embeddings,
        )
        .map_err(|error| error.to_string())?;
        let mut state = self
            .row_state
            .lock()
            .map_err(|_| "Qwen3.5 row state poisoned")?;
        let start = usize::try_from(params.position_start)
            .map_err(|_| "Qwen3.5 position does not fit usize")?;
        if start == 0 {
            reset_row_state(&mut state, &self.config);
        }
        for (token, hidden) in embeddings.chunks_exact(self.config.n_embd).enumerate() {
            let position = usize::try_from(params.mrope_positions[token][0])
                .map_err(|_| "Qwen3.5 position does not fit usize")?;
            if position != state.next_position || position >= self.config.n_ctx {
                return Err(format!(
                            "Qwen3.5 Row mode requires contiguous positions from zero; expected {}, got {position}",
                            state.next_position
                        ));
            }
            let mut hidden = hidden.to_vec();
            self.forward_row_token(
                run,
                &mut state,
                position,
                params.mrope_positions[token],
                &mut hidden,
            )?;
            if token + 1 == tokens.len() {
                let normalized =
                    rms_normed(&hidden, &self.row_weights.final_norm, self.config.norm_eps);
                let logits = self.execute_matrix_row(run, self.tensor_ids.output, &normalized)?;
                if output.len() != logits.len() {
                    return Err("Qwen3.5 output buffer size mismatch".into());
                }
                output.copy_from_slice(&logits);
            }
            state.next_position = state
                .next_position
                .checked_add(1)
                .ok_or("Qwen3.5 position overflow")?;
        }
        Ok(())
    }

    fn forward_row_token(
        &self,
        run: &mut ExecutionRun,
        state: &mut Qwen35RowState,
        position: usize,
        positions: [u32; 4],
        hidden: &mut [f32],
    ) -> Result<(), String> {
        for (layer, (ids, weights)) in self
            .tensor_ids
            .layers
            .iter()
            .zip(&self.row_weights.layers)
            .enumerate()
        {
            let attn_norm = match weights {
                Qwen35RowLayerWeights::Dense { attn_norm, .. }
                | Qwen35RowLayerWeights::Recurrent { attn_norm, .. } => attn_norm,
            };
            let attn_input = rms_normed(hidden, attn_norm, self.config.norm_eps);
            let attention_output = match (ids, weights) {
                (
                    Qwen35LayerTensorIds::Dense { q, k, v, o, .. },
                    Qwen35RowLayerWeights::Dense { q_norm, k_norm, .. },
                ) => self.forward_dense_row(
                    run,
                    state,
                    layer,
                    position,
                    positions,
                    &attn_input,
                    *q,
                    *k,
                    *v,
                    *o,
                    q_norm,
                    k_norm,
                )?,
                (
                    Qwen35LayerTensorIds::Recurrent {
                        qkv,
                        gate,
                        beta,
                        alpha,
                        ssm_output,
                        ..
                    },
                    Qwen35RowLayerWeights::Recurrent {
                        conv_weight,
                        dt_bias,
                        ssm_a,
                        ssm_norm,
                        ..
                    },
                ) => self.forward_recurrent_row(
                    run,
                    state,
                    layer,
                    &attn_input,
                    *qkv,
                    *gate,
                    *beta,
                    *alpha,
                    *ssm_output,
                    conv_weight,
                    dt_bias,
                    ssm_a,
                    ssm_norm,
                )?,
                _ => return Err("Qwen3.5 layer weights do not match tensor IDs".into()),
            };
            add_assign(hidden, &attention_output, "Qwen3.5 attention output")?;
            let post_attn_norm = match weights {
                Qwen35RowLayerWeights::Dense { post_attn_norm, .. }
                | Qwen35RowLayerWeights::Recurrent { post_attn_norm, .. } => post_attn_norm,
            };
            let ffn_input = rms_normed(hidden, post_attn_norm, self.config.norm_eps);
            let (gate, up, down) = match ids {
                Qwen35LayerTensorIds::Dense { gate, up, down, .. } => (*gate, *up, *down),
                Qwen35LayerTensorIds::Recurrent {
                    ffn_gate,
                    ffn_up,
                    ffn_down,
                    ..
                } => (*ffn_gate, *ffn_up, *ffn_down),
            };
            let gate = self.execute_matrix_row(run, gate, &ffn_input)?;
            let mut up = self.execute_matrix_row(run, up, &ffn_input)?;
            if gate.len() != self.config.n_ff || up.len() != self.config.n_ff {
                return Err("Qwen3.5 FFN up projection shape mismatch".into());
            }
            crate::silu_mul_inplace(&gate, &mut up);
            let down = self.execute_matrix_row(run, down, &up)?;
            add_assign(hidden, &down, "Qwen3.5 FFN down projection")?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn forward_dense_row(
        &self,
        run: &mut ExecutionRun,
        state: &mut Qwen35RowState,
        layer: usize,
        position: usize,
        positions: [u32; 4],
        input: &[f32],
        q_tensor: TensorId,
        k_tensor: TensorId,
        v_tensor: TensorId,
        o_tensor: TensorId,
        q_norm: &[f32],
        k_norm: &[f32],
    ) -> Result<Vec<f32>, String> {
        let head_dim = self.config.key_length;
        let key_width = self.config.n_head_kv * head_dim;
        let value_width = self.config.n_head_kv * self.config.value_length;
        let group = self
            .config
            .n_head
            .checked_div(self.config.n_head_kv)
            .filter(|group| *group != 0)
            .ok_or("Invalid Qwen3.5 grouped-query attention")?;
        let mut q = self.execute_matrix_row(run, q_tensor, input)?;
        let mut k = self.execute_matrix_row(run, k_tensor, input)?;
        let v = self.execute_matrix_row(run, v_tensor, input)?;
        let attention_width = self
            .config
            .n_head
            .checked_mul(head_dim)
            .ok_or("Qwen3.5 dense attention width overflow")?;
        if q.len() != 2 * attention_width || k.len() != key_width || v.len() != value_width {
            return Err("Qwen3.5 dense attention projection shape mismatch".into());
        }
        let (q_values, q_gate) = q.split_at_mut(attention_width);
        for head in q_values.chunks_exact_mut(head_dim) {
            crate::rms_norm_inplace(head, q_norm, self.config.norm_eps);
        }
        for head in k.chunks_exact_mut(head_dim) {
            crate::rms_norm_inplace(head, k_norm, self.config.norm_eps);
        }
        let positions = positions
            .map(|position| {
                usize::try_from(position).map_err(|_| "Qwen3.5 position does not fit usize")
            })
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let positions: [usize; 4] = positions
            .try_into()
            .map_err(|_| "Invalid Qwen3.5 position vector")?;
        for head in q_values.chunks_exact_mut(head_dim) {
            crate::rope_mrope(
                head,
                positions,
                self.config.rope_dimension_sections,
                self.config.rope_dimension_count,
                self.config.rope_freq_base,
            );
        }
        for head in k.chunks_exact_mut(head_dim) {
            crate::rope_mrope(
                head,
                positions,
                self.config.rope_dimension_sections,
                self.config.rope_dimension_count,
                self.config.rope_freq_base,
            );
        }
        let keys = state
            .dense_keys
            .get_mut(layer)
            .ok_or("Missing Qwen3.5 dense key state")?;
        let values = state
            .dense_values
            .get_mut(layer)
            .ok_or("Missing Qwen3.5 dense value state")?;
        keys[position * key_width..(position + 1) * key_width].copy_from_slice(&k);
        values[position * value_width..(position + 1) * value_width].copy_from_slice(&v);
        let mut attention = vec![0.0; self.config.n_head * self.config.value_length];
        let scale = 1.0 / (head_dim as f32).sqrt();
        for head in 0..self.config.n_head {
            let kv_head = head / group;
            let q_head = &q_values[head * head_dim..(head + 1) * head_dim];
            let mut scores = (0..=position)
                .map(|prior| {
                    let offset = prior * key_width + kv_head * head_dim;
                    crate::dot_f32(q_head, &keys[offset..offset + head_dim], head_dim) * scale
                })
                .collect::<Vec<_>>();
            crate::softmax(&mut scores);
            let output = &mut attention
                [head * self.config.value_length..(head + 1) * self.config.value_length];
            for (prior, score) in scores.into_iter().enumerate() {
                let offset = prior * value_width + kv_head * self.config.value_length;
                for (output, value) in output
                    .iter_mut()
                    .zip(&values[offset..offset + self.config.value_length])
                {
                    *output += score * value;
                }
            }
            for (value, gate) in output
                .iter_mut()
                .zip(&q_gate[head * head_dim..(head + 1) * head_dim])
            {
                *value *= sigmoid(*gate);
            }
        }
        self.execute_matrix_row(run, o_tensor, &attention)
    }

    #[allow(clippy::too_many_arguments)]
    fn forward_recurrent_row(
        &self,
        run: &mut ExecutionRun,
        state: &mut Qwen35RowState,
        layer: usize,
        input: &[f32],
        qkv_tensor: TensorId,
        gate_tensor: TensorId,
        beta_tensor: TensorId,
        alpha_tensor: TensorId,
        output_tensor: TensorId,
        conv_weight: &[f32],
        dt_bias: &[f32],
        ssm_a: &[f32],
        ssm_norm: &[f32],
    ) -> Result<Vec<f32>, String> {
        let key_dim = self.config.key_dim();
        let value_dim = self.config.value_dim();
        let conv_dim = self.config.conv_dim();
        let head_v_dim = self.config.head_v_dim();
        let mut qkv = self.execute_matrix_row(run, qkv_tensor, input)?;
        let z = self.execute_matrix_row(run, gate_tensor, input)?;
        let beta = self.execute_matrix_row(run, beta_tensor, input)?;
        let alpha = self.execute_matrix_row(run, alpha_tensor, input)?;
        if qkv.len() != conv_dim || z.len() != value_dim {
            return Err("Qwen3.5 recurrent projection shape mismatch".into());
        }
        let conv = state
            .conv
            .get_mut(layer)
            .ok_or("Missing Qwen3.5 convolution state")?;
        for channel in 0..conv_dim {
            for tap in 0..self.config.ssm_d_conv.saturating_sub(1) {
                conv[tap * conv_dim + channel] = conv[(tap + 1) * conv_dim + channel];
            }
            conv[(self.config.ssm_d_conv - 1) * conv_dim + channel] = qkv[channel];
            let mut value = 0.0;
            for tap in 0..self.config.ssm_d_conv {
                value += conv_weight[channel * self.config.ssm_d_conv + tap]
                    * conv[tap * conv_dim + channel];
            }
            qkv[channel] = crate::silu(value);
        }
        let mut attention = vec![0.0; value_dim];
        let ssm = state
            .ssm
            .get_mut(layer)
            .ok_or("Missing Qwen3.5 SSM state")?;
        for value_head in 0..self.config.ssm_dt_rank {
            let key_head = value_head % self.config.ssm_n_group;
            let q =
                &qkv[key_head * self.config.ssm_d_state..(key_head + 1) * self.config.ssm_d_state];
            let k = &qkv[key_dim + key_head * self.config.ssm_d_state
                ..key_dim + (key_head + 1) * self.config.ssm_d_state];
            let v = &qkv[2 * key_dim + value_head * head_v_dim
                ..2 * key_dim + (value_head + 1) * head_v_dim];
            let mut q = q.to_vec();
            let mut k = k.to_vec();
            l2_norm(&mut q, self.config.norm_eps);
            l2_norm(&mut k, self.config.norm_eps);
            let state_offset = value_head * head_v_dim * head_v_dim;
            let state_slice = &mut ssm[state_offset..state_offset + head_v_dim * head_v_dim];
            let beta = sigmoid(beta.get(value_head).copied().unwrap_or(0.0));
            let alpha =
                softplus(alpha.get(value_head).copied().unwrap_or(0.0) + dt_bias[value_head])
                    * ssm_a[value_head];
            crate::ssm_state_decay(state_slice, alpha.exp());
            let mut prior = vec![0.0; head_v_dim];
            crate::ssm_matvec(
                state_slice,
                &k[..head_v_dim],
                head_v_dim,
                head_v_dim,
                &mut prior,
            );
            let delta = v
                .iter()
                .zip(&prior)
                .map(|(value, prior)| (value - prior) * beta)
                .collect::<Vec<_>>();
            crate::ssm_outer_product_update(state_slice, &k[..head_v_dim], &delta, head_v_dim);
            let output = &mut attention[value_head * head_v_dim..(value_head + 1) * head_v_dim];
            crate::ssm_matvec_scaled(
                state_slice,
                &q[..head_v_dim],
                head_v_dim,
                head_v_dim,
                output,
                1.0 / (self.config.ssm_d_state as f32).sqrt(),
            );
            crate::rms_norm_inplace(output, ssm_norm, self.config.norm_eps);
        }
        crate::silu_mul_inplace(&z, &mut attention);
        self.execute_matrix_row(run, output_tensor, &attention)
    }

    fn execute_matrix_row(
        &self,
        run: &mut ExecutionRun,
        tensor: TensorId,
        input: &[f32],
    ) -> Result<Vec<f32>, String> {
        if let Some(&expected) = self.tensor_ids.q8_input_widths.get(&tensor) {
            if input.len() != expected {
                return Err(format!(
                    "Qwen3.5 compiled Q8 input width mismatch: expected {expected}, got {}",
                    input.len()
                ));
            }
            let shards = run.plan().components[&ComponentId::Llm]
                .row_shards
                .get(&tensor)
                .ok_or_else(|| format!("Missing compiled Q8 program for {tensor:?}"))?;
            let rows =
                usize::try_from(shards.iter().map(|shard| shard.rows.end).max().unwrap_or(0))
                    .map_err(|_| "Qwen3.5 output rows do not fit usize")?;
            let mut output = vec![0.0; rows];
            run.execute_q8(ComponentId::Llm, tensor, input, 1, &mut output)
                .map_err(|error| error.to_string())?;
            return Ok(output);
        }
        self.execute_cpu_row_matrix(run, tensor, input)
    }

    fn execute_cpu_row_matrix(
        &self,
        run: &ExecutionRun,
        tensor: TensorId,
        input: &[f32],
    ) -> Result<Vec<f32>, String> {
        let component = run
            .plan()
            .components
            .get(&ComponentId::Llm)
            .ok_or("Missing Qwen3.5 compiled component")?;
        let primary = run
            .plan()
            .devices
            .get(&component.primary)
            .ok_or("Missing Qwen3.5 compiled primary device")?;
        if component.mode != PlacementMode::Row
            || primary.descriptor.backend != crate::BackendKind::Cpu
        {
            return Err(format!(
                "Qwen3.5 CPU Row matrix {tensor:?} is not assigned to CPU"
            ));
        }
        let matrix = self
            .cpu_row_matrices
            .get(&tensor)
            .ok_or_else(|| format!("Missing Qwen3.5 CPU Row matrix for {tensor:?}"))?;
        let (input_width, rows) = matrix.dimensions();
        if input.len() != input_width {
            return Err(format!(
                "Qwen3.5 CPU Row input width mismatch: expected {input_width}, got {}",
                input.len()
            ));
        }
        let output = match matrix {
            CpuRowMatrix::F32 { values, .. } | CpuRowMatrix::F16 { values, .. } => values
                .chunks_exact(input_width)
                .map(|row| crate::dot_f32(row, input, input_width))
                .collect(),
            CpuRowMatrix::Q4K { values, .. } => {
                crate::quant::matmul_q4k_q8k(values, input, input_width, rows)
            }
            CpuRowMatrix::Q5K { values, .. } => {
                crate::quant::matmul_q5k_q8k(values, input, input_width, rows)
            }
            CpuRowMatrix::Q6K { values, .. } => {
                crate::quant::matmul_q6k_q8k(values, input, input_width, rows)
            }
        };
        Ok(output)
    }
}

impl CpuRowMatrix {
    fn dimensions(&self) -> (usize, usize) {
        match self {
            Self::F32 {
                input_width, rows, ..
            }
            | Self::F16 {
                input_width, rows, ..
            }
            | Self::Q4K {
                input_width, rows, ..
            }
            | Self::Q5K {
                input_width, rows, ..
            }
            | Self::Q6K {
                input_width, rows, ..
            } => (*input_width, *rows),
        }
    }
}

fn load_row_f32(
    catalog: &TensorCatalog,
    tensor: TensorId,
    expected: usize,
) -> Result<Vec<f32>, String> {
    let entry = catalog
        .entry(tensor)
        .ok_or("Missing Qwen3.5 auxiliary tensor")?;
    let bytes = catalog.bytes(tensor).map_err(|error| error.to_string())?;
    let values: Vec<f32> = match entry.ggml_type {
        GGMLType::F32 => bytes
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().expect("F32 chunks are four bytes")))
            .collect(),
        GGMLType::F16 => bytes
            .chunks_exact(2)
            .map(|bytes| {
                crate::f16_to_f32(u16::from_le_bytes(
                    bytes.try_into().expect("F16 chunks are two bytes"),
                ))
            })
            .collect(),
        other => {
            return Err(format!(
                "Unsupported Qwen3.5 auxiliary tensor type: {other:?}"
            ))
        }
    };
    if values.len() != expected {
        return Err(format!(
            "Qwen3.5 auxiliary tensor shape mismatch: expected {expected}, got {}",
            values.len()
        ));
    }
    Ok(values)
}

fn reset_row_state(state: &mut Qwen35RowState, config: &Qwen35Config) {
    let key_width = config.n_head_kv * config.key_length;
    let value_width = config.n_head_kv * config.value_length;
    let conv_len = config.ssm_d_conv * config.conv_dim();
    let ssm_len = config.ssm_dt_rank * config.head_v_dim() * config.head_v_dim();
    state.next_position = 0;
    state.dense_keys = (0..config.n_layer)
        .map(|_| vec![0.0; config.n_ctx * key_width])
        .collect();
    state.dense_values = (0..config.n_layer)
        .map(|_| vec![0.0; config.n_ctx * value_width])
        .collect();
    state.conv = (0..config.n_layer).map(|_| vec![0.0; conv_len]).collect();
    state.ssm = (0..config.n_layer).map(|_| vec![0.0; ssm_len]).collect();
}

fn rms_normed(input: &[f32], weight: &[f32], eps: f32) -> Vec<f32> {
    let mut output = input.to_vec();
    crate::rms_norm_inplace(&mut output, weight, eps);
    output
}

fn add_assign(destination: &mut [f32], source: &[f32], operation: &str) -> Result<(), String> {
    if destination.len() != source.len() {
        return Err(format!("{operation} shape mismatch"));
    }
    crate::vec_add_into(source, destination);
    Ok(())
}

fn l2_norm(values: &mut [f32], eps: f32) {
    let magnitude = values
        .iter()
        .map(|value| f64::from(*value * *value))
        .sum::<f64>();
    let scale = 1.0 / (magnitude as f32).sqrt().max(eps);
    for value in values {
        *value *= scale;
    }
}

fn sigmoid(value: f32) -> f32 {
    1.0 / (1.0 + (-value).exp())
}

fn softplus(value: f32) -> f32 {
    if value > 20.0 {
        value
    } else {
        (1.0 + value.exp()).ln()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qwen35_positions_use_time_row_column_order() {
        let grid = VisionGrid {
            grid_t: 1,
            grid_h: 2,
            grid_w: 3,
            patch_size: 16,
            merge_size: 2,
        };
        let tokens = [10, 99, 99, 99, 99, 99, 99, 11];
        let (positions, next) = build_qwen35_positions(&tokens, Some(99), &[grid]).unwrap();
        assert_eq!(positions[1], [1, 1, 1, 0]);
        assert_eq!(positions[6], [1, 2, 3, 0]);
        assert_eq!(positions[7], [4, 4, 4, 0]);
        assert_eq!(next, 5);
    }

    #[test]
    fn qwen35_placeholder_count_must_equal_grid_tokens() {
        let grid = VisionGrid {
            grid_t: 1,
            grid_h: 2,
            grid_w: 3,
            patch_size: 16,
            merge_size: 2,
        };
        assert!(build_qwen35_positions(&[10, 99, 99, 11], Some(99), &[grid]).is_err());
    }

    #[test]
    fn qwen35_positions_reject_public_grid_token_overflow() {
        let grid = VisionGrid {
            grid_t: 1,
            grid_h: usize::MAX,
            grid_w: 2,
            patch_size: 1,
            merge_size: 1,
        };
        assert!(build_qwen35_positions(&[99], Some(99), &[grid]).is_err());
    }

    #[test]
    fn cpu_row_matrix_f32_and_f16_use_the_expected_rows() {
        let f32 = CpuRowMatrix::F32 {
            values: vec![1.0, 2.0, 3.0, 4.0],
            input_width: 2,
            rows: 2,
        };
        let f16 = CpuRowMatrix::F16 {
            values: vec![1.0, 2.0, 3.0, 4.0],
            input_width: 2,
            rows: 2,
        };
        for matrix in [f32, f16] {
            let (input_width, rows) = matrix.dimensions();
            assert_eq!((input_width, rows), (2, 2));
            let values = match matrix {
                CpuRowMatrix::F32 { values, .. } | CpuRowMatrix::F16 { values, .. } => values,
                _ => unreachable!(),
            };
            assert_eq!(
                values
                    .chunks_exact(input_width)
                    .map(|row| crate::dot_f32(row, &[5.0, 6.0], input_width))
                    .collect::<Vec<_>>(),
                vec![17.0, 39.0],
            );
        }
    }

    #[test]
    fn cpu_row_matrix_keeps_all_supported_fallback_types() {
        assert_eq!(
            CpuRowMatrix::Q4K {
                values: vec![],
                input_width: 256,
                rows: 1,
            }
            .dimensions(),
            (256, 1)
        );
        assert_eq!(
            CpuRowMatrix::Q5K {
                values: vec![],
                input_width: 256,
                rows: 1,
            }
            .dimensions(),
            (256, 1)
        );
        assert_eq!(
            CpuRowMatrix::Q6K {
                values: vec![],
                input_width: 256,
                rows: 1,
            }
            .dimensions(),
            (256, 1)
        );
    }
}
