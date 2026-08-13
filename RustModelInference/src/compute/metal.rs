use super::device::{
    BackendError, BackendKind, DeviceCapabilities, DeviceDescriptor, DeviceDiscovery,
    DeviceProvider, DeviceSession, FenceId, LifecycleProbe, ProgramId, RunParams, SessionStats,
    SlotId,
};
use super::program::{
    DevicePlan, LayerOp, ProgramKind, ProgramPlan, ResidentTensorPlan, SlotKind, SlotPlan,
    SlotStorage,
};
use crate::{ComponentId, DeviceId, GGMLType, LayerFamily, PlacementMode, TensorCatalog, TensorId};
use metal::objc::{msg_send, rc::autoreleasepool, runtime::Object, sel, sel_impl};
use metal::{
    Buffer, CommandBuffer, CommandBufferRef, CommandQueue, CompileOptions, ComputePipelineState,
    Device, Library, MTLCommandBufferStatus, MTLResourceOptions, MTLSize, NSRange,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ffi::CStr;
use std::mem::size_of;
use std::ops::Range;
use std::sync::Arc;

const Q8_BLOCK_ELEMENTS: u64 = 32;

pub struct MetalProvider {
    adapters: Vec<AdapterInfo>,
}

struct AdapterInfo {
    descriptor: DeviceDescriptor,
    registry_id: u64,
    max_buffer_length: u64,
    device: Device,
}

impl MetalProvider {
    pub fn new() -> Result<Self, BackendError> {
        Ok(Self {
            adapters: enumerate_adapters(),
        })
    }
}

impl DeviceDiscovery for MetalProvider {
    fn backend(&self) -> BackendKind {
        BackendKind::Metal
    }

    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, BackendError> {
        Ok(self
            .adapters
            .iter()
            .map(|adapter| adapter.descriptor.clone())
            .collect())
    }
}

impl DeviceProvider for MetalProvider {
    fn open(
        &self,
        descriptor: &DeviceDescriptor,
        plan: &DevicePlan,
        catalog: Arc<TensorCatalog>,
    ) -> Result<Box<dyn DeviceSession>, BackendError> {
        if descriptor.backend != BackendKind::Metal || descriptor != &plan.descriptor {
            return Err(BackendError::InvalidHandle);
        }
        let expected = self
            .adapters
            .iter()
            .find(|adapter| adapter.descriptor.id == descriptor.id)
            .ok_or_else(|| BackendError::DeviceUnavailable {
                device: descriptor.id.clone(),
            })?;
        let current = select_adapter(enumerate_adapters(), descriptor, expected)?;
        let validated = validate_plan(plan, &catalog, &current)?;
        MetalSession::open(current, plan, catalog, validated)
            .map(|session| Box::new(session) as Box<dyn DeviceSession>)
    }
}

fn enumerate_adapters() -> Vec<AdapterInfo> {
    let mut devices = Device::all();
    devices.sort_by_key(|device| device.registry_id());
    devices
        .into_iter()
        .enumerate()
        .map(|(index, device)| {
            let registry_id = device.registry_id();
            let max_buffer_length = device.max_buffer_length() as u64;
            let recommended = device.recommended_max_working_set_size();
            let usable_bytes = if recommended == 0 {
                max_buffer_length
            } else {
                recommended
            };
            AdapterInfo {
                descriptor: DeviceDescriptor {
                    id: DeviceId::parse(&format!("metal{index}"))
                        .expect("enumerated Metal id is valid"),
                    backend: BackendKind::Metal,
                    physical_key: format!("metal:{registry_id:016x}"),
                    name: device.name().to_owned(),
                    usable_bytes,
                    max_allocation_bytes: max_buffer_length,
                    buffer_alignment: 4,
                    unified_memory: device.has_unified_memory(),
                    capabilities: DeviceCapabilities {
                        components: BTreeSet::from([ComponentId::Llm]),
                        modes: BTreeSet::from([PlacementMode::Layer, PlacementMode::Row]),
                        layer_families: BTreeSet::from([LayerFamily::Qwen3]),
                        tensor_types: BTreeSet::from([
                            GGMLType::F32,
                            GGMLType::F16,
                            GGMLType::Q8_0,
                        ]),
                    },
                },
                registry_id,
                max_buffer_length,
                device,
            }
        })
        .collect()
}

fn select_adapter(
    adapters: Vec<AdapterInfo>,
    descriptor: &DeviceDescriptor,
    expected: &AdapterInfo,
) -> Result<AdapterInfo, BackendError> {
    match adapters
        .into_iter()
        .find(|adapter| adapter.descriptor.id == descriptor.id)
    {
        Some(adapter)
            if immutable_adapter_matches(&adapter, expected)
                && immutable_descriptor_matches(descriptor, &expected.descriptor) =>
        {
            Ok(adapter)
        }
        Some(_) => Err(BackendError::InvalidHandle),
        None => Err(BackendError::DeviceUnavailable {
            device: descriptor.id.clone(),
        }),
    }
}

fn immutable_adapter_matches(current: &AdapterInfo, expected: &AdapterInfo) -> bool {
    immutable_descriptor_matches(&current.descriptor, &expected.descriptor)
        && current.registry_id == expected.registry_id
        && current.max_buffer_length == expected.max_buffer_length
}

fn immutable_descriptor_matches(current: &DeviceDescriptor, expected: &DeviceDescriptor) -> bool {
    current.id == expected.id
        && current.backend == expected.backend
        && current.physical_key == expected.physical_key
        && current.name == expected.name
        && current.max_allocation_bytes == expected.max_allocation_bytes
        && current.buffer_alignment == expected.buffer_alignment
        && current.unified_memory == expected.unified_memory
        && current.capabilities == expected.capabilities
}

#[derive(Clone)]
struct ChunkSpec {
    tensor: TensorId,
    source_bytes: Range<u64>,
    local_rows: u32,
    global_row_start: u32,
}

struct ValidatedPlan {
    slots: BTreeMap<SlotId, SlotPlan>,
    chunks: Vec<ChunkSpec>,
    programs: Vec<ProgramResource>,
    runtime_bytes: u64,
}

struct ResidentChunk {
    buffer: Buffer,
    spec: ChunkSpec,
}

struct SlotResource {
    plan: SlotPlan,
    buffer: Buffer,
    staging: Option<Buffer>,
    staging_dirty: bool,
    host_readable: bool,
    host_writable: bool,
}

struct ProgramResource {
    plan: ProgramPlan,
    chunks: Vec<usize>,
    n_in: u32,
    output_stride: u32,
    mode: u32,
    layer_ops: Vec<BoundLayerOp>,
}

enum BoundLayerOp {
    RmsNorm {
        input: SlotId,
        weight: usize,
        output: SlotId,
        elements: u32,
        groups: u32,
        epsilon_bits: u32,
        weight_f16: bool,
    },
    Q8Matmul {
        input: SlotId,
        chunks: Vec<usize>,
        output: SlotId,
        n_in: u32,
        rows: u32,
    },
    Rope {
        q: SlotId,
        k: SlotId,
        q_width: u32,
        k_width: u32,
        key_head_dim: u32,
        freq_base_bits: u32,
    },
    KvAppend {
        k: SlotId,
        v: SlotId,
        key_state: SlotId,
        value_state: SlotId,
        key_width: u32,
        value_width: u32,
    },
    Attention {
        q: SlotId,
        output: SlotId,
        head_count: u32,
        kv_head_count: u32,
        key_state: SlotId,
        value_state: SlotId,
        key_head_dim: u32,
        value_head_dim: u32,
        context_capacity: u32,
    },
    SiluMul {
        gate: SlotId,
        up: SlotId,
        elements: u32,
    },
    Add {
        left: SlotId,
        right: SlotId,
        output: SlotId,
        elements: u32,
    },
}

fn drain_pending<T>(
    pending: &mut VecDeque<(FenceId, T)>,
    target: FenceId,
    poisoned: &mut bool,
    mut finish: impl FnMut(T) -> Result<(), BackendError>,
) -> Result<(), BackendError> {
    if !pending.iter().any(|(id, _)| *id == target) {
        return Err(BackendError::InvalidHandle);
    }
    loop {
        let (id, command) = pending.pop_front().expect("validated pending command");
        if let Err(error) = finish(command) {
            *poisoned = true;
            pending.clear();
            return Err(error);
        }
        if id == target {
            return Ok(());
        }
    }
}

pub struct MetalSession {
    descriptor: DeviceDescriptor,
    queue: CommandQueue,
    _library: Library,
    pipelines: BTreeMap<&'static str, ComputePipelineState>,
    resident: Vec<ResidentChunk>,
    slots: BTreeMap<SlotId, SlotResource>,
    programs: BTreeMap<ProgramId, ProgramResource>,
    pending: VecDeque<(FenceId, CommandBuffer)>,
    next_fence: u64,
    poisoned: bool,
    stats: SessionStats,
    probe: LifecycleProbe,
    _device: Device,
}

impl MetalSession {
    fn open(
        adapter: AdapterInfo,
        plan: &DevicePlan,
        catalog: Arc<TensorCatalog>,
        validated: ValidatedPlan,
    ) -> Result<Self, BackendError> {
        let device = adapter.device;
        let queue = device.new_command_queue();
        let (library, pipelines) = autoreleasepool(|| {
            let source = include_str!("metal/kernels.metal");
            let options = CompileOptions::new();
            options.set_fast_math_enabled(false);
            debug_assert!(!options.is_fast_math_enabled());
            let library = device
                .new_library_with_source(source, &options)
                .map_err(|message| BackendError::Pipeline {
                    device: adapter.descriptor.id.clone(),
                    message,
                })?;
            let pipelines = [
                "q8_rows",
                "rms_norm",
                "rope",
                "kv_append",
                "attention",
                "silu_mul",
                "add",
            ]
            .into_iter()
            .map(|name| {
                let function =
                    library
                        .get_function(name, None)
                        .map_err(|message| BackendError::Pipeline {
                            device: adapter.descriptor.id.clone(),
                            message,
                        })?;
                let pipeline = device
                    .new_compute_pipeline_state_with_function(&function)
                    .map_err(|message| BackendError::Pipeline {
                        device: adapter.descriptor.id.clone(),
                        message,
                    })?;
                Ok((name, pipeline))
            })
            .collect::<Result<_, BackendError>>()?;
            Ok((library, pipelines))
        })?;

        let mut resident = Vec::with_capacity(validated.chunks.len());
        let mut upload_staging = Vec::with_capacity(validated.chunks.len());
        for spec in &validated.chunks {
            let bytes = source_bytes(&catalog, spec.tensor, spec.source_bytes.clone())?;
            let private =
                device.new_buffer(bytes.len() as u64, MTLResourceOptions::StorageModePrivate);
            let staging = device.new_buffer_with_data(
                bytes.as_ptr().cast(),
                bytes.len() as u64,
                MTLResourceOptions::StorageModeShared,
            );
            resident.push(ResidentChunk {
                buffer: private,
                spec: spec.clone(),
            });
            upload_staging.push(staging);
        }
        upload_resident(&queue, &resident, &upload_staging, &adapter.descriptor)?;
        drop(upload_staging);

        let input_slots = validated
            .programs
            .iter()
            .map(|program| program.plan.input)
            .collect::<BTreeSet<_>>();
        let output_slots = validated
            .programs
            .iter()
            .map(|program| program.plan.output)
            .collect::<BTreeSet<_>>();
        let mut slots = BTreeMap::new();
        for plan in validated.slots.into_values() {
            let host_readable = output_slots.contains(&plan.id);
            let host_writable = input_slots.contains(&plan.id);
            let buffer = device.new_buffer(
                plan.byte_len,
                if host_readable {
                    MTLResourceOptions::StorageModeShared
                } else {
                    MTLResourceOptions::StorageModePrivate
                },
            );
            let staging = host_writable
                .then(|| device.new_buffer(plan.byte_len, MTLResourceOptions::StorageModeShared));
            slots.insert(
                plan.id,
                SlotResource {
                    plan,
                    buffer,
                    staging,
                    staging_dirty: false,
                    host_readable,
                    host_writable,
                },
            );
        }
        let programs = validated
            .programs
            .into_iter()
            .map(|program| (program.plan.id, program))
            .collect();
        let allocation_count = resident.len() as u64
            + slots
                .values()
                .map(|slot| 1 + u64::from(slot.staging.is_some()))
                .sum::<u64>()
            + 1;
        let weight_upload_bytes = plan
            .tensors
            .iter()
            .map(|tensor| tensor.source_bytes.end - tensor.source_bytes.start)
            .sum();

        Ok(Self {
            descriptor: adapter.descriptor,
            queue,
            _library: library,
            pipelines,
            resident,
            slots,
            programs,
            pending: VecDeque::new(),
            next_fence: 1,
            poisoned: false,
            stats: SessionStats {
                resident_bytes: validated.runtime_bytes,
                resident_allocations: allocation_count,
                weight_uploads: plan.tensors.len() as u64,
                weight_upload_bytes,
                ..SessionStats::default()
            },
            probe: LifecycleProbe::default(),
            _device: device,
        })
    }

    fn require_idle(&self) -> Result<(), BackendError> {
        if self.poisoned {
            Err(BackendError::PoisonedRun)
        } else if !self.pending.is_empty() {
            Err(submission(&self.descriptor, "Metal work is pending"))
        } else {
            Ok(())
        }
    }

    fn finish_command(&mut self, command: &CommandBufferRef) -> Result<(), BackendError> {
        command.wait_until_completed();
        if command.status() == MTLCommandBufferStatus::Completed {
            Ok(())
        } else {
            self.poisoned = true;
            Err(submission(
                &self.descriptor,
                command_error(command).unwrap_or_else(|| {
                    format!("Metal command ended with status {:?}", command.status())
                }),
            ))
        }
    }

    fn commit_command(&mut self, command: CommandBuffer) -> FenceId {
        command.commit();
        let id = FenceId(self.next_fence);
        self.pending.push_back((id, command));
        self.next_fence += 1;
        self.stats.submissions += 1;
        id
    }

    #[allow(clippy::too_many_arguments)]
    fn encode_q8_rows(
        &self,
        command: &CommandBufferRef,
        chunks: &[usize],
        input: SlotId,
        output: SlotId,
        batch: u32,
        n_in: u32,
        output_stride: u32,
        mode: u32,
        first_row: u32,
    ) -> Result<(), BackendError> {
        let pipeline = &self.pipelines["q8_rows"];
        for &chunk_index in chunks {
            let chunk = &self.resident[chunk_index];
            let encoder = command.new_compute_command_encoder();
            encoder.set_compute_pipeline_state(pipeline);
            encoder.set_buffer(0, Some(&chunk.buffer), 0);
            encoder.set_buffer(1, Some(&self.slots[&input].buffer), 0);
            encoder.set_buffer(2, Some(&self.slots[&output].buffer), 0);
            let push = [
                batch,
                n_in,
                chunk.spec.local_rows,
                chunk.spec.global_row_start,
                output_stride,
                mode,
                0,
                chunk.spec.global_row_start.saturating_sub(first_row),
            ];
            encoder.set_bytes(3, 32, push.as_ptr().cast());
            let work = if mode == 0 {
                batch.checked_mul(chunk.spec.local_rows)
            } else {
                batch.checked_mul(n_in)
            }
            .ok_or(BackendError::InvalidHandle)?;
            encoder.dispatch_threads(
                MTLSize::new(u64::from(work), 1, 1),
                MTLSize::new(pipeline.thread_execution_width(), 1, 1),
            );
            encoder.end_encoding();
        }
        Ok(())
    }

    fn encode_layer_op(
        &self,
        command: &CommandBufferRef,
        op: &BoundLayerOp,
        params: &RunParams<'_>,
    ) -> Result<(), BackendError> {
        let batch = params.token_count;
        let fits_f32 = |slot: SlotId, width: u32| {
            u64::from(batch)
                .checked_mul(u64::from(width))
                .and_then(|values| values.checked_mul(4))
                .is_some_and(|bytes| bytes <= self.slots[&slot].plan.byte_len)
        };
        let dispatch =
            |name: &'static str, work: u32, buffers: &[(&Buffer, u64)], push: &[u32; 8]| {
                let pipeline = &self.pipelines[name];
                let encoder = command.new_compute_command_encoder();
                encoder.set_compute_pipeline_state(pipeline);
                for (index, (buffer, offset)) in buffers.iter().enumerate() {
                    encoder.set_buffer(index as u64, Some(buffer), *offset);
                }
                encoder.set_bytes(buffers.len() as u64, 32, push.as_ptr().cast());
                encoder.dispatch_threads(
                    MTLSize::new(u64::from(work), 1, 1),
                    MTLSize::new(pipeline.thread_execution_width(), 1, 1),
                );
                encoder.end_encoding();
            };
        match op {
            BoundLayerOp::RmsNorm {
                input,
                weight,
                output,
                elements,
                groups,
                epsilon_bits,
                weight_f16,
            } => {
                let width = elements
                    .checked_mul(*groups)
                    .ok_or(BackendError::InvalidHandle)?;
                if !fits_f32(*input, width) || !fits_f32(*output, width) {
                    return Err(BackendError::InvalidHandle);
                }
                let work = batch
                    .checked_mul(*groups)
                    .ok_or(BackendError::InvalidHandle)?;
                let push = [
                    batch,
                    *elements,
                    *groups,
                    *epsilon_bits,
                    u32::from(*weight_f16),
                    0,
                    0,
                    0,
                ];
                dispatch(
                    "rms_norm",
                    work,
                    &[
                        (&self.resident[*weight].buffer, 0),
                        (&self.slots[input].buffer, 0),
                        (&self.slots[output].buffer, 0),
                    ],
                    &push,
                );
            }
            BoundLayerOp::Q8Matmul {
                input,
                chunks,
                output,
                n_in,
                rows,
            } => {
                if !fits_f32(*input, *n_in) || !fits_f32(*output, *rows) {
                    return Err(BackendError::InvalidHandle);
                }
                self.encode_q8_rows(command, chunks, *input, *output, batch, *n_in, *rows, 0, 0)?;
            }
            BoundLayerOp::Rope {
                q,
                k,
                q_width,
                k_width,
                key_head_dim,
                freq_base_bits,
            } => {
                if !fits_f32(*q, *q_width) || !fits_f32(*k, *k_width) {
                    return Err(BackendError::InvalidHandle);
                }
                let work = batch
                    .checked_mul((q_width + k_width) / 2)
                    .ok_or(BackendError::InvalidHandle)?;
                let push = [
                    batch,
                    *q_width,
                    *k_width,
                    *key_head_dim,
                    params.position_start,
                    *freq_base_bits,
                    0,
                    0,
                ];
                dispatch(
                    "rope",
                    work,
                    &[(&self.slots[q].buffer, 0), (&self.slots[k].buffer, 0)],
                    &push,
                );
            }
            BoundLayerOp::KvAppend {
                k,
                v,
                key_state,
                value_state,
                key_width,
                value_width,
            } => {
                let end = params
                    .position_start
                    .checked_add(batch)
                    .ok_or(BackendError::InvalidHandle)?;
                let state_fits = |slot: SlotId, width: u32| {
                    u64::from(end)
                        .checked_mul(u64::from(width))
                        .and_then(|values| values.checked_mul(2))
                        .is_some_and(|bytes| bytes <= self.slots[&slot].plan.byte_len)
                };
                if !fits_f32(*k, *key_width)
                    || !fits_f32(*v, *value_width)
                    || !state_fits(*key_state, *key_width)
                    || !state_fits(*value_state, *value_width)
                {
                    return Err(BackendError::InvalidHandle);
                }
                let work = batch
                    .checked_mul(key_width + value_width)
                    .ok_or(BackendError::InvalidHandle)?;
                let push = [
                    batch,
                    *key_width,
                    *value_width,
                    params.position_start,
                    0,
                    0,
                    0,
                    0,
                ];
                dispatch(
                    "kv_append",
                    work,
                    &[
                        (&self.slots[k].buffer, 0),
                        (&self.slots[v].buffer, 0),
                        (&self.slots[key_state].buffer, 0),
                        (&self.slots[value_state].buffer, 0),
                    ],
                    &push,
                );
            }
            BoundLayerOp::Attention {
                q,
                output,
                head_count,
                kv_head_count,
                key_state,
                value_state,
                key_head_dim,
                value_head_dim,
                context_capacity,
            } => {
                if params
                    .position_start
                    .checked_add(batch)
                    .is_none_or(|end| end > *context_capacity)
                {
                    return Err(BackendError::InvalidHandle);
                }
                let q_width = head_count
                    .checked_mul(*key_head_dim)
                    .ok_or(BackendError::InvalidHandle)?;
                let output_width = head_count
                    .checked_mul(*value_head_dim)
                    .ok_or(BackendError::InvalidHandle)?;
                if !fits_f32(*q, q_width) || !fits_f32(*output, output_width) {
                    return Err(BackendError::InvalidHandle);
                }
                let work = batch
                    .checked_mul(*head_count)
                    .and_then(|items| items.checked_mul(*value_head_dim))
                    .ok_or(BackendError::InvalidHandle)?;
                let push = [
                    batch,
                    *head_count,
                    *kv_head_count,
                    0,
                    *key_head_dim,
                    *value_head_dim,
                    params.position_start,
                    0,
                ];
                dispatch(
                    "attention",
                    work,
                    &[
                        (&self.slots[q].buffer, 0),
                        (&self.slots[key_state].buffer, 0),
                        (&self.slots[value_state].buffer, 0),
                        (&self.slots[output].buffer, 0),
                    ],
                    &push,
                );
            }
            BoundLayerOp::SiluMul { gate, up, elements } => {
                if !fits_f32(*gate, *elements) || !fits_f32(*up, *elements) {
                    return Err(BackendError::InvalidHandle);
                }
                let work = batch
                    .checked_mul(*elements)
                    .ok_or(BackendError::InvalidHandle)?;
                let push = [batch, *elements, 0, 0, 0, 0, 0, 0];
                dispatch(
                    "silu_mul",
                    work,
                    &[(&self.slots[gate].buffer, 0), (&self.slots[up].buffer, 0)],
                    &push,
                );
            }
            BoundLayerOp::Add {
                left,
                right,
                output,
                elements,
            } => {
                if !fits_f32(*left, *elements)
                    || !fits_f32(*right, *elements)
                    || !fits_f32(*output, *elements)
                {
                    return Err(BackendError::InvalidHandle);
                }
                let work = batch
                    .checked_mul(*elements)
                    .ok_or(BackendError::InvalidHandle)?;
                let push = [batch, *elements, 0, 0, 0, 0, 0, 0];
                dispatch(
                    "add",
                    work,
                    &[
                        (&self.slots[left].buffer, 0),
                        (&self.slots[right].buffer, 0),
                        (&self.slots[output].buffer, 0),
                    ],
                    &push,
                );
            }
        }
        Ok(())
    }
}

impl DeviceSession for MetalSession {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.descriptor
    }

    fn write_f32(&mut self, slot: SlotId, values: &[f32]) -> Result<(), BackendError> {
        self.require_idle()?;
        let resource = self
            .slots
            .get(&slot)
            .filter(|resource| {
                resource.plan.storage == SlotStorage::F32
                    && resource.host_writable
                    && matches!(resource.plan.kind, SlotKind::Activation | SlotKind::Scratch)
            })
            .ok_or(BackendError::InvalidHandle)?;
        let byte_len = values
            .len()
            .checked_mul(size_of::<f32>())
            .and_then(|bytes| u64::try_from(bytes).ok())
            .ok_or(BackendError::InvalidHandle)?;
        if byte_len > resource.plan.byte_len {
            return Err(BackendError::InvalidHandle);
        }
        let destination = resource.staging.as_ref().unwrap_or(&resource.buffer);
        unsafe {
            std::ptr::copy_nonoverlapping(
                values.as_ptr().cast::<u8>(),
                destination.contents().cast::<u8>(),
                byte_len as usize,
            );
        }
        self.slots.get_mut(&slot).unwrap().staging_dirty = true;
        self.stats.activation_h2d_bytes += byte_len;
        Ok(())
    }

    fn submit(
        &mut self,
        program: ProgramId,
        params: &RunParams<'_>,
    ) -> Result<FenceId, BackendError> {
        if self.poisoned {
            return Err(BackendError::PoisonedRun);
        }
        let resource = self
            .programs
            .get(&program)
            .ok_or(BackendError::InvalidHandle)?;
        if matches!(
            resource.plan.kind,
            ProgramKind::LayerSegment { .. } | ProgramKind::FinalNormQ8Logits { .. }
        ) {
            if params.token_count == 0 {
                return Err(BackendError::InvalidHandle);
            }
            let input = resource.plan.input;
            let copy_input = self.slots[&input].staging_dirty;
            let command = autoreleasepool(|| -> Result<CommandBuffer, BackendError> {
                let command = self.queue.new_command_buffer().to_owned();
                if copy_input {
                    let slot = &self.slots[&input];
                    let blit = command.new_blit_command_encoder();
                    blit.copy_from_buffer(
                        slot.staging.as_ref().ok_or(BackendError::InvalidHandle)?,
                        0,
                        &slot.buffer,
                        0,
                        slot.plan.byte_len,
                    );
                    blit.end_encoding();
                }
                for op in &resource.layer_ops {
                    self.encode_layer_op(&command, op, params)?;
                }
                Ok(command)
            })?;
            if copy_input {
                self.slots.get_mut(&input).unwrap().staging_dirty = false;
            }
            return Ok(self.commit_command(command));
        }
        let batch_capacity = match &resource.plan.kind {
            ProgramKind::Q8Rows { batch_capacity, .. } => *batch_capacity,
            ProgramKind::EmbeddingRows { row_count, .. } => {
                if params.token_ids.len() != params.token_count as usize
                    || params.token_ids.iter().any(|token| token >= row_count)
                {
                    return Err(BackendError::InvalidHandle);
                }
                u32::try_from(
                    self.slots[&resource.plan.output].plan.byte_len / 4 / u64::from(resource.n_in),
                )
                .map_err(|_| BackendError::InvalidHandle)?
            }
            _ => return Err(BackendError::InvalidHandle),
        };
        if params.token_count == 0 || params.token_count > batch_capacity {
            return Err(BackendError::InvalidHandle);
        }
        let input = &self.slots[&resource.plan.input];
        let output = &self.slots[&resource.plan.output];
        let input_bytes = if resource.mode == 0 {
            u64::from(params.token_count)
                .checked_mul(u64::from(resource.n_in))
                .and_then(|values| values.checked_mul(4))
                .ok_or(BackendError::InvalidHandle)?
        } else {
            let bytes = params
                .token_ids
                .len()
                .checked_mul(size_of::<u32>())
                .ok_or(BackendError::InvalidHandle)?;
            if bytes as u64 > input.plan.byte_len {
                return Err(BackendError::InvalidHandle);
            }
            let staging = input.staging.as_ref().ok_or(BackendError::InvalidHandle)?;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    params.token_ids.as_ptr().cast::<u8>(),
                    staging.contents().cast::<u8>(),
                    bytes,
                );
            }
            self.stats.activation_h2d_bytes += bytes as u64;
            bytes as u64
        };
        let output_bytes = u64::from(params.token_count)
            .checked_mul(u64::from(resource.output_stride))
            .and_then(|values| values.checked_mul(4))
            .ok_or(BackendError::InvalidHandle)?;
        if input_bytes > input.plan.byte_len || output_bytes > output.plan.byte_len {
            return Err(BackendError::InvalidHandle);
        }

        let input_staging = input.staging.as_ref().ok_or(BackendError::InvalidHandle)?;
        let command = autoreleasepool(|| {
            let command = self.queue.new_command_buffer().to_owned();
            let blit = command.new_blit_command_encoder();
            blit.copy_from_buffer(input_staging, 0, &input.buffer, 0, input_bytes);
            blit.end_encoding();

            let first_row = match &resource.plan.kind {
                ProgramKind::Q8Rows { rows, .. } => rows.start,
                ProgramKind::EmbeddingRows { .. } => 0,
                _ => 0,
            };
            self.encode_q8_rows(
                &command,
                &resource.chunks,
                resource.plan.input,
                resource.plan.output,
                params.token_count,
                resource.n_in,
                resource.output_stride,
                resource.mode,
                first_row,
            )
            .expect("validated Metal Q8 dispatch");
            command
        });
        Ok(self.commit_command(command))
    }

    fn wait(&mut self, fence: FenceId) -> Result<(), BackendError> {
        self.stats.host_waits += 1;
        let descriptor = self.descriptor.clone();
        drain_pending(&mut self.pending, fence, &mut self.poisoned, |command| {
            command.wait_until_completed();
            if command.status() == MTLCommandBufferStatus::Completed {
                Ok(())
            } else {
                Err(submission(
                    &descriptor,
                    command_error(&command).unwrap_or_else(|| {
                        format!("Metal command ended with status {:?}", command.status())
                    }),
                ))
            }
        })
    }

    fn read_f32(&mut self, slot: SlotId, values: &mut [f32]) -> Result<(), BackendError> {
        self.require_idle()?;
        let resource = self
            .slots
            .get(&slot)
            .filter(|resource| {
                resource.plan.storage == SlotStorage::F32
                    && resource.host_readable
                    && matches!(resource.plan.kind, SlotKind::Activation | SlotKind::Result)
            })
            .ok_or(BackendError::InvalidHandle)?;
        let byte_len = values
            .len()
            .checked_mul(size_of::<f32>())
            .and_then(|bytes| u64::try_from(bytes).ok())
            .ok_or(BackendError::InvalidHandle)?;
        if byte_len > resource.plan.byte_len {
            return Err(BackendError::InvalidHandle);
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                resource.buffer.contents().cast::<u8>(),
                values.as_mut_ptr().cast::<u8>(),
                byte_len as usize,
            );
        }
        self.stats.activation_d2h_bytes += byte_len;
        Ok(())
    }

    fn reset_state(&mut self) -> Result<(), BackendError> {
        self.require_idle()?;
        let command = autoreleasepool(|| {
            let command = self.queue.new_command_buffer().to_owned();
            let blit = command.new_blit_command_encoder();
            for slot in self.slots.values() {
                blit.fill_buffer(&slot.buffer, NSRange::new(0, slot.plan.byte_len), 0);
                if let Some(staging) = &slot.staging {
                    unsafe {
                        std::ptr::write_bytes(
                            staging.contents().cast::<u8>(),
                            0,
                            slot.plan.byte_len as usize,
                        );
                    }
                }
            }
            blit.end_encoding();
            command.commit();
            command
        });
        self.finish_command(&command)
    }

    fn stats(&self) -> SessionStats {
        self.stats.clone()
    }

    fn lifecycle_probe(&self) -> LifecycleProbe {
        self.probe.clone()
    }
}

impl Drop for MetalSession {
    fn drop(&mut self) {
        while let Some((_, command)) = self.pending.pop_front() {
            command.wait_until_completed();
        }
        self.stats.resident_frees = self.stats.resident_allocations;
    }
}

fn validate_plan(
    plan: &DevicePlan,
    catalog: &TensorCatalog,
    adapter: &AdapterInfo,
) -> Result<ValidatedPlan, BackendError> {
    if plan.descriptor.backend != BackendKind::Metal
        || plan.descriptor.physical_key != adapter.descriptor.physical_key
        || plan.descriptor.name != adapter.descriptor.name
        || plan.descriptor.buffer_alignment == 0
        || plan.descriptor.buffer_alignment != adapter.descriptor.buffer_alignment
        || plan.descriptor.max_allocation_bytes != adapter.descriptor.max_allocation_bytes
        || plan.descriptor.unified_memory != adapter.descriptor.unified_memory
        || plan.descriptor.capabilities != adapter.descriptor.capabilities
    {
        return Err(BackendError::InvalidHandle);
    }
    let slots = validate_slots(plan, adapter.max_buffer_length)?;
    validate_resident_ranges(plan, catalog)?;

    let mut chunks = Vec::new();
    let mut chunk_ranges = BTreeMap::new();
    for resident in &plan.tensors {
        let entry = catalog
            .entry(resident.tensor)
            .ok_or(BackendError::InvalidHandle)?;
        let n_in = u32::try_from(entry.shape[0]).map_err(|_| BackendError::InvalidHandle)?;
        if n_in == 0 || (entry.ggml_type == GGMLType::Q8_0 && n_in % Q8_BLOCK_ELEMENTS as u32 != 0)
        {
            return Err(BackendError::InvalidHandle);
        }
        let start = chunks.len();
        let resident_chunks = row_chunks(resident, entry.row_bytes, adapter.max_buffer_length)?;
        for chunk in &resident_chunks {
            source_bytes(catalog, chunk.tensor, chunk.source_bytes.clone())?;
        }
        chunks.extend(resident_chunks);
        chunk_ranges.insert(
            (resident.tensor, resident.rows.start, resident.rows.end),
            start..chunks.len(),
        );
    }

    let mut ids = BTreeSet::new();
    let mut programs = Vec::with_capacity(plan.programs.len());
    for program in &plan.programs {
        if !ids.insert(program.id)
            || (program.input == program.output
                && !matches!(program.kind, ProgramKind::LayerSegment { .. }))
        {
            return Err(BackendError::InvalidHandle);
        }
        if matches!(
            program.kind,
            ProgramKind::LayerSegment { .. } | ProgramKind::FinalNormQ8Logits { .. }
        ) {
            if !slots.contains_key(&program.input) || !slots.contains_key(&program.output) {
                return Err(BackendError::InvalidHandle);
            }
            let layer_ops = bind_layer_ops(program, catalog, &slots, &chunk_ranges)?;
            programs.push(ProgramResource {
                plan: program.clone(),
                chunks: Vec::new(),
                n_in: 0,
                output_stride: 0,
                mode: 2,
                layer_ops,
            });
            continue;
        }
        let (tensor, rows, batch_capacity, mode) = match &program.kind {
            ProgramKind::Q8Rows {
                tensor,
                rows,
                batch_capacity,
            } => (*tensor, rows.clone(), *batch_capacity, 0),
            ProgramKind::EmbeddingRows { tensor, row_count } => (*tensor, 0..*row_count, 0, 1),
            _ => {
                return Err(BackendError::Unsupported {
                    device: adapter.descriptor.id.clone(),
                    operation: "Metal program kind",
                })
            }
        };
        let entry = catalog.entry(tensor).ok_or(BackendError::InvalidHandle)?;
        let n_in = u32::try_from(entry.shape[0]).map_err(|_| BackendError::InvalidHandle)?;
        let input = slots
            .get(&program.input)
            .ok_or(BackendError::InvalidHandle)?;
        let output = slots
            .get(&program.output)
            .ok_or(BackendError::InvalidHandle)?;
        let chunks = chunk_ranges
            .get(&(tensor, rows.start, rows.end))
            .ok_or(BackendError::InvalidHandle)?
            .clone()
            .collect::<Vec<_>>();
        let output_stride = if mode == 0 { rows.len() as u32 } else { n_in };
        let valid = if mode == 0 {
            let input_values = u64::from(batch_capacity).checked_mul(u64::from(n_in));
            let output_values = u64::from(batch_capacity).checked_mul(rows.len() as u64);
            batch_capacity != 0
                && matches!(input.kind, SlotKind::Activation | SlotKind::Scratch)
                && input.storage == SlotStorage::F32
                && output.storage == SlotStorage::F32
                && output.kind == SlotKind::Result
                && input_values.is_some_and(|values| {
                    values <= u64::from(u32::MAX)
                        && values
                            .checked_mul(4)
                            .is_some_and(|bytes| bytes <= input.byte_len)
                })
                && output_values.is_some_and(|values| {
                    values <= u64::from(u32::MAX)
                        && values
                            .checked_mul(4)
                            .is_some_and(|bytes| bytes <= output.byte_len)
                })
        } else {
            let output_row_bytes = u64::from(n_in).checked_mul(4);
            let input_capacity = input.byte_len / 4;
            rows.end != 0
                && entry.row_count == u64::from(rows.end)
                && input.byte_len >= 4
                && input.kind == SlotKind::Scratch
                && input.storage == SlotStorage::I8
                && output.storage == SlotStorage::F32
                && matches!(output.kind, SlotKind::Activation | SlotKind::Result)
                && output_row_bytes.is_some_and(|bytes| {
                    bytes != 0
                        && output.byte_len % bytes == 0
                        && output.byte_len / bytes <= input_capacity
                        && output.byte_len / bytes <= u64::from(u32::MAX)
                        && output.byte_len / 4 <= u64::from(u32::MAX)
                })
        };
        let layer_ops_valid = program.layer_ops.iter().all(|op| match op {
            super::program::LayerOp::Q8Matmul {
                input,
                weight,
                output,
            } => *input == program.input && *weight == tensor && *output == program.output,
            _ => false,
        });
        if entry.ggml_type != GGMLType::Q8_0
            || n_in == 0
            || !valid
            || chunks.is_empty()
            || !layer_ops_valid
        {
            return Err(BackendError::InvalidHandle);
        }
        programs.push(ProgramResource {
            plan: program.clone(),
            chunks,
            n_in,
            output_stride,
            mode,
            layer_ops: Vec::new(),
        });
    }
    if programs.is_empty() {
        return Err(BackendError::InvalidHandle);
    }

    let slot_bytes = slots
        .values()
        .try_fold(0_u64, |total, slot| total.checked_add(slot.byte_len));
    let input_slots = programs
        .iter()
        .map(|program| program.plan.input)
        .collect::<BTreeSet<_>>();
    let staging_bytes = input_slots.into_iter().try_fold(0_u64, |total, id| {
        total.checked_add(slots.get(&id)?.byte_len)
    });
    let slot_bytes = slot_bytes.ok_or(BackendError::InvalidHandle)?;
    let staging_bytes = staging_bytes.ok_or(BackendError::InvalidHandle)?;
    let runtime_bytes = plan
        .memory
        .resident_bytes
        .checked_add(slot_bytes)
        .and_then(|bytes| bytes.checked_add(staging_bytes))
        .ok_or(BackendError::InvalidHandle)?;
    let upload_peak = plan
        .memory
        .resident_bytes
        .checked_mul(2)
        .ok_or(BackendError::InvalidHandle)?;
    if runtime_bytes.max(upload_peak) > adapter.descriptor.usable_bytes
        || plan.memory.required_bytes > adapter.descriptor.usable_bytes
    {
        return Err(BackendError::InvalidHandle);
    }
    Ok(ValidatedPlan {
        slots,
        chunks,
        programs,
        runtime_bytes,
    })
}

fn bind_layer_ops(
    program: &ProgramPlan,
    catalog: &TensorCatalog,
    slots: &BTreeMap<SlotId, SlotPlan>,
    chunk_ranges: &BTreeMap<(TensorId, u32, u32), Range<usize>>,
) -> Result<Vec<BoundLayerOp>, BackendError> {
    if let ProgramKind::LayerSegment { families, .. } = &program.kind {
        if families.iter().any(|family| *family != LayerFamily::Qwen3) {
            return Err(BackendError::InvalidHandle);
        }
    }
    let f32_slot = |id| {
        slots
            .get(&id)
            .is_some_and(|slot| slot.storage == SlotStorage::F32)
    };
    let tensor_chunks = |tensor| -> Result<Vec<usize>, BackendError> {
        let entry = catalog.entry(tensor).ok_or(BackendError::InvalidHandle)?;
        Ok(chunk_ranges
            .get(&(tensor, 0, entry.row_count as u32))
            .ok_or(BackendError::InvalidHandle)?
            .clone()
            .collect())
    };
    let mut widths = BTreeMap::new();
    let mut bound = Vec::with_capacity(program.layer_ops.len());
    for op in &program.layer_ops {
        match *op {
            LayerOp::RmsNorm {
                input,
                weight,
                output,
                epsilon_bits,
            } => {
                let entry = catalog.entry(weight).ok_or(BackendError::InvalidHandle)?;
                let elements =
                    u32::try_from(entry.shape[0]).map_err(|_| BackendError::InvalidHandle)?;
                let chunks = tensor_chunks(weight)?;
                let groups = u32::try_from(slots[&input].byte_len / 4 / u64::from(elements))
                    .map_err(|_| BackendError::InvalidHandle)?;
                if !matches!(entry.ggml_type, GGMLType::F32 | GGMLType::F16)
                    || chunks.len() != 1
                    || groups == 0
                    || !f32_slot(input)
                    || !f32_slot(output)
                {
                    return Err(BackendError::InvalidHandle);
                }
                widths.insert(input, elements);
                widths.insert(output, elements);
                bound.push(BoundLayerOp::RmsNorm {
                    input,
                    weight: chunks[0],
                    output,
                    elements,
                    groups,
                    epsilon_bits,
                    weight_f16: entry.ggml_type == GGMLType::F16,
                });
            }
            LayerOp::Q8Matmul {
                input,
                weight,
                output,
            } => {
                let entry = catalog.entry(weight).ok_or(BackendError::InvalidHandle)?;
                let n_in =
                    u32::try_from(entry.shape[0]).map_err(|_| BackendError::InvalidHandle)?;
                let rows =
                    u32::try_from(entry.row_count).map_err(|_| BackendError::InvalidHandle)?;
                if entry.ggml_type != GGMLType::Q8_0
                    || !f32_slot(input)
                    || !f32_slot(output)
                    || widths.get(&input).is_some_and(|width| *width != n_in)
                {
                    return Err(BackendError::InvalidHandle);
                }
                widths.insert(input, n_in);
                widths.insert(output, rows);
                bound.push(BoundLayerOp::Q8Matmul {
                    input,
                    chunks: tensor_chunks(weight)?,
                    output,
                    n_in,
                    rows,
                });
            }
            LayerOp::Rope {
                q,
                k,
                key_head_dim,
                rope_dims,
                freq_base_bits,
            } if f32_slot(q) && f32_slot(k) && key_head_dim == rope_dims => {
                let q_width = *widths.get(&q).ok_or(BackendError::InvalidHandle)?;
                let k_width = *widths.get(&k).ok_or(BackendError::InvalidHandle)?;
                bound.push(BoundLayerOp::Rope {
                    q,
                    k,
                    q_width,
                    k_width,
                    key_head_dim,
                    freq_base_bits,
                });
            }
            LayerOp::KvAppend {
                k,
                v,
                key_state,
                value_state,
                ..
            } => {
                let attention = program.layer_ops.iter().find_map(|op| match op {
                    LayerOp::Attention {
                        kv_head_count,
                        key_state: keys,
                        value_state: values,
                        key_head_dim,
                        value_head_dim,
                        ..
                    } if *keys == key_state && *values == value_state => {
                        Some((kv_head_count * key_head_dim, kv_head_count * value_head_dim))
                    }
                    _ => None,
                });
                let Some((key_width, value_width)) = attention else {
                    return Err(BackendError::InvalidHandle);
                };
                if !f32_slot(k)
                    || !f32_slot(v)
                    || !matches!(slots.get(&key_state), Some(slot) if slot.storage == SlotStorage::F16)
                    || !matches!(slots.get(&value_state), Some(slot) if slot.storage == SlotStorage::F16)
                {
                    return Err(BackendError::InvalidHandle);
                }
                bound.push(BoundLayerOp::KvAppend {
                    k,
                    v,
                    key_state,
                    value_state,
                    key_width,
                    value_width,
                });
            }
            LayerOp::Attention {
                q,
                output,
                head_count,
                kv_head_count,
                key_state,
                value_state,
                key_head_dim,
                value_head_dim,
                context_capacity,
                ..
            } if f32_slot(q) && f32_slot(output) && kv_head_count != 0 => {
                widths.insert(output, head_count * value_head_dim);
                bound.push(BoundLayerOp::Attention {
                    q,
                    output,
                    head_count,
                    kv_head_count,
                    key_state,
                    value_state,
                    key_head_dim,
                    value_head_dim,
                    context_capacity,
                });
            }
            LayerOp::SiluMul { gate, up } if f32_slot(gate) && f32_slot(up) => {
                let elements = *widths.get(&up).ok_or(BackendError::InvalidHandle)?;
                if widths.get(&gate) != Some(&elements) {
                    return Err(BackendError::InvalidHandle);
                }
                bound.push(BoundLayerOp::SiluMul { gate, up, elements });
            }
            LayerOp::Add {
                left,
                right,
                output,
            } if f32_slot(left) && f32_slot(right) && f32_slot(output) => {
                let elements = *widths
                    .get(&left)
                    .or_else(|| widths.get(&right))
                    .ok_or(BackendError::InvalidHandle)?;
                widths.insert(output, elements);
                bound.push(BoundLayerOp::Add {
                    left,
                    right,
                    output,
                    elements,
                });
            }
            _ => return Err(BackendError::InvalidHandle),
        }
    }
    if bound.is_empty() {
        return Err(BackendError::InvalidHandle);
    }
    Ok(bound)
}

fn validate_slots(
    plan: &DevicePlan,
    max_buffer_length: u64,
) -> Result<BTreeMap<SlotId, SlotPlan>, BackendError> {
    let mut slots = BTreeMap::new();
    let mut ranges = Vec::with_capacity(plan.slots.len());
    for (index, slot) in plan.slots.iter().enumerate() {
        let divisor = match slot.storage {
            SlotStorage::F32 => 4,
            SlotStorage::F16 => 2,
            SlotStorage::I8 => 1,
        };
        if slot.id.0 as usize != index
            || slot.byte_len == 0
            || slot.byte_len % divisor != 0
            || slot.byte_len > max_buffer_length
            || slot.alignment != plan.descriptor.buffer_alignment
            || slot.arena_offset % plan.descriptor.buffer_alignment != 0
        {
            return Err(BackendError::InvalidHandle);
        }
        ranges.push(
            slot.arena_offset
                ..slot
                    .arena_offset
                    .checked_add(slot.byte_len)
                    .ok_or(BackendError::InvalidHandle)?,
        );
        slots.insert(slot.id, slot.clone());
    }
    ranges.sort_by_key(|range| range.start);
    if ranges.windows(2).any(|pair| pair[0].end > pair[1].start) {
        return Err(BackendError::InvalidHandle);
    }
    Ok(slots)
}

fn validate_resident_ranges(
    plan: &DevicePlan,
    catalog: &TensorCatalog,
) -> Result<(), BackendError> {
    let mut ranges = Vec::with_capacity(plan.tensors.len());
    for resident in &plan.tensors {
        let entry = catalog
            .entry(resident.tensor)
            .ok_or(BackendError::InvalidHandle)?;
        let expected_start = entry
            .segment_byte_range
            .start
            .checked_add(
                u64::from(resident.rows.start)
                    .checked_mul(entry.row_bytes)
                    .ok_or(BackendError::InvalidHandle)?,
            )
            .ok_or(BackendError::InvalidHandle)?;
        let expected_end = entry
            .segment_byte_range
            .start
            .checked_add(
                u64::from(resident.rows.end)
                    .checked_mul(entry.row_bytes)
                    .ok_or(BackendError::InvalidHandle)?,
            )
            .ok_or(BackendError::InvalidHandle)?;
        let len = expected_end
            .checked_sub(expected_start)
            .ok_or(BackendError::InvalidHandle)?;
        let end = resident
            .arena_offset
            .checked_add(len)
            .ok_or(BackendError::InvalidHandle)?;
        if resident.rows.start >= resident.rows.end
            || u64::from(resident.rows.end) > entry.row_count
            || resident.source_bytes != (expected_start..expected_end)
            || end > plan.memory.resident_bytes
            || resident.arena_offset % plan.descriptor.buffer_alignment != 0
        {
            return Err(BackendError::InvalidHandle);
        }
        ranges.push(resident.arena_offset..end);
    }
    ranges.sort_by_key(|range| range.start);
    if ranges.windows(2).any(|pair| pair[0].end > pair[1].start) {
        return Err(BackendError::InvalidHandle);
    }
    Ok(())
}

fn row_chunks(
    resident: &ResidentTensorPlan,
    row_bytes: u64,
    max_buffer_length: u64,
) -> Result<Vec<ChunkSpec>, BackendError> {
    let rows_per_chunk = max_buffer_length / row_bytes;
    if rows_per_chunk == 0 {
        return Err(BackendError::InvalidHandle);
    }
    let total_rows = resident
        .rows
        .end
        .checked_sub(resident.rows.start)
        .ok_or(BackendError::InvalidHandle)?;
    let mut chunks = Vec::new();
    let mut local_start = 0_u32;
    while local_start < total_rows {
        let local_rows = u64::from(total_rows - local_start).min(rows_per_chunk) as u32;
        let byte_start = resident
            .source_bytes
            .start
            .checked_add(u64::from(local_start) * row_bytes)
            .ok_or(BackendError::InvalidHandle)?;
        let byte_end = byte_start
            .checked_add(u64::from(local_rows) * row_bytes)
            .ok_or(BackendError::InvalidHandle)?;
        chunks.push(ChunkSpec {
            tensor: resident.tensor,
            source_bytes: byte_start..byte_end,
            local_rows,
            global_row_start: resident.rows.start + local_start,
        });
        local_start += local_rows;
    }
    Ok(chunks)
}

fn source_bytes<'a>(
    catalog: &'a TensorCatalog,
    tensor: TensorId,
    range: Range<u64>,
) -> Result<&'a [u8], BackendError> {
    let entry = catalog.entry(tensor).ok_or(BackendError::InvalidHandle)?;
    let source = catalog
        .bytes(tensor)
        .map_err(|_| BackendError::InvalidHandle)?;
    let start = range
        .start
        .checked_sub(entry.segment_byte_range.start)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(BackendError::InvalidHandle)?;
    let end = range
        .end
        .checked_sub(entry.segment_byte_range.start)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(BackendError::InvalidHandle)?;
    source.get(start..end).ok_or(BackendError::InvalidHandle)
}

fn upload_resident(
    queue: &CommandQueue,
    resident: &[ResidentChunk],
    staging: &[Buffer],
    descriptor: &DeviceDescriptor,
) -> Result<(), BackendError> {
    let command = autoreleasepool(|| {
        let command = queue.new_command_buffer().to_owned();
        let blit = command.new_blit_command_encoder();
        for (resident, staging) in resident.iter().zip(staging) {
            blit.copy_from_buffer(staging, 0, &resident.buffer, 0, resident.buffer.length());
        }
        blit.end_encoding();
        command.commit();
        command
    });
    command.wait_until_completed();
    if command.status() == MTLCommandBufferStatus::Completed {
        Ok(())
    } else {
        Err(BackendError::Upload {
            device: descriptor.id.clone(),
            message: command_error(&command)
                .unwrap_or_else(|| format!("Metal upload status {:?}", command.status())),
        })
    }
}

#[allow(unexpected_cfgs)]
fn command_error(command: &CommandBufferRef) -> Option<String> {
    unsafe {
        let error: *mut Object = msg_send![command, error];
        if error.is_null() {
            return None;
        }
        let description: *mut Object = msg_send![error, localizedDescription];
        let bytes: *const i8 = msg_send![description, UTF8String];
        (!bytes.is_null()).then(|| CStr::from_ptr(bytes).to_string_lossy().into_owned())
    }
}

fn submission(descriptor: &DeviceDescriptor, message: impl Into<String>) -> BackendError {
    BackendError::Submission {
        device: descriptor.id.clone(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MetaValue, SourceFormat, SourceTensorRecord, TensorInfo, TensorSource};

    struct TestSource {
        info: TensorInfo,
        bytes: Vec<u8>,
    }

    impl TensorSource for TestSource {
        fn metadata(&self, _key: &str) -> Option<&MetaValue> {
            None
        }

        fn tensor_info(&self, name: &str) -> Option<&TensorInfo> {
            (name == self.info.name).then_some(&self.info)
        }

        fn tensor_slice(&self, name: &str) -> Option<&[u8]> {
            (name == self.info.name).then_some(&self.bytes)
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

    #[test]
    fn resident_chunks_are_whole_rows_and_cover_the_tail() {
        let resident = ResidentTensorPlan {
            tensor: TensorId(3),
            rows: 17..27,
            source_bytes: 17 * 68..27 * 68,
            arena_offset: 0,
        };
        let chunks = row_chunks(&resident, 68, 300).unwrap();
        assert_eq!(
            chunks
                .iter()
                .map(|chunk| (chunk.global_row_start, chunk.local_rows))
                .collect::<Vec<_>>(),
            vec![(17, 4), (21, 4), (25, 2)]
        );
        assert!(chunks
            .iter()
            .all(|chunk| (chunk.source_bytes.end - chunk.source_bytes.start) % 68 == 0));
    }

    fn descriptor() -> DeviceDescriptor {
        DeviceDescriptor {
            id: DeviceId::parse("metal0").unwrap(),
            backend: BackendKind::Metal,
            physical_key: "metal:0000000000000042".into(),
            name: "test Metal device".into(),
            usable_bytes: 1 << 20,
            max_allocation_bytes: 1 << 18,
            buffer_alignment: 4,
            unified_memory: true,
            capabilities: DeviceCapabilities {
                components: BTreeSet::from([ComponentId::Llm]),
                modes: BTreeSet::from([PlacementMode::Row]),
                layer_families: BTreeSet::new(),
                tensor_types: BTreeSet::from([GGMLType::Q8_0]),
            },
        }
    }

    #[test]
    fn immutable_snapshot_rejects_physical_or_limit_drift() {
        let expected = descriptor();
        let mut changed_identity = expected.clone();
        changed_identity.physical_key = "metal:0000000000000043".into();
        assert!(!immutable_descriptor_matches(&changed_identity, &expected));

        let mut changed_limit = expected.clone();
        changed_limit.max_allocation_bytes /= 2;
        assert!(!immutable_descriptor_matches(&changed_limit, &expected));
        assert!(immutable_descriptor_matches(&expected, &expected));
    }

    #[test]
    fn slot_validation_rejects_forged_bounds_before_open() {
        let descriptor = descriptor();
        let mut plan = DevicePlan {
            descriptor: descriptor.clone(),
            tensors: Vec::new(),
            slots: vec![SlotPlan {
                id: SlotId(0),
                kind: SlotKind::Activation,
                storage: SlotStorage::F32,
                byte_len: 16,
                alignment: descriptor.buffer_alignment,
                arena_offset: 0,
            }],
            programs: Vec::new(),
            memory: Default::default(),
        };
        assert!(validate_slots(&plan, descriptor.max_allocation_bytes).is_ok());

        plan.slots[0].byte_len = descriptor.max_allocation_bytes + 4;
        assert!(validate_slots(&plan, descriptor.max_allocation_bytes).is_err());
        plan.slots[0].byte_len = 16;
        plan.slots[0].alignment = 8;
        assert!(validate_slots(&plan, descriptor.max_allocation_bytes).is_err());
    }

    #[test]
    fn q8_result_input_is_rejected_before_resource_creation() {
        let device = Device::system_default().expect("Metal tests require a device");
        let descriptor = DeviceDescriptor {
            id: DeviceId::parse("metal0").unwrap(),
            backend: BackendKind::Metal,
            physical_key: format!("metal:{:016x}", device.registry_id()),
            name: device.name().to_owned(),
            usable_bytes: 1 << 20,
            max_allocation_bytes: 1 << 18,
            buffer_alignment: 4,
            unified_memory: device.has_unified_memory(),
            capabilities: DeviceCapabilities {
                components: BTreeSet::from([ComponentId::Llm]),
                modes: BTreeSet::from([PlacementMode::Row]),
                layer_families: BTreeSet::new(),
                tensor_types: BTreeSet::from([GGMLType::Q8_0]),
            },
        };
        let adapter = AdapterInfo {
            descriptor: descriptor.clone(),
            registry_id: device.registry_id(),
            max_buffer_length: 1 << 18,
            device,
        };
        let catalog = TensorCatalog::from_sources(vec![(
            ComponentId::Llm,
            Arc::new(TestSource {
                info: TensorInfo {
                    name: "weight".into(),
                    dims: vec![32, 1],
                    ggml_type: GGMLType::Q8_0,
                    offset: 0,
                },
                bytes: vec![0; 34],
            }),
        )])
        .unwrap();
        let plan = DevicePlan {
            descriptor,
            tensors: vec![ResidentTensorPlan {
                tensor: TensorId(0),
                rows: 0..1,
                source_bytes: 0..34,
                arena_offset: 0,
            }],
            slots: vec![
                SlotPlan {
                    id: SlotId(0),
                    kind: SlotKind::Result,
                    storage: SlotStorage::F32,
                    byte_len: 128,
                    alignment: 4,
                    arena_offset: 0,
                },
                SlotPlan {
                    id: SlotId(1),
                    kind: SlotKind::Result,
                    storage: SlotStorage::F32,
                    byte_len: 4,
                    alignment: 4,
                    arena_offset: 128,
                },
            ],
            programs: vec![ProgramPlan {
                id: ProgramId(0),
                kind: ProgramKind::Q8Rows {
                    tensor: TensorId(0),
                    rows: 0..1,
                    batch_capacity: 1,
                },
                input: SlotId(0),
                output: SlotId(1),
                layer_ops: Vec::new(),
            }],
            memory: super::super::MemoryPlan {
                resident_bytes: 34,
                scratch_bytes: 132,
                staging_bytes: 34,
                required_bytes: 200,
                largest_allocation_bytes: 132,
                ..super::super::MemoryPlan::default()
            },
        };

        assert!(validate_plan(&plan, &catalog, &adapter).is_err());
    }

    #[test]
    fn final_fence_drains_predecessors_with_one_host_wait_boundary() {
        let provider = MetalProvider::new().unwrap();
        let descriptor = provider.enumerate().unwrap().remove(0);
        let catalog = Arc::new(
            TensorCatalog::from_sources(vec![(
                ComponentId::Llm,
                Arc::new(TestSource {
                    info: TensorInfo {
                        name: "weight".into(),
                        dims: vec![32, 1],
                        ggml_type: GGMLType::Q8_0,
                        offset: 0,
                    },
                    bytes: vec![0; 34],
                }),
            )])
            .unwrap(),
        );
        let plan = DevicePlan {
            descriptor: descriptor.clone(),
            tensors: vec![ResidentTensorPlan {
                tensor: TensorId(0),
                rows: 0..1,
                source_bytes: 0..34,
                arena_offset: 0,
            }],
            slots: vec![
                SlotPlan {
                    id: SlotId(0),
                    kind: SlotKind::Activation,
                    storage: SlotStorage::F32,
                    byte_len: 128,
                    alignment: descriptor.buffer_alignment,
                    arena_offset: 0,
                },
                SlotPlan {
                    id: SlotId(1),
                    kind: SlotKind::Result,
                    storage: SlotStorage::F32,
                    byte_len: 4,
                    alignment: descriptor.buffer_alignment,
                    arena_offset: 128,
                },
            ],
            programs: vec![ProgramPlan {
                id: ProgramId(0),
                kind: ProgramKind::Q8Rows {
                    tensor: TensorId(0),
                    rows: 0..1,
                    batch_capacity: 1,
                },
                input: SlotId(0),
                output: SlotId(1),
                layer_ops: vec![LayerOp::Q8Matmul {
                    input: SlotId(0),
                    weight: TensorId(0),
                    output: SlotId(1),
                }],
            }],
            memory: super::super::MemoryPlan {
                resident_bytes: 34,
                scratch_bytes: 132,
                staging_bytes: 128,
                required_bytes: 294,
                largest_allocation_bytes: 132,
                ..super::super::MemoryPlan::default()
            },
        };
        let mut session = provider.open(&descriptor, &plan, catalog).unwrap();
        session.write_f32(SlotId(0), &[1.0; 32]).unwrap();
        let params = RunParams {
            token_count: 1,
            position_start: 0,
            mrope_positions: &[],
            token_ids: &[],
        };
        session.submit(ProgramId(0), &params).unwrap();
        session.submit(ProgramId(0), &params).unwrap();
        let final_fence = session.submit(ProgramId(0), &params).unwrap();
        session.wait(final_fence).unwrap();
        assert_eq!(session.stats().host_waits, 1);
    }

    #[test]
    fn predecessor_failure_clears_pending_and_poisons_future_work() {
        let mut pending = VecDeque::from([(FenceId(1), "fails"), (FenceId(2), "must not run")]);
        let mut poisoned = false;
        let error = drain_pending(&mut pending, FenceId(2), &mut poisoned, |command| {
            if command == "fails" {
                Err(BackendError::Submission {
                    device: DeviceId::parse("metal0").unwrap(),
                    message: "injected predecessor failure".into(),
                })
            } else {
                panic!("successor ran after predecessor failure")
            }
        })
        .unwrap_err();
        assert!(matches!(error, BackendError::Submission { .. }));
        assert!(poisoned);
        assert!(pending.is_empty());
        assert!(matches!(
            poisoned.then_some(BackendError::PoisonedRun),
            Some(BackendError::PoisonedRun)
        ));
    }
}
