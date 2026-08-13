#![allow(dead_code)]

use std::ops::Range;

#[derive(Clone)]
pub struct Q8Block {
    pub scale: f32,
    pub qs: [i8; 32],
}

pub struct Q8Fixture {
    pub batch: usize,
    pub n_in: usize,
    pub n_out: usize,
    pub rows: Range<u32>,
    pub weight_blocks: Vec<Q8Block>,
    pub weight_bytes: Vec<u8>,
    pub input: Vec<f32>,
    pub expected: Vec<f32>,
}

pub fn q8_fixture(batch: usize, n_in: usize, n_out: usize, rows: Range<u32>) -> Q8Fixture {
    assert_eq!(n_in % 32, 0);
    assert!(rows.start < rows.end && rows.end as usize <= n_out);

    let blocks_per_row = n_in / 32;
    let mut weight_blocks = Vec::with_capacity(n_out * blocks_per_row);
    let mut weight_bytes = Vec::with_capacity(n_out * blocks_per_row * 34);
    for row in 0..n_out {
        for block in 0..blocks_per_row {
            let scale = 0.01 * (1 + (row + block) % 7) as f32;
            let qs =
                std::array::from_fn(|lane| ((row * 3 + block * 5 + lane) as i32 % 31 - 15) as i8);
            weight_bytes.extend_from_slice(&half::f16::from_f32(scale).to_bits().to_le_bytes());
            weight_bytes.extend(qs.iter().map(|value| *value as u8));
            weight_blocks.push(Q8Block {
                scale: half::f16::from_f32(scale).to_f32(),
                qs,
            });
        }
    }

    let input = (0..batch * n_in)
        .map(|index| {
            let lane = index % 32;
            if lane == 0 {
                127.0
            } else {
                (index as i32 % 31 - 15) as f32
            }
        })
        .collect::<Vec<_>>();
    let mut expected = vec![0.0; batch * rows.len()];
    for item in 0..batch {
        for (local_row, row) in rows.clone().enumerate() {
            let mut sum = 0.0;
            for block in 0..blocks_per_row {
                let weight = &weight_blocks[row as usize * blocks_per_row + block];
                for lane in 0..32 {
                    sum += weight.scale
                        * weight.qs[lane] as f32
                        * input[item * n_in + block * 32 + lane];
                }
            }
            expected[item * rows.len() + local_row] = sum;
        }
    }

    Q8Fixture {
        batch,
        n_in,
        n_out,
        rows,
        weight_blocks,
        weight_bytes,
        input,
        expected,
    }
}

pub fn assert_close(actual: &[f32], expected: &[f32], atol: f32, rtol: f32) {
    assert_eq!(actual.len(), expected.len());
    for (index, (&actual, &expected)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (actual - expected).abs() <= atol + rtol * expected.abs(),
            "mismatch at {index}: actual={actual}, expected={expected}"
        );
    }
}

use rust_model_inference::{
    parse_placement, BackendError, BackendKind, CompiledModel, ComponentId, DeviceCapabilities,
    DeviceDescriptor, DeviceDiscovery, DeviceId, DevicePlan, DeviceProvider, DeviceRegistry,
    DeviceSession, FenceId, GGMLType, LayerOp, LifecycleProbe, PlacementCompiler, ProgramId,
    ProgramKind, RunParams, SessionStats, SlotId, SourceFormat, SourceTensorRecord, TensorCatalog,
    TensorId, TensorInfo, TensorSource,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

pub struct PlacementFixture {
    catalog: Arc<TensorCatalog>,
    token_embedding: TensorId,
    output: TensorId,
    model: PlacementModel,
}

pub struct Qwen3LayerRun {
    pub logits: Vec<f32>,
    pub first_token_logits: Vec<f32>,
    pub second_token_logits: Vec<f32>,
    pub tokens: Vec<usize>,
    pub same_device_internal_host_waits: u64,
    pub kv_transfer_bytes: u64,
}

pub struct Qwen35RecurrentRun {
    pub first_token_logits: Vec<f32>,
    pub second_token_logits: Vec<f32>,
    pub first_after_reset_logits: Vec<f32>,
    pub tokens: Vec<usize>,
    pub recurrent_transfer_bytes: u64,
    pub conv_state_bytes: u64,
    pub ssm_state_bytes: u64,
}

enum PlacementModel {
    Qwen3(rust_model_inference::Qwen3Model),
    Qwen35(rust_model_inference::Qwen35Model),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlacementTrace {
    pub q8_matrix_tensor_ids: BTreeSet<TensorId>,
    pub embedding_tensor_ids: BTreeSet<TensorId>,
    pub program_kinds: Vec<ProgramKind>,
    pub layer_ops: Vec<LayerOp>,
    pub slot_byte_lengths: BTreeMap<SlotId, u64>,
}

impl PlacementFixture {
    pub fn catalog(&self) -> &TensorCatalog {
        &self.catalog
    }

    pub fn token_embedding_id(&self) -> TensorId {
        self.token_embedding
    }

    pub fn output_id(&self) -> TensorId {
        self.output
    }

    pub fn requirements(&self) -> rust_model_inference::ComponentRequirements {
        match &self.model {
            PlacementModel::Qwen3(model) => model.requirements(),
            PlacementModel::Qwen35(model) => model.requirements(),
        }
    }

    pub fn run_recording_forward_two_tokens(&self) -> Result<PlacementTrace, String> {
        self.run_recording_forward("llm:row=cpu0@1", &[1, 2], &[[0, 0, 0, 0], [1, 1, 1, 0]])
            .map(|(_, trace)| trace)
    }

    pub fn run_recording_forward(
        &self,
        placement: &str,
        tokens: &[u32],
        positions: &[[u32; 4]],
    ) -> Result<(Result<(), String>, PlacementTrace), String> {
        let requirements = match &self.model {
            PlacementModel::Qwen3(model) => model.requirements(),
            PlacementModel::Qwen35(model) => model.requirements(),
        };
        let trace = Arc::new(Mutex::new(PlacementTrace::default()));
        let mut registry = DeviceRegistry::new();
        registry
            .register_provider(Arc::new(RecordingProvider {
                descriptor: recording_descriptor(),
                trace: Arc::clone(&trace),
            }))
            .map_err(|error| error.to_string())?;
        registry
            .discover(&BTreeSet::from([BackendKind::Cpu]))
            .map_err(|error| error.to_string())?;
        let registry = Arc::new(registry);
        let plan = PlacementCompiler {
            catalog: &self.catalog,
            registry: &registry,
            requirements: std::slice::from_ref(&requirements),
        }
        .compile(&BTreeMap::from([(
            ComponentId::Llm,
            parse_placement(placement).map_err(|error| error.to_string())?,
        )]))
        .map_err(|error| error.to_string())?;
        let model = CompiledModel::compile(Arc::clone(&self.catalog), plan, registry)
            .map_err(|error| error.to_string())?;
        let mut run = model.start_run().map_err(|error| error.to_string())?;
        let mut logits = vec![0.0; 64];
        let result: Result<(), String> = match &self.model {
            PlacementModel::Qwen3(model) => model.forward(&mut run, tokens, positions, &mut logits),
            PlacementModel::Qwen35(model) => {
                model.forward_compiled(&mut run, tokens, positions, &mut logits)
            }
        };
        drop(run);
        let trace = trace
            .lock()
            .map_err(|_| "recording trace poisoned".to_string())
            .map(|trace| trace.clone())?;
        Ok((result, trace))
    }

    pub fn run_cpu_forward_two_tokens(&self) -> Result<Vec<f32>, String> {
        let requirements = self.requirements();
        let mut registry = DeviceRegistry::new();
        registry
            .register_provider(Arc::new(rust_model_inference::compute::CpuProvider::new(1)))
            .map_err(|error| error.to_string())?;
        registry
            .discover(&BTreeSet::from([BackendKind::Cpu]))
            .map_err(|error| error.to_string())?;
        let registry = Arc::new(registry);
        let plan = PlacementCompiler {
            catalog: &self.catalog,
            registry: &registry,
            requirements: std::slice::from_ref(&requirements),
        }
        .compile(&BTreeMap::from([(
            ComponentId::Llm,
            parse_placement("llm:row=cpu0@1").map_err(|error| error.to_string())?,
        )]))
        .map_err(|error| error.to_string())?;
        let model = CompiledModel::compile(Arc::clone(&self.catalog), plan, registry)
            .map_err(|error| error.to_string())?;
        let mut run = model.start_run().map_err(|error| error.to_string())?;
        let tokens = [1, 2];
        let positions = [[0, 0, 0, 0], [1, 1, 1, 0]];
        let mut logits = vec![0.0; 64];
        let result: Result<(), String> = match &self.model {
            PlacementModel::Qwen3(model) => {
                for (&token, &position) in tokens.iter().zip(&positions) {
                    model.forward(&mut run, &[token], &[position], &mut logits)?;
                }
                Ok(())
            }
            PlacementModel::Qwen35(model) => {
                for (&token, &position) in tokens.iter().zip(&positions) {
                    model.forward_compiled(&mut run, &[token], &[position], &mut logits)?;
                }
                Ok(())
            }
        };
        result?;
        Ok(logits)
    }

    pub fn run_cpu_forward_two_tokens_in_one_call(&self) -> Result<Vec<f32>, String> {
        let requirements = self.requirements();
        let mut registry = DeviceRegistry::new();
        registry
            .register_provider(Arc::new(rust_model_inference::compute::CpuProvider::new(1)))
            .map_err(|error| error.to_string())?;
        registry
            .discover(&BTreeSet::from([BackendKind::Cpu]))
            .map_err(|error| error.to_string())?;
        let registry = Arc::new(registry);
        let plan = PlacementCompiler {
            catalog: &self.catalog,
            registry: &registry,
            requirements: std::slice::from_ref(&requirements),
        }
        .compile(&BTreeMap::from([(
            ComponentId::Llm,
            parse_placement("llm:row=cpu0@1").map_err(|error| error.to_string())?,
        )]))
        .map_err(|error| error.to_string())?;
        let model = CompiledModel::compile(Arc::clone(&self.catalog), plan, registry)
            .map_err(|error| error.to_string())?;
        let mut run = model.start_run().map_err(|error| error.to_string())?;
        let tokens = [1, 2];
        let positions = [[0, 0, 0, 0], [1, 1, 1, 0]];
        let mut logits = vec![0.0; 64];
        match &self.model {
            PlacementModel::Qwen3(model) => {
                model.forward(&mut run, &tokens, &positions, &mut logits)?
            }
            PlacementModel::Qwen35(model) => {
                model.forward_compiled(&mut run, &tokens, &positions, &mut logits)?
            }
        }
        Ok(logits)
    }

    pub fn cpu_reference_two_tokens(&self) -> Result<Qwen3LayerRun, String> {
        self.run_layer_two_tokens(BackendKind::Cpu, "row", [1, 2])
    }

    pub fn cpu_reference_recurrent_two_tokens(&self) -> Result<Qwen3LayerRun, String> {
        self.run_layer_two_tokens(BackendKind::Cpu, "row", [1, 1])
    }

    pub fn compiled_cpu_two_tokens(&self) -> Result<Qwen3LayerRun, String> {
        self.run_layer_two_tokens(BackendKind::Cpu, "layer", [1, 2])
    }

    pub fn compiled_backend_two_tokens(
        &self,
        backend: BackendKind,
    ) -> Result<Qwen3LayerRun, String> {
        self.run_layer_two_tokens(backend, "layer", [1, 2])
    }

    pub fn compiled_backend_two_token_batch(
        &self,
        backend: BackendKind,
    ) -> Result<Vec<f32>, String> {
        let PlacementModel::Qwen35(model) = &self.model else {
            return Err("fixture is not Qwen3.5".into());
        };
        let mut registry = DeviceRegistry::new();
        let requested = BTreeSet::from([backend]);
        rust_model_inference::compute::register_requested_providers(&mut registry, &requested, 1)
            .map_err(|error| error.to_string())?;
        registry
            .discover(&requested)
            .map_err(|error| error.to_string())?;
        let device = match backend {
            BackendKind::Cpu => "cpu0",
            BackendKind::Vulkan => "vulkan0",
            BackendKind::Metal => "metal0",
            BackendKind::Npu => return Err("NPU backend is not implemented".into()),
        };
        let registry = Arc::new(registry);
        let plan = PlacementCompiler {
            catalog: &self.catalog,
            registry: &registry,
            requirements: std::slice::from_ref(&self.requirements()),
        }
        .compile(&BTreeMap::from([(
            ComponentId::Llm,
            parse_placement(&format!("llm:layer={device}@1")).map_err(|error| error.to_string())?,
        )]))
        .map_err(|error| error.to_string())?;
        let compiled = CompiledModel::compile(Arc::clone(&self.catalog), plan, registry)
            .map_err(|error| format!("compiled batch open: {error}"))?;
        let mut run = compiled.start_run().map_err(|error| error.to_string())?;
        run.batch_capacity(ComponentId::Llm)
            .map_err(|error| format!("compiled batch capacity: {error}"))?;
        let mut logits = vec![0.0; 64];
        model
            .forward_compiled(
                &mut run,
                &[1, 2],
                &[[0, 0, 0, 0], [1, 2, 3, 4]],
                &mut logits,
            )
            .map_err(|error| format!("compiled batch forward: {error}"))?;
        Ok(logits)
    }

    pub fn compiled_backend_recurrent_two_tokens(
        &self,
        backend: BackendKind,
    ) -> Result<Qwen35RecurrentRun, String> {
        let PlacementModel::Qwen35(model) = &self.model else {
            return Err("fixture is not Qwen3.5".into());
        };
        let mut registry = DeviceRegistry::new();
        let requested = BTreeSet::from([backend]);
        rust_model_inference::compute::register_requested_providers(&mut registry, &requested, 1)
            .map_err(|error| error.to_string())?;
        registry
            .discover(&requested)
            .map_err(|error| error.to_string())?;
        let device = match backend {
            BackendKind::Cpu => "cpu0",
            BackendKind::Vulkan => "vulkan0",
            BackendKind::Metal => "metal0",
            BackendKind::Npu => return Err("NPU backend is not implemented".into()),
        };
        let registry = Arc::new(registry);
        let plan = PlacementCompiler {
            catalog: &self.catalog,
            registry: &registry,
            requirements: std::slice::from_ref(&self.requirements()),
        }
        .compile(&BTreeMap::from([(
            ComponentId::Llm,
            parse_placement(&format!("llm:layer={device}@1")).map_err(|error| error.to_string())?,
        )]))
        .map_err(|error| error.to_string())?;
        let recurrent_transfer_bytes = plan.components[&ComponentId::Llm]
            .activation_transfers
            .iter()
            .map(|transfer| u64::from(transfer.f32_values_per_token) * 4)
            .sum();
        let (conv_state_bytes, ssm_state_bytes) = plan
            .devices
            .values()
            .flat_map(|plan| &plan.slots)
            .fold((0, 0), |(conv, ssm), slot| match slot.kind {
                rust_model_inference::SlotKind::ConvState => (conv + slot.byte_len, ssm),
                rust_model_inference::SlotKind::SsmState => (conv, ssm + slot.byte_len),
                _ => (conv, ssm),
            });
        let compiled = CompiledModel::compile(Arc::clone(&self.catalog), plan, registry)
            .map_err(|error| error.to_string())?;
        let mut run = compiled.start_run().map_err(|error| error.to_string())?;
        let mut first_token_logits = vec![0.0; 64];
        model.forward_compiled(&mut run, &[1], &[[0, 0, 0, 0]], &mut first_token_logits)?;
        let mut second_token_logits = vec![0.0; 64];
        model.forward_compiled(&mut run, &[1], &[[1, 2, 3, 4]], &mut second_token_logits)?;
        run.reset_state().map_err(|error| error.to_string())?;
        let mut first_after_reset_logits = vec![0.0; 64];
        model.forward_compiled(
            &mut run,
            &[1],
            &[[0, 0, 0, 0]],
            &mut first_after_reset_logits,
        )?;
        let tokens = [&first_token_logits, &second_token_logits]
            .into_iter()
            .map(|logits| {
                logits
                    .iter()
                    .enumerate()
                    .max_by(|(_, left), (_, right)| left.total_cmp(right))
                    .map(|(index, _)| index)
                    .unwrap()
            })
            .collect();
        Ok(Qwen35RecurrentRun {
            first_token_logits,
            second_token_logits,
            first_after_reset_logits,
            tokens,
            recurrent_transfer_bytes,
            conv_state_bytes,
            ssm_state_bytes,
        })
    }

    fn run_layer_two_tokens(
        &self,
        backend: BackendKind,
        mode: &str,
        tokens: [u32; 2],
    ) -> Result<Qwen3LayerRun, String> {
        let mut registry = DeviceRegistry::new();
        let requested = BTreeSet::from([backend]);
        rust_model_inference::compute::register_requested_providers(&mut registry, &requested, 1)
            .map_err(|error| error.to_string())?;
        registry
            .discover(&requested)
            .map_err(|error| error.to_string())?;
        let device = match backend {
            BackendKind::Cpu => "cpu0",
            BackendKind::Vulkan => "vulkan0",
            BackendKind::Metal => "metal0",
            BackendKind::Npu => return Err("NPU backend is not implemented".into()),
        };
        let registry = Arc::new(registry);
        let plan = PlacementCompiler {
            catalog: &self.catalog,
            registry: &registry,
            requirements: std::slice::from_ref(&self.requirements()),
        }
        .compile(&BTreeMap::from([(
            ComponentId::Llm,
            parse_placement(&format!("llm:{mode}={device}@1"))
                .map_err(|error| error.to_string())?,
        )]))
        .map_err(|error| error.to_string())?;
        let kv_transfer_bytes = plan.components[&ComponentId::Llm]
            .activation_transfers
            .iter()
            .map(|transfer| u64::from(transfer.f32_values_per_token) * 4)
            .sum();
        let compiled = CompiledModel::compile(Arc::clone(&self.catalog), plan, registry)
            .map_err(|error| error.to_string())?;
        let mut run = compiled.start_run().map_err(|error| error.to_string())?;
        let second_position = match &self.model {
            PlacementModel::Qwen3(_) => [1, 1, 1, 0],
            PlacementModel::Qwen35(_) => [1, 2, 3, 4],
        };
        let mut first_token_logits = vec![0.0; 64];
        match &self.model {
            PlacementModel::Qwen3(model) => model.forward(
                &mut run,
                &tokens[..1],
                &[[0, 0, 0, 0]],
                &mut first_token_logits,
            )?,
            PlacementModel::Qwen35(model) => model.forward_compiled(
                &mut run,
                &tokens[..1],
                &[[0, 0, 0, 0]],
                &mut first_token_logits,
            )?,
        }
        let mut second_token_logits = vec![0.0; 64];
        match &self.model {
            PlacementModel::Qwen3(model) => model.forward(
                &mut run,
                &tokens[1..],
                &[second_position],
                &mut second_token_logits,
            )?,
            PlacementModel::Qwen35(model) => model.forward_compiled(
                &mut run,
                &tokens[1..],
                &[second_position],
                &mut second_token_logits,
            )?,
        }
        let tokens = [&first_token_logits, &second_token_logits]
            .into_iter()
            .map(|logits| {
                logits
                    .iter()
                    .enumerate()
                    .max_by(|(_, left), (_, right)| left.total_cmp(right))
                    .map(|(index, _)| index)
                    .unwrap()
            })
            .collect();
        let host_waits: u64 = run.stats().values().map(|stats| stats.host_waits).sum();
        Ok(Qwen3LayerRun {
            logits: second_token_logits.clone(),
            first_token_logits,
            second_token_logits,
            tokens,
            same_device_internal_host_waits: host_waits.saturating_sub(2),
            kv_transfer_bytes,
        })
    }

    pub fn run_cpu_qwen3_embedding(
        &self,
        tokens: &[u32],
        config: rust_model_inference::Qwen3EmbeddingConfig,
    ) -> Result<Vec<f32>, String> {
        let PlacementModel::Qwen3(model) = &self.model else {
            return Err("fixture is not Qwen3".into());
        };
        let compiled = self.compile_with(
            "llm:row=cpu0@1",
            Arc::new(rust_model_inference::compute::CpuProvider::new(1)),
        )?;
        let mut run = compiled.start_run().map_err(|error| error.to_string())?;
        model.embed(&mut run, tokens, config)
    }

    pub fn run_recording_qwen3_embedding(
        &self,
        placement: &str,
        tokens: &[u32],
        config: rust_model_inference::Qwen3EmbeddingConfig,
    ) -> Result<(Result<Vec<f32>, String>, PlacementTrace), String> {
        let PlacementModel::Qwen3(model) = &self.model else {
            return Err("fixture is not Qwen3".into());
        };
        let trace = Arc::new(Mutex::new(PlacementTrace::default()));
        let compiled = self.compile_with(
            placement,
            Arc::new(RecordingProvider {
                descriptor: recording_descriptor(),
                trace: Arc::clone(&trace),
            }),
        )?;
        let mut run = compiled.start_run().map_err(|error| error.to_string())?;
        let result = model.embed(&mut run, tokens, config);
        drop(run);
        let trace = trace
            .lock()
            .map_err(|_| "recording trace poisoned".to_string())?
            .clone();
        Ok((result, trace))
    }

    fn compile_with(
        &self,
        placement: &str,
        provider: Arc<dyn DeviceProvider>,
    ) -> Result<CompiledModel, String> {
        let mut registry = DeviceRegistry::new();
        registry
            .register_provider(provider)
            .map_err(|error| error.to_string())?;
        registry
            .discover(&BTreeSet::from([BackendKind::Cpu]))
            .map_err(|error| error.to_string())?;
        let registry = Arc::new(registry);
        let plan = PlacementCompiler {
            catalog: &self.catalog,
            registry: &registry,
            requirements: std::slice::from_ref(&self.requirements()),
        }
        .compile(&BTreeMap::from([(
            ComponentId::Llm,
            parse_placement(placement).map_err(|error| error.to_string())?,
        )]))
        .map_err(|error| error.to_string())?;
        CompiledModel::compile(Arc::clone(&self.catalog), plan, registry)
            .map_err(|error| error.to_string())
    }
}

pub fn tiny_qwen3() -> PlacementFixture {
    #[rustfmt::skip]
    let catalog = fixture_catalog_with_overrides(
        qwen3_tensors(),
        model_metadata("qwen3", 17, 2, 4, 2, 96, 64),
        BTreeMap::from([
            ("token_embd.weight".into(), q8_0_matrix_bytes(64, 64, |row, col| i8::from((row, col) == (1, 0) || (row, col) == (2, 1)))),
            ("blk.0.attn_q.weight".into(), q8_0_matrix_bytes(64, 64, |_, _| 0)),
            ("blk.0.attn_k.weight".into(), q8_0_matrix_bytes(32, 64, |_, _| 0)),
            ("blk.0.attn_v.weight".into(), q8_0_matrix_bytes(32, 64, |row, col| if row == 0 && col == 0 { 1 } else if row == 0 && col == 1 { -1 } else { 0 })),
            ("blk.0.attn_output.weight".into(), q8_0_matrix_bytes(64, 64, |row, col| i8::from((row, col) == (2, 0)))),
            ("blk.0.ffn_gate.weight".into(), q8_0_matrix_bytes(96, 64, |_, _| 0)),
            ("blk.0.ffn_up.weight".into(), q8_0_matrix_bytes(96, 64, |_, _| 0)),
            ("blk.0.ffn_down.weight".into(), q8_0_matrix_bytes(64, 96, |_, _| 0)),
            ("blk.1.attn_q.weight".into(), q8_0_matrix_bytes(64, 64, |_, _| 0)),
            ("blk.1.attn_k.weight".into(), q8_0_matrix_bytes(32, 64, |_, _| 0)),
            ("blk.1.attn_v.weight".into(), q8_0_matrix_bytes(32, 64, |_, _| 0)),
            ("blk.1.attn_output.weight".into(), q8_0_matrix_bytes(64, 64, |_, _| 0)),
            ("blk.1.ffn_gate.weight".into(), q8_0_matrix_bytes(96, 64, |_, _| 0)),
            ("blk.1.ffn_up.weight".into(), q8_0_matrix_bytes(96, 64, |_, _| 0)),
            ("blk.1.ffn_down.weight".into(), q8_0_matrix_bytes(64, 96, |_, _| 0)),
            ("output.weight".into(), q8_0_matrix_bytes(64, 64, |row, col| {
                if (row == 0 && col == 0) || (row == 2 && col == 0) { 1 } else if (row == 1 || row == 2) && col == 1 { 2 } else { 0 }
            })),
        ]),
    );
    let model = rust_model_inference::Qwen3Model::from_catalog(&catalog).unwrap();
    PlacementFixture {
        token_embedding: model.tensors.token_embedding,
        output: model.tensors.output,
        catalog,
        model: PlacementModel::Qwen3(model),
    }
}

pub fn tiny_qwen3_tied() -> PlacementFixture {
    let catalog = fixture_catalog(
        qwen3_tied_tensors(),
        model_metadata("qwen3", 17, 2, 4, 2, 96, 64),
    );
    let model = rust_model_inference::Qwen3Model::from_catalog(&catalog).unwrap();
    PlacementFixture {
        token_embedding: model.tensors.token_embedding,
        output: model.tensors.output,
        catalog,
        model: PlacementModel::Qwen3(model),
    }
}

pub fn tiny_qwen35_hybrid() -> PlacementFixture {
    let mut metadata = model_metadata("qwen35", 19, 2, 4, 2, 96, 64);
    metadata.insert(
        "qwen35.vocab_size".into(),
        rust_model_inference::MetaValue::Uint32(0),
    );
    metadata.insert(
        "tokenizer.ggml.tokens".into(),
        rust_model_inference::MetaValue::Array(
            rust_model_inference::MetaValueType::String,
            (0..64)
                .map(|index| rust_model_inference::MetaValue::String(format!("{index}")))
                .collect(),
        ),
    );
    metadata.extend(BTreeMap::from([
        (
            "qwen35.attention.key_length".into(),
            rust_model_inference::MetaValue::Uint32(16),
        ),
        (
            "qwen35.attention.value_length".into(),
            rust_model_inference::MetaValue::Uint32(16),
        ),
        (
            "qwen35.rope.dimension_count".into(),
            rust_model_inference::MetaValue::Uint32(16),
        ),
        (
            "qwen35.rope.dimension_sections".into(),
            rust_model_inference::MetaValue::Array(
                rust_model_inference::MetaValueType::Uint32,
                vec![
                    rust_model_inference::MetaValue::Uint32(4),
                    rust_model_inference::MetaValue::Uint32(4),
                    rust_model_inference::MetaValue::Uint32(4),
                    rust_model_inference::MetaValue::Uint32(4),
                ],
            ),
        ),
        (
            "qwen35.ssm.conv_kernel".into(),
            rust_model_inference::MetaValue::Uint32(4),
        ),
        (
            "qwen35.ssm.state_size".into(),
            rust_model_inference::MetaValue::Uint32(32),
        ),
        (
            "qwen35.ssm.group_count".into(),
            rust_model_inference::MetaValue::Uint32(1),
        ),
        (
            "qwen35.ssm.time_step_rank".into(),
            rust_model_inference::MetaValue::Uint32(1),
        ),
        (
            "qwen35.ssm.inner_size".into(),
            rust_model_inference::MetaValue::Uint32(32),
        ),
        (
            "qwen35.full_attention_interval".into(),
            rust_model_inference::MetaValue::Uint32(4),
        ),
        (
            "qwen35.attention.recurrent_layers".into(),
            rust_model_inference::MetaValue::Array(
                rust_model_inference::MetaValueType::Uint32,
                vec![
                    rust_model_inference::MetaValue::Uint32(0),
                    rust_model_inference::MetaValue::Uint32(1),
                ],
            ),
        ),
    ]));
    let catalog = fixture_catalog_with_overrides(
        qwen35_tensors(),
        metadata,
        BTreeMap::from([
            (
                "token_embd.weight".into(),
                q8_0_matrix_bytes(64, 64, |row, column| {
                    i8::from(matches!((row, column), (1, 0) | (2, 1)))
                }),
            ),
            (
                "output.weight".into(),
                q8_0_matrix_bytes(64, 64, |row, column| i8::from(row == column)),
            ),
            (
                "blk.0.attn_q.weight".into(),
                q8_0_matrix_bytes(128, 64, |_, _| 0),
            ),
            (
                "blk.0.attn_k.weight".into(),
                q8_0_matrix_bytes(32, 64, |_, _| 0),
            ),
            (
                "blk.0.attn_v.weight".into(),
                q8_0_matrix_bytes(32, 64, |_, _| 0),
            ),
            (
                "blk.0.attn_output.weight".into(),
                q8_0_matrix_bytes(64, 64, |_, _| 0),
            ),
            (
                "blk.0.ffn_gate.weight".into(),
                q8_0_matrix_bytes(96, 64, |_, _| 0),
            ),
            (
                "blk.0.ffn_up.weight".into(),
                q8_0_matrix_bytes(96, 64, |_, _| 0),
            ),
            (
                "blk.0.ffn_down.weight".into(),
                q8_0_matrix_bytes(64, 96, |_, _| 0),
            ),
            (
                "blk.1.attn_qkv.weight".into(),
                q8_0_matrix_bytes(96, 64, |row, column| {
                    i8::from(column == 0 && matches!(row, 0 | 1 | 32 | 33 | 64 | 65))
                }),
            ),
            (
                "blk.1.attn_gate.weight".into(),
                q8_0_matrix_bytes(32, 64, |row, column| i8::from(column == 0 && row < 2)),
            ),
            (
                "blk.1.ssm_beta.weight".into(),
                q8_0_matrix_bytes(1, 64, |_, _| 0),
            ),
            (
                "blk.1.ssm_alpha.weight".into(),
                q8_0_matrix_bytes(1, 64, |_, _| 0),
            ),
            (
                "blk.1.ssm_conv1d.weight".into(),
                f32_matrix_bytes(1, 384, |_, index| {
                    let channel = index / 4;
                    let tap = index % 4;
                    if matches!(channel, 0 | 1 | 32 | 33 | 64 | 65) && tap == 3 {
                        1.0
                    } else if channel == 64 && tap == 2 {
                        0.5
                    } else if channel == 65 && tap == 2 {
                        -0.25
                    } else {
                        0.0
                    }
                }),
            ),
            (
                "blk.1.ssm_dt.bias".into(),
                f32_matrix_bytes(1, 1, |_, _| 0.0),
            ),
            ("blk.1.ssm_a".into(), f32_matrix_bytes(1, 1, |_, _| -0.5)),
            (
                "blk.1.ssm_out.weight".into(),
                q8_0_matrix_bytes(64, 32, |row, column| {
                    i8::from((row, column) == (1, 0) || (row, column) == (2, 1))
                }),
            ),
            (
                "blk.1.ffn_gate.weight".into(),
                q8_0_matrix_bytes(96, 64, |_, _| 0),
            ),
            (
                "blk.1.ffn_up.weight".into(),
                q8_0_matrix_bytes(96, 64, |_, _| 0),
            ),
            (
                "blk.1.ffn_down.weight".into(),
                q8_0_matrix_bytes(64, 96, |_, _| 0),
            ),
        ]),
    );
    let model = rust_model_inference::Qwen35Model::from_catalog(&catalog).unwrap();
    PlacementFixture {
        token_embedding: catalog.find(ComponentId::Llm, "token_embd.weight").unwrap(),
        output: catalog.find(ComponentId::Llm, "output.weight").unwrap(),
        catalog,
        model: PlacementModel::Qwen35(model),
    }
}

pub fn tiny_qwen35_f32_dense() -> PlacementFixture {
    tiny_qwen35_dense(GGMLType::F32, BTreeMap::new())
}

pub fn tiny_qwen35_f16_dense() -> PlacementFixture {
    tiny_qwen35_dense(GGMLType::F16, BTreeMap::new())
}

pub fn tiny_qwen35_q8_dense() -> PlacementFixture {
    tiny_qwen35_dense(
        GGMLType::Q8_0,
        BTreeMap::from([
            (
                "token_embd.weight".into(),
                q8_0_matrix_bytes(64, 64, |row, col| {
                    i8::from((row, col) == (1, 0) || (row, col) == (2, 1))
                }),
            ),
            (
                "blk.0.attn_q.weight".into(),
                q8_0_matrix_bytes(128, 64, |row, col| {
                    if row < 64 && (row == col || row == col + 16) {
                        1
                    } else if row >= 64 && row - 64 == col {
                        2
                    } else {
                        0
                    }
                }),
            ),
            (
                "blk.0.attn_k.weight".into(),
                q8_0_matrix_bytes(32, 64, |row, col| i8::from(row == col || row == col + 16)),
            ),
            (
                "blk.0.attn_v.weight".into(),
                q8_0_matrix_bytes(32, 64, |row, col| i8::from(row == col || row == col + 16)),
            ),
            (
                "blk.0.attn_output.weight".into(),
                q8_0_matrix_bytes(64, 64, |row, col| {
                    i8::from((row, col) == (2, 0) || (row, col) == (3, 1) || (row, col) == (4, 16))
                }),
            ),
            (
                "blk.0.ffn_gate.weight".into(),
                q8_0_matrix_bytes(96, 64, |row, col| {
                    i8::from((row, col) == (0, 0) || (row, col) == (1, 2))
                }),
            ),
            (
                "blk.0.ffn_up.weight".into(),
                q8_0_matrix_bytes(96, 64, |row, col| {
                    i8::from((row, col) == (0, 2) || (row, col) == (1, 0))
                }),
            ),
            (
                "blk.0.ffn_down.weight".into(),
                q8_0_matrix_bytes(64, 96, |row, col| {
                    i8::from((row, col) == (4, 0) || (row, col) == (5, 1))
                }),
            ),
            (
                "output.weight".into(),
                q8_0_matrix_bytes(64, 64, |row, col| i8::from(row == col)),
            ),
        ]),
    )
}

pub fn tiny_qwen35_f32_dense_with_attention_gate(gate_value: f32) -> PlacementFixture {
    let input_width = 64;
    let attention_width = 64;
    tiny_qwen35_dense(
        GGMLType::F32,
        BTreeMap::from([
            (
                "blk.0.attn_q.weight".into(),
                f32_matrix_bytes(attention_width * 2, input_width, |row, _| {
                    if row < attention_width {
                        1.0
                    } else {
                        gate_value
                    }
                }),
            ),
            (
                "blk.0.attn_v.weight".into(),
                f32_matrix_bytes(32, input_width, |row, _| if row < 16 { 1.0 } else { 0.0 }),
            ),
            (
                "blk.0.attn_output.weight".into(),
                f32_matrix_bytes(64, 64, |row, column| f32::from(row == column)),
            ),
            (
                "blk.0.ffn_gate.weight".into(),
                f32_matrix_bytes(96, input_width, |_, _| 0.0),
            ),
            (
                "blk.0.ffn_up.weight".into(),
                f32_matrix_bytes(96, input_width, |_, _| 0.0),
            ),
            (
                "blk.0.ffn_down.weight".into(),
                f32_matrix_bytes(input_width, 96, |_, _| 0.0),
            ),
            (
                "output.weight".into(),
                q8_0_matrix_bytes(input_width, input_width, |row, column| {
                    i8::from(row == column)
                }),
            ),
        ]),
    )
}

pub fn qwen3_metadata_layer_count_mismatch() -> String {
    let catalog = fixture_catalog(
        qwen3_tensors(),
        model_metadata("qwen3", 17, 3, 4, 2, 96, 64),
    );
    rust_model_inference::Qwen3Model::from_catalog(&catalog)
        .err()
        .expect("Qwen3 layer-count mismatch must fail")
}

pub fn qwen35_metadata_layer_selection_mismatch() -> String {
    let mut metadata = model_metadata("qwen35", 19, 1, 4, 2, 96, 64);
    metadata.extend(qwen35_dense_metadata());
    metadata.insert(
        "tokenizer.ggml.tokens".into(),
        rust_model_inference::MetaValue::Array(
            rust_model_inference::MetaValueType::String,
            (0..64)
                .map(|index| rust_model_inference::MetaValue::String(format!("{index}")))
                .collect(),
        ),
    );
    metadata.insert(
        "qwen35.attention.recurrent_layers".into(),
        rust_model_inference::MetaValue::Array(
            rust_model_inference::MetaValueType::Uint32,
            vec![rust_model_inference::MetaValue::Uint32(1)],
        ),
    );
    let catalog = fixture_catalog(
        qwen35_dense_tensors_with_matrix_type(GGMLType::F32),
        metadata,
    );
    rust_model_inference::Qwen35Model::from_catalog(&catalog)
        .err()
        .expect("Qwen3.5 recurrent metadata must reject dense tensors")
}

fn tiny_qwen35_dense(
    matrix_type: GGMLType,
    tensor_overrides: BTreeMap<String, Vec<u8>>,
) -> PlacementFixture {
    let mut metadata = model_metadata("qwen35", 19, 1, 4, 2, 96, 64);
    metadata.insert(
        "qwen35.vocab_size".into(),
        rust_model_inference::MetaValue::Uint32(0),
    );
    metadata.insert(
        "tokenizer.ggml.tokens".into(),
        rust_model_inference::MetaValue::Array(
            rust_model_inference::MetaValueType::String,
            (0..64)
                .map(|index| rust_model_inference::MetaValue::String(format!("{index}")))
                .collect(),
        ),
    );
    metadata.extend(BTreeMap::from([
        (
            "qwen35.attention.key_length".into(),
            rust_model_inference::MetaValue::Uint32(16),
        ),
        (
            "qwen35.attention.value_length".into(),
            rust_model_inference::MetaValue::Uint32(16),
        ),
        (
            "qwen35.rope.dimension_count".into(),
            rust_model_inference::MetaValue::Uint32(16),
        ),
        (
            "qwen35.rope.dimension_sections".into(),
            rust_model_inference::MetaValue::Array(
                rust_model_inference::MetaValueType::Uint32,
                vec![
                    rust_model_inference::MetaValue::Uint32(4),
                    rust_model_inference::MetaValue::Uint32(4),
                    rust_model_inference::MetaValue::Uint32(4),
                    rust_model_inference::MetaValue::Uint32(4),
                ],
            ),
        ),
    ]));
    metadata.extend(BTreeMap::from([
        (
            "qwen35.ssm.conv_kernel".into(),
            rust_model_inference::MetaValue::Uint32(4),
        ),
        (
            "qwen35.ssm.state_size".into(),
            rust_model_inference::MetaValue::Uint32(32),
        ),
        (
            "qwen35.ssm.group_count".into(),
            rust_model_inference::MetaValue::Uint32(1),
        ),
        (
            "qwen35.ssm.time_step_rank".into(),
            rust_model_inference::MetaValue::Uint32(1),
        ),
        (
            "qwen35.ssm.inner_size".into(),
            rust_model_inference::MetaValue::Uint32(32),
        ),
        (
            "qwen35.full_attention_interval".into(),
            rust_model_inference::MetaValue::Uint32(4),
        ),
        (
            "qwen35.attention.recurrent_layers".into(),
            rust_model_inference::MetaValue::Array(
                rust_model_inference::MetaValueType::Uint32,
                vec![rust_model_inference::MetaValue::Uint32(0)],
            ),
        ),
    ]));
    let catalog = fixture_catalog_with_overrides(
        qwen35_dense_tensors_with_matrix_type(matrix_type),
        metadata,
        tensor_overrides,
    );
    let model = rust_model_inference::Qwen35Model::from_catalog(&catalog).unwrap();
    PlacementFixture {
        token_embedding: catalog.find(ComponentId::Llm, "token_embd.weight").unwrap(),
        output: catalog.find(ComponentId::Llm, "output.weight").unwrap(),
        catalog,
        model: PlacementModel::Qwen35(model),
    }
}

pub fn tiny_qwen35_quantized_dense(matrix_type: GGMLType) -> PlacementFixture {
    assert!(matches!(
        matrix_type,
        GGMLType::Q4K | GGMLType::Q5K | GGMLType::Q6K
    ));
    let mut metadata = model_metadata("qwen35", 19, 1, 4, 2, 256, 64);
    metadata.insert(
        "qwen35.embedding_length".into(),
        rust_model_inference::MetaValue::Uint32(256),
    );
    metadata.insert(
        "qwen35.vocab_size".into(),
        rust_model_inference::MetaValue::Uint32(0),
    );
    metadata.insert(
        "tokenizer.ggml.tokens".into(),
        rust_model_inference::MetaValue::Array(
            rust_model_inference::MetaValueType::String,
            (0..64)
                .map(|index| rust_model_inference::MetaValue::String(format!("{index}")))
                .collect(),
        ),
    );
    metadata.extend(qwen35_dense_metadata());
    let catalog = fixture_catalog(qwen35_quantized_dense_tensors(matrix_type), metadata);
    let model = rust_model_inference::Qwen35Model::from_catalog(&catalog).unwrap();
    PlacementFixture {
        token_embedding: catalog.find(ComponentId::Llm, "token_embd.weight").unwrap(),
        output: catalog.find(ComponentId::Llm, "output.weight").unwrap(),
        catalog,
        model: PlacementModel::Qwen35(model),
    }
}

fn qwen35_dense_metadata() -> BTreeMap<String, rust_model_inference::MetaValue> {
    BTreeMap::from([
        (
            "qwen35.attention.key_length".into(),
            rust_model_inference::MetaValue::Uint32(64),
        ),
        (
            "qwen35.attention.value_length".into(),
            rust_model_inference::MetaValue::Uint32(64),
        ),
        (
            "qwen35.rope.dimension_count".into(),
            rust_model_inference::MetaValue::Uint32(64),
        ),
        (
            "qwen35.rope.dimension_sections".into(),
            rust_model_inference::MetaValue::Array(
                rust_model_inference::MetaValueType::Uint32,
                vec![
                    rust_model_inference::MetaValue::Uint32(16),
                    rust_model_inference::MetaValue::Uint32(16),
                    rust_model_inference::MetaValue::Uint32(16),
                    rust_model_inference::MetaValue::Uint32(16),
                ],
            ),
        ),
        (
            "qwen35.ssm.conv_kernel".into(),
            rust_model_inference::MetaValue::Uint32(4),
        ),
        (
            "qwen35.ssm.state_size".into(),
            rust_model_inference::MetaValue::Uint32(32),
        ),
        (
            "qwen35.ssm.group_count".into(),
            rust_model_inference::MetaValue::Uint32(1),
        ),
        (
            "qwen35.ssm.time_step_rank".into(),
            rust_model_inference::MetaValue::Uint32(1),
        ),
        (
            "qwen35.ssm.inner_size".into(),
            rust_model_inference::MetaValue::Uint32(32),
        ),
        (
            "qwen35.full_attention_interval".into(),
            rust_model_inference::MetaValue::Uint32(4),
        ),
        (
            "qwen35.attention.recurrent_layers".into(),
            rust_model_inference::MetaValue::Array(
                rust_model_inference::MetaValueType::Uint32,
                vec![rust_model_inference::MetaValue::Uint32(0)],
            ),
        ),
    ])
}

fn model_metadata(
    arch: &str,
    n_ctx: u32,
    n_layer: u32,
    n_head: u32,
    n_head_kv: u32,
    n_ff: u32,
    vocab: u32,
) -> BTreeMap<String, rust_model_inference::MetaValue> {
    BTreeMap::from([
        (
            "general.architecture".into(),
            rust_model_inference::MetaValue::String(arch.into()),
        ),
        (
            format!("{arch}.embedding_length"),
            rust_model_inference::MetaValue::Uint32(64),
        ),
        (
            format!("{arch}.block_count"),
            rust_model_inference::MetaValue::Uint32(n_layer),
        ),
        (
            format!("{arch}.attention.head_count"),
            rust_model_inference::MetaValue::Uint32(n_head),
        ),
        (
            format!("{arch}.attention.head_count_kv"),
            rust_model_inference::MetaValue::Uint32(n_head_kv),
        ),
        (
            format!("{arch}.feed_forward_length"),
            rust_model_inference::MetaValue::Uint32(n_ff),
        ),
        (
            format!("{arch}.context_length"),
            rust_model_inference::MetaValue::Uint32(n_ctx),
        ),
        (
            format!("{arch}.vocab_size"),
            rust_model_inference::MetaValue::Uint32(vocab),
        ),
        (
            format!("{arch}.rope.freq_base"),
            rust_model_inference::MetaValue::Float32(1_000_000.0),
        ),
        (
            format!("{arch}.attention.layer_norm_rms_epsilon"),
            rust_model_inference::MetaValue::Float32(1e-6),
        ),
    ])
}

struct FixtureSource {
    metadata: BTreeMap<String, rust_model_inference::MetaValue>,
    records: Vec<SourceTensorRecord>,
    bytes: BTreeMap<String, Vec<u8>>,
}

impl TensorSource for FixtureSource {
    fn metadata(&self, key: &str) -> Option<&rust_model_inference::MetaValue> {
        self.metadata.get(key)
    }

    fn tensor_info(&self, name: &str) -> Option<&TensorInfo> {
        self.records
            .iter()
            .find(|record| record.info.name == name)
            .map(|record| &record.info)
    }

    fn tensor_slice(&self, name: &str) -> Option<&[u8]> {
        self.bytes.get(name).map(Vec::as_slice)
    }

    fn source_format(&self) -> SourceFormat {
        SourceFormat::Gguf
    }

    fn tensor_records(&self) -> Vec<SourceTensorRecord> {
        self.records.clone()
    }
}

fn fixture_catalog(
    tensors: Vec<(String, Vec<u64>, GGMLType)>,
    metadata: BTreeMap<String, rust_model_inference::MetaValue>,
) -> Arc<TensorCatalog> {
    fixture_catalog_with_overrides(tensors, metadata, BTreeMap::new())
}

fn fixture_catalog_with_overrides(
    tensors: Vec<(String, Vec<u64>, GGMLType)>,
    metadata: BTreeMap<String, rust_model_inference::MetaValue>,
    tensor_overrides: BTreeMap<String, Vec<u8>>,
) -> Arc<TensorCatalog> {
    let mut offset = 0_u64;
    let mut records = Vec::new();
    let mut bytes = BTreeMap::new();
    for (name, dims, ggml_type) in tensors {
        let elements = dims.iter().product::<u64>();
        let (block_elements, block_bytes) = ggml_type.type_traits();
        let len =
            elements / u64::try_from(block_elements).unwrap() * u64::try_from(block_bytes).unwrap();
        let mut tensor_bytes = vec![0; usize::try_from(len).unwrap()];
        match ggml_type {
            GGMLType::Q8_0 => {
                for block in tensor_bytes.chunks_exact_mut(34) {
                    block[..2].copy_from_slice(&[0x00, 0x3c]);
                    block[2..].fill(1);
                }
            }
            GGMLType::F32 => {
                for value in tensor_bytes.chunks_exact_mut(4) {
                    value.copy_from_slice(&1.0_f32.to_le_bytes());
                }
            }
            GGMLType::F16 => {
                for value in tensor_bytes.chunks_exact_mut(2) {
                    value.copy_from_slice(&half::f16::from_f32(1.0).to_bits().to_le_bytes());
                }
            }
            GGMLType::Q4K | GGMLType::Q5K | GGMLType::Q6K => {
                for block in tensor_bytes.chunks_exact_mut(block_bytes) {
                    block[..2].copy_from_slice(&[0x00, 0x3c]);
                    block[2..].fill(1);
                }
            }
            _ => {}
        }
        if let Some(overridden) = tensor_overrides.get(&name) {
            assert_eq!(overridden.len(), tensor_bytes.len(), "{name}");
            tensor_bytes.copy_from_slice(overridden);
        }
        records.push(SourceTensorRecord {
            info: TensorInfo {
                name: name.clone(),
                dims,
                ggml_type,
                offset,
            },
            segment_id: 0,
            segment_byte_range: offset..offset + len,
            layer: None,
        });
        bytes.insert(name, tensor_bytes);
        offset += len;
    }
    Arc::new(
        TensorCatalog::from_sources(vec![(
            ComponentId::Llm,
            Arc::new(FixtureSource {
                metadata,
                records,
                bytes,
            }) as Arc<dyn TensorSource>,
        )])
        .unwrap(),
    )
}

fn f32_matrix_bytes(
    rows: usize,
    input_width: usize,
    values: impl Fn(usize, usize) -> f32,
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(rows * input_width * 4);
    for row in 0..rows {
        for column in 0..input_width {
            bytes.extend(values(row, column).to_le_bytes());
        }
    }
    bytes
}

fn q8_0_matrix_bytes(
    rows: usize,
    input_width: usize,
    values: impl Fn(usize, usize) -> i8,
) -> Vec<u8> {
    assert_eq!(input_width % 32, 0);
    let mut bytes = Vec::with_capacity(rows * input_width / 32 * 34);
    for row in 0..rows {
        for block in 0..input_width / 32 {
            bytes.extend(half::f16::from_f32(1.0).to_bits().to_le_bytes());
            for lane in 0..32 {
                bytes.push(values(row, block * 32 + lane) as u8);
            }
        }
    }
    bytes
}

fn qwen3_tensors() -> Vec<(String, Vec<u64>, GGMLType)> {
    let mut tensors = vec![
        ("token_embd.weight".into(), vec![64, 64], GGMLType::Q8_0),
        ("output_norm.weight".into(), vec![64], GGMLType::F32),
        ("output.weight".into(), vec![64, 64], GGMLType::Q8_0),
    ];
    for layer in 0..2 {
        tensors.extend(qwen3_layer_tensors(layer));
    }
    tensors
}

fn qwen3_tied_tensors() -> Vec<(String, Vec<u64>, GGMLType)> {
    let mut tensors = vec![
        ("token_embd.weight".into(), vec![64, 64], GGMLType::Q8_0),
        ("output_norm.weight".into(), vec![64], GGMLType::F32),
    ];
    for layer in 0..2 {
        tensors.extend(qwen3_layer_tensors(layer));
    }
    tensors
}

fn qwen3_layer_tensors(layer: usize) -> Vec<(String, Vec<u64>, GGMLType)> {
    let prefix = format!("blk.{layer}");
    vec![
        (
            format!("{prefix}.attn_norm.weight"),
            vec![64],
            GGMLType::F32,
        ),
        (
            format!("{prefix}.attn_q_norm.weight"),
            vec![16],
            GGMLType::F32,
        ),
        (
            format!("{prefix}.attn_k_norm.weight"),
            vec![16],
            GGMLType::F32,
        ),
        (
            format!("{prefix}.attn_q.weight"),
            vec![64, 64],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.attn_k.weight"),
            vec![64, 32],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.attn_v.weight"),
            vec![64, 32],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.attn_output.weight"),
            vec![64, 64],
            GGMLType::Q8_0,
        ),
        (format!("{prefix}.ffn_norm.weight"), vec![64], GGMLType::F32),
        (
            format!("{prefix}.ffn_gate.weight"),
            vec![64, 96],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.ffn_up.weight"),
            vec![64, 96],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.ffn_down.weight"),
            vec![96, 64],
            GGMLType::Q8_0,
        ),
    ]
}

fn qwen35_tensors() -> Vec<(String, Vec<u64>, GGMLType)> {
    let mut tensors = vec![
        ("token_embd.weight".into(), vec![64, 64], GGMLType::Q8_0),
        ("output_norm.weight".into(), vec![64], GGMLType::F32),
        ("output.weight".into(), vec![64, 64], GGMLType::Q8_0),
    ];
    tensors.extend(qwen35_dense_tensors(0));
    tensors.extend(qwen35_recurrent_tensors(1));
    tensors
}

fn qwen35_dense_tensors(layer: usize) -> Vec<(String, Vec<u64>, GGMLType)> {
    let prefix = format!("blk.{layer}");
    vec![
        (
            format!("{prefix}.attn_norm.weight"),
            vec![64],
            GGMLType::F32,
        ),
        (
            format!("{prefix}.post_attention_norm.weight"),
            vec![64],
            GGMLType::F32,
        ),
        (
            format!("{prefix}.attn_q_norm.weight"),
            vec![16],
            GGMLType::F32,
        ),
        (
            format!("{prefix}.attn_k_norm.weight"),
            vec![16],
            GGMLType::F32,
        ),
        (
            format!("{prefix}.attn_q.weight"),
            vec![64, 128],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.attn_k.weight"),
            vec![64, 32],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.attn_v.weight"),
            vec![64, 32],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.attn_output.weight"),
            vec![64, 64],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.ffn_gate.weight"),
            vec![64, 96],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.ffn_up.weight"),
            vec![64, 96],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.ffn_down.weight"),
            vec![96, 64],
            GGMLType::Q8_0,
        ),
    ]
}

fn qwen35_recurrent_tensors(layer: usize) -> Vec<(String, Vec<u64>, GGMLType)> {
    let prefix = format!("blk.{layer}");
    vec![
        (
            format!("{prefix}.attn_norm.weight"),
            vec![64],
            GGMLType::F32,
        ),
        (
            format!("{prefix}.post_attention_norm.weight"),
            vec![64],
            GGMLType::F32,
        ),
        (
            format!("{prefix}.attn_qkv.weight"),
            vec![64, 96],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.attn_gate.weight"),
            vec![64, 32],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.ssm_beta.weight"),
            vec![64, 1],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.ssm_alpha.weight"),
            vec![64, 1],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.ssm_conv1d.weight"),
            vec![384],
            GGMLType::F32,
        ),
        (format!("{prefix}.ssm_dt.bias"), vec![1], GGMLType::F32),
        (format!("{prefix}.ssm_a"), vec![1], GGMLType::F32),
        (format!("{prefix}.ssm_norm.weight"), vec![32], GGMLType::F32),
        (
            format!("{prefix}.ssm_out.weight"),
            vec![32, 64],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.ffn_gate.weight"),
            vec![64, 96],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.ffn_up.weight"),
            vec![64, 96],
            GGMLType::Q8_0,
        ),
        (
            format!("{prefix}.ffn_down.weight"),
            vec![96, 64],
            GGMLType::Q8_0,
        ),
    ]
}

fn qwen35_dense_tensors_with_matrix_type(
    matrix_type: GGMLType,
) -> Vec<(String, Vec<u64>, GGMLType)> {
    let mut tensors = vec![
        ("token_embd.weight".into(), vec![64, 64], GGMLType::Q8_0),
        ("output_norm.weight".into(), vec![64], GGMLType::F32),
        ("output.weight".into(), vec![64, 64], GGMLType::Q8_0),
    ];
    for (name, dims, _) in qwen35_dense_tensors(0) {
        let ggml_type = if dims.len() >= 2 {
            matrix_type
        } else {
            GGMLType::F32
        };
        tensors.push((name, dims, ggml_type));
    }
    tensors
}

fn qwen35_quantized_dense_tensors(matrix_type: GGMLType) -> Vec<(String, Vec<u64>, GGMLType)> {
    let prefix = "blk.0";
    vec![
        ("token_embd.weight".into(), vec![256, 64], GGMLType::Q8_0),
        ("output_norm.weight".into(), vec![256], GGMLType::F32),
        ("output.weight".into(), vec![256, 64], GGMLType::Q8_0),
        (
            format!("{prefix}.attn_norm.weight"),
            vec![256],
            GGMLType::F32,
        ),
        (
            format!("{prefix}.post_attention_norm.weight"),
            vec![256],
            GGMLType::F32,
        ),
        (
            format!("{prefix}.attn_q_norm.weight"),
            vec![64],
            GGMLType::F32,
        ),
        (
            format!("{prefix}.attn_k_norm.weight"),
            vec![64],
            GGMLType::F32,
        ),
        (
            format!("{prefix}.attn_q.weight"),
            vec![256, 512],
            matrix_type,
        ),
        (
            format!("{prefix}.attn_k.weight"),
            vec![256, 128],
            matrix_type,
        ),
        (
            format!("{prefix}.attn_v.weight"),
            vec![256, 128],
            matrix_type,
        ),
        (
            format!("{prefix}.attn_output.weight"),
            vec![256, 256],
            matrix_type,
        ),
        (
            format!("{prefix}.ffn_gate.weight"),
            vec![256, 256],
            matrix_type,
        ),
        (
            format!("{prefix}.ffn_up.weight"),
            vec![256, 256],
            matrix_type,
        ),
        (
            format!("{prefix}.ffn_down.weight"),
            vec![256, 256],
            matrix_type,
        ),
    ]
}

struct RecordingProvider {
    descriptor: DeviceDescriptor,
    trace: Arc<Mutex<PlacementTrace>>,
}

impl DeviceDiscovery for RecordingProvider {
    fn backend(&self) -> BackendKind {
        BackendKind::Cpu
    }

    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, BackendError> {
        Ok(vec![self.descriptor.clone()])
    }
}

impl DeviceProvider for RecordingProvider {
    fn open(
        &self,
        descriptor: &DeviceDescriptor,
        plan: &DevicePlan,
        _catalog: Arc<TensorCatalog>,
    ) -> Result<Box<dyn DeviceSession>, BackendError> {
        if descriptor != &self.descriptor || &plan.descriptor != descriptor {
            return Err(BackendError::InvalidHandle);
        }
        Ok(Box::new(RecordingSession {
            descriptor: descriptor.clone(),
            programs: plan
                .programs
                .iter()
                .map(|program| (program.id, program.clone()))
                .collect(),
            slots: plan
                .slots
                .iter()
                .map(|slot| {
                    (
                        slot.id,
                        vec![0.0; usize::try_from(slot.byte_len / 4).unwrap()],
                    )
                })
                .collect(),
            trace: Arc::clone(&self.trace),
            fence: 0,
        }))
    }
}

struct RecordingSession {
    descriptor: DeviceDescriptor,
    programs: BTreeMap<ProgramId, rust_model_inference::ProgramPlan>,
    slots: BTreeMap<SlotId, Vec<f32>>,
    trace: Arc<Mutex<PlacementTrace>>,
    fence: u64,
}

impl DeviceSession for RecordingSession {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.descriptor
    }

    fn write_f32(&mut self, slot: SlotId, values: &[f32]) -> Result<(), BackendError> {
        let target = self
            .slots
            .get_mut(&slot)
            .ok_or(BackendError::InvalidHandle)?;
        if values.len() > target.len() {
            return Err(BackendError::InvalidHandle);
        }
        target[..values.len()].copy_from_slice(values);
        Ok(())
    }

    fn submit(
        &mut self,
        program: ProgramId,
        _params: &RunParams<'_>,
    ) -> Result<FenceId, BackendError> {
        let plan = self
            .programs
            .get(&program)
            .ok_or(BackendError::InvalidHandle)?;
        let layer_ops = plan.layer_ops.clone();
        let conv_states = layer_ops
            .iter()
            .filter_map(|op| match op {
                LayerOp::DepthwiseCausalConv { state, .. } => self
                    .slots
                    .get(state)
                    .map(|values| (*state, u64::try_from(values.len()).unwrap() * 4)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut trace = self.trace.lock().map_err(|_| BackendError::PoisonedRun)?;
        trace.program_kinds.push(plan.kind.clone());
        match &plan.kind {
            ProgramKind::Q8Rows { tensor, .. } => {
                trace.q8_matrix_tensor_ids.insert(*tensor);
            }
            ProgramKind::EmbeddingRows { tensor, .. } => {
                trace.embedding_tensor_ids.insert(*tensor);
            }
            ProgramKind::LayerSegment { .. } => {
                trace.layer_ops.extend(layer_ops);
                trace.slot_byte_lengths.extend(conv_states);
                for operation in &plan.layer_ops {
                    if let rust_model_inference::LayerOp::Q8Matmul { weight, .. } = operation {
                        trace.q8_matrix_tensor_ids.insert(*weight);
                    }
                }
            }
            ProgramKind::FinalNormQ8Logits { output, .. } => {
                trace.q8_matrix_tensor_ids.insert(*output);
            }
        }
        drop(trace);
        if let ProgramKind::EmbeddingRows { row_count, .. } = plan.kind {
            if _params.token_count == 0
                || _params.token_ids.len() != _params.token_count as usize
                || _params.token_ids.iter().any(|token| *token >= row_count)
            {
                return Err(BackendError::InvalidHandle);
            }
        }
        self.fence += 1;
        Ok(FenceId(self.fence))
    }

    fn wait(&mut self, fence: FenceId) -> Result<(), BackendError> {
        (fence.0 <= self.fence)
            .then_some(())
            .ok_or(BackendError::InvalidHandle)
    }

    fn read_f32(&mut self, slot: SlotId, values: &mut [f32]) -> Result<(), BackendError> {
        self.read_f32_at(slot, 0, values)
    }

    fn read_f32_at(
        &mut self,
        slot: SlotId,
        offset: usize,
        values: &mut [f32],
    ) -> Result<(), BackendError> {
        let source = self.slots.get(&slot).ok_or(BackendError::InvalidHandle)?;
        let end = offset
            .checked_add(values.len())
            .filter(|end| *end <= source.len())
            .ok_or(BackendError::InvalidHandle)?;
        values.copy_from_slice(&source[offset..end]);
        Ok(())
    }

    fn reset_state(&mut self) -> Result<(), BackendError> {
        Ok(())
    }

    fn stats(&self) -> SessionStats {
        SessionStats::default()
    }

    fn lifecycle_probe(&self) -> LifecycleProbe {
        LifecycleProbe::default()
    }
}

fn recording_descriptor() -> DeviceDescriptor {
    DeviceDescriptor {
        id: DeviceId::parse("cpu0").unwrap(),
        backend: BackendKind::Cpu,
        physical_key: "recording-cpu".into(),
        name: "recording-cpu".into(),
        usable_bytes: u64::MAX,
        max_allocation_bytes: u64::MAX,
        buffer_alignment: 4,
        unified_memory: true,
        capabilities: DeviceCapabilities {
            components: BTreeSet::from([ComponentId::Llm]),
            modes: BTreeSet::from([
                rust_model_inference::PlacementMode::Row,
                rust_model_inference::PlacementMode::Layer,
            ]),
            layer_families: BTreeSet::from([
                rust_model_inference::LayerFamily::Qwen3,
                rust_model_inference::LayerFamily::Qwen35Dense,
                rust_model_inference::LayerFamily::Qwen35Recurrent,
            ]),
            tensor_types: BTreeSet::from([GGMLType::F32, GGMLType::Q8_0]),
        },
    }
}

#[cfg(any(feature = "vulkan", all(target_os = "macos", feature = "metal")))]
mod gpu {
    use super::Q8Fixture;
    use rust_model_inference::{
        parse_placement, BackendError, BackendKind, CompiledModel, ComponentId,
        ComponentRequirements, ComponentWorkload, DeviceDescriptor, DeviceDiscovery, DevicePlan,
        DeviceProvider, DeviceRegistry, GGMLType, KvCacheType, LlmRequirements, MemoryPlan,
        MetaValue, PlacementCompiler, ProgramId, ProgramKind, ProgramPlan, ResidentTensorPlan,
        RunParams, SlotId, SlotKind, SlotPlan, SlotStorage, SourceFormat, SourceTensorRecord,
        TensorCatalog, TensorId, TensorInfo, TensorSource,
    };
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    fn provider(backend: BackendKind) -> Result<Arc<dyn DeviceProvider>, BackendError> {
        match backend {
            BackendKind::Cpu => Ok(Arc::new(rust_model_inference::compute::CpuProvider::new(2))),
            #[cfg(feature = "vulkan")]
            BackendKind::Vulkan => Ok(Arc::new(
                rust_model_inference::compute::VulkanProvider::new()?,
            )),
            #[cfg(all(target_os = "macos", feature = "metal"))]
            BackendKind::Metal => Ok(Arc::new(
                rust_model_inference::compute::MetalProvider::new()?
            )),
            _ => Err(BackendError::BackendUnavailable { backend }),
        }
    }

    struct FixtureSource {
        info: TensorInfo,
        bytes: Vec<u8>,
    }

    impl TensorSource for FixtureSource {
        fn metadata(&self, _key: &str) -> Option<&MetaValue> {
            None
        }

        fn tensor_info(&self, name: &str) -> Option<&TensorInfo> {
            (name == self.info.name).then_some(&self.info)
        }

        fn tensor_slice(&self, name: &str) -> Option<&[u8]> {
            (name == self.info.name).then_some(self.bytes.as_slice())
        }

        fn source_format(&self) -> SourceFormat {
            SourceFormat::Gguf
        }

        fn tensor_records(&self) -> Vec<SourceTensorRecord> {
            vec![SourceTensorRecord {
                info: self.info.clone(),
                segment_id: 0,
                segment_byte_range: 0..self.bytes.len() as u64,
                layer: None,
            }]
        }
    }

    struct PlannerSource {
        records: Vec<SourceTensorRecord>,
        bytes: BTreeMap<String, Vec<u8>>,
    }

    impl TensorSource for PlannerSource {
        fn metadata(&self, _key: &str) -> Option<&MetaValue> {
            None
        }

        fn tensor_info(&self, name: &str) -> Option<&TensorInfo> {
            self.records
                .iter()
                .find(|record| record.info.name == name)
                .map(|record| &record.info)
        }

        fn tensor_slice(&self, name: &str) -> Option<&[u8]> {
            self.bytes.get(name).map(Vec::as_slice)
        }

        fn source_format(&self) -> SourceFormat {
            SourceFormat::Gguf
        }

        fn tensor_records(&self) -> Vec<SourceTensorRecord> {
            self.records.clone()
        }
    }

    struct DescriptorDiscovery(DeviceDescriptor);

    impl DeviceDiscovery for DescriptorDiscovery {
        fn backend(&self) -> BackendKind {
            self.0.backend
        }

        fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, BackendError> {
            Ok(vec![self.0.clone()])
        }
    }

    struct PlannerFixture {
        catalog: Arc<TensorCatalog>,
        requirements: ComponentRequirements,
        matrix: TensorId,
        embedding: TensorId,
    }

    fn planner_fixture(fixture: &Q8Fixture) -> PlannerFixture {
        let tensors = [
            (
                "token_embd.weight",
                vec![fixture.n_in as u64, fixture.n_out as u64],
                GGMLType::Q8_0,
                fixture.weight_bytes.clone(),
            ),
            (
                "matrix.weight",
                vec![fixture.n_in as u64, fixture.n_out as u64],
                GGMLType::Q8_0,
                fixture.weight_bytes.clone(),
            ),
            (
                "output_norm.weight",
                vec![fixture.n_in as u64],
                GGMLType::F32,
                vec![0; fixture.n_in * size_of::<f32>()],
            ),
            (
                "output.weight",
                vec![fixture.n_in as u64, fixture.n_out as u64],
                GGMLType::Q8_0,
                fixture.weight_bytes.clone(),
            ),
        ];
        let mut offset = 0_u64;
        let mut records = Vec::with_capacity(tensors.len());
        let mut bytes = BTreeMap::new();
        for (name, dims, ggml_type, tensor_bytes) in tensors {
            let len = tensor_bytes.len() as u64;
            records.push(SourceTensorRecord {
                info: TensorInfo {
                    name: name.into(),
                    dims,
                    ggml_type,
                    offset,
                },
                segment_id: 0,
                segment_byte_range: offset..offset + len,
                layer: None,
            });
            bytes.insert(name.into(), tensor_bytes);
            offset += len;
        }
        let catalog = Arc::new(
            TensorCatalog::from_sources(vec![(
                ComponentId::Llm,
                Arc::new(PlannerSource { records, bytes }),
            )])
            .unwrap(),
        );
        let matrix = catalog.find(ComponentId::Llm, "matrix.weight").unwrap();
        let embedding = catalog.find(ComponentId::Llm, "token_embd.weight").unwrap();
        let requirements = ComponentRequirements {
            component: ComponentId::Llm,
            workload: ComponentWorkload::Llm(LlmRequirements {
                layers: Vec::new(),
                hidden_size: fixture.n_in as u32,
                context_length: 8,
                max_batch_tokens: fixture.batch as u32,
                kv_cache: KvCacheType::F16,
                final_norm: catalog
                    .find(ComponentId::Llm, "output_norm.weight")
                    .unwrap(),
                output: catalog.find(ComponentId::Llm, "output.weight").unwrap(),
                norm_epsilon_bits: 1e-6_f32.to_bits(),
            }),
        };
        PlannerFixture {
            catalog,
            requirements,
            matrix,
            embedding,
        }
    }

    fn compile_row_plan(
        fixture: &PlannerFixture,
        registry: &DeviceRegistry,
        placement: &str,
    ) -> rust_model_inference::ExecutionPlan {
        let rule = parse_placement(placement).unwrap();
        PlacementCompiler {
            catalog: &fixture.catalog,
            registry,
            requirements: std::slice::from_ref(&fixture.requirements),
        }
        .compile(&BTreeMap::from([(ComponentId::Llm, rule)]))
        .unwrap()
    }

    fn registered_registry(
        backend: BackendKind,
    ) -> Result<(Arc<DeviceRegistry>, Arc<dyn DeviceProvider>), BackendError> {
        let cpu = provider(BackendKind::Cpu)?;
        let selected = provider(backend)?;
        let mut registry = DeviceRegistry::new();
        registry.register_provider(cpu)?;
        if backend != BackendKind::Cpu {
            registry.register_provider(Arc::clone(&selected))?;
        }
        let requested = if backend == BackendKind::Cpu {
            BTreeSet::from([BackendKind::Cpu])
        } else {
            BTreeSet::from([BackendKind::Cpu, backend])
        };
        registry.discover(&requested)?;
        Ok((Arc::new(registry), selected))
    }

    fn assert_q8_public_input_abi(
        plan: &rust_model_inference::ExecutionPlan,
        tensor: TensorId,
        batch: usize,
        n_in: usize,
    ) {
        for shard in &plan.components[&ComponentId::Llm].row_shards[&tensor] {
            let device = &plan.devices[&shard.device];
            let input = &device.slots[shard.input.0 as usize];
            assert!(matches!(
                input.kind,
                SlotKind::Activation | SlotKind::Scratch
            ));
            assert_eq!(input.storage, SlotStorage::F32);
            assert_eq!(input.byte_len, (batch * n_in * size_of::<f32>()) as u64);
            assert!(device.slots.iter().all(|slot| {
                device.programs.iter().any(|program| {
                    program.input == slot.id
                        || program.output == slot.id
                        || program.layer_ops.iter().any(|op| {
                            matches!(
                                op,
                                rust_model_inference::LayerOp::Q8Matmul { input, output, .. }
                                    if *input == slot.id || *output == slot.id
                            )
                        })
                })
            }));
        }
    }

    pub fn run_planner_q8_backend(
        backend: BackendKind,
        fixture: &Q8Fixture,
    ) -> Result<Vec<f32>, BackendError> {
        let planner = planner_fixture(fixture);
        let (registry, _) = registered_registry(backend)?;
        let placement = if backend == BackendKind::Cpu {
            "llm:row=cpu0@1".to_owned()
        } else {
            format!("llm:row=cpu0@1,{}@1", backend_name(backend))
        };
        let plan = compile_row_plan(&planner, &registry, &placement);
        assert_q8_public_input_abi(&plan, planner.matrix, fixture.batch, fixture.n_in);
        let model = CompiledModel::compile(Arc::clone(&planner.catalog), plan, registry)?;
        let mut actual = vec![0.0; fixture.batch * fixture.n_out];
        model.start_run()?.execute_q8(
            ComponentId::Llm,
            planner.matrix,
            &fixture.input,
            fixture.batch as u32,
            &mut actual,
        )?;
        Ok(actual)
    }

    pub fn run_planner_embedding_backend(
        backend: BackendKind,
        fixture: &Q8Fixture,
        token_ids: &[u32],
    ) -> Result<Vec<f32>, BackendError> {
        let planner = planner_fixture(fixture);
        let target = provider(backend)?;
        let descriptor = target
            .enumerate()?
            .into_iter()
            .next()
            .ok_or(BackendError::BackendUnavailable { backend })?;
        let mut planning_descriptor = descriptor.clone();
        planning_descriptor.id = rust_model_inference::DeviceId::parse("cpu0").unwrap();
        planning_descriptor.backend = BackendKind::Cpu;
        planning_descriptor.physical_key = "planner-cpu0".into();
        planning_descriptor
            .capabilities
            .tensor_types
            .insert(GGMLType::F32);
        let mut registry = DeviceRegistry::new();
        registry.register_discovery(Arc::new(DescriptorDiscovery(planning_descriptor)))?;
        registry.discover(&BTreeSet::from([BackendKind::Cpu]))?;
        let mut plan = compile_row_plan(&planner, &registry, "llm:row=cpu0@1");
        let binding = plan.components[&ComponentId::Llm]
            .embedding
            .clone()
            .ok_or(BackendError::InvalidHandle)?;
        let mut device_plan = plan
            .devices
            .remove(&rust_model_inference::DeviceId::parse("cpu0").unwrap())
            .ok_or(BackendError::InvalidHandle)?;
        device_plan.descriptor = descriptor.clone();
        device_plan
            .tensors
            .retain(|resident| resident.tensor == planner.embedding);
        device_plan
            .slots
            .retain(|slot| slot.id == binding.input || slot.id == binding.output);
        device_plan
            .programs
            .retain(|program| program.id == binding.program);
        let mut session = target.open(&descriptor, &device_plan, Arc::clone(&planner.catalog))?;
        let fence = session.submit(
            binding.program,
            &RunParams {
                token_count: token_ids.len() as u32,
                position_start: 0,
                mrope_positions: &[],
                token_ids,
            },
        )?;
        session.wait(fence)?;
        let mut actual = vec![0.0; token_ids.len() * fixture.n_in];
        session.read_f32(binding.output, &mut actual)?;
        Ok(actual)
    }

    pub fn embedding_expected(fixture: &Q8Fixture, token_ids: &[u32]) -> Vec<f32> {
        let blocks_per_row = fixture.n_in / 32;
        let mut expected = vec![0.0; token_ids.len() * fixture.n_in];
        for (item, &token) in token_ids.iter().enumerate() {
            for column in 0..fixture.n_in {
                let block = column / 32;
                let lane = column % 32;
                let weight = &fixture.weight_blocks[token as usize * blocks_per_row + block];
                expected[item * fixture.n_in + column] = weight.scale * weight.qs[lane] as f32;
            }
        }
        expected
    }

    fn backend_name(backend: BackendKind) -> &'static str {
        match backend {
            BackendKind::Cpu => "cpu0",
            BackendKind::Vulkan => "vulkan0",
            BackendKind::Metal => "metal0",
            BackendKind::Npu => panic!("NPU is outside the GPU fixture"),
        }
    }

    pub fn require_backend(name: &str) {
        let backend = match name {
            "vulkan" => BackendKind::Vulkan,
            "metal" => BackendKind::Metal,
            _ => panic!("unknown backend {name}"),
        };
        let required = std::env::var("RMI_REQUIRE_BACKEND").ok();
        assert!(required.as_deref().is_none_or(|value| value == name));
        let provider = provider(backend).unwrap_or_else(|error| {
            panic!("required {name} backend initialization failed: {error}")
        });
        let adapters = provider
            .enumerate()
            .unwrap_or_else(|error| panic!("required {name} discovery failed: {error}"));
        assert!(
            !adapters.is_empty(),
            "required {name} adapter is unavailable"
        );
    }

    pub fn run_q8_backend(
        backend: BackendKind,
        fixture: &Q8Fixture,
    ) -> Result<Vec<f32>, BackendError> {
        let provider = provider(backend)?;
        let descriptor = provider
            .enumerate()?
            .into_iter()
            .next()
            .ok_or(BackendError::BackendUnavailable { backend })?;
        let catalog = Arc::new(
            TensorCatalog::from_sources(vec![(
                ComponentId::Llm,
                Arc::new(FixtureSource {
                    info: TensorInfo {
                        name: "weight".into(),
                        dims: vec![fixture.n_in as u64, fixture.n_out as u64],
                        ggml_type: GGMLType::Q8_0,
                        offset: 0,
                    },
                    bytes: fixture.weight_bytes.clone(),
                }),
            )])
            .map_err(|_| BackendError::InvalidHandle)?,
        );
        let row_bytes = (fixture.n_in / 32 * 34) as u64;
        let resident_bytes = fixture.rows.len() as u64 * row_bytes;
        let activation_bytes = (fixture.batch * fixture.n_in * size_of::<f32>()) as u64;
        let result_bytes = (fixture.batch * fixture.rows.len() * size_of::<f32>()) as u64;
        let plan = DevicePlan {
            descriptor: descriptor.clone(),
            tensors: vec![ResidentTensorPlan {
                tensor: TensorId(0),
                rows: fixture.rows.clone(),
                source_bytes: fixture.rows.start as u64 * row_bytes
                    ..fixture.rows.end as u64 * row_bytes,
                arena_offset: 0,
            }],
            slots: vec![
                SlotPlan {
                    id: SlotId(0),
                    kind: SlotKind::Activation,
                    storage: SlotStorage::F32,
                    byte_len: activation_bytes,
                    alignment: descriptor.buffer_alignment,
                    arena_offset: 0,
                },
                SlotPlan {
                    id: SlotId(1),
                    kind: SlotKind::Result,
                    storage: SlotStorage::F32,
                    byte_len: result_bytes,
                    alignment: descriptor.buffer_alignment,
                    arena_offset: activation_bytes,
                },
            ],
            programs: vec![ProgramPlan {
                id: ProgramId(0),
                kind: ProgramKind::Q8Rows {
                    tensor: TensorId(0),
                    rows: fixture.rows.clone(),
                    batch_capacity: fixture.batch as u32,
                },
                input: SlotId(0),
                output: SlotId(1),
                layer_ops: Vec::new(),
            }],
            memory: MemoryPlan {
                resident_bytes,
                scratch_bytes: activation_bytes + result_bytes,
                staging_bytes: resident_bytes.max(activation_bytes).max(result_bytes),
                required_bytes: resident_bytes * 2 + activation_bytes + result_bytes,
                largest_allocation_bytes: resident_bytes.max(activation_bytes + result_bytes),
                ..MemoryPlan::default()
            },
        };
        let mut session = provider.open(&descriptor, &plan, Arc::clone(&catalog))?;
        let opened = session.stats();
        assert_eq!(opened.weight_uploads, 1);
        assert_eq!(opened.weight_upload_bytes, resident_bytes);
        assert!(opened.resident_allocations > 0);
        assert_eq!(opened.submissions, 0);
        assert_eq!(opened.host_waits, 0);
        session.write_f32(SlotId(0), &fixture.input)?;
        let fence = session.submit(
            ProgramId(0),
            &RunParams {
                token_count: fixture.batch as u32,
                position_start: 0,
                mrope_positions: &[],
                token_ids: &[],
            },
        )?;
        session.wait(fence)?;
        let mut actual = vec![0.0; fixture.expected.len()];
        session.read_f32(SlotId(1), &mut actual)?;
        let completed = session.stats();
        assert_eq!(completed.weight_uploads, opened.weight_uploads);
        assert_eq!(completed.resident_allocations, opened.resident_allocations);
        assert_eq!(completed.submissions, 1);
        assert_eq!(completed.host_waits, 1);
        Ok(actual)
    }
}

#[cfg(all(target_os = "macos", feature = "metal"))]
pub use gpu::{embedding_expected, run_planner_embedding_backend};
#[cfg(any(feature = "vulkan", all(target_os = "macos", feature = "metal")))]
pub use gpu::{require_backend, run_planner_q8_backend, run_q8_backend};
