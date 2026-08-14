use crate::model::{
    model_config_from_source, GGMLType, MetaValue, MetaValueType, TensorInfo, TensorSource,
};
use crate::ops::*;
#[cfg(feature = "parity-trace")]
use crate::parity_trace;
use crate::scratchpad::{ExecutionScratchpad, KvCache, KvCacheF16};
use crate::thread_pool::ComputePool;
use crate::tokenizer::BPETokenizer;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Qwen3Rope {
    Neox,
    Interleaved { sections: [i32; 4], n_dims: usize },
}

pub struct Qwen3Config {
    pub architecture: String,
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
    pub has_qk_norm: bool,
    pub rope: Qwen3Rope,
}

impl Qwen3Config {
    fn from_source(source: &dyn TensorSource, tokenizer_vocab: usize) -> Result<Self, String> {
        let architecture = source
            .metadata("general.architecture")
            .and_then(MetaValue::to_string_val)
            .ok_or_else(|| "Missing metadata: general.architecture".to_string())?;
        if !matches!(architecture, "qwen2" | "qwen3" | "qwen3vl" | "llama") {
            return Err(format!("Unsupported architecture: {architecture}"));
        }

        let config = model_config_from_source(source)?;
        if config.vocab_size != tokenizer_vocab {
            return Err(format!(
                "{architecture}.vocab_size {} does not match tokenizer vocab {tokenizer_vocab}",
                config.vocab_size
            ));
        }
        if config.n_head_kv == 0 || config.n_head % config.n_head_kv != 0 {
            return Err(format!(
                "Invalid {architecture} grouped-query heads: head_count={}, head_count_kv={}",
                config.n_head, config.n_head_kv
            ));
        }
        if config.n_embd == 0
            || config.n_layer == 0
            || config.n_ff == 0
            || config.vocab_size == 0
            || config.n_ctx == 0
        {
            return Err(format!("Invalid zero-sized {architecture} configuration"));
        }
        if !config.norm_eps.is_finite()
            || config.norm_eps <= 0.0
            || !config.rope_freq_base.is_finite()
            || config.rope_freq_base <= 0.0
        {
            return Err(format!(
                "Invalid {architecture} normalization or RoPE metadata"
            ));
        }

        let n_embd_head_k =
            optional_usize(source, &format!("{architecture}.attention.key_length"))?
                .unwrap_or(config.n_embd_head);
        let n_embd_head_v =
            optional_usize(source, &format!("{architecture}.attention.value_length"))?
                .unwrap_or(config.n_embd_head);
        if n_embd_head_k == 0 || n_embd_head_v == 0 {
            return Err(format!(
                "Invalid {architecture} attention head lengths: key={n_embd_head_k}, value={n_embd_head_v}"
            ));
        }

        let has_qk_norm = matches!(architecture, "qwen3" | "qwen3vl");
        let rope = if architecture == "qwen3vl" {
            let sections = read_i32_array(source, "qwen3vl.rope.dimension_sections")?;
            if sections != [24, 20, 20, 0] {
                return Err(format!(
                    "Unsupported qwen3vl.rope.dimension_sections: {sections:?}"
                ));
            }
            Qwen3Rope::Interleaved {
                sections,
                n_dims: n_embd_head_k,
            }
        } else {
            Qwen3Rope::Neox
        };

        if architecture == "qwen3vl"
            && (
                config.n_embd,
                config.n_layer,
                config.n_head,
                config.n_head_kv,
                n_embd_head_k,
                n_embd_head_v,
                config.n_ff,
                config.n_ctx,
                config.norm_eps,
                config.rope_freq_base,
            ) != (1024, 28, 16, 8, 128, 128, 3072, 65_536, 1e-6, 1_000_000.0)
        {
            return Err("Unsupported qwen3vl main-model configuration".into());
        }

        Ok(Self {
            architecture: architecture.into(),
            n_embd: config.n_embd,
            n_layer: config.n_layer,
            n_head: config.n_head,
            n_head_kv: config.n_head_kv,
            n_embd_head_k,
            n_embd_head_v,
            n_ff: config.n_ff,
            vocab: config.vocab_size,
            n_ctx: config.n_ctx,
            eps: config.norm_eps,
            freq_base: config.rope_freq_base,
            has_qk_norm,
            rope,
        })
    }
}

fn optional_usize(source: &dyn TensorSource, key: &str) -> Result<Option<usize>, String> {
    let Some(value) = source.metadata(key) else {
        return Ok(None);
    };
    let value = value
        .to_u64()
        .ok_or_else(|| format!("Invalid metadata: {key}"))?;
    usize::try_from(value)
        .map(Some)
        .map_err(|_| format!("{key} does not fit usize"))
}

fn read_i32_array(source: &dyn TensorSource, key: &str) -> Result<[i32; 4], String> {
    let values = match source.metadata(key) {
        Some(MetaValue::Array(MetaValueType::Int32, values)) => values,
        _ => return Err(format!("Missing or invalid metadata: {key}")),
    };
    let values: Vec<i32> = values
        .iter()
        .map(|value| match value {
            MetaValue::Int32(value) => Ok(*value),
            _ => Err(format!("Invalid Int32 array value in {key}")),
        })
        .collect::<Result<_, _>>()?;
    values
        .try_into()
        .map_err(|values: Vec<i32>| format!("{key} has {} values; expected 4", values.len()))
}

fn checked_session_capacity(
    prompt: usize,
    generation: usize,
    context: usize,
) -> Result<usize, String> {
    let capacity = prompt
        .checked_add(generation)
        .ok_or_else(|| "Session capacity overflow".to_string())?;
    if capacity > context {
        return Err(format!(
            "Session capacity {capacity} exceeds model context {context}"
        ));
    }
    Ok(capacity)
}

fn checked_decoder_steps(
    prompt: usize,
    generation: usize,
    context: usize,
) -> Result<usize, String> {
    checked_session_capacity(prompt, generation, context)?
        .checked_sub(1)
        .ok_or_else(|| "Decoder step count underflow".to_string())
}

fn checked_generated_position(
    prompt_positions: &[[usize; 4]],
    generated_index: usize,
) -> Result<[usize; 4], String> {
    let last_prompt_position = prompt_positions
        .last()
        .ok_or_else(|| "Cannot generate a position without prompt positions".to_string())?[0];
    let position = last_prompt_position
        .checked_add(1)
        .and_then(|position| position.checked_add(generated_index))
        .ok_or_else(|| "Generated position overflow".to_string())?;
    Ok([position, position, position, 0])
}

fn validate_input_shapes(
    token_count: usize,
    embedding_dim: usize,
    position_count: usize,
    embedding_values: Option<usize>,
) -> Result<(), String> {
    if position_count != token_count {
        return Err(format!(
            "Position count {position_count} does not match token count {token_count}"
        ));
    }
    if let Some(values) = embedding_values {
        let expected = token_count
            .checked_mul(embedding_dim)
            .ok_or_else(|| "Input embedding shape overflow".to_string())?;
        if values != expected {
            return Err(format!(
                "Embedding value count {values} does not match expected {expected}"
            ));
        }
    }
    Ok(())
}

fn greedy_token(logits: &[f32]) -> Result<u32, String> {
    let (&first, rest) = logits
        .split_first()
        .ok_or_else(|| "Cannot sample empty logits".to_string())?;
    if !first.is_finite() {
        return Err("Cannot sample non-finite logits".into());
    }
    let mut best_id = 0usize;
    let mut best = first;
    for (index, &logit) in rest.iter().enumerate() {
        if !logit.is_finite() {
            return Err("Cannot sample non-finite logits".into());
        }
        if logit > best {
            best = logit;
            best_id = index + 1;
        }
    }
    u32::try_from(best_id).map_err(|_| "Token ID does not fit u32".into())
}

pub struct Qwen3Input<'a> {
    pub token_ids: &'a [u32],
    pub positions: &'a [[usize; 4]],
    pub embeddings: Option<&'a [f32]>,
}

#[derive(Debug, Clone, Copy)]
pub struct Qwen3GenerateOptions {
    pub max_new_tokens: usize,
    pub temperature: f32,
}

pub struct Qwen3Generation {
    pub text: String,
    pub rendered_tokens: Vec<String>,
    pub token_ids: Vec<u32>,
    pub prompt_tokens: usize,
}

pub struct Qwen3Model {
    source: Arc<dyn TensorSource>,
    tokenizer: Arc<BPETokenizer>,
    pool: Arc<ComputePool>,
    config: Qwen3Config,
    layers: Vec<Qwen3LayerWeights>,
    output_norm: Vec<f32>,
    token_embedding: &'static [u8],
    output: &'static [u8],
}

struct Qwen3LayerWeights {
    attn_norm: Vec<f32>,
    ffn_norm: Vec<f32>,
    q_norm: Option<Vec<f32>>,
    k_norm: Option<Vec<f32>>,
    wq: &'static [u8],
    wk: &'static [u8],
    wv: &'static [u8],
    wo: &'static [u8],
    w_gate: &'static [u8],
    w_up: &'static [u8],
    w_down: &'static [u8],
}

pub struct Qwen3Session<'model> {
    model: &'model Qwen3Model,
    kv_cache: KvCache,
    scratch: ExecutionScratchpad,
    capacity: usize,
}

impl Qwen3Model {
    pub fn from_source(
        source: Arc<dyn TensorSource>,
        tokenizer: Arc<BPETokenizer>,
        pool: Arc<ComputePool>,
    ) -> Result<Self, String> {
        let config = Qwen3Config::from_source(source.as_ref(), tokenizer.vocab_size())?;
        let n_embd_q = checked_product("query width", config.n_head, config.n_embd_head_k)?;
        let n_embd_k = checked_product("key width", config.n_head_kv, config.n_embd_head_k)?;
        let n_embd_v = checked_product("value width", config.n_head_kv, config.n_embd_head_v)?;
        let n_attn = checked_product(
            "attention output width",
            config.n_head,
            config.n_embd_head_v,
        )?;

        let output_norm = load_f32_tensor(
            source.as_ref(),
            "output_norm.weight",
            &[usize_to_u64(config.n_embd, "embedding width")?],
        )?;
        let embedding_dims = [
            usize_to_u64(config.n_embd, "embedding width")?,
            usize_to_u64(config.vocab, "vocabulary size")?,
        ];
        let token_embedding = static_q8_tensor(&source, "token_embd.weight", &embedding_dims)?;
        let output = if source.tensor_info("output.weight").is_some() {
            static_q8_tensor(&source, "output.weight", &embedding_dims)?
        } else {
            token_embedding
        };

        check_allocation(
            "decoder layers",
            config.n_layer,
            std::mem::size_of::<Qwen3LayerWeights>(),
        )?;
        let mut layers = Vec::new();
        layers
            .try_reserve_exact(config.n_layer)
            .map_err(|error| format!("Failed to allocate decoder layers: {error}"))?;
        for layer in 0..config.n_layer {
            let name = |suffix: &str| format!("blk.{layer}.{suffix}");
            let n_embd_dim = [usize_to_u64(config.n_embd, "embedding width")?];
            let head_dim = [usize_to_u64(config.n_embd_head_k, "key head width")?];
            layers.push(Qwen3LayerWeights {
                attn_norm: load_f32_tensor(
                    source.as_ref(),
                    &name("attn_norm.weight"),
                    &n_embd_dim,
                )?,
                ffn_norm: load_f32_tensor(source.as_ref(), &name("ffn_norm.weight"), &n_embd_dim)?,
                q_norm: if config.has_qk_norm {
                    Some(load_f32_tensor(
                        source.as_ref(),
                        &name("attn_q_norm.weight"),
                        &head_dim,
                    )?)
                } else {
                    None
                },
                k_norm: if config.has_qk_norm {
                    Some(load_f32_tensor(
                        source.as_ref(),
                        &name("attn_k_norm.weight"),
                        &head_dim,
                    )?)
                } else {
                    None
                },
                wq: static_q8_matrix(&source, &name("attn_q.weight"), config.n_embd, n_embd_q)?,
                wk: static_q8_matrix(&source, &name("attn_k.weight"), config.n_embd, n_embd_k)?,
                wv: static_q8_matrix(&source, &name("attn_v.weight"), config.n_embd, n_embd_v)?,
                wo: static_q8_matrix(&source, &name("attn_output.weight"), n_attn, config.n_embd)?,
                w_gate: static_q8_matrix(
                    &source,
                    &name("ffn_gate.weight"),
                    config.n_embd,
                    config.n_ff,
                )?,
                w_up: static_q8_matrix(
                    &source,
                    &name("ffn_up.weight"),
                    config.n_embd,
                    config.n_ff,
                )?,
                w_down: static_q8_matrix(
                    &source,
                    &name("ffn_down.weight"),
                    config.n_ff,
                    config.n_embd,
                )?,
            });
        }

        Ok(Self {
            source,
            tokenizer,
            pool,
            config,
            layers,
            output_norm,
            token_embedding,
            output,
        })
    }

    pub fn config(&self) -> &Qwen3Config {
        &self.config
    }

    pub fn tokenizer(&self) -> &BPETokenizer {
        &self.tokenizer
    }

    pub fn pool(&self) -> Arc<ComputePool> {
        Arc::clone(&self.pool)
    }

    pub fn layers(&self) -> &Vec<Qwen3LayerWeights> {
        &self.layers
    }

    pub fn output_norm(&self) -> &Vec<f32> {
        &self.output_norm
    }

    pub fn embed_tokens(&self, token_ids: &[u32]) -> Result<Vec<f32>, String> {
        validate_token_ids(token_ids, self.config.vocab)?;
        let len = checked_product(
            "token embedding values",
            token_ids.len(),
            self.config.n_embd,
        )?;
        check_allocation("token embeddings", len, std::mem::size_of::<f32>())?;
        let mut embeddings = vec![0.0; len];
        for (row, &token_id) in embeddings
            .chunks_exact_mut(self.config.n_embd)
            .zip(token_ids)
        {
            embedding_lookup_q8_0(self.token_embedding, token_id, self.config.n_embd, row);
        }
        Ok(embeddings)
    }

    pub fn generate(
        &self,
        input: Qwen3Input<'_>,
        options: Qwen3GenerateOptions,
    ) -> Result<Qwen3Generation, String> {
        self.generate_with_asr_trace(input, options, false)
    }

    pub(crate) fn generate_asr(
        &self,
        input: Qwen3Input<'_>,
        options: Qwen3GenerateOptions,
    ) -> Result<Qwen3Generation, String> {
        self.generate_with_asr_trace(input, options, true)
    }

    fn generate_with_asr_trace(
        &self,
        input: Qwen3Input<'_>,
        options: Qwen3GenerateOptions,
        asr_trace: bool,
    ) -> Result<Qwen3Generation, String> {
        validate_generation(self, &input, options)?;
        let capacity = checked_session_capacity(
            input.token_ids.len(),
            options.max_new_tokens,
            self.config.n_ctx,
        )?;
        Qwen3Session::new(self, capacity)?.generate_with_asr_trace(input, options, asr_trace)
    }
}

impl<'model> Qwen3Session<'model> {
    pub fn new(model: &'model Qwen3Model, capacity: usize) -> Result<Self, String> {
        if capacity == 0 || capacity > model.config.n_ctx {
            return Err(format!(
                "Session capacity {capacity} must be within 1..={}",
                model.config.n_ctx
            ));
        }
        let config = &model.config;
        let n_embd_q = checked_product("query width", config.n_head, config.n_embd_head_k)?;
        let n_embd_k = checked_product("key width", config.n_head_kv, config.n_embd_head_k)?;
        let n_embd_v = checked_product("value width", config.n_head_kv, config.n_embd_head_v)?;
        let n_attn = checked_product(
            "attention output width",
            config.n_head,
            config.n_embd_head_v,
        )?;
        let kv_stride = n_embd_k.max(n_embd_v);
        let kv_size = checked_product(
            "KV cache values",
            checked_product("KV cache rows", config.n_layer, capacity)?,
            kv_stride,
        )?;
        check_allocation("F16 KV cache", kv_size, std::mem::size_of::<u16>())?;

        let max_n_in = n_embd_q.max(n_attn).max(config.n_ff);
        let score_stride = capacity
            .checked_add(255)
            .map(|value| value / 256 * 256)
            .ok_or_else(|| "Attention score stride overflow".to_string())?;
        let score_values =
            checked_product("attention scores", model.pool.n_threads(), score_stride)?;
        for (name, len, bytes) in [
            ("hidden state", config.n_embd, std::mem::size_of::<f32>()),
            (
                "normalized state",
                config.n_embd,
                std::mem::size_of::<f32>(),
            ),
            ("queries", n_embd_q, std::mem::size_of::<f32>()),
            ("keys", kv_stride, std::mem::size_of::<f32>()),
            ("values", kv_stride, std::mem::size_of::<f32>()),
            ("attention output", n_attn, std::mem::size_of::<f32>()),
            (
                "attention projection",
                config.n_embd,
                std::mem::size_of::<f32>(),
            ),
            ("down projection", config.n_embd, std::mem::size_of::<f32>()),
            ("gate projection", config.n_ff, std::mem::size_of::<f32>()),
            ("up projection", config.n_ff, std::mem::size_of::<f32>()),
            ("logits", config.vocab, std::mem::size_of::<f32>()),
            ("quantized activations", max_n_in, std::mem::size_of::<u8>()),
            (
                "quantization scales",
                max_n_in / 32,
                std::mem::size_of::<f32>(),
            ),
            ("attention scores", score_values, std::mem::size_of::<f32>()),
        ] {
            check_allocation(name, len, bytes)?;
        }

        Ok(Self {
            model,
            kv_cache: KvCache::F16(KvCacheF16 {
                k: vec![0; kv_size],
                v: vec![0; kv_size],
            }),
            scratch: ExecutionScratchpad {
                x: vec![0.0; config.n_embd],
                normed: vec![0.0; config.n_embd],
                q: vec![0.0; n_embd_q],
                k_new: vec![0.0; kv_stride],
                v_new: vec![0.0; kv_stride],
                attn_out: vec![0.0; n_attn],
                attn_proj: vec![0.0; config.n_embd],
                down_buf: vec![0.0; config.n_embd],
                gate_buf: vec![0.0; config.n_ff],
                up_buf: vec![0.0; config.n_ff],
                logits: vec![0.0; config.vocab],
                q8_buf: vec![0; max_n_in],
                scale_buf: vec![0.0; max_n_in / 32],
                score_stride,
                scores: vec![0.0; score_values],
            },
            capacity,
        })
    }

    pub fn generate(
        &mut self,
        input: Qwen3Input<'_>,
        options: Qwen3GenerateOptions,
    ) -> Result<Qwen3Generation, String> {
        self.generate_with_asr_trace(input, options, false)
    }

    fn generate_with_asr_trace(
        &mut self,
        input: Qwen3Input<'_>,
        options: Qwen3GenerateOptions,
        asr_trace: bool,
    ) -> Result<Qwen3Generation, String> {
        validate_generation(self.model, &input, options)?;
        let required = checked_session_capacity(
            input.token_ids.len(),
            options.max_new_tokens,
            self.model.config.n_ctx,
        )?;
        if required > self.capacity {
            return Err(format!(
                "Generation requires capacity {required}; session has {}",
                self.capacity
            ));
        }
        self.generate_inner(input, options, asr_trace)
    }

    fn generate_inner(
        &mut self,
        input: Qwen3Input<'_>,
        options: Qwen3GenerateOptions,
        asr_trace: bool,
    ) -> Result<Qwen3Generation, String> {
        let _source = &self.model.source;
        let model = self.model;
        let config = &model.config;
        let capacity = self.capacity;
        let n_prompt = input.token_ids.len();
        let n_embd_q = checked_product("query width", config.n_head, config.n_embd_head_k)?;
        let n_embd_k = checked_product("key width", config.n_head_kv, config.n_embd_head_k)?;
        let n_embd_v = checked_product("value width", config.n_head_kv, config.n_embd_head_v)?;
        let n_attn = checked_product(
            "attention output width",
            config.n_head,
            config.n_embd_head_v,
        )?;
        let kv_stride = n_embd_k.max(n_embd_v);
        let kv_cache_size = checked_product(
            "KV cache values",
            checked_product("KV cache rows", config.n_layer, capacity)?,
            kv_stride,
        )?;
        let max_n_in = n_embd_q.max(n_attn).max(config.n_ff);
        let group_size = config.n_head / config.n_head_kv;
        let kq_scale = 1.0 / (config.n_embd_head_k as f32).sqrt();
        let (k_cache_ptr, v_cache_ptr) = match &mut self.kv_cache {
            KvCache::F16(cache) => (cache.k.as_mut_ptr(), cache.v.as_mut_ptr()),
            KvCache::F32(_) => return Err("Qwen3Session requires an F16 KV cache".into()),
        };

        #[cfg(feature = "parity-trace")]
        {
            if asr_trace {
                parity_trace::report(parity_trace::token_ids("asr.prompt_ids", input.token_ids));
                let position_values =
                    checked_product("ASR position values", input.positions.len(), 4)?;
                let mut positions = Vec::new();
                positions
                    .try_reserve_exact(position_values)
                    .map_err(|error| format!("Failed to allocate ASR positions: {error}"))?;
                for position in input.positions {
                    positions.extend_from_slice(position);
                }
                parity_trace::report(parity_trace::usize_values(
                    "asr.positions",
                    &[input.positions.len(), 4],
                    &positions,
                ));
            } else {
                parity_trace::report(parity_trace::token_ids("prompt_ids", input.token_ids));
                let text_positions: Vec<usize> =
                    input.positions.iter().map(|value| value[0]).collect();
                parity_trace::report(parity_trace::usize_values(
                    "qwen3.positions",
                    &[text_positions.len()],
                    &text_positions,
                ));
            }
        }
        #[cfg(not(feature = "parity-trace"))]
        let _ = asr_trace;

        let mut generated_tokens = Vec::new();
        generated_tokens
            .try_reserve_exact(options.max_new_tokens)
            .map_err(|error| format!("Failed to allocate generated tokens: {error}"))?;
        let mut rendered_tokens = Vec::new();
        rendered_tokens
            .try_reserve_exact(options.max_new_tokens)
            .map_err(|error| format!("Failed to allocate rendered tokens: {error}"))?;
        let mut decoder = model.tokenizer.streaming_decoder(false);

        let decoder_steps = checked_decoder_steps(n_prompt, options.max_new_tokens, config.n_ctx)?;
        for step in 0..decoder_steps {
            let position = if step < n_prompt {
                input.positions[step]
            } else {
                checked_generated_position(input.positions, step - n_prompt)?
            };
            if step < n_prompt {
                if let Some(embeddings) = input.embeddings {
                    let start = step * config.n_embd;
                    self.scratch
                        .x
                        .copy_from_slice(&embeddings[start..start + config.n_embd]);
                } else {
                    embedding_lookup_q8_0(
                        model.token_embedding,
                        input.token_ids[step],
                        config.n_embd,
                        &mut self.scratch.x,
                    );
                }
            } else {
                let token_id = *generated_tokens
                    .last()
                    .ok_or_else(|| "Missing generated token for decoder step".to_string())?;
                embedding_lookup_q8_0(
                    model.token_embedding,
                    token_id,
                    config.n_embd,
                    &mut self.scratch.x,
                );
            }
            #[cfg(feature = "parity-trace")]
            parity_trace::report(parity_trace::checkpoint(
                "model.input_embed",
                None,
                &[1, config.n_embd],
                &self.scratch.x,
            ));

            for layer in 0..config.n_layer {
                let weights = &model.layers[layer];
                let x_ptr = self.scratch.x.as_mut_ptr();
                let normed_ptr = self.scratch.normed.as_mut_ptr();
                let q_ptr = self.scratch.q.as_mut_ptr();
                let k_ptr = self.scratch.k_new.as_mut_ptr();
                let v_ptr = self.scratch.v_new.as_mut_ptr();
                let attn_out_ptr = self.scratch.attn_out.as_mut_ptr();
                let attn_proj_ptr = self.scratch.attn_proj.as_mut_ptr();
                let down_buf_ptr = self.scratch.down_buf.as_mut_ptr();
                let gate_buf_ptr = self.scratch.gate_buf.as_mut_ptr();
                let up_buf_ptr = self.scratch.up_buf.as_mut_ptr();
                let q8_buf_ptr = self.scratch.q8_buf.as_mut_ptr();
                let scale_buf_ptr = self.scratch.scale_buf.as_mut_ptr();

                let x = unsafe { std::slice::from_raw_parts_mut(x_ptr, config.n_embd) };
                let normed = unsafe { std::slice::from_raw_parts_mut(normed_ptr, config.n_embd) };
                let q8_buf = unsafe { std::slice::from_raw_parts_mut(q8_buf_ptr, max_n_in) };
                let scale_buf =
                    unsafe { std::slice::from_raw_parts_mut(scale_buf_ptr, max_n_in / 32) };

                rms_norm(x, &weights.attn_norm, normed, config.eps);
                #[cfg(feature = "parity-trace")]
                if layer == 0 {
                    parity_trace::report(parity_trace::checkpoint(
                        "attn_norm-0",
                        Some(0),
                        &[1, config.n_embd],
                        normed,
                    ));
                }
                quantize_q8_0_into(
                    normed,
                    config.n_embd,
                    &mut q8_buf[..config.n_embd],
                    &mut scale_buf[..config.n_embd / 32],
                );
                let q8 = q8_buf[..config.n_embd].as_ptr();
                let scales = scale_buf[..config.n_embd / 32].as_ptr();
                let pool = Arc::clone(&model.pool);
                pool.compute(move |thread, threads| {
                    let q8 = unsafe { std::slice::from_raw_parts(q8, config.n_embd) };
                    let scales = unsafe { std::slice::from_raw_parts(scales, config.n_embd / 32) };
                    let q = unsafe { std::slice::from_raw_parts_mut(q_ptr, n_embd_q) };
                    let k = unsafe { std::slice::from_raw_parts_mut(k_ptr, n_embd_k) };
                    let v = unsafe { std::slice::from_raw_parts_mut(v_ptr, n_embd_v) };
                    matmul_q8_0_quantized_parallel_rows(
                        weights.wq,
                        q8,
                        scales,
                        q,
                        config.n_embd,
                        n_embd_q,
                        thread,
                        threads,
                    );
                    matmul_q8_0_quantized_parallel_rows(
                        weights.wk,
                        q8,
                        scales,
                        k,
                        config.n_embd,
                        n_embd_k,
                        thread,
                        threads,
                    );
                    matmul_q8_0_quantized_parallel_rows(
                        weights.wv,
                        q8,
                        scales,
                        v,
                        config.n_embd,
                        n_embd_v,
                        thread,
                        threads,
                    );
                });

                {
                    let q = unsafe { std::slice::from_raw_parts_mut(q_ptr, n_embd_q) };
                    let k = unsafe { std::slice::from_raw_parts_mut(k_ptr, n_embd_k) };
                    let v = unsafe { std::slice::from_raw_parts_mut(v_ptr, n_embd_v) };
                    if let (Some(q_norm), Some(k_norm)) =
                        (weights.q_norm.as_deref(), weights.k_norm.as_deref())
                    {
                        for head in q.chunks_exact_mut(config.n_embd_head_k) {
                            rms_norm_inplace(head, q_norm, config.eps);
                        }
                        for head in k.chunks_exact_mut(config.n_embd_head_k) {
                            rms_norm_inplace(head, k_norm, config.eps);
                        }
                    }
                    #[cfg(feature = "parity-trace")]
                    if layer == 0 {
                        parity_trace::report(parity_trace::checkpoint(
                            "Qcur_normed-0",
                            Some(0),
                            &[config.n_head, config.n_embd_head_k],
                            q,
                        ));
                        parity_trace::report(parity_trace::checkpoint(
                            "Kcur_normed-0",
                            Some(0),
                            &[config.n_head_kv, config.n_embd_head_k],
                            k,
                        ));
                    }
                    for head in q.chunks_exact_mut(config.n_embd_head_k) {
                        match config.rope {
                            Qwen3Rope::Neox => {
                                rope_neox(head, position[0], config.n_embd_head_k, config.freq_base)
                            }
                            Qwen3Rope::Interleaved { sections, n_dims } => rope_mrope_interleaved(
                                head,
                                position,
                                sections,
                                config.n_embd_head_k,
                                config.freq_base,
                                n_dims,
                            ),
                        }
                    }
                    for head in k.chunks_exact_mut(config.n_embd_head_k) {
                        match config.rope {
                            Qwen3Rope::Neox => {
                                rope_neox(head, position[0], config.n_embd_head_k, config.freq_base)
                            }
                            Qwen3Rope::Interleaved { sections, n_dims } => rope_mrope_interleaved(
                                head,
                                position,
                                sections,
                                config.n_embd_head_k,
                                config.freq_base,
                                n_dims,
                            ),
                        }
                    }
                    #[cfg(feature = "parity-trace")]
                    if layer == 0 {
                        parity_trace::report(parity_trace::checkpoint(
                            "Qcur-0",
                            Some(0),
                            &[config.n_head, config.n_embd_head_k],
                            q,
                        ));
                        parity_trace::report(parity_trace::checkpoint(
                            "Kcur-0",
                            Some(0),
                            &[config.n_head_kv, config.n_embd_head_k],
                            k,
                        ));
                    }

                    let layer_base = layer * capacity * kv_stride;
                    let k_cache =
                        unsafe { std::slice::from_raw_parts_mut(k_cache_ptr, kv_cache_size) };
                    let v_cache =
                        unsafe { std::slice::from_raw_parts_mut(v_cache_ptr, kv_cache_size) };
                    for head in 0..config.n_head_kv {
                        let k_offset = head * config.n_embd_head_k;
                        let v_offset = head * config.n_embd_head_v;
                        let cache_row = layer_base + step * kv_stride;
                        f32_slice_to_f16(
                            &k[k_offset..k_offset + config.n_embd_head_k],
                            &mut k_cache
                                [cache_row + k_offset..cache_row + k_offset + config.n_embd_head_k],
                        );
                        f32_slice_to_f16(
                            &v[v_offset..v_offset + config.n_embd_head_v],
                            &mut v_cache
                                [cache_row + v_offset..cache_row + v_offset + config.n_embd_head_v],
                        );
                    }
                }

                let pool = Arc::clone(&model.pool);
                let scores_ptr = self.scratch.scores.as_mut_ptr();
                let score_stride = self.scratch.score_stride;
                pool.compute(move |thread, threads| {
                    let q = unsafe { std::slice::from_raw_parts(q_ptr, n_embd_q) };
                    let attn_out = unsafe { std::slice::from_raw_parts_mut(attn_out_ptr, n_attn) };
                    let k_cache = unsafe { std::slice::from_raw_parts(k_cache_ptr, kv_cache_size) };
                    let v_cache = unsafe { std::slice::from_raw_parts(v_cache_ptr, kv_cache_size) };
                    let scores = unsafe {
                        std::slice::from_raw_parts_mut(
                            scores_ptr.add(thread * score_stride),
                            score_stride,
                        )
                    };
                    let f16_scratch = scores.as_mut_ptr().cast::<u16>();
                    let head_start = thread * config.n_head / threads;
                    let head_end = (thread + 1) * config.n_head / threads;
                    let layer_base = layer * capacity * kv_stride;
                    let n_padded = (step + 1).div_ceil(256) * 256;
                    for head in head_start..head_end {
                        let kv_head = head / group_size;
                        let q_offset = head * config.n_embd_head_k;
                        let output_offset = head * config.n_embd_head_v;
                        let output =
                            &mut attn_out[output_offset..output_offset + config.n_embd_head_v];
                        let query = unsafe {
                            std::slice::from_raw_parts_mut(
                                output.as_mut_ptr().cast::<u16>(),
                                config.n_embd_head_k,
                            )
                        };
                        f32_slice_to_f16(&q[q_offset..q_offset + config.n_embd_head_k], query);
                        scores[..n_padded].fill(f32::NEG_INFINITY);
                        for token in 0..=step {
                            let row = layer_base + token * kv_stride;
                            let key_offset = row + kv_head * config.n_embd_head_k;
                            scores[token] = dot_f16(
                                query,
                                &k_cache[key_offset..key_offset + config.n_embd_head_k],
                                config.n_embd_head_k,
                            ) * kq_scale;
                        }
                        softmax(&mut scores[..n_padded]);
                        for index in 0..n_padded {
                            unsafe { *f16_scratch.add(index) = f32_to_f16(scores[index]) };
                        }
                        let weights = unsafe { std::slice::from_raw_parts(f16_scratch, n_padded) };
                        let values = unsafe {
                            std::slice::from_raw_parts_mut(f16_scratch.add(score_stride), n_padded)
                        };
                        values[step + 1..].fill(0);
                        for dimension in 0..config.n_embd_head_v {
                            for token in 0..=step {
                                let row = layer_base + token * kv_stride;
                                values[token] =
                                    v_cache[row + kv_head * config.n_embd_head_v + dimension];
                            }
                            output[dimension] = dot_f16(values, weights, n_padded);
                        }
                    }
                });

                let attn_out = unsafe { std::slice::from_raw_parts_mut(attn_out_ptr, n_attn) };
                #[cfg(feature = "parity-trace")]
                if layer == 0 {
                    parity_trace::report(parity_trace::checkpoint(
                        "kqv_out-0",
                        Some(0),
                        &[config.n_head, config.n_embd_head_v],
                        attn_out,
                    ));
                }
                let q8_buf = unsafe { std::slice::from_raw_parts_mut(q8_buf_ptr, max_n_in) };
                let scale_buf =
                    unsafe { std::slice::from_raw_parts_mut(scale_buf_ptr, max_n_in / 32) };
                quantize_q8_0_into(
                    attn_out,
                    n_attn,
                    &mut q8_buf[..n_attn],
                    &mut scale_buf[..n_attn / 32],
                );
                let q8 = q8_buf[..n_attn].as_ptr();
                let scales = scale_buf[..n_attn / 32].as_ptr();
                let pool = Arc::clone(&model.pool);
                pool.compute(move |thread, threads| {
                    let q8 = unsafe { std::slice::from_raw_parts(q8, n_attn) };
                    let scales = unsafe { std::slice::from_raw_parts(scales, n_attn / 32) };
                    let output =
                        unsafe { std::slice::from_raw_parts_mut(attn_proj_ptr, config.n_embd) };
                    matmul_q8_0_quantized_parallel_rows(
                        weights.wo,
                        q8,
                        scales,
                        output,
                        n_attn,
                        config.n_embd,
                        thread,
                        threads,
                    );
                });

                let x = unsafe { std::slice::from_raw_parts_mut(x_ptr, config.n_embd) };
                let normed = unsafe { std::slice::from_raw_parts_mut(normed_ptr, config.n_embd) };
                let attn_projection =
                    unsafe { std::slice::from_raw_parts_mut(attn_proj_ptr, config.n_embd) };
                for (hidden, projection) in x.iter_mut().zip(attn_projection) {
                    *hidden += *projection;
                }
                rms_norm(x, &weights.ffn_norm, normed, config.eps);
                quantize_q8_0_into(
                    normed,
                    config.n_embd,
                    &mut q8_buf[..config.n_embd],
                    &mut scale_buf[..config.n_embd / 32],
                );
                let q8 = q8_buf[..config.n_embd].as_ptr();
                let scales = scale_buf[..config.n_embd / 32].as_ptr();
                let pool = Arc::clone(&model.pool);
                pool.compute(move |thread, threads| {
                    let q8 = unsafe { std::slice::from_raw_parts(q8, config.n_embd) };
                    let scales = unsafe { std::slice::from_raw_parts(scales, config.n_embd / 32) };
                    let gate = unsafe { std::slice::from_raw_parts_mut(gate_buf_ptr, config.n_ff) };
                    let up = unsafe { std::slice::from_raw_parts_mut(up_buf_ptr, config.n_ff) };
                    matmul_q8_0_quantized_parallel_rows(
                        weights.w_gate,
                        q8,
                        scales,
                        up,
                        config.n_embd,
                        config.n_ff,
                        thread,
                        threads,
                    );
                    matmul_q8_0_quantized_parallel_rows(
                        weights.w_up,
                        q8,
                        scales,
                        gate,
                        config.n_embd,
                        config.n_ff,
                        thread,
                        threads,
                    );
                    let start = thread * config.n_ff / threads;
                    let end = (thread + 1) * config.n_ff / threads;
                    silu_mul_inplace(&up[start..end], &mut gate[start..end]);
                });

                let gate = unsafe { std::slice::from_raw_parts_mut(gate_buf_ptr, config.n_ff) };
                quantize_q8_0_into(
                    gate,
                    config.n_ff,
                    &mut q8_buf[..config.n_ff],
                    &mut scale_buf[..config.n_ff / 32],
                );
                let q8 = q8_buf[..config.n_ff].as_ptr();
                let scales = scale_buf[..config.n_ff / 32].as_ptr();
                let pool = Arc::clone(&model.pool);
                pool.compute(move |thread, threads| {
                    let q8 = unsafe { std::slice::from_raw_parts(q8, config.n_ff) };
                    let scales = unsafe { std::slice::from_raw_parts(scales, config.n_ff / 32) };
                    let down =
                        unsafe { std::slice::from_raw_parts_mut(down_buf_ptr, config.n_embd) };
                    matmul_q8_0_quantized_parallel_rows(
                        weights.w_down,
                        q8,
                        scales,
                        down,
                        config.n_ff,
                        config.n_embd,
                        thread,
                        threads,
                    );
                });

                let down = unsafe { std::slice::from_raw_parts_mut(down_buf_ptr, config.n_embd) };
                #[cfg(feature = "parity-trace")]
                if layer == 0 {
                    parity_trace::report(parity_trace::checkpoint(
                        "ffn_out-0",
                        Some(0),
                        &[1, config.n_embd],
                        down,
                    ));
                }
                let x = unsafe { std::slice::from_raw_parts_mut(x_ptr, config.n_embd) };
                for (hidden, projection) in x.iter_mut().zip(down) {
                    *hidden += *projection;
                }
            }

            rms_norm(
                &self.scratch.x,
                &model.output_norm,
                &mut self.scratch.normed,
                config.eps,
            );
            #[cfg(feature = "parity-trace")]
            parity_trace::report(parity_trace::checkpoint(
                "result_norm",
                None,
                &[1, config.n_embd],
                &self.scratch.normed,
            ));
            quantize_q8_0_into(
                &self.scratch.normed,
                config.n_embd,
                &mut self.scratch.q8_buf[..config.n_embd],
                &mut self.scratch.scale_buf[..config.n_embd / 32],
            );
            let q8 = self.scratch.q8_buf[..config.n_embd].as_ptr();
            let scales = self.scratch.scale_buf[..config.n_embd / 32].as_ptr();
            let logits_ptr = self.scratch.logits.as_mut_ptr();
            let pool = Arc::clone(&model.pool);
            pool.compute(move |thread, threads| {
                let q8 = unsafe { std::slice::from_raw_parts(q8, config.n_embd) };
                let scales = unsafe { std::slice::from_raw_parts(scales, config.n_embd / 32) };
                let logits = unsafe { std::slice::from_raw_parts_mut(logits_ptr, config.vocab) };
                matmul_q8_0_quantized_parallel_rows(
                    model.output,
                    q8,
                    scales,
                    logits,
                    config.n_embd,
                    config.vocab,
                    thread,
                    threads,
                );
            });
            #[cfg(feature = "parity-trace")]
            parity_trace::report(parity_trace::checkpoint(
                "result_output",
                None,
                &[config.vocab],
                &self.scratch.logits,
            ));

            #[cfg(feature = "parity-trace")]
            if asr_trace && step == n_prompt - 1 {
                parity_trace::report(parity_trace::checkpoint(
                    "asr.decoder_first_logits",
                    None,
                    &[config.vocab],
                    &self.scratch.logits,
                ));
            }

            if step < n_prompt - 1 {
                continue;
            }
            let token_id = sample_token(&self.scratch.logits, options.temperature)?;
            if model.tokenizer.eos_id() == Some(token_id)
                || model.tokenizer.special_token_id("im_end") == Some(token_id)
            {
                break;
            }
            if generated_tokens.len() >= options.max_new_tokens {
                break;
            }
            let text = decoder.push(token_id);
            if !text.is_empty() {
                rendered_tokens.push(text);
            }
            generated_tokens.push(token_id);
        }

        #[cfg(feature = "parity-trace")]
        parity_trace::report(parity_trace::token_ids(
            if asr_trace {
                "asr.generated_ids"
            } else {
                "generated_ids"
            },
            &generated_tokens,
        ));
        let tail = decoder.finish();
        if !tail.is_empty() {
            rendered_tokens.push(tail);
        }
        Ok(Qwen3Generation {
            text: rendered_tokens.concat(),
            rendered_tokens,
            token_ids: generated_tokens,
            prompt_tokens: n_prompt,
        })
    }
}

#[cfg(test)]
struct TestTensorSource;

#[cfg(test)]
impl TensorSource for TestTensorSource {
    fn metadata(&self, _key: &str) -> Option<&MetaValue> {
        None
    }

    fn tensor_info(&self, _name: &str) -> Option<&TensorInfo> {
        None
    }

    fn tensor_slice(&self, _name: &str) -> Option<&[u8]> {
        None
    }
}

#[cfg(test)]
pub(crate) fn test_model(tokenizer: Arc<BPETokenizer>, n_ctx: usize, n_embd: usize) -> Qwen3Model {
    assert!(n_embd > 0 && n_embd % 32 == 0);
    let row_bytes = n_embd / 32 * 34;
    let token_embedding = Box::leak(vec![0; tokenizer.vocab_size() * row_bytes].into_boxed_slice());
    Qwen3Model {
        source: Arc::new(TestTensorSource),
        pool: Arc::new(ComputePool::new(1)),
        config: Qwen3Config {
            architecture: "qwen3".into(),
            n_embd,
            n_layer: 0,
            n_head: 1,
            n_head_kv: 1,
            n_embd_head_k: n_embd,
            n_embd_head_v: n_embd,
            n_ff: n_embd,
            vocab: tokenizer.vocab_size(),
            n_ctx,
            eps: 1e-6,
            freq_base: 1_000_000.0,
            has_qk_norm: false,
            rope: Qwen3Rope::Neox,
        },
        tokenizer,
        layers: Vec::new(),
        output_norm: vec![1.0; n_embd],
        token_embedding,
        output: token_embedding,
    }
}

pub fn qwen_text_positions(n_tokens: usize) -> Vec<[usize; 4]> {
    (0..n_tokens).map(|position| [position; 4]).collect()
}

fn validate_generation(
    model: &Qwen3Model,
    input: &Qwen3Input<'_>,
    options: Qwen3GenerateOptions,
) -> Result<(), String> {
    if input.token_ids.is_empty() {
        return Err("Qwen3 prompt must contain at least one token".into());
    }
    if options.max_new_tokens == 0 {
        return Err("Qwen3 generation must request at least one token".into());
    }
    if !options.temperature.is_finite() || options.temperature < 0.0 {
        return Err(format!(
            "Invalid generation temperature: {}",
            options.temperature
        ));
    }
    validate_input_shapes(
        input.token_ids.len(),
        model.config.n_embd,
        input.positions.len(),
        input.embeddings.map(<[f32]>::len),
    )?;
    validate_token_ids(input.token_ids, model.config.vocab)?;
    if input
        .embeddings
        .is_some_and(|values| values.iter().any(|value| !value.is_finite()))
    {
        return Err("Input embeddings contain NaN or infinity".into());
    }
    checked_session_capacity(
        input.token_ids.len(),
        options.max_new_tokens,
        model.config.n_ctx,
    )?;
    Ok(())
}

fn validate_token_ids(token_ids: &[u32], vocab: usize) -> Result<(), String> {
    for &token_id in token_ids {
        let token =
            usize::try_from(token_id).map_err(|_| format!("Invalid token ID {token_id}"))?;
        if token >= vocab {
            return Err(format!("Token ID {token_id} exceeds vocabulary {vocab}"));
        }
    }
    Ok(())
}

fn sample_token(logits: &[f32], temperature: f32) -> Result<u32, String> {
    if temperature == 0.0 {
        return greedy_token(logits);
    }
    let mut max_logit = f32::NEG_INFINITY;
    for &logit in logits {
        if !logit.is_finite() {
            return Err("Cannot sample non-finite logits".into());
        }
        max_logit = max_logit.max(logit);
    }
    if logits.is_empty() {
        return Err("Cannot sample empty logits".into());
    }
    let sum: f32 = logits
        .iter()
        .map(|logit| ((logit - max_logit) / temperature).exp())
        .sum();
    if !sum.is_finite() || sum <= 0.0 {
        return Err("Sampling probability sum is not finite and positive".into());
    }
    let target = rand::random::<f32>() * sum;
    let mut cumulative = 0.0;
    for (index, &logit) in logits.iter().enumerate() {
        cumulative += ((logit - max_logit) / temperature).exp();
        if cumulative >= target {
            return u32::try_from(index).map_err(|_| "Token ID does not fit u32".into());
        }
    }
    u32::try_from(logits.len() - 1).map_err(|_| "Token ID does not fit u32".into())
}

fn static_q8_matrix(
    source: &Arc<dyn TensorSource>,
    name: &str,
    columns: usize,
    rows: usize,
) -> Result<&'static [u8], String> {
    static_q8_tensor(
        source,
        name,
        &[
            usize_to_u64(columns, "matrix columns")?,
            usize_to_u64(rows, "matrix rows")?,
        ],
    )
}

fn static_q8_tensor(
    source: &Arc<dyn TensorSource>,
    name: &str,
    dims: &[u64],
) -> Result<&'static [u8], String> {
    let bytes = checked_tensor(source.as_ref(), name, dims, GGMLType::Q8_0)?;
    // SAFETY: Qwen3Model stores a strong Arc to this immutable TensorSource and never exposes
    // unloading. Every lifetime-extended weight slice is therefore valid until the model drops.
    Ok(unsafe { std::mem::transmute::<&[u8], &'static [u8]>(bytes) })
}

fn load_f32_tensor(
    source: &dyn TensorSource,
    name: &str,
    dims: &[u64],
) -> Result<Vec<f32>, String> {
    let bytes = checked_tensor(source, name, dims, GGMLType::F32)?;
    let len = bytes.len() / std::mem::size_of::<f32>();
    check_allocation(name, len, std::mem::size_of::<f32>())?;
    Ok(bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().expect("four-byte chunk")))
        .collect())
}

fn checked_tensor<'a>(
    source: &'a dyn TensorSource,
    name: &str,
    dims: &[u64],
    ggml_type: GGMLType,
) -> Result<&'a [u8], String> {
    let info: &TensorInfo = source
        .tensor_info(name)
        .ok_or_else(|| format!("Missing tensor: {name}"))?;
    if info.dims != dims || info.ggml_type != ggml_type {
        return Err(format!(
            "Invalid tensor {name}: shape {:?} type {:?}; expected {:?} {:?}",
            info.dims, info.ggml_type, dims, ggml_type
        ));
    }
    let expected = usize::try_from(
        info.checked_nbytes()
            .ok_or_else(|| format!("Invalid tensor byte size: {name}"))?,
    )
    .map_err(|_| format!("Tensor byte size does not fit usize: {name}"))?;
    let bytes = source
        .tensor_slice(name)
        .ok_or_else(|| format!("Missing tensor data: {name}"))?;
    if bytes.len() != expected {
        return Err(format!(
            "Invalid tensor data length for {name}: {}; expected {expected}",
            bytes.len()
        ));
    }
    Ok(bytes)
}

fn checked_product(name: &str, left: usize, right: usize) -> Result<usize, String> {
    left.checked_mul(right)
        .ok_or_else(|| format!("{name} overflows usize"))
}

fn check_allocation(name: &str, len: usize, element_bytes: usize) -> Result<(), String> {
    let bytes = checked_product(name, len, element_bytes)?;
    if bytes > isize::MAX as usize {
        return Err(format!("{name} allocation is too large"));
    }
    Ok(())
}

fn usize_to_u64(value: usize, name: &str) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| format!("{name} does not fit u64"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MetaValue, MetaValueType, TensorInfo, TensorSource};
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

    fn qwen3vl_metadata_source() -> MapTensorSource {
        MapTensorSource {
            metadata: HashMap::from([
                (
                    "general.architecture".into(),
                    MetaValue::String("qwen3vl".into()),
                ),
                ("qwen3vl.embedding_length".into(), MetaValue::Uint32(1024)),
                ("qwen3vl.block_count".into(), MetaValue::Uint32(28)),
                ("qwen3vl.attention.head_count".into(), MetaValue::Uint32(16)),
                (
                    "qwen3vl.attention.head_count_kv".into(),
                    MetaValue::Uint32(8),
                ),
                (
                    "qwen3vl.attention.key_length".into(),
                    MetaValue::Uint32(128),
                ),
                (
                    "qwen3vl.attention.value_length".into(),
                    MetaValue::Uint32(128),
                ),
                (
                    "qwen3vl.feed_forward_length".into(),
                    MetaValue::Uint32(3072),
                ),
                ("qwen3vl.context_length".into(), MetaValue::Uint32(65_536)),
                (
                    "qwen3vl.rope.freq_base".into(),
                    MetaValue::Float32(1_000_000.0),
                ),
                (
                    "qwen3vl.rope.dimension_sections".into(),
                    MetaValue::Array(
                        MetaValueType::Int32,
                        [24, 20, 20, 0].map(MetaValue::Int32).to_vec(),
                    ),
                ),
                (
                    "qwen3vl.attention.layer_norm_rms_epsilon".into(),
                    MetaValue::Float32(1e-6),
                ),
                ("qwen3vl.vocab_size".into(), MetaValue::Uint32(151_936)),
            ]),
            tensors: HashMap::new(),
        }
    }

    #[test]
    fn qwen3vl_requires_qk_norm_and_fixed_imrope_sections() {
        let config = Qwen3Config::from_source(&qwen3vl_metadata_source(), 151_936).unwrap();
        assert!(config.has_qk_norm);
        assert_eq!(
            config.rope,
            Qwen3Rope::Interleaved {
                sections: [24, 20, 20, 0],
                n_dims: 128,
            }
        );
    }

    #[test]
    fn session_capacity_is_prompt_plus_generation_not_model_context() {
        assert_eq!(checked_session_capacity(23, 17, 65_536).unwrap(), 40);
        assert!(checked_session_capacity(65_500, 37, 65_536).is_err());
        assert!(checked_session_capacity(usize::MAX, 1, 65_536).is_err());
    }

    #[test]
    fn decoder_does_not_evaluate_the_last_generated_token() {
        assert_eq!(checked_decoder_steps(23, 17, 65_536).unwrap(), 39);
    }

    #[test]
    fn generated_positions_continue_from_prompt_text_positions() {
        let prompt = [[7, 8, 9, 10], [42, 100, 200, 300]];
        assert_eq!(
            checked_generated_position(&prompt, 0).unwrap(),
            [43, 43, 43, 0]
        );
        assert_eq!(
            checked_generated_position(&prompt, 1).unwrap(),
            [44, 44, 44, 0]
        );
        assert!(checked_generated_position(&[[usize::MAX; 4]], 0).is_err());
    }

    #[test]
    fn decoder_input_rejects_position_and_embedding_shape_mismatch() {
        assert!(validate_input_shapes(3, 1024, 2, None).is_err());
        assert!(validate_input_shapes(3, 1024, 3, Some(3 * 1024 - 1)).is_err());
    }

    #[test]
    fn greedy_ties_choose_the_lowest_token_id() {
        assert_eq!(greedy_token(&[1.0, 2.0, 2.0]).unwrap(), 1);
        assert!(greedy_token(&[1.0, f32::NAN]).is_err());
    }
}
