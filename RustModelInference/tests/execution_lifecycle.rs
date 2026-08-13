use rust_model_inference::{
    embedding_lookup_q8_0, ActivationTransfer, BackendError, BackendKind, CompiledModel,
    ComponentId, ComponentPlan, DeviceCapabilities, DeviceDescriptor, DeviceDiscovery, DeviceId,
    DevicePlan, DeviceProvider, DeviceRegistry, DeviceSession, ExecutionPlan, FenceId, GGMLType,
    LayerFamily, LayerSpan, LifecycleProbe, MemoryPlan, MetaValue, PlacementMode, ProgramBinding,
    ProgramId, ProgramKind, ProgramPlan, ResidentTensorPlan, RowShard, RunParams, SessionStats,
    SlotId, SlotKind, SlotPlan, SlotStorage, SourceFormat, SourceTensorRecord, TensorCatalog,
    TensorId, TensorInfo, TensorSource, TransferTarget,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const ROW_WEIGHT: TensorId = TensorId(0);
const EMBEDDING: TensorId = TensorId(1);
const FINAL_NORM: TensorId = TensorId(2);
const LOGITS: TensorId = TensorId(3);
const HIDDEN: usize = 32;

fn id(value: &str) -> DeviceId {
    DeviceId::parse(value).unwrap()
}

fn backend(value: &str) -> BackendKind {
    if value.starts_with("cpu") {
        BackendKind::Cpu
    } else if value.starts_with("vulkan") {
        BackendKind::Vulkan
    } else {
        BackendKind::Metal
    }
}

fn descriptor(value: &str, physical_key: &str) -> DeviceDescriptor {
    let backend = backend(value);
    DeviceDescriptor {
        id: id(value),
        backend,
        physical_key: physical_key.into(),
        name: value.into(),
        usable_bytes: 1 << 20,
        max_allocation_bytes: 1 << 20,
        buffer_alignment: 16,
        unified_memory: backend == BackendKind::Cpu,
        capabilities: DeviceCapabilities {
            components: BTreeSet::from([ComponentId::Llm]),
            modes: BTreeSet::from([PlacementMode::Layer, PlacementMode::Row]),
            layer_families: BTreeSet::from([LayerFamily::Qwen3]),
            tensor_types: BTreeSet::from([GGMLType::F32, GGMLType::Q8_0]),
        },
    }
}

fn slot(id: u32, kind: SlotKind, values: usize) -> SlotPlan {
    SlotPlan {
        id: SlotId(id),
        kind,
        storage: SlotStorage::F32,
        byte_len: (values * 4) as u64,
        alignment: 16,
        arena_offset: (id as u64) * 4096,
    }
}

fn program(id: u32, kind: ProgramKind, input: u32, output: u32) -> ProgramPlan {
    ProgramPlan {
        id: ProgramId(id),
        kind,
        input: SlotId(input),
        output: SlotId(output),
        layer_ops: Vec::new(),
    }
}

fn resident(tensor: TensorId, rows: Range<u32>, bytes: Range<u64>) -> ResidentTensorPlan {
    ResidentTensorPlan {
        tensor,
        rows,
        source_bytes: bytes,
        arena_offset: 0,
    }
}

fn device_plan(
    descriptor: DeviceDescriptor,
    tensors: Vec<ResidentTensorPlan>,
    slots: Vec<SlotPlan>,
    programs: Vec<ProgramPlan>,
) -> DevicePlan {
    DevicePlan {
        descriptor,
        tensors,
        slots,
        programs,
        memory: MemoryPlan {
            resident_bytes: 4096,
            state_bytes: 256,
            scratch_bytes: 4096,
            staging_bytes: 4096,
            required_bytes: 12_544,
            largest_allocation_bytes: 4096,
        },
    }
}

struct TestSource {
    records: Vec<SourceTensorRecord>,
    bytes: BTreeMap<String, Vec<u8>>,
}

impl TensorSource for TestSource {
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

fn q8_rows(rows: usize, scale_start: f32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(rows * 34);
    for row in 0..rows {
        bytes.extend_from_slice(
            &half::f16::from_f32(scale_start + row as f32)
                .to_bits()
                .to_le_bytes(),
        );
        bytes.extend((0..32).map(|value| (value as i8 - 16) as u8));
    }
    bytes
}

fn test_catalog() -> Arc<TensorCatalog> {
    let tensors = [
        ("row.weight", vec![64, 12], GGMLType::Q8_0, q8_rows(24, 1.0)),
        (
            "token_embd.weight",
            vec![32, 4],
            GGMLType::Q8_0,
            q8_rows(4, 1.0),
        ),
        (
            "output_norm.weight",
            vec![32],
            GGMLType::F32,
            vec![0; 32 * 4],
        ),
        (
            "output.weight",
            vec![32, 4],
            GGMLType::Q8_0,
            q8_rows(4, 1.0),
        ),
    ];
    let mut offset = 0_u64;
    let mut records = Vec::new();
    let mut bytes = BTreeMap::new();
    for (name, dims, ggml_type, data) in tensors {
        let len = data.len() as u64;
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
        bytes.insert(name.into(), data);
        offset += len;
    }
    Arc::new(
        TensorCatalog::from_sources(vec![(
            ComponentId::Llm,
            Arc::new(TestSource { records, bytes }),
        )])
        .unwrap(),
    )
}

#[derive(Default)]
struct MockState {
    trace: Mutex<Vec<String>>,
    probes: Mutex<BTreeMap<DeviceId, Arc<Mutex<SessionStats>>>>,
    opens: AtomicUsize,
    fail_open: Mutex<Option<BackendKind>>,
    fail_submit: Mutex<Option<(DeviceId, ProgramId)>>,
    fail_wait: Mutex<Option<DeviceId>>,
    fail_read: Mutex<Option<DeviceId>>,
}

struct MockProvider {
    backend: BackendKind,
    descriptors: Vec<DeviceDescriptor>,
    state: Arc<MockState>,
}

impl DeviceDiscovery for MockProvider {
    fn backend(&self) -> BackendKind {
        self.backend
    }

    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, BackendError> {
        Ok(self.descriptors.clone())
    }
}

impl DeviceProvider for MockProvider {
    fn open(
        &self,
        descriptor: &DeviceDescriptor,
        plan: &DevicePlan,
        catalog: Arc<TensorCatalog>,
    ) -> Result<Box<dyn DeviceSession>, BackendError> {
        self.state.opens.fetch_add(1, Ordering::SeqCst);
        self.state
            .trace
            .lock()
            .unwrap()
            .push(format!("open:{}", descriptor.id.as_str()));
        if *self.state.fail_open.lock().unwrap() == Some(self.backend) {
            return Err(BackendError::Allocation {
                device: descriptor.id.clone(),
                message: "mock open failure".into(),
            });
        }
        let stats = Arc::new(Mutex::new(SessionStats {
            resident_bytes: plan.memory.resident_bytes,
            resident_allocations: 1,
            weight_uploads: plan.tensors.len() as u64,
            weight_upload_bytes: plan
                .tensors
                .iter()
                .map(|tensor| tensor.source_bytes.end - tensor.source_bytes.start)
                .sum(),
            ..SessionStats::default()
        }));
        self.state
            .probes
            .lock()
            .unwrap()
            .insert(descriptor.id.clone(), Arc::clone(&stats));
        Ok(Box::new(MockSession {
            descriptor: descriptor.clone(),
            programs: plan
                .programs
                .iter()
                .map(|program| (program.id, program.clone()))
                .collect(),
            slots: plan
                .slots
                .iter()
                .map(|slot| (slot.id, vec![0.0; slot.byte_len as usize / 4]))
                .collect(),
            catalog,
            state: Arc::clone(&self.state),
            stats,
            next_fence: 0,
        }))
    }
}

struct MockSession {
    descriptor: DeviceDescriptor,
    programs: BTreeMap<ProgramId, ProgramPlan>,
    slots: BTreeMap<SlotId, Vec<f32>>,
    catalog: Arc<TensorCatalog>,
    state: Arc<MockState>,
    stats: Arc<Mutex<SessionStats>>,
    next_fence: u64,
}

impl MockSession {
    fn trace(&self, event: String) {
        self.state.trace.lock().unwrap().push(event);
    }
}

impl DeviceSession for MockSession {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.descriptor
    }

    fn write_f32(&mut self, slot: SlotId, values: &[f32]) -> Result<(), BackendError> {
        self.trace(format!(
            "write:{}:{}:{}",
            self.descriptor.id.as_str(),
            slot.0,
            values.len() * 4
        ));
        let target = self
            .slots
            .get_mut(&slot)
            .ok_or(BackendError::InvalidHandle)?;
        if values.len() > target.len() {
            return Err(BackendError::InvalidHandle);
        }
        target[..values.len()].copy_from_slice(values);
        self.stats.lock().unwrap().activation_h2d_bytes += (values.len() * 4) as u64;
        Ok(())
    }

    fn submit(
        &mut self,
        program_id: ProgramId,
        params: &RunParams<'_>,
    ) -> Result<FenceId, BackendError> {
        let program = self
            .programs
            .get(&program_id)
            .cloned()
            .ok_or(BackendError::InvalidHandle)?;
        self.trace(format!(
            "submit:{}:{}:{:?}",
            self.descriptor.id.as_str(),
            program_id.0,
            program.kind
        ));
        self.stats.lock().unwrap().submissions += 1;
        if self
            .state
            .fail_submit
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|failure| failure == &(self.descriptor.id.clone(), program_id))
        {
            return Err(BackendError::Submission {
                device: self.descriptor.id.clone(),
                message: "mock submission failure".into(),
            });
        }
        match program.kind {
            ProgramKind::Q8Rows { rows, .. } => {
                let local_rows = (rows.end - rows.start) as usize;
                let output = self
                    .slots
                    .get_mut(&program.output)
                    .ok_or(BackendError::InvalidHandle)?;
                for item in 0..params.token_count as usize {
                    for (local, row) in rows.clone().enumerate() {
                        output[item * local_rows + local] = row as f32;
                    }
                }
            }
            ProgramKind::EmbeddingRows { tensor, row_count } => {
                if params.token_ids.iter().any(|token| *token >= row_count) {
                    return Err(BackendError::InvalidHandle);
                }
                let entry = self
                    .catalog
                    .entry(tensor)
                    .ok_or(BackendError::InvalidHandle)?;
                let hidden = entry.shape[0] as usize;
                let bytes = self
                    .catalog
                    .bytes(tensor)
                    .map_err(|_| BackendError::InvalidHandle)?;
                let output = self
                    .slots
                    .get_mut(&program.output)
                    .ok_or(BackendError::InvalidHandle)?;
                for (row, token) in params.token_ids.iter().copied().enumerate() {
                    embedding_lookup_q8_0(
                        bytes,
                        token,
                        hidden,
                        &mut output[row * hidden..(row + 1) * hidden],
                    );
                }
            }
            ProgramKind::LayerSegment { .. } => {
                let input = self
                    .slots
                    .get(&program.input)
                    .ok_or(BackendError::InvalidHandle)?
                    .clone();
                let output = self
                    .slots
                    .get_mut(&program.output)
                    .ok_or(BackendError::InvalidHandle)?;
                output[..input.len()].copy_from_slice(&input);
            }
            ProgramKind::FinalNormQ8Logits { .. } => {
                let output = self
                    .slots
                    .get_mut(&program.output)
                    .ok_or(BackendError::InvalidHandle)?;
                for (index, value) in output.iter_mut().enumerate() {
                    *value = 1000.0 + index as f32;
                }
            }
        }
        self.next_fence += 1;
        Ok(FenceId(self.next_fence))
    }

    fn wait(&mut self, fence: FenceId) -> Result<(), BackendError> {
        self.trace(format!("wait:{}:{}", self.descriptor.id.as_str(), fence.0));
        self.stats.lock().unwrap().host_waits += 1;
        if self
            .state
            .fail_wait
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|device| device == &self.descriptor.id)
        {
            return Err(BackendError::Submission {
                device: self.descriptor.id.clone(),
                message: "mock wait failure".into(),
            });
        }
        Ok(())
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
        self.trace(format!(
            "read:{}:{}:{}",
            self.descriptor.id.as_str(),
            slot.0,
            values.len() * 4
        ));
        if self
            .state
            .fail_read
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|device| device == &self.descriptor.id)
        {
            return Err(BackendError::Submission {
                device: self.descriptor.id.clone(),
                message: "mock read failure".into(),
            });
        }
        let source = self.slots.get(&slot).ok_or(BackendError::InvalidHandle)?;
        let end = offset
            .checked_add(values.len())
            .filter(|end| *end <= source.len())
            .ok_or(BackendError::InvalidHandle)?;
        values.copy_from_slice(&source[offset..end]);
        self.stats.lock().unwrap().activation_d2h_bytes += (values.len() * 4) as u64;
        Ok(())
    }

    fn reset_state(&mut self) -> Result<(), BackendError> {
        self.trace(format!("reset:{}", self.descriptor.id.as_str()));
        Ok(())
    }

    fn stats(&self) -> SessionStats {
        self.stats.lock().unwrap().clone()
    }

    fn lifecycle_probe(&self) -> LifecycleProbe {
        LifecycleProbe::default()
    }
}

impl Drop for MockSession {
    fn drop(&mut self) {
        let mut stats = self.stats.lock().unwrap();
        stats.resident_frees += stats.resident_allocations;
        drop(stats);
        self.trace(format!("drop:{}", self.descriptor.id.as_str()));
    }
}

fn registry(descriptors: &[DeviceDescriptor], state: Arc<MockState>) -> Arc<DeviceRegistry> {
    let mut grouped = BTreeMap::<BackendKind, Vec<DeviceDescriptor>>::new();
    for descriptor in descriptors {
        grouped
            .entry(descriptor.backend)
            .or_default()
            .push(descriptor.clone());
    }
    let mut registry = DeviceRegistry::new();
    for (backend, descriptors) in grouped {
        registry
            .register_provider(Arc::new(MockProvider {
                backend,
                descriptors,
                state: Arc::clone(&state),
            }))
            .unwrap();
    }
    Arc::new(registry)
}

fn compile(plan: ExecutionPlan, state: Arc<MockState>) -> CompiledModel {
    let descriptors = plan
        .devices
        .values()
        .map(|device| device.descriptor.clone())
        .collect::<Vec<_>>();
    CompiledModel::compile(test_catalog(), plan, registry(&descriptors, state)).unwrap()
}

fn row_plan() -> ExecutionPlan {
    let specs = [
        ("cpu0", "cpu", 0..4, 10),
        ("vulkan0", "vk", 4..8, 20),
        ("metal0", "metal", 8..12, 30),
    ];
    let mut devices = BTreeMap::new();
    let mut shards = Vec::new();
    for (name, physical, rows, program_id) in specs {
        let device = id(name);
        devices.insert(
            device.clone(),
            device_plan(
                descriptor(name, physical),
                vec![resident(ROW_WEIGHT, rows.clone(), 0..272)],
                vec![
                    slot(0, SlotKind::Activation, 64),
                    slot(1, SlotKind::Result, 4),
                ],
                vec![program(
                    program_id,
                    ProgramKind::Q8Rows {
                        tensor: ROW_WEIGHT,
                        rows: rows.clone(),
                        batch_capacity: 1,
                    },
                    0,
                    1,
                )],
            ),
        );
        shards.push(RowShard {
            device,
            rows,
            tensor_bytes: 0..272,
            program: ProgramId(program_id),
            input: SlotId(0),
            output: SlotId(1),
        });
    }
    ExecutionPlan {
        components: BTreeMap::from([(
            ComponentId::Llm,
            ComponentPlan {
                component: ComponentId::Llm,
                mode: PlacementMode::Row,
                primary: id("cpu0"),
                embedding: None,
                finalization: None,
                layer_spans: Vec::new(),
                activation_transfers: Vec::new(),
                row_shards: BTreeMap::from([(ROW_WEIGHT, shards)]),
            },
        )]),
        devices,
    }
}

fn same_primary_embedding_plan() -> ExecutionPlan {
    let cpu = id("cpu0");
    let programs = vec![
        program(
            0,
            ProgramKind::EmbeddingRows {
                tensor: EMBEDDING,
                row_count: 4,
            },
            0,
            1,
        ),
        program(
            1,
            ProgramKind::LayerSegment {
                layers: 0..2,
                families: vec![LayerFamily::Qwen3; 2],
            },
            1,
            2,
        ),
        program(
            2,
            ProgramKind::FinalNormQ8Logits {
                norm: FINAL_NORM,
                output: LOGITS,
                epsilon_bits: 0,
                batch_capacity: 1,
            },
            2,
            3,
        ),
    ];
    ExecutionPlan {
        components: BTreeMap::from([(
            ComponentId::Llm,
            ComponentPlan {
                component: ComponentId::Llm,
                mode: PlacementMode::Layer,
                primary: cpu.clone(),
                embedding: Some(ProgramBinding {
                    device: cpu.clone(),
                    program: ProgramId(0),
                    input: SlotId(0),
                    output: SlotId(1),
                }),
                finalization: Some(ProgramBinding {
                    device: cpu.clone(),
                    program: ProgramId(2),
                    input: SlotId(2),
                    output: SlotId(3),
                }),
                layer_spans: vec![LayerSpan {
                    device: cpu.clone(),
                    layers: 0..2,
                    program: ProgramId(1),
                    input: SlotId(1),
                    output: SlotId(2),
                }],
                activation_transfers: Vec::new(),
                row_shards: BTreeMap::new(),
            },
        )]),
        devices: BTreeMap::from([(
            cpu,
            device_plan(
                descriptor("cpu0", "cpu"),
                vec![resident(EMBEDDING, 0..4, 816..952)],
                vec![
                    slot(0, SlotKind::Scratch, 1),
                    slot(1, SlotKind::Activation, HIDDEN),
                    slot(2, SlotKind::Activation, HIDDEN),
                    slot(3, SlotKind::Result, 4),
                    slot(90, SlotKind::KvState, HIDDEN),
                ],
                programs,
            ),
        )]),
    }
}

fn two_span_failure_plan() -> ExecutionPlan {
    let cpu = id("cpu0");
    let metal = id("metal0");
    ExecutionPlan {
        components: BTreeMap::from([(
            ComponentId::Llm,
            ComponentPlan {
                component: ComponentId::Llm,
                mode: PlacementMode::Layer,
                primary: cpu.clone(),
                embedding: None,
                finalization: None,
                layer_spans: vec![
                    LayerSpan {
                        device: cpu.clone(),
                        layers: 0..1,
                        program: ProgramId(10),
                        input: SlotId(1),
                        output: SlotId(2),
                    },
                    LayerSpan {
                        device: metal.clone(),
                        layers: 1..3,
                        program: ProgramId(20),
                        input: SlotId(3),
                        output: SlotId(4),
                    },
                ],
                activation_transfers: vec![ActivationTransfer {
                    after_span: Some(0),
                    target: TransferTarget::Span(1),
                    from_device: cpu.clone(),
                    from_slot: SlotId(2),
                    to_device: metal.clone(),
                    to_slot: SlotId(3),
                    f32_values_per_token: HIDDEN as u32,
                }],
                row_shards: BTreeMap::new(),
            },
        )]),
        devices: BTreeMap::from([
            (
                cpu,
                device_plan(
                    descriptor("cpu0", "cpu"),
                    Vec::new(),
                    vec![
                        slot(1, SlotKind::Activation, HIDDEN),
                        slot(2, SlotKind::Activation, HIDDEN),
                        slot(90, SlotKind::KvState, HIDDEN),
                    ],
                    vec![program(
                        10,
                        ProgramKind::LayerSegment {
                            layers: 0..1,
                            families: vec![LayerFamily::Qwen3],
                        },
                        1,
                        2,
                    )],
                ),
            ),
            (
                metal,
                device_plan(
                    descriptor("metal0", "metal"),
                    Vec::new(),
                    vec![
                        slot(3, SlotKind::Activation, HIDDEN),
                        slot(4, SlotKind::Activation, HIDDEN),
                        slot(91, SlotKind::KvState, HIDDEN),
                    ],
                    vec![program(
                        20,
                        ProgramKind::LayerSegment {
                            layers: 1..3,
                            families: vec![LayerFamily::Qwen3; 2],
                        },
                        3,
                        4,
                    )],
                ),
            ),
        ]),
    }
}

fn finalization_transfer_plan() -> ExecutionPlan {
    let cpu = id("cpu0");
    let metal = id("metal0");
    ExecutionPlan {
        components: BTreeMap::from([(
            ComponentId::Llm,
            ComponentPlan {
                component: ComponentId::Llm,
                mode: PlacementMode::Layer,
                primary: cpu.clone(),
                embedding: None,
                finalization: Some(ProgramBinding {
                    device: cpu.clone(),
                    program: ProgramId(30),
                    input: SlotId(10),
                    output: SlotId(11),
                }),
                layer_spans: vec![LayerSpan {
                    device: metal.clone(),
                    layers: 0..2,
                    program: ProgramId(40),
                    input: SlotId(20),
                    output: SlotId(21),
                }],
                activation_transfers: vec![ActivationTransfer {
                    after_span: Some(0),
                    target: TransferTarget::Finalization,
                    from_device: metal.clone(),
                    from_slot: SlotId(21),
                    to_device: cpu.clone(),
                    to_slot: SlotId(10),
                    f32_values_per_token: HIDDEN as u32,
                }],
                row_shards: BTreeMap::new(),
            },
        )]),
        devices: BTreeMap::from([
            (
                cpu,
                device_plan(
                    descriptor("cpu0", "cpu"),
                    vec![resident(FINAL_NORM, 0..1, 952..1080)],
                    vec![
                        slot(10, SlotKind::Activation, HIDDEN),
                        slot(11, SlotKind::Result, 4),
                        slot(90, SlotKind::KvState, HIDDEN),
                    ],
                    vec![program(
                        30,
                        ProgramKind::FinalNormQ8Logits {
                            norm: FINAL_NORM,
                            output: LOGITS,
                            epsilon_bits: 0,
                            batch_capacity: 1,
                        },
                        10,
                        11,
                    )],
                ),
            ),
            (
                metal,
                device_plan(
                    descriptor("metal0", "metal"),
                    Vec::new(),
                    vec![
                        slot(20, SlotKind::Activation, HIDDEN),
                        slot(21, SlotKind::Activation, HIDDEN),
                        slot(91, SlotKind::KvState, HIDDEN),
                    ],
                    vec![program(
                        40,
                        ProgramKind::LayerSegment {
                            layers: 0..2,
                            families: vec![LayerFamily::Qwen3; 2],
                        },
                        20,
                        21,
                    )],
                ),
            ),
        ]),
    }
}

fn embedding_row_plan() -> ExecutionPlan {
    let cpu = id("cpu0");
    ExecutionPlan {
        components: BTreeMap::from([(
            ComponentId::Llm,
            ComponentPlan {
                component: ComponentId::Llm,
                mode: PlacementMode::Row,
                primary: cpu.clone(),
                embedding: Some(ProgramBinding {
                    device: cpu.clone(),
                    program: ProgramId(50),
                    input: SlotId(0),
                    output: SlotId(1),
                }),
                finalization: None,
                layer_spans: Vec::new(),
                activation_transfers: Vec::new(),
                row_shards: BTreeMap::new(),
            },
        )]),
        devices: BTreeMap::from([(
            cpu,
            device_plan(
                descriptor("cpu0", "cpu"),
                vec![resident(EMBEDDING, 0..4, 816..952)],
                vec![
                    slot(0, SlotKind::Scratch, 2),
                    slot(1, SlotKind::Activation, HIDDEN * 2),
                ],
                vec![program(
                    50,
                    ProgramKind::EmbeddingRows {
                        tensor: EMBEDDING,
                        row_count: 4,
                    },
                    0,
                    1,
                )],
            ),
        )]),
    }
}

#[test]
fn weights_upload_once_and_row_submits_all_before_any_wait() {
    let state = Arc::new(MockState::default());
    let compiled = compile(row_plan(), Arc::clone(&state));
    assert_eq!(
        *state.trace.lock().unwrap(),
        ["open:cpu0", "open:metal0", "open:vulkan0"]
    );
    let mut run = compiled.start_run().unwrap();
    assert_eq!(run.plan(), compiled.plan());
    let after_compile = run.stats();
    state.trace.lock().unwrap().clear();
    let mut output = vec![0.0; 12];
    run.execute_q8(ComponentId::Llm, ROW_WEIGHT, &[1.0; 64], 1, &mut output)
        .unwrap();
    assert_eq!(
        output,
        (0..12).map(|value| value as f32).collect::<Vec<_>>()
    );
    let after_token = run.stats();
    for device in [id("cpu0"), id("vulkan0"), id("metal0")] {
        assert_eq!(
            after_token[&device].weight_uploads,
            after_compile[&device].weight_uploads
        );
        assert_eq!(
            after_token[&device].resident_allocations,
            after_compile[&device].resident_allocations
        );
    }
    let trace = state.trace.lock().unwrap();
    let first_wait = trace
        .iter()
        .position(|event| event.starts_with("wait:"))
        .unwrap();
    assert_eq!(
        trace[..first_wait]
            .iter()
            .filter(|event| event.starts_with("submit:"))
            .count(),
        3
    );
}

#[test]
fn q8_and_embedding_failures_poison_the_stateful_run() {
    for failure in [
        ("q8 submit", false, true, false),
        ("q8 wait", false, false, true),
        ("embedding read", true, false, false),
    ] {
        let state = Arc::new(MockState::default());
        let compiled = if failure.1 {
            compile(embedding_row_plan(), Arc::clone(&state))
        } else {
            compile(row_plan(), Arc::clone(&state))
        };
        if failure.2 {
            *state.fail_submit.lock().unwrap() = Some((id("cpu0"), ProgramId(10)));
        }
        if failure.3 {
            *state.fail_wait.lock().unwrap() = Some(id("cpu0"));
        }
        if failure.0 == "embedding read" {
            *state.fail_read.lock().unwrap() = Some(id("cpu0"));
        }
        let mut run = compiled.start_run().unwrap();
        let result = if failure.1 {
            let mut output = [0.0; HIDDEN];
            run.execute_embedding(ComponentId::Llm, EMBEDDING, &[1], &mut output)
        } else {
            let mut output = vec![0.0; 12];
            run.execute_q8(ComponentId::Llm, ROW_WEIGHT, &[1.0; 64], 1, &mut output)
        };
        assert!(result.is_err(), "{}", failure.0);
        let retry = if failure.1 {
            let mut output = [0.0; HIDDEN];
            run.execute_embedding(ComponentId::Llm, EMBEDDING, &[1], &mut output)
        } else {
            let mut output = vec![0.0; 12];
            run.execute_q8(ComponentId::Llm, ROW_WEIGHT, &[1.0; 64], 1, &mut output)
        };
        assert!(
            matches!(retry, Err(BackendError::PoisonedRun)),
            "{}",
            failure.0
        );
        assert!(
            matches!(run.reset_state(), Err(BackendError::PoisonedRun)),
            "{}",
            failure.0
        );
    }
}

#[test]
fn same_device_embedding_and_layer_span_stay_resident_until_final_logits() {
    let state = Arc::new(MockState::default());
    let compiled = compile(same_primary_embedding_plan(), Arc::clone(&state));
    let mut run = compiled.start_run().unwrap();
    state.trace.lock().unwrap().clear();
    run.execute_embedding_into_layers(
        ComponentId::Llm,
        EMBEDDING,
        &[1],
        &RunParams {
            token_count: 1,
            position_start: 0,
            mrope_positions: &[[0; 4]],
            token_ids: &[1],
        },
    )
    .unwrap();
    let mut logits = [0.0; 4];
    run.execute_logits(
        ComponentId::Llm,
        &RunParams {
            token_count: 1,
            position_start: 0,
            mrope_positions: &[[0; 4]],
            token_ids: &[1],
        },
        &mut logits,
    )
    .unwrap();
    assert_eq!(logits, [1000.0, 1001.0, 1002.0, 1003.0]);
    let trace = state.trace.lock().unwrap();
    let first_wait = trace
        .iter()
        .position(|event| event.starts_with("wait:"))
        .unwrap();
    assert_eq!(
        trace[..first_wait]
            .iter()
            .filter(|event| event.starts_with("submit:"))
            .count(),
        3
    );
    assert!(!trace[..first_wait]
        .iter()
        .any(|event| event.starts_with("read:")));
    assert!(!trace.iter().any(|event| event.contains(":90:")));
}

#[test]
fn layer_boundary_transfers_only_hidden_and_stateful_failure_poisons_model() {
    let state = Arc::new(MockState::default());
    *state.fail_submit.lock().unwrap() = Some((id("metal0"), ProgramId(20)));
    let compiled = compile(two_span_failure_plan(), Arc::clone(&state));
    let mut run = compiled.start_run().unwrap();
    state.trace.lock().unwrap().clear();
    let error = run
        .execute_layers(
            ComponentId::Llm,
            &mut [0.0; HIDDEN],
            &RunParams {
                token_count: 1,
                position_start: 0,
                mrope_positions: &[[0; 4]],
                token_ids: &[],
            },
        )
        .unwrap_err();
    assert!(matches!(error, BackendError::Submission { .. }));
    assert!(matches!(run.reset_state(), Err(BackendError::PoisonedRun)));
    let trace = state.trace.lock().unwrap();
    assert_eq!(
        trace
            .iter()
            .filter(|event| event.as_str() == "read:cpu0:2:128")
            .count(),
        1
    );
    assert_eq!(
        trace
            .iter()
            .filter(|event| event.as_str() == "write:metal0:3:128")
            .count(),
        1
    );
    assert!(!trace
        .iter()
        .any(|event| event.contains(":90:") || event.contains(":91:")));
    drop(trace);
    drop(run);
    assert!(matches!(
        compiled.start_run(),
        Err(BackendError::PoisonedRun)
    ));
}

#[test]
fn layer_finalization_runs_on_primary_after_last_span_transfer() {
    let state = Arc::new(MockState::default());
    let compiled = compile(finalization_transfer_plan(), Arc::clone(&state));
    let mut run = compiled.start_run().unwrap();
    state.trace.lock().unwrap().clear();
    let params = RunParams {
        token_count: 1,
        position_start: 0,
        mrope_positions: &[[0; 4]],
        token_ids: &[],
    };
    run.execute_layers(ComponentId::Llm, &mut [5.0; HIDDEN], &params)
        .unwrap();
    let mut logits = [0.0; 4];
    run.execute_logits(ComponentId::Llm, &params, &mut logits)
        .unwrap();
    assert_eq!(logits, [1000.0, 1001.0, 1002.0, 1003.0]);
    let trace = state.trace.lock().unwrap();
    assert_eq!(
        trace
            .iter()
            .filter(|event| event.as_str() == "read:metal0:21:128")
            .count(),
        1
    );
    assert_eq!(
        trace
            .iter()
            .filter(|event| event.as_str() == "write:cpu0:10:128")
            .count(),
        1
    );
    assert_eq!(
        trace
            .iter()
            .filter(|event| event.contains("submit:cpu0:30:FinalNormQ8Logits"))
            .count(),
        1
    );
}

#[test]
fn same_primary_finalization_alias_needs_no_transfer() {
    let state = Arc::new(MockState::default());
    let compiled = compile(same_primary_embedding_plan(), Arc::clone(&state));
    let component = &compiled.plan().components[&ComponentId::Llm];
    assert_eq!(
        component.layer_spans.last().unwrap().output,
        component.finalization.as_ref().unwrap().input
    );
    let mut run = compiled.start_run().unwrap();
    state.trace.lock().unwrap().clear();
    let params = RunParams {
        token_count: 1,
        position_start: 0,
        mrope_positions: &[[0; 4]],
        token_ids: &[],
    };
    run.execute_layers(ComponentId::Llm, &mut [2.0; HIDDEN], &params)
        .unwrap();
    let mut logits = [0.0; 4];
    run.execute_logits(ComponentId::Llm, &params, &mut logits)
        .unwrap();
    assert_eq!(logits, [1000.0, 1001.0, 1002.0, 1003.0]);
    let trace = state.trace.lock().unwrap();
    assert!(!trace
        .iter()
        .any(|event| { event == "read:cpu0:2:128" || event == "write:cpu0:2:128" }));
}

#[test]
fn embedding_rows_decode_requested_q8_tokens() {
    let catalog = test_catalog();
    let state = Arc::new(MockState::default());
    let plan = embedding_row_plan();
    let descriptors = plan
        .devices
        .values()
        .map(|device| device.descriptor.clone())
        .collect::<Vec<_>>();
    let compiled = CompiledModel::compile(
        Arc::clone(&catalog),
        plan,
        registry(&descriptors, Arc::clone(&state)),
    )
    .unwrap();
    let mut run = compiled.start_run().unwrap();
    state.trace.lock().unwrap().clear();
    let mut output = [0.0; HIDDEN * 2];
    run.execute_embedding(ComponentId::Llm, EMBEDDING, &[3, 1], &mut output)
        .unwrap();
    let bytes = catalog.bytes(EMBEDDING).unwrap();
    let mut expected = [0.0; HIDDEN * 2];
    embedding_lookup_q8_0(bytes, 3, HIDDEN, &mut expected[..HIDDEN]);
    embedding_lookup_q8_0(bytes, 1, HIDDEN, &mut expected[HIDDEN..]);
    assert_eq!(output, expected);
    let trace = state.trace.lock().unwrap();
    assert!(trace.iter().any(|event| event.contains("EmbeddingRows")));
    assert!(!trace.iter().any(|event| event.contains("Q8Rows")));
}

#[test]
fn partial_compile_rolls_back_every_opened_resident_session() {
    let state = Arc::new(MockState::default());
    *state.fail_open.lock().unwrap() = Some(BackendKind::Metal);
    let plan = row_plan();
    let descriptors = plan
        .devices
        .values()
        .map(|device| device.descriptor.clone())
        .collect::<Vec<_>>();
    let result = CompiledModel::compile(
        test_catalog(),
        plan,
        registry(&descriptors, Arc::clone(&state)),
    );
    assert!(matches!(result, Err(BackendError::Allocation { .. })));
    let probes = state.probes.lock().unwrap();
    let cpu = probes[&id("cpu0")].lock().unwrap();
    assert_eq!(cpu.resident_allocations, cpu.resident_frees);
    assert_eq!(state.opens.load(Ordering::SeqCst), 2);
}

#[test]
fn successful_model_drop_frees_every_resident_session() {
    let state = Arc::new(MockState::default());
    let compiled = compile(row_plan(), Arc::clone(&state));
    let probes = state.probes.lock().unwrap().clone();
    drop(compiled);
    for stats in probes.values() {
        let stats = stats.lock().unwrap();
        assert_eq!(stats.resident_allocations, stats.resident_frees);
    }
}

#[test]
fn duplicate_physical_devices_fail_before_any_provider_open() {
    let state = Arc::new(MockState::default());
    let cpu0 = descriptor("cpu0", "same-physical-device");
    let cpu1 = descriptor("cpu1", "same-physical-device");
    let plan = ExecutionPlan {
        components: BTreeMap::new(),
        devices: BTreeMap::from([
            (
                cpu0.id.clone(),
                device_plan(cpu0.clone(), Vec::new(), Vec::new(), Vec::new()),
            ),
            (
                cpu1.id.clone(),
                device_plan(cpu1.clone(), Vec::new(), Vec::new(), Vec::new()),
            ),
        ]),
    };
    let result = CompiledModel::compile(
        test_catalog(),
        plan,
        registry(&[cpu0, cpu1], Arc::clone(&state)),
    );
    assert!(result.is_err());
    assert_eq!(state.opens.load(Ordering::SeqCst), 0);
}
