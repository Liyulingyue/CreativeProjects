use super::device::{
    BackendError, BackendKind, DeviceCapabilities, DeviceDescriptor, DeviceDiscovery,
    DeviceProvider, DeviceSession, FenceId, LifecycleProbe, ProgramId, RunParams, SessionStats,
    SlotId,
};
use super::program::{
    DevicePlan, ProgramKind, ProgramPlan, ResidentTensorPlan, SlotKind, SlotPlan, SlotStorage,
};
use crate::{ComponentId, DeviceId, GGMLType, PlacementMode, TensorCatalog, TensorId};
use metal::objc::{msg_send, rc::autoreleasepool, runtime::Object, sel, sel_impl};
use metal::{
    Buffer, CommandBuffer, CommandBufferRef, CommandQueue, CompileOptions, ComputePipelineState,
    Device, Library, MTLCommandBufferStatus, MTLResourceOptions, MTLSize, NSRange,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::CStr;
use std::mem::size_of;
use std::ops::Range;
use std::sync::Arc;

const Q8_BLOCK_ELEMENTS: u64 = 32;
const Q8_BLOCK_BYTES: u64 = 34;

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
                        modes: BTreeSet::from([PlacementMode::Row]),
                        layer_families: BTreeSet::new(),
                        tensor_types: BTreeSet::from([GGMLType::Q8_0]),
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

struct ProgramSpec {
    plan: ProgramPlan,
    chunks: Vec<usize>,
    n_in: u32,
    output_stride: u32,
    mode: u32,
}

struct ValidatedPlan {
    slots: BTreeMap<SlotId, SlotPlan>,
    chunks: Vec<ChunkSpec>,
    programs: Vec<ProgramSpec>,
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
}

struct ProgramResource {
    plan: ProgramPlan,
    chunks: Vec<usize>,
    n_in: u32,
    output_stride: u32,
    mode: u32,
}

struct Pending {
    id: FenceId,
    command: CommandBuffer,
}

pub struct MetalSession {
    descriptor: DeviceDescriptor,
    queue: CommandQueue,
    _library: Library,
    pipeline: ComputePipelineState,
    resident: Vec<ResidentChunk>,
    slots: BTreeMap<SlotId, SlotResource>,
    programs: BTreeMap<ProgramId, ProgramResource>,
    pending: Option<Pending>,
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
        let function =
            library
                .get_function("q8_rows", None)
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

        let mut slots = BTreeMap::new();
        for plan in validated.slots.into_values() {
            let shared = plan.kind == SlotKind::Result;
            let buffer = device.new_buffer(
                plan.byte_len,
                if shared {
                    MTLResourceOptions::StorageModeShared
                } else {
                    MTLResourceOptions::StorageModePrivate
                },
            );
            let staging = (!shared)
                .then(|| device.new_buffer(plan.byte_len, MTLResourceOptions::StorageModeShared));
            slots.insert(
                plan.id,
                SlotResource {
                    plan,
                    buffer,
                    staging,
                },
            );
        }
        let programs = validated
            .programs
            .into_iter()
            .map(|spec| {
                (
                    spec.plan.id,
                    ProgramResource {
                        plan: spec.plan,
                        chunks: spec.chunks,
                        n_in: spec.n_in,
                        output_stride: spec.output_stride,
                        mode: spec.mode,
                    },
                )
            })
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
            pipeline,
            resident,
            slots,
            programs,
            pending: None,
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
        } else if self.pending.is_some() {
            Err(submission(&self.descriptor, "Metal work is pending"))
        } else {
            Ok(())
        }
    }

    fn finish_command(&mut self, command: &CommandBufferRef) -> Result<(), BackendError> {
        command.wait_until_completed();
        self.stats.host_waits += 1;
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
            .filter(|resource| resource.plan.storage == SlotStorage::F32)
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
        self.stats.activation_h2d_bytes += byte_len;
        Ok(())
    }

    fn submit(
        &mut self,
        program: ProgramId,
        params: &RunParams<'_>,
    ) -> Result<FenceId, BackendError> {
        self.require_idle()?;
        let resource = self
            .programs
            .get(&program)
            .ok_or(BackendError::InvalidHandle)?;
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
        let id = FenceId(self.next_fence);
        let command = autoreleasepool(|| {
            let command = self.queue.new_command_buffer().to_owned();
            let blit = command.new_blit_command_encoder();
            blit.copy_from_buffer(input_staging, 0, &input.buffer, 0, input_bytes);
            blit.end_encoding();

            for &chunk_index in &resource.chunks {
                let chunk = &self.resident[chunk_index];
                let encoder = command.new_compute_command_encoder();
                encoder.set_compute_pipeline_state(&self.pipeline);
                encoder.set_buffer(0, Some(&chunk.buffer), 0);
                encoder.set_buffer(1, Some(&input.buffer), 0);
                encoder.set_buffer(2, Some(&output.buffer), 0);
                let output_row_start = chunk.spec.global_row_start.saturating_sub(match &resource
                    .plan
                    .kind
                {
                    ProgramKind::Q8Rows { rows, .. } => rows.start,
                    ProgramKind::EmbeddingRows { .. } => 0,
                    _ => 0,
                });
                let push = [
                    params.token_count,
                    resource.n_in,
                    chunk.spec.local_rows,
                    chunk.spec.global_row_start,
                    resource.output_stride,
                    resource.mode,
                    0,
                    output_row_start,
                ];
                encoder.set_bytes(3, 32, push.as_ptr().cast());
                let work = if resource.mode == 0 {
                    params.token_count.checked_mul(chunk.spec.local_rows)
                } else {
                    params.token_count.checked_mul(resource.n_in)
                }
                .expect("validated Metal dispatch size");
                let width = self.pipeline.thread_execution_width();
                encoder.dispatch_threads(
                    MTLSize::new(u64::from(work), 1, 1),
                    MTLSize::new(width, 1, 1),
                );
                encoder.end_encoding();
            }
            command.commit();
            command
        });
        self.pending = Some(Pending { id, command });
        self.next_fence += 1;
        self.stats.submissions += 1;
        Ok(id)
    }

    fn wait(&mut self, fence: FenceId) -> Result<(), BackendError> {
        self.pending
            .as_ref()
            .filter(|pending| pending.id == fence)
            .ok_or(BackendError::InvalidHandle)?;
        let pending = self
            .pending
            .take()
            .expect("validated pending Metal command");
        self.finish_command(&pending.command)
    }

    fn read_f32(&mut self, slot: SlotId, values: &mut [f32]) -> Result<(), BackendError> {
        self.require_idle()?;
        let resource = self
            .slots
            .get(&slot)
            .filter(|resource| {
                resource.plan.storage == SlotStorage::F32 && resource.plan.kind == SlotKind::Result
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
        if let Some(pending) = self.pending.take() {
            pending.command.wait_until_completed();
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
        if entry.ggml_type != GGMLType::Q8_0 || n_in == 0 || n_in % Q8_BLOCK_ELEMENTS as u32 != 0 {
            return Err(BackendError::InvalidHandle);
        }
        let row_bytes = u64::from(n_in) / Q8_BLOCK_ELEMENTS * Q8_BLOCK_BYTES;
        let start = chunks.len();
        let resident_chunks = row_chunks(resident, row_bytes, adapter.max_buffer_length)?;
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
        if !ids.insert(program.id) || program.input == program.output {
            return Err(BackendError::InvalidHandle);
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
                && input.kind != SlotKind::Result
                && output.storage == SlotStorage::F32
                && output.kind == SlotKind::Result
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
        programs.push(ProgramSpec {
            plan: program.clone(),
            chunks,
            n_in,
            output_stride,
            mode,
        });
    }
    if programs.is_empty() {
        return Err(BackendError::InvalidHandle);
    }

    let slot_bytes = slots
        .values()
        .try_fold(0_u64, |total, slot| total.checked_add(slot.byte_len));
    let staging_bytes = slots
        .values()
        .filter(|slot| slot.kind != SlotKind::Result)
        .try_fold(0_u64, |total, slot| total.checked_add(slot.byte_len));
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
        || runtime_bytes.max(upload_peak) > plan.memory.required_bytes
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
}
