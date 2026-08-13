#![allow(dead_code)]

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

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

#[derive(Default)]
pub struct ProviderProbes {
    pub vulkan_discover: Arc<AtomicUsize>,
    pub vulkan_open: Arc<AtomicUsize>,
    pub metal_discover: Arc<AtomicUsize>,
    pub metal_open: Arc<AtomicUsize>,
}

pub fn assert_gpu_probes(probes: &ProviderProbes, expected: (usize, usize, usize, usize)) {
    assert_eq!(
        (
            probes.vulkan_discover.load(Ordering::SeqCst),
            probes.vulkan_open.load(Ordering::SeqCst),
            probes.metal_discover.load(Ordering::SeqCst),
            probes.metal_open.load(Ordering::SeqCst),
        ),
        expected
    );
}

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
    pub fn catalog_arc(&self) -> Arc<TensorCatalog> {
        Arc::clone(&self.catalog)
    }

    pub fn catalog(&self) -> &TensorCatalog {
        &self.catalog
    }

    pub fn llm_source(&self) -> Arc<dyn TensorSource> {
        self.catalog
            .source(ComponentId::Llm)
            .expect("fixture always has an LLM source")
            .clone()
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

pub fn tiny_qwen3_source_with_architecture(architecture: Option<&str>) -> Arc<dyn TensorSource> {
    let mut metadata = model_metadata("qwen3", 17, 2, 4, 2, 96, 64);
    match architecture {
        Some(architecture) => {
            metadata.insert(
                "general.architecture".into(),
                rust_model_inference::MetaValue::String(architecture.into()),
            );
        }
        None => {
            metadata.remove("general.architecture");
        }
    }
    fixture_catalog(qwen3_tensors(), metadata)
        .source(ComponentId::Llm)
        .unwrap()
        .clone()
}

pub fn empty_vision_source() -> Arc<dyn TensorSource> {
    Arc::new(FixtureSource {
        metadata: BTreeMap::new(),
        records: Vec::new(),
        bytes: BTreeMap::new(),
    })
}

struct ProbeProvider {
    descriptor: DeviceDescriptor,
    discovers: Arc<AtomicUsize>,
    opens: Arc<AtomicUsize>,
}

impl DeviceDiscovery for ProbeProvider {
    fn backend(&self) -> BackendKind {
        self.descriptor.backend
    }

    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, BackendError> {
        self.discovers.fetch_add(1, Ordering::SeqCst);
        Ok(vec![self.descriptor.clone()])
    }
}

impl DeviceProvider for ProbeProvider {
    fn open(
        &self,
        _descriptor: &DeviceDescriptor,
        _plan: &DevicePlan,
        _catalog: Arc<TensorCatalog>,
    ) -> Result<Box<dyn DeviceSession>, BackendError> {
        self.opens.fetch_add(1, Ordering::SeqCst);
        Err(BackendError::InvalidHandle)
    }
}

fn probe_descriptor(id: &str, backend: BackendKind) -> DeviceDescriptor {
    DeviceDescriptor {
        id: DeviceId::parse(id).unwrap(),
        backend,
        physical_key: id.into(),
        name: id.into(),
        usable_bytes: 1 << 30,
        max_allocation_bytes: 1 << 30,
        buffer_alignment: 16,
        unified_memory: false,
        capabilities: DeviceCapabilities {
            components: BTreeSet::from([ComponentId::Llm]),
            modes: BTreeSet::from([rust_model_inference::PlacementMode::Layer]),
            layer_families: BTreeSet::from([
                rust_model_inference::LayerFamily::Qwen3,
                rust_model_inference::LayerFamily::Qwen35Dense,
                rust_model_inference::LayerFamily::Qwen35Recurrent,
            ]),
            tensor_types: BTreeSet::from([GGMLType::F32, GGMLType::Q8_0]),
        },
    }
}

/// Compiles the production planner with probes on optional GPU providers.
/// Probe providers deliberately cannot open: successful CPU-only compilation
/// proves neither optional backend was discovered or opened.
pub fn compile_fixture_with_probes(
    fixture: &PlacementFixture,
    placements: &[&str],
    probes: &ProviderProbes,
) -> Result<(), String> {
    let mut registry = DeviceRegistry::new();
    registry
        .register_provider(Arc::new(rust_model_inference::compute::CpuProvider::new(1)))
        .map_err(|error| error.to_string())?;
    registry
        .register_provider(Arc::new(ProbeProvider {
            descriptor: probe_descriptor("vulkan0", BackendKind::Vulkan),
            discovers: Arc::clone(&probes.vulkan_discover),
            opens: Arc::clone(&probes.vulkan_open),
        }))
        .map_err(|error| error.to_string())?;
    registry
        .register_provider(Arc::new(ProbeProvider {
            descriptor: probe_descriptor("metal0", BackendKind::Metal),
            discovers: Arc::clone(&probes.metal_discover),
            opens: Arc::clone(&probes.metal_open),
        }))
        .map_err(|error| error.to_string())?;
    rust_model_inference::compile_model_with_registered_providers(
        vec![(ComponentId::Llm, fixture.llm_source())],
        &rust_model_inference::ExecutionOptions {
            placements: placements.iter().map(|value| (*value).to_owned()).collect(),
            thread_count: 1,
            max_batch_tokens: 1,
            ..Default::default()
        },
        registry,
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

pub struct ModelSourceFixture {
    dir: PathBuf,
    pub gguf: PathBuf,
    pub ggufrs: PathBuf,
}

impl Drop for ModelSourceFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

static MODEL_SOURCE_ID: AtomicUsize = AtomicUsize::new(0);

fn fixture_bytes(dims: &[u64], ggml_type: GGMLType) -> Vec<u8> {
    let count = dims.iter().product::<u64>();
    let (block_elements, block_bytes) = ggml_type.type_traits();
    let mut bytes = vec![0; (count / block_elements as u64 * block_bytes as u64) as usize];
    match ggml_type {
        GGMLType::Q8_0 => {
            for block in bytes.chunks_exact_mut(34) {
                block[..2].copy_from_slice(&half::f16::from_f32(1.0).to_bits().to_le_bytes());
                block[2..].fill(1);
            }
        }
        GGMLType::F32 => {
            for value in bytes.chunks_exact_mut(4) {
                value.copy_from_slice(&1.0_f32.to_le_bytes());
            }
        }
        _ => unreachable!("fixture only writes F32 and Q8_0 model tensors"),
    }
    bytes
}

fn put_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn put_i32(output: &mut Vec<u8>, value: i32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn put_string(output: &mut Vec<u8>, value: &str) {
    put_u64(output, value.len() as u64);
    output.extend_from_slice(value.as_bytes());
}

fn put_metadata_value(output: &mut Vec<u8>, value: &rust_model_inference::MetaValue) {
    use rust_model_inference::MetaValue;
    match value {
        MetaValue::Uint32(value) => put_u32(output, *value),
        MetaValue::Float32(value) => output.extend_from_slice(&value.to_le_bytes()),
        MetaValue::String(value) => put_string(output, value),
        MetaValue::Array(element_type, values) => {
            put_i32(output, *element_type as i32);
            put_u64(output, values.len() as u64);
            for value in values {
                put_metadata_value(output, value);
            }
        }
        value => panic!("unsupported tiny GGUF metadata {value:?}"),
    }
}

fn metadata_type(value: &rust_model_inference::MetaValue) -> rust_model_inference::MetaValueType {
    use rust_model_inference::{MetaValue, MetaValueType};
    match value {
        MetaValue::Uint32(_) => MetaValueType::Uint32,
        MetaValue::Float32(_) => MetaValueType::Float32,
        MetaValue::String(_) => MetaValueType::String,
        MetaValue::Array(_, _) => MetaValueType::Array,
        value => panic!("unsupported tiny GGUF metadata {value:?}"),
    }
}

fn write_tiny_gguf(
    path: &Path,
    metadata: BTreeMap<String, rust_model_inference::MetaValue>,
    tensors: Vec<(String, Vec<u64>, GGMLType)>,
) {
    let tensors = tensors
        .into_iter()
        .map(|(name, dims, ggml_type)| {
            let bytes = fixture_bytes(&dims, ggml_type);
            (name, dims, ggml_type, bytes)
        })
        .collect::<Vec<_>>();
    let mut output = Vec::new();
    output.extend_from_slice(b"GGUF");
    put_u32(&mut output, 3);
    put_u64(&mut output, tensors.len() as u64);
    put_u64(&mut output, metadata.len() as u64);
    for (key, value) in metadata {
        put_string(&mut output, &key);
        put_i32(&mut output, metadata_type(&value) as i32);
        put_metadata_value(&mut output, &value);
    }
    let mut offset = 0_u64;
    for (name, dims, ggml_type, bytes) in &tensors {
        put_string(&mut output, name);
        put_u32(&mut output, dims.len() as u32);
        for dimension in dims {
            put_u64(&mut output, *dimension);
        }
        put_i32(&mut output, *ggml_type as i32);
        put_u64(&mut output, offset);
        offset = (offset + bytes.len() as u64 + 31) & !31;
    }
    output.resize((output.len() + 31) & !31, 0);
    for (_, _, _, bytes) in tensors {
        output.extend_from_slice(&bytes);
        output.resize((output.len() + 31) & !31, 0);
    }
    std::fs::write(path, output).unwrap();
}

fn model_source_fixture(
    mut metadata: BTreeMap<String, rust_model_inference::MetaValue>,
    tensors: Vec<(String, Vec<u64>, GGMLType)>,
    name: &str,
) -> ModelSourceFixture {
    metadata.insert(
        "general.alignment".into(),
        rust_model_inference::MetaValue::Uint32(32),
    );
    let id = MODEL_SOURCE_ID.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("rmi-task13-{name}-{}-{id}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let gguf = dir.join(format!("{name}.gguf"));
    let ggufrs = dir.join(format!("{name}.ggufrs"));
    write_tiny_gguf(&gguf, metadata, tensors);
    rust_model_inference::export_ggufrs(
        &ggufrs,
        &gguf,
        None,
        rust_model_inference::ExportOptions::default(),
    )
    .unwrap();
    ModelSourceFixture { dir, gguf, ggufrs }
}

pub fn tiny_qwen3_sources() -> ModelSourceFixture {
    model_source_fixture(
        model_metadata("qwen3", 17, 2, 4, 2, 96, 64),
        qwen3_tensors(),
        "qwen3",
    )
}

pub fn tiny_qwen35_sources() -> ModelSourceFixture {
    let mut metadata = model_metadata("qwen35", 19, 1, 4, 2, 96, 64);
    metadata.extend(qwen35_dense_metadata());
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
    metadata.insert(
        "tokenizer.ggml.tokens".into(),
        rust_model_inference::MetaValue::Array(
            rust_model_inference::MetaValueType::String,
            (0..64)
                .map(|index| rust_model_inference::MetaValue::String(index.to_string()))
                .collect(),
        ),
    );
    let mut tensors = vec![
        ("token_embd.weight".into(), vec![64, 64], GGMLType::Q8_0),
        ("output_norm.weight".into(), vec![64], GGMLType::F32),
        ("output.weight".into(), vec![64, 64], GGMLType::Q8_0),
    ];
    tensors.extend(qwen35_dense_tensors(0));
    model_source_fixture(metadata, tensors, "qwen35")
}

pub struct FixturePromptRun {
    pub logits: Vec<f32>,
    pub tokens: Vec<u32>,
}

fn selected_token(logits: &[f32]) -> u32 {
    logits
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.total_cmp(right.1))
        .map(|(index, _)| index as u32)
        .unwrap()
}

pub fn run_fixture_prompt(path: &Path) -> Result<FixturePromptRun, String> {
    let sources =
        rust_model_inference::load_model_sources(path, None).map_err(|error| error.to_string())?;
    let (compiled, runner) = rust_model_inference::compile_model(
        sources,
        &rust_model_inference::ExecutionOptions {
            placements: vec!["llm:layer=cpu0@1".into()],
            thread_count: 1,
            max_batch_tokens: 1,
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?;
    let mut run = compiled.start_run().map_err(|error| error.to_string())?;
    match runner {
        rust_model_inference::QwenRunner::Qwen3(model) => {
            let mut logits = vec![0.0; model.config.vocab];
            model
                .forward(&mut run, &[1], &[[0, 0, 0, 0]], &mut logits)
                .map_err(|error| error.to_string())?;
            let first = selected_token(&logits);
            model
                .forward(&mut run, &[first], &[[1, 1, 1, 0]], &mut logits)
                .map_err(|error| error.to_string())?;
            let second = selected_token(&logits);
            Ok(FixturePromptRun {
                logits,
                tokens: vec![first, second],
            })
        }
        rust_model_inference::QwenRunner::Qwen35(model) => {
            let mut logits = vec![0.0; model.config.vocab_size];
            model
                .forward_compiled(&mut run, &[1], &[[0, 0, 0, 0]], &mut logits)
                .map_err(|error| error.to_string())?;
            let first = selected_token(&logits);
            model
                .forward_compiled(&mut run, &[first], &[[1, 1, 1, 0]], &mut logits)
                .map_err(|error| error.to_string())?;
            let second = selected_token(&logits);
            Ok(FixturePromptRun {
                logits,
                tokens: vec![first, second],
            })
        }
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

pub fn qwen35_recurrent_contract_error(case: &str) -> Option<String> {
    let fixture = tiny_qwen35_hybrid();
    let source = fixture.llm_source();
    let mut records = source.tensor_records();
    let mut bytes = BTreeMap::new();
    let mut metadata = BTreeMap::new();
    let mut replace = |name: &str, dims: Vec<u64>, ggml_type: GGMLType| {
        let record = records
            .iter_mut()
            .find(|record| record.info.name == name)
            .expect("fixture recurrent tensor exists");
        let data = fixture_bytes(&dims, ggml_type);
        record.info.dims = dims;
        record.info.ggml_type = ggml_type;
        record.segment_byte_range.end = record.segment_byte_range.start + data.len() as u64;
        bytes.insert(name.to_owned(), data);
    };

    match case {
        "metadata" => {
            metadata.insert(
                "qwen35.ssm.inner_size".into(),
                rust_model_inference::MetaValue::Uint32(64),
            );
            replace("blk.1.attn_qkv.weight", vec![64, 128], GGMLType::Q8_0);
            replace("blk.1.attn_gate.weight", vec![64, 64], GGMLType::Q8_0);
            replace("blk.1.ssm_norm.weight", vec![64], GGMLType::F32);
            replace("blk.1.ssm_out.weight", vec![64, 64], GGMLType::Q8_0);
        }
        "qkv-shape" => replace("blk.1.attn_qkv.weight", vec![64, 64], GGMLType::Q8_0),
        "gate-shape" => replace("blk.1.attn_gate.weight", vec![64, 64], GGMLType::Q8_0),
        "beta-shape" => replace("blk.1.ssm_beta.weight", vec![64, 2], GGMLType::Q8_0),
        "alpha-shape" => replace("blk.1.ssm_alpha.weight", vec![64, 2], GGMLType::Q8_0),
        "conv-shape" => replace("blk.1.ssm_conv1d.weight", vec![380], GGMLType::F32),
        "dt-bias-shape" => replace("blk.1.ssm_dt.bias", vec![2], GGMLType::F32),
        "ssm-a-shape" => replace("blk.1.ssm_a", vec![2], GGMLType::F32),
        "ssm-norm-shape" => replace("blk.1.ssm_norm.weight", vec![64], GGMLType::F32),
        "ssm-output-shape" => replace("blk.1.ssm_out.weight", vec![64, 64], GGMLType::Q8_0),
        "qkv-format" => replace("blk.1.attn_qkv.weight", vec![64, 96], GGMLType::F32),
        _ => panic!("unknown recurrent contract case: {case}"),
    }

    let catalog = TensorCatalog::from_sources(vec![(
        ComponentId::Llm,
        Arc::new(OverrideSource {
            source,
            metadata,
            records,
            bytes,
        }) as Arc<dyn TensorSource>,
    )])
    .unwrap();
    rust_model_inference::Qwen35Model::from_catalog(&catalog).err()
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

struct OverrideSource {
    source: Arc<dyn TensorSource>,
    metadata: BTreeMap<String, rust_model_inference::MetaValue>,
    records: Vec<SourceTensorRecord>,
    bytes: BTreeMap<String, Vec<u8>>,
}

impl TensorSource for OverrideSource {
    fn metadata(&self, key: &str) -> Option<&rust_model_inference::MetaValue> {
        self.metadata.get(key).or_else(|| self.source.metadata(key))
    }

    fn tensor_info(&self, name: &str) -> Option<&TensorInfo> {
        self.records
            .iter()
            .find(|record| record.info.name == name)
            .map(|record| &record.info)
    }

    fn tensor_slice(&self, name: &str) -> Option<&[u8]> {
        self.bytes
            .get(name)
            .map(Vec::as_slice)
            .or_else(|| self.source.tensor_slice(name))
    }

    fn source_format(&self) -> SourceFormat {
        self.source.source_format()
    }

    fn tensor_records(&self) -> Vec<SourceTensorRecord> {
        self.records.clone()
    }
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
            vec![4, 96],
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
