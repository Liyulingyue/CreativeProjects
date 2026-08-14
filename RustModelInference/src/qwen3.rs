use crate::{
    ComponentId, ComponentRequirements, ComponentWorkload, ExecutionRun, GGMLType, KvCacheType,
    LlmLayerSpec, LlmRequirements, MetaValue, PlacementMode, Qwen3LayerSpec, RunParams,
    TensorCatalog, TensorId,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct Qwen3Config {
    pub n_embd: usize,
    pub n_layer: usize,
    pub n_head: usize,
    pub n_head_kv: usize,
    pub n_embd_head_k: usize,
    pub n_embd_head_v: usize,
    pub n_ff: usize,
    pub vocab: usize,
    pub n_ctx: usize,
    pub eps: f32,
    pub freq_base: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Qwen3EmbeddingPooling {
    Mean,
    Last,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Qwen3EmbeddingConfig {
    pub causal: bool,
    pub pooling: Qwen3EmbeddingPooling,
}

impl Qwen3EmbeddingConfig {
    pub fn from_metadata(get: impl Fn(&str) -> Option<MetaValue>) -> Result<Self, String> {
        let pooling_key = "qwen3.pooling_type";
        let pooling = match get(pooling_key).and_then(|value| value.to_u64()) {
            Some(1) => Qwen3EmbeddingPooling::Mean,
            Some(3) => Qwen3EmbeddingPooling::Last,
            Some(value) => {
                return Err(format!(
                    "Unsupported {pooling_key}: {value}; expected 1=MEAN or 3=LAST"
                ));
            }
            None => return Err(format!("Missing or invalid metadata: {pooling_key}")),
        };
        let causal_key = "qwen3.attention.causal";
        let causal = match get(causal_key) {
            None => true,
            Some(MetaValue::Bool(value)) => value,
            Some(value) => {
                return Err(format!(
                    "Invalid metadata {causal_key}: expected bool, got {value:?}"
                ));
            }
        };
        Ok(Self { causal, pooling })
    }
}

#[derive(Debug, Clone)]
pub struct Qwen3Model {
    pub config: Qwen3Config,
    pub tensors: Qwen3TensorIds,
    pub auxiliary: Qwen3AuxiliaryWeights,
    q8_input_widths: BTreeMap<TensorId, usize>,
    row_weights: Qwen3RowWeights,
    row_state: Arc<Mutex<Qwen3RowState>>,
    layer_next_position: Arc<Mutex<usize>>,
}

#[derive(Debug, Clone)]
pub struct Qwen3TensorIds {
    pub token_embedding: TensorId,
    pub layers: Vec<Qwen3LayerTensorIds>,
    pub output: TensorId,
}

#[derive(Debug, Clone)]
pub struct Qwen3LayerTensorIds {
    pub q: TensorId,
    pub k: TensorId,
    pub v: TensorId,
    pub o: TensorId,
    pub gate: TensorId,
    pub up: TensorId,
    pub down: TensorId,
}

#[derive(Debug, Clone)]
pub struct Qwen3AuxiliaryWeights {
    pub attention_norms: Vec<TensorId>,
    pub q_norms: Vec<Option<TensorId>>,
    pub k_norms: Vec<Option<TensorId>>,
    pub ffn_norms: Vec<TensorId>,
    pub final_norm: TensorId,
}

#[derive(Debug, Clone)]
struct Qwen3RowWeights {
    attention_norms: Vec<Vec<f32>>,
    q_norms: Vec<Option<Vec<f32>>>,
    k_norms: Vec<Option<Vec<f32>>>,
    ffn_norms: Vec<Vec<f32>>,
    final_norm: Vec<f32>,
}

#[derive(Debug, Default)]
struct Qwen3RowState {
    next_position: usize,
    keys: Vec<Vec<f32>>,
    values: Vec<Vec<f32>>,
}

impl Qwen3Model {
    pub fn from_catalog(catalog: &TensorCatalog) -> Result<Self, String> {
        let find = |name: &str| {
            catalog
                .find(ComponentId::Llm, name)
                .ok_or_else(|| format!("Missing {name}"))
        };
        let source = catalog
            .source(ComponentId::Llm)
            .ok_or("Missing Qwen3 LLM source")?;
        let config = source.model_config()?;
        let token_embedding = find("token_embd.weight")?;
        let embedding = catalog
            .entry(token_embedding)
            .ok_or("Missing token embedding entry")?;
        let n_embd = usize::try_from(*embedding.shape.first().ok_or("Invalid token embedding")?)
            .map_err(|_| "token embedding width does not fit usize")?;
        let vocab = usize::try_from(*embedding.shape.get(1).ok_or("Invalid token embedding")?)
            .map_err(|_| "vocabulary size does not fit usize")?;
        let output = catalog
            .find(ComponentId::Llm, "output.weight")
            .unwrap_or(token_embedding);
        let final_norm = find("output_norm.weight")?;
        if config.n_embd != n_embd || config.vocab_size != vocab {
            return Err("Qwen3 metadata does not match embedding tensor shape".into());
        }

        let mut layers = Vec::new();
        let mut attention_norms = Vec::new();
        let mut q_norms = Vec::new();
        let mut k_norms = Vec::new();
        let mut ffn_norms = Vec::new();
        for layer in 0_u32.. {
            let prefix = format!("blk.{layer}");
            let q = match catalog.find(ComponentId::Llm, &format!("{prefix}.attn_q.weight")) {
                Some(q) => q,
                None => break,
            };
            layers.push(Qwen3LayerTensorIds {
                q,
                k: find(&format!("{prefix}.attn_k.weight"))?,
                v: find(&format!("{prefix}.attn_v.weight"))?,
                o: find(&format!("{prefix}.attn_output.weight"))?,
                gate: find(&format!("{prefix}.ffn_gate.weight"))?,
                up: find(&format!("{prefix}.ffn_up.weight"))?,
                down: find(&format!("{prefix}.ffn_down.weight"))?,
            });
            attention_norms.push(find(&format!("{prefix}.attn_norm.weight"))?);
            q_norms.push(catalog.find(ComponentId::Llm, &format!("{prefix}.attn_q_norm.weight")));
            k_norms.push(catalog.find(ComponentId::Llm, &format!("{prefix}.attn_k_norm.weight")));
            ffn_norms.push(find(&format!("{prefix}.ffn_norm.weight"))?);
        }
        if layers.is_empty() {
            return Err("Missing Qwen3 layers".into());
        }
        if layers.len() != config.n_layer {
            return Err(format!(
                "Qwen3 metadata/tensor layer count mismatch: metadata={}, tensors={}",
                config.n_layer,
                layers.len()
            ));
        }

        let q = catalog.entry(layers[0].q).ok_or("Missing Q tensor entry")?;
        let k = catalog.entry(layers[0].k).ok_or("Missing K tensor entry")?;
        let q_rows = usize::try_from(q.row_count).map_err(|_| "Q row count does not fit usize")?;
        let k_rows = usize::try_from(k.row_count).map_err(|_| "K row count does not fit usize")?;
        let n_embd_head_k = config.n_embd_head;
        if q_rows != config.n_head * n_embd_head_k
            || k_rows != config.n_head_kv * n_embd_head_k
            || n_embd_head_k == 0
        {
            return Err("Invalid Qwen3 attention dimensions".into());
        }
        let n_ff = usize::try_from(
            catalog
                .entry(layers[0].up)
                .and_then(|entry| entry.shape.get(1))
                .copied()
                .ok_or("Invalid Qwen3 FFN tensor")?,
        )
        .map_err(|_| "FFN size does not fit usize")?;

        let row_weights = Qwen3RowWeights {
            attention_norms: attention_norms
                .iter()
                .map(|&tensor| load_row_f32(catalog, tensor, n_embd))
                .collect::<Result<_, _>>()?,
            q_norms: q_norms
                .iter()
                .map(|tensor| {
                    tensor
                        .map(|tensor| load_row_f32(catalog, tensor, n_embd_head_k))
                        .transpose()
                })
                .collect::<Result<_, _>>()?,
            k_norms: k_norms
                .iter()
                .map(|tensor| {
                    tensor
                        .map(|tensor| load_row_f32(catalog, tensor, n_embd_head_k))
                        .transpose()
                })
                .collect::<Result<_, _>>()?,
            ffn_norms: ffn_norms
                .iter()
                .map(|&tensor| load_row_f32(catalog, tensor, n_embd))
                .collect::<Result<_, _>>()?,
            final_norm: load_row_f32(catalog, final_norm, n_embd)?,
        };

        Ok(Self {
            config: Qwen3Config {
                n_embd,
                n_layer: config.n_layer,
                n_head: config.n_head,
                n_head_kv: config.n_head_kv,
                n_embd_head_k,
                n_embd_head_v: n_embd_head_k,
                n_ff,
                vocab,
                n_ctx: config.n_ctx,
                eps: config.norm_eps,
                freq_base: config.rope_freq_base,
            },
            tensors: Qwen3TensorIds {
                token_embedding,
                layers,
                output,
            },
            auxiliary: Qwen3AuxiliaryWeights {
                attention_norms,
                q_norms,
                k_norms,
                ffn_norms,
                final_norm,
            },
            q8_input_widths: catalog
                .entries()
                .iter()
                .filter(|entry| {
                    entry.component == ComponentId::Llm && entry.ggml_type == crate::GGMLType::Q8_0
                })
                .filter_map(|entry| {
                    usize::try_from(*entry.shape.first()?)
                        .ok()
                        .map(|width| (entry.id, width))
                })
                .collect(),
            row_weights,
            row_state: Arc::new(Mutex::new(Qwen3RowState::default())),
            layer_next_position: Arc::new(Mutex::new(0)),
        })
    }

    pub fn requirements(&self) -> ComponentRequirements {
        ComponentRequirements {
            component: ComponentId::Llm,
            workload: ComponentWorkload::Llm(LlmRequirements {
                layers: self
                    .tensors
                    .layers
                    .iter()
                    .enumerate()
                    .map(|(layer, tensors)| {
                        LlmLayerSpec::Qwen3(Qwen3LayerSpec {
                            layer: u32::try_from(layer)
                                .expect("Qwen3 layer count was catalog-validated"),
                            attn_norm: self.auxiliary.attention_norms[layer],
                            q_norm: self.auxiliary.q_norms[layer],
                            k_norm: self.auxiliary.k_norms[layer],
                            q: tensors.q,
                            k: tensors.k,
                            v: tensors.v,
                            o: tensors.o,
                            ffn_norm: self.auxiliary.ffn_norms[layer],
                            ffn_gate: tensors.gate,
                            ffn_up: tensors.up,
                            ffn_down: tensors.down,
                            head_count: u32::try_from(self.config.n_head)
                                .expect("Qwen3 head count was catalog-validated"),
                            kv_head_count: u32::try_from(self.config.n_head_kv)
                                .expect("Qwen3 KV head count was catalog-validated"),
                            key_head_dim: u32::try_from(self.config.n_embd_head_k)
                                .expect("Qwen3 head width was catalog-validated"),
                            value_head_dim: u32::try_from(self.config.n_embd_head_v)
                                .expect("Qwen3 head width was catalog-validated"),
                            rope_dims: u32::try_from(self.config.n_embd_head_k)
                                .expect("Qwen3 rope width was catalog-validated"),
                            rope_freq_base_bits: self.config.freq_base.to_bits(),
                            norm_epsilon_bits: self.config.eps.to_bits(),
                        })
                    })
                    .collect(),
                hidden_size: u32::try_from(self.config.n_embd)
                    .expect("Qwen3 width was catalog-validated"),
                context_length: u32::try_from(self.config.n_ctx)
                    .expect("Qwen3 context was catalog-validated"),
                max_batch_tokens: 1,
                kv_cache: KvCacheType::F16,
                final_norm: self.auxiliary.final_norm,
                output: self.tensors.output,
                norm_epsilon_bits: self.config.eps.to_bits(),
            }),
        }
    }

    pub fn embed(
        &self,
        run: &mut ExecutionRun,
        tokens: &[u32],
        config: Qwen3EmbeddingConfig,
    ) -> Result<Vec<f32>, String> {
        if run.plan().components[&ComponentId::Llm].mode != PlacementMode::Row {
            return Err("Qwen3 embedding requires Row placement".into());
        }
        if tokens.is_empty() {
            return Err("Embedding input produced no tokens".into());
        }
        if tokens.len() > self.config.n_ctx {
            return Err(format!(
                "Embedding token count exceeds context: tokens={}, context={}",
                tokens.len(),
                self.config.n_ctx
            ));
        }
        if tokens
            .iter()
            .any(|token| *token as usize >= self.config.vocab)
        {
            return Err("token ID exceeds vocabulary".into());
        }

        let mut hidden = vec![0.0; tokens.len() * self.config.n_embd];
        for (token, row) in tokens
            .iter()
            .zip(hidden.chunks_exact_mut(self.config.n_embd))
        {
            run.execute_embedding(
                ComponentId::Llm,
                self.tensors.token_embedding,
                std::slice::from_ref(token),
                row,
            )
            .map_err(|error| error.to_string())?;
        }

        let (_, _, group_size) = self.attention_dimensions()?;

        for layer in 0..self.tensors.layers.len() {
            let mut queries = Vec::with_capacity(tokens.len());
            let mut keys = Vec::with_capacity(tokens.len());
            let mut values = Vec::with_capacity(tokens.len());
            for (position, row) in hidden.chunks_exact(self.config.n_embd).enumerate() {
                let attention_input = rms_normed(
                    row,
                    &self.row_weights.attention_norms[layer],
                    self.config.eps,
                );
                let (q, k, v) = self.project_qkv(run, layer, position, &attention_input)?;
                queries.push(q);
                keys.push(k);
                values.push(v);
            }

            let scale = 1.0 / (self.config.n_embd_head_k as f32).sqrt();
            let mut attention_rows = Vec::with_capacity(tokens.len());
            for (position, query) in queries.iter().enumerate() {
                let key_end = if config.causal {
                    position + 1
                } else {
                    tokens.len()
                };
                let mut attention = vec![0.0; self.config.n_head * self.config.n_embd_head_v];
                for head in 0..self.config.n_head {
                    let kv_head = head / group_size;
                    let q_head = &query
                        [head * self.config.n_embd_head_k..(head + 1) * self.config.n_embd_head_k];
                    let mut scores = Vec::with_capacity(key_end);
                    for key in keys.iter().take(key_end) {
                        let offset = kv_head * self.config.n_embd_head_k;
                        scores.push(
                            crate::dot_f32(
                                q_head,
                                &key[offset..offset + self.config.n_embd_head_k],
                                self.config.n_embd_head_k,
                            ) * scale,
                        );
                    }
                    crate::softmax(&mut scores);
                    let output_head = &mut attention
                        [head * self.config.n_embd_head_v..(head + 1) * self.config.n_embd_head_v];
                    for (value, score) in values.iter().take(key_end).zip(scores) {
                        let offset = kv_head * self.config.n_embd_head_v;
                        for (output, value) in output_head
                            .iter_mut()
                            .zip(&value[offset..offset + self.config.n_embd_head_v])
                        {
                            *output += score * value;
                        }
                    }
                }
                attention_rows.push(attention);
            }
            for (row, attention) in hidden
                .chunks_exact_mut(self.config.n_embd)
                .zip(attention_rows.iter())
            {
                let attention_output =
                    self.execute_q8_row(run, self.tensors.layers[layer].o, attention)?;
                add_assign(row, &attention_output, "Qwen3 attention output")?;
                self.apply_ffn(run, layer, row)?;
            }
        }

        for row in hidden.chunks_exact_mut(self.config.n_embd) {
            crate::rms_norm_inplace(row, &self.row_weights.final_norm, self.config.eps);
        }
        pool_and_normalize_embedding(&hidden, self.config.n_embd, config.pooling)
    }

    pub fn forward(
        &self,
        run: &mut ExecutionRun,
        tokens: &[u32],
        positions: &[[u32; 4]],
        output: &mut [f32],
    ) -> Result<(), String> {
        let mode = run.plan().components[&ComponentId::Llm].mode;
        let expected_position = match mode {
            PlacementMode::Row => {
                self.row_state
                    .lock()
                    .map_err(|_| "Qwen3 row state poisoned")?
                    .next_position
            }
            PlacementMode::Layer => *self
                .layer_next_position
                .lock()
                .map_err(|_| "Qwen3 layer state poisoned")?,
        };
        let expected_position = positions
            .first()
            .filter(|position| position[0] == 0)
            .map(|_| 0)
            .unwrap_or(expected_position);
        let params = validate_model_inputs(
            run,
            tokens,
            positions,
            output.len(),
            self.config.vocab,
            self.config.n_ctx,
            expected_position,
        )?;
        match mode {
            PlacementMode::Row => {
                for (token, position) in tokens.iter().zip(positions) {
                    let params = checked_params(
                        std::slice::from_ref(token),
                        std::slice::from_ref(position),
                    )?;
                    self.forward_rows(run, std::slice::from_ref(token), &params, output)?;
                }
                Ok(())
            }
            PlacementMode::Layer => {
                run.execute_embedding_into_layers(
                    ComponentId::Llm,
                    self.tensors.token_embedding,
                    tokens,
                    &params,
                )
                .map_err(|error| error.to_string())?;
                run.execute_logits(ComponentId::Llm, &params, output)
                    .map_err(|error| error.to_string())?;
                *self
                    .layer_next_position
                    .lock()
                    .map_err(|_| "Qwen3 layer state poisoned")? =
                    usize::try_from(positions.last().expect("validated non-empty positions")[0])
                        .map_err(|_| "Qwen3 position does not fit usize")?
                        .checked_add(1)
                        .ok_or("Qwen3 position overflow")?;
                Ok(())
            }
        }
    }

    fn forward_rows(
        &self,
        run: &mut ExecutionRun,
        tokens: &[u32],
        params: &RunParams<'_>,
        output: &mut [f32],
    ) -> Result<(), String> {
        let mut embeddings = vec![0.0; tokens.len() * self.config.n_embd];
        run.execute_embedding(
            ComponentId::Llm,
            self.tensors.token_embedding,
            tokens,
            &mut embeddings,
        )
        .map_err(|error| error.to_string())?;
        let mut state = self
            .row_state
            .lock()
            .map_err(|_| "Qwen3 row state poisoned")?;
        let start = usize::try_from(params.position_start)
            .map_err(|_| "Qwen3 position does not fit usize")?;
        if start == 0 {
            reset_row_state(&mut state, &self.config);
        }

        for (token, hidden) in embeddings.chunks_exact(self.config.n_embd).enumerate() {
            let position = usize::try_from(params.mrope_positions[token][0])
                .map_err(|_| "Qwen3 position does not fit usize")?;
            if position != state.next_position || position >= self.config.n_ctx {
                return Err(format!(
                    "Qwen3 Row mode requires contiguous positions from zero; expected {}, got {position}",
                    state.next_position
                ));
            }
            let mut hidden = hidden.to_vec();
            self.forward_row_token(run, &mut state, position, &mut hidden)?;
            if token + 1 == tokens.len() {
                let normalized = rms_normed(&hidden, &self.row_weights.final_norm, self.config.eps);
                let logits = self.execute_q8_row(run, self.tensors.output, &normalized)?;
                if output.len() != logits.len() {
                    return Err("Qwen3 output buffer size mismatch".into());
                }
                output.copy_from_slice(&logits);
            }
            state.next_position = state
                .next_position
                .checked_add(1)
                .ok_or("Qwen3 position overflow")?;
        }
        Ok(())
    }

    fn forward_row_token(
        &self,
        run: &mut ExecutionRun,
        state: &mut Qwen3RowState,
        position: usize,
        hidden: &mut [f32],
    ) -> Result<(), String> {
        let (key_width, value_width, group_size) = self.attention_dimensions()?;

        for (layer, tensors) in self.tensors.layers.iter().enumerate() {
            let attention_input = rms_normed(
                hidden,
                &self.row_weights.attention_norms[layer],
                self.config.eps,
            );
            let (q, k, v) = self.project_qkv(run, layer, position, &attention_input)?;

            let cache_key = state.keys.get_mut(layer).ok_or("Missing Qwen3 key state")?;
            let cache_value = state
                .values
                .get_mut(layer)
                .ok_or("Missing Qwen3 value state")?;
            let key_offset = position
                .checked_mul(key_width)
                .ok_or("Qwen3 key cache offset overflow")?;
            let value_offset = position
                .checked_mul(value_width)
                .ok_or("Qwen3 value cache offset overflow")?;
            cache_key[key_offset..key_offset + key_width].copy_from_slice(&k);
            cache_value[value_offset..value_offset + value_width].copy_from_slice(&v);

            let mut attention = vec![0.0; self.config.n_head * self.config.n_embd_head_v];
            let scale = 1.0 / (self.config.n_embd_head_k as f32).sqrt();
            for head in 0..self.config.n_head {
                let kv_head = head / group_size;
                let q_head =
                    &q[head * self.config.n_embd_head_k..(head + 1) * self.config.n_embd_head_k];
                let mut scores = Vec::with_capacity(position + 1);
                for prior in 0..=position {
                    let offset = prior * key_width + kv_head * self.config.n_embd_head_k;
                    scores.push(
                        crate::dot_f32(
                            q_head,
                            &cache_key[offset..offset + self.config.n_embd_head_k],
                            self.config.n_embd_head_k,
                        ) * scale,
                    );
                }
                crate::softmax(&mut scores);
                let output_head = &mut attention
                    [head * self.config.n_embd_head_v..(head + 1) * self.config.n_embd_head_v];
                for (prior, score) in scores.into_iter().enumerate() {
                    let offset = prior * value_width + kv_head * self.config.n_embd_head_v;
                    for (output, value) in output_head
                        .iter_mut()
                        .zip(&cache_value[offset..offset + self.config.n_embd_head_v])
                    {
                        *output += score * value;
                    }
                }
            }
            let attention_output = self.execute_q8_row(run, tensors.o, &attention)?;
            add_assign(hidden, &attention_output, "Qwen3 attention output")?;
            self.apply_ffn(run, layer, hidden)?;
        }
        Ok(())
    }

    fn project_qkv(
        &self,
        run: &mut ExecutionRun,
        layer: usize,
        position: usize,
        attention_input: &[f32],
    ) -> Result<(Vec<f32>, Vec<f32>, Vec<f32>), String> {
        let tensors = &self.tensors.layers[layer];
        let mut q = self.execute_q8_row(run, tensors.q, attention_input)?;
        let mut k = self.execute_q8_row(run, tensors.k, attention_input)?;
        let v = self.execute_q8_row(run, tensors.v, attention_input)?;
        let (key_width, value_width, _) = self.attention_dimensions()?;
        if q.len() != self.config.n_head * self.config.n_embd_head_k
            || k.len() != key_width
            || v.len() != value_width
        {
            return Err("Qwen3 attention projection shape mismatch".into());
        }
        if let Some(weight) = &self.row_weights.q_norms[layer] {
            for head in q.chunks_exact_mut(self.config.n_embd_head_k) {
                crate::rms_norm_inplace(head, weight, self.config.eps);
            }
        }
        if let Some(weight) = &self.row_weights.k_norms[layer] {
            for head in k.chunks_exact_mut(self.config.n_embd_head_k) {
                crate::rms_norm_inplace(head, weight, self.config.eps);
            }
        }
        crate::rope_neox(
            &mut q,
            position,
            self.config.n_embd_head_k,
            self.config.freq_base,
        );
        crate::rope_neox(
            &mut k,
            position,
            self.config.n_embd_head_k,
            self.config.freq_base,
        );
        Ok((q, k, v))
    }

    fn attention_dimensions(&self) -> Result<(usize, usize, usize), String> {
        let key_width = self
            .config
            .n_head_kv
            .checked_mul(self.config.n_embd_head_k)
            .ok_or("Qwen3 key width overflow")?;
        let value_width = self
            .config
            .n_head_kv
            .checked_mul(self.config.n_embd_head_v)
            .ok_or("Qwen3 value width overflow")?;
        let group_size = self
            .config
            .n_head
            .checked_div(self.config.n_head_kv)
            .filter(|size| *size != 0)
            .ok_or("Invalid Qwen3 grouped-query attention")?;
        Ok((key_width, value_width, group_size))
    }

    fn apply_ffn(
        &self,
        run: &mut ExecutionRun,
        layer: usize,
        hidden: &mut [f32],
    ) -> Result<(), String> {
        let tensors = &self.tensors.layers[layer];
        let ffn_input = rms_normed(hidden, &self.row_weights.ffn_norms[layer], self.config.eps);
        let gate = self.execute_q8_row(run, tensors.gate, &ffn_input)?;
        let mut up = self.execute_q8_row(run, tensors.up, &ffn_input)?;
        if gate.len() != self.config.n_ff || up.len() != self.config.n_ff {
            return Err("Qwen3 FFN up projection shape mismatch".into());
        }
        crate::silu_mul_inplace(&gate, &mut up);
        let down = self.execute_q8_row(run, tensors.down, &up)?;
        add_assign(hidden, &down, "Qwen3 FFN down projection")
    }

    fn execute_q8_row(
        &self,
        run: &mut ExecutionRun,
        tensor: TensorId,
        input: &[f32],
    ) -> Result<Vec<f32>, String> {
        let expected_input = *self
            .q8_input_widths
            .get(&tensor)
            .ok_or_else(|| format!("Missing Qwen3 input width for {tensor:?}"))?;
        if input.len() != expected_input {
            return Err(format!(
                "Qwen3 compiled Q8 input width mismatch: expected {expected_input}, got {}",
                input.len()
            ));
        }
        let shards = run.plan().components[&ComponentId::Llm]
            .row_shards
            .get(&tensor)
            .ok_or_else(|| format!("Missing compiled Q8 program for {tensor:?}"))?;
        let rows = usize::try_from(shards.iter().map(|shard| shard.rows.end).max().unwrap_or(0))
            .map_err(|_| "Qwen3 output rows do not fit usize")?;
        let mut result = vec![0.0; rows];
        run.execute_q8(ComponentId::Llm, tensor, input, 1, &mut result)
            .map_err(|error| error.to_string())?;
        Ok(result)
    }
}

fn pool_and_normalize_embedding(
    hidden: &[f32],
    width: usize,
    pooling: Qwen3EmbeddingPooling,
) -> Result<Vec<f32>, String> {
    if width == 0 || hidden.is_empty() || hidden.len() % width != 0 {
        return Err("Embedding rows have an invalid shape".into());
    }
    let mut embedding = match pooling {
        Qwen3EmbeddingPooling::Last => hidden[hidden.len() - width..].to_vec(),
        Qwen3EmbeddingPooling::Mean => {
            let rows = hidden.len() / width;
            let mut mean = vec![0.0; width];
            for row in hidden.chunks_exact(width) {
                for (output, value) in mean.iter_mut().zip(row) {
                    *output += value;
                }
            }
            for value in &mut mean {
                *value /= rows as f32;
            }
            mean
        }
    };
    if embedding.iter().any(|value| !value.is_finite()) {
        return Err("Embedding contains a non-finite value".into());
    }
    let sum = embedding
        .iter()
        .map(|&value| f64::from(value * value))
        .sum::<f64>();
    let scale = if sum > 0.0 {
        (1.0 / sum.sqrt()) as f32
    } else {
        0.0
    };
    for value in &mut embedding {
        *value *= scale;
    }
    if embedding.iter().any(|value| !value.is_finite()) {
        return Err("Normalized embedding contains a non-finite value".into());
    }
    Ok(embedding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_l2_matches_llama_f32_product_and_scale_bits() {
        assert_eq!(
            pool_and_normalize_embedding(&[f32::from_bits(1)], 1, Qwen3EmbeddingPooling::Last)
                .unwrap(),
            [0.0]
        );
    }
}

fn load_row_f32(
    catalog: &TensorCatalog,
    tensor: TensorId,
    expected: usize,
) -> Result<Vec<f32>, String> {
    let entry = catalog
        .entry(tensor)
        .ok_or("Missing Qwen3 auxiliary tensor")?;
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
                "Unsupported Qwen3 auxiliary tensor type: {other:?}"
            ))
        }
    };
    if values.len() != expected {
        return Err(format!(
            "Qwen3 auxiliary tensor shape mismatch: expected {expected}, got {}",
            values.len()
        ));
    }
    Ok(values)
}

fn reset_row_state(state: &mut Qwen3RowState, config: &Qwen3Config) {
    let key_width = config.n_head_kv * config.n_embd_head_k;
    let value_width = config.n_head_kv * config.n_embd_head_v;
    state.next_position = 0;
    state.keys = (0..config.n_layer)
        .map(|_| vec![0.0; config.n_ctx * key_width])
        .collect();
    state.values = (0..config.n_layer)
        .map(|_| vec![0.0; config.n_ctx * value_width])
        .collect();
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

pub(crate) fn checked_params<'a>(
    tokens: &'a [u32],
    positions: &'a [[u32; 4]],
) -> Result<RunParams<'a>, String> {
    if tokens.is_empty() {
        return Err("token count must be non-zero".into());
    }
    if positions.len() != tokens.len() {
        return Err(format!(
            "position count mismatch: tokens={}, positions={}",
            tokens.len(),
            positions.len()
        ));
    }
    let token_count = u32::try_from(tokens.len()).map_err(|_| "token count does not fit u32")?;
    let position_start = positions.first().map(|position| position[0]).unwrap_or(0);
    Ok(RunParams {
        token_count,
        position_start,
        mrope_positions: positions,
        token_ids: tokens,
    })
}

pub(crate) fn validate_model_inputs<'a>(
    run: &ExecutionRun,
    tokens: &'a [u32],
    positions: &'a [[u32; 4]],
    output_len: usize,
    vocab: usize,
    context: usize,
    expected_position: usize,
) -> Result<RunParams<'a>, String> {
    let params = checked_params(tokens, positions)?;
    if output_len != vocab {
        return Err(format!(
            "model output buffer size mismatch: expected {vocab}, got {output_len}"
        ));
    }
    if tokens.iter().any(|token| *token as usize >= vocab) {
        return Err("token ID exceeds vocabulary".into());
    }
    let capacity = usize::try_from(
        run.batch_capacity(ComponentId::Llm)
            .map_err(|error| error.to_string())?,
    )
    .map_err(|_| "compiled batch capacity does not fit usize")?;
    if run.plan().components[&ComponentId::Llm].mode == PlacementMode::Layer
        && tokens.len() > capacity
    {
        return Err(format!(
            "token batch exceeds compiled capacity: tokens={}, capacity={capacity}",
            tokens.len()
        ));
    }
    if run.plan().components[&ComponentId::Llm].mode == PlacementMode::Row && capacity == 0 {
        return Err("compiled Row capacity is zero".into());
    }
    for (offset, position) in positions.iter().enumerate() {
        let position = usize::try_from(position[0]).map_err(|_| "position does not fit usize")?;
        let expected = expected_position
            .checked_add(offset)
            .ok_or("position overflow")?;
        if position != expected || position >= context {
            return Err(format!(
                "positions must be contiguous within context from {expected_position}; got {position}"
            ));
        }
    }
    Ok(params)
}
