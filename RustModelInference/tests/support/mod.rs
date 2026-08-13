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
