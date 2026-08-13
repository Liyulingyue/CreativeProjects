use super::device::{
    BackendError, BackendKind, DeviceCapabilities, DeviceDescriptor, DeviceDiscovery,
    DeviceProvider, DeviceSession, FenceId, LifecycleProbe, ProgramId, RunParams, SessionStats,
    SlotId,
};
use super::program::{
    DevicePlan, ProgramKind, ProgramPlan, ResidentTensorPlan, SlotKind, SlotPlan, SlotStorage,
};
use crate::thread_pool::ComputePool;
use crate::{ComponentId, DeviceId, GGMLType, PlacementMode, TensorCatalog};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::JoinHandle;

pub struct CpuProvider {
    thread_count: usize,
}

impl CpuProvider {
    pub fn new(thread_count: usize) -> Self {
        Self {
            thread_count: thread_count.max(1),
        }
    }

    fn descriptor() -> DeviceDescriptor {
        let name = if crate::ops::has_avx2_fma() {
            "CPU-AVX2"
        } else if cfg!(target_arch = "aarch64") {
            "CPU-NEON"
        } else {
            "CPU-Scalar"
        };
        DeviceDescriptor {
            id: DeviceId::parse("cpu0").expect("cpu0 is a valid device id"),
            backend: BackendKind::Cpu,
            physical_key: "cpu0".into(),
            name: name.into(),
            usable_bytes: usize::MAX as u64,
            max_allocation_bytes: usize::MAX as u64,
            buffer_alignment: std::mem::align_of::<f32>() as u64,
            unified_memory: true,
            capabilities: DeviceCapabilities {
                components: BTreeSet::from([ComponentId::Llm, ComponentId::Vision]),
                modes: BTreeSet::from([PlacementMode::Row]),
                layer_families: BTreeSet::new(),
                tensor_types: BTreeSet::from([GGMLType::F32, GGMLType::F16, GGMLType::Q8_0]),
            },
        }
    }
}

impl DeviceDiscovery for CpuProvider {
    fn backend(&self) -> BackendKind {
        BackendKind::Cpu
    }

    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, BackendError> {
        Ok(vec![Self::descriptor()])
    }
}

impl DeviceProvider for CpuProvider {
    fn open(
        &self,
        descriptor: &DeviceDescriptor,
        plan: &DevicePlan,
        catalog: Arc<TensorCatalog>,
    ) -> Result<Box<dyn DeviceSession>, BackendError> {
        if descriptor.backend != BackendKind::Cpu || descriptor != &plan.descriptor {
            return Err(BackendError::InvalidHandle);
        }

        let resident_bytes = validate_resident_ranges(descriptor, &catalog, &plan.tensors)?;
        let programs = plan
            .programs
            .iter()
            .cloned()
            .map(|program| (program.id, program))
            .collect::<BTreeMap<_, _>>();
        if programs.len() != plan.programs.len() {
            return Err(BackendError::InvalidHandle);
        }

        let mut max_batch = 1_usize;
        let mut max_q8_input = 0_usize;
        for program in programs.values() {
            match &program.kind {
                ProgramKind::Q8Rows {
                    tensor,
                    rows,
                    batch_capacity,
                } => {
                    let entry = catalog.entry(*tensor).ok_or(BackendError::InvalidHandle)?;
                    let n_in =
                        usize::try_from(entry.shape[0]).map_err(|_| BackendError::InvalidHandle)?;
                    let input = plan
                        .slots
                        .get(program.input.0 as usize)
                        .filter(|slot| {
                            slot.storage == SlotStorage::F32
                                && matches!(slot.kind, SlotKind::Activation | SlotKind::Scratch)
                        })
                        .ok_or(BackendError::InvalidHandle)?;
                    let output = plan
                        .slots
                        .get(program.output.0 as usize)
                        .filter(|slot| {
                            slot.storage == SlotStorage::F32 && slot.kind == SlotKind::Result
                        })
                        .ok_or(BackendError::InvalidHandle)?;
                    let input_bytes = (*batch_capacity as u64)
                        .checked_mul(n_in as u64)
                        .and_then(|values| values.checked_mul(size_of::<f32>() as u64));
                    let output_bytes = (*batch_capacity as u64)
                        .checked_mul(rows.len() as u64)
                        .and_then(|values| values.checked_mul(size_of::<f32>() as u64));
                    if entry.ggml_type != GGMLType::Q8_0
                        || n_in % 32 != 0
                        || program.input == program.output
                        || input_bytes.is_none_or(|bytes| bytes > input.byte_len)
                        || output_bytes.is_none_or(|bytes| bytes > output.byte_len)
                        || !plan
                            .tensors
                            .iter()
                            .any(|resident| resident.tensor == *tensor && resident.rows == *rows)
                    {
                        return Err(BackendError::InvalidHandle);
                    }
                    max_batch = max_batch.max(*batch_capacity as usize);
                    max_q8_input = max_q8_input.max(n_in);
                }
                ProgramKind::FinalNormQ8Logits { batch_capacity, .. } => {
                    max_batch = max_batch.max(*batch_capacity as usize);
                }
                ProgramKind::EmbeddingRows { tensor, row_count } => {
                    let entry = catalog.entry(*tensor).ok_or(BackendError::InvalidHandle)?;
                    let width =
                        usize::try_from(entry.shape[0]).map_err(|_| BackendError::InvalidHandle)?;
                    let output = plan
                        .slots
                        .get(program.output.0 as usize)
                        .filter(|slot| {
                            slot.storage == SlotStorage::F32
                                && matches!(slot.kind, SlotKind::Activation | SlotKind::Result)
                        })
                        .ok_or(BackendError::InvalidHandle)?;
                    let input = plan
                        .slots
                        .get(program.input.0 as usize)
                        .filter(|slot| {
                            slot.storage == SlotStorage::I8 && slot.kind == SlotKind::Scratch
                        })
                        .ok_or(BackendError::InvalidHandle)?;
                    let capacity = usize::try_from(output.byte_len / size_of::<f32>() as u64)
                        .map_err(|_| BackendError::InvalidHandle)?
                        .checked_div(width)
                        .ok_or(BackendError::InvalidHandle)?;
                    if *row_count as u64 != entry.row_count
                        || width == 0
                        || program.input == program.output
                        || input.byte_len < 4
                        || output.byte_len / (width as u64 * size_of::<f32>() as u64)
                            > input.byte_len / size_of::<u32>() as u64
                        || !plan.tensors.iter().any(|resident| {
                            resident.tensor == *tensor && resident.rows == (0..*row_count)
                        })
                    {
                        return Err(BackendError::InvalidHandle);
                    }
                    max_batch = max_batch.max(capacity);
                }
                ProgramKind::LayerSegment { .. } => {}
            }
        }

        let mut slots = Vec::with_capacity(plan.slots.len());
        for (index, slot) in plan.slots.iter().enumerate() {
            if slot.id.0 as usize != index {
                return Err(BackendError::InvalidHandle);
            }
            let divisor = match slot.storage {
                SlotStorage::F32 => 4,
                SlotStorage::F16 => 2,
                SlotStorage::I8 => 1,
            };
            if slot.byte_len % divisor != 0 {
                return Err(BackendError::InvalidHandle);
            }
            let len = usize::try_from(slot.byte_len / divisor)
                .map_err(|_| allocation_error(descriptor, "slot size does not fit usize"))?;
            slots.push(vec![0.0; len].into_boxed_slice());
        }

        let slot_ptrs = slots
            .iter_mut()
            .map(|slot| SlotPtr {
                ptr: slot.as_mut_ptr() as usize,
                len: slot.len(),
            })
            .collect();
        let worker = CpuWorker::start(
            self.thread_count,
            descriptor.id.clone(),
            Arc::clone(&catalog),
            programs.clone(),
            slot_ptrs,
            max_batch,
            max_q8_input,
        )?;
        let resident_count = plan.tensors.len() as u64;

        Ok(Box::new(CpuSession {
            descriptor: descriptor.clone(),
            catalog,
            resident: plan.tensors.clone(),
            slots,
            slot_plans: plan.slots.clone(),
            programs,
            worker,
            stats: SessionStats {
                resident_bytes,
                resident_allocations: resident_count,
                weight_uploads: resident_count,
                weight_upload_bytes: resident_bytes,
                ..SessionStats::default()
            },
        }))
    }
}

pub struct CpuSession {
    descriptor: DeviceDescriptor,
    catalog: Arc<TensorCatalog>,
    resident: Vec<ResidentTensorPlan>,
    slots: Vec<Box<[f32]>>,
    slot_plans: Vec<SlotPlan>,
    programs: BTreeMap<ProgramId, ProgramPlan>,
    worker: CpuWorker,
    stats: SessionStats,
}

impl DeviceSession for CpuSession {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.descriptor
    }

    fn write_f32(&mut self, slot: SlotId, values: &[f32]) -> Result<(), BackendError> {
        self.require_idle("write while CPU work is pending")?;
        self.slot_plans
            .get(slot.0 as usize)
            .filter(|plan| {
                plan.storage == SlotStorage::F32
                    && matches!(plan.kind, SlotKind::Activation | SlotKind::Scratch)
            })
            .ok_or(BackendError::InvalidHandle)?;
        let destination = self
            .slots
            .get_mut(slot.0 as usize)
            .ok_or(BackendError::InvalidHandle)?;
        if values.len() > destination.len() {
            return Err(BackendError::InvalidHandle);
        }
        destination[..values.len()].copy_from_slice(values);
        self.stats.activation_h2d_bytes += (values.len() * size_of::<f32>()) as u64;
        Ok(())
    }

    fn submit(
        &mut self,
        program: ProgramId,
        params: &RunParams<'_>,
    ) -> Result<FenceId, BackendError> {
        if !self.programs.contains_key(&program) {
            return Err(BackendError::InvalidHandle);
        }
        let fence = self.worker.submit(program, params)?;
        self.stats.submissions += 1;
        Ok(fence)
    }

    fn wait(&mut self, fence: FenceId) -> Result<(), BackendError> {
        if self.worker.pending != Some(fence) {
            return Err(BackendError::InvalidHandle);
        }
        let result = self.worker.wait(fence);
        self.stats.host_waits += 1;
        result
    }

    fn read_f32(&mut self, slot: SlotId, values: &mut [f32]) -> Result<(), BackendError> {
        self.require_idle("read while CPU work is pending")?;
        self.slot_plans
            .get(slot.0 as usize)
            .filter(|plan| {
                plan.storage == SlotStorage::F32
                    && matches!(plan.kind, SlotKind::Activation | SlotKind::Result)
            })
            .ok_or(BackendError::InvalidHandle)?;
        let source = self
            .slots
            .get(slot.0 as usize)
            .ok_or(BackendError::InvalidHandle)?;
        if values.len() > source.len() {
            return Err(BackendError::InvalidHandle);
        }
        values.copy_from_slice(&source[..values.len()]);
        self.stats.activation_d2h_bytes += (values.len() * size_of::<f32>()) as u64;
        Ok(())
    }

    fn reset_state(&mut self) -> Result<(), BackendError> {
        self.require_idle("reset while CPU work is pending")?;
        for slot in &mut self.slots {
            slot.fill(0.0);
        }
        Ok(())
    }

    fn stats(&self) -> SessionStats {
        let _ = (&self.catalog, &self.resident);
        self.stats.clone()
    }

    fn lifecycle_probe(&self) -> LifecycleProbe {
        LifecycleProbe::default()
    }
}

impl CpuSession {
    fn require_idle(&self, message: &'static str) -> Result<(), BackendError> {
        if self.worker.pending.is_some() {
            Err(BackendError::Submission {
                device: self.descriptor.id.clone(),
                message: message.into(),
            })
        } else {
            Ok(())
        }
    }
}

impl Drop for CpuSession {
    fn drop(&mut self) {
        self.worker.shutdown();
    }
}

struct CpuWorker {
    requests: Option<SyncSender<(FenceId, ProgramId)>>,
    completions: Receiver<(FenceId, Result<(), BackendError>)>,
    thread: Option<JoinHandle<()>>,
    params: Box<CpuRunParams>,
    next_fence: u64,
    pending: Option<FenceId>,
    device: DeviceId,
}

impl CpuWorker {
    #[allow(clippy::too_many_arguments)]
    fn start(
        thread_count: usize,
        device: DeviceId,
        catalog: Arc<TensorCatalog>,
        programs: BTreeMap<ProgramId, ProgramPlan>,
        slots: Vec<SlotPtr>,
        parameter_capacity: usize,
        q8_len: usize,
    ) -> Result<Self, BackendError> {
        let (requests, request_rx) = mpsc::sync_channel(1);
        let (completion_tx, completions) = mpsc::sync_channel(1);
        let (ready_tx, ready_rx) = mpsc::sync_channel(0);
        let mut params = Box::new(CpuRunParams::new(parameter_capacity));
        let params_ptr = (&mut *params as *mut CpuRunParams) as usize;
        let q8 = vec![0; q8_len].into_boxed_slice();
        let scales = vec![0.0; q8_len / 32].into_boxed_slice();
        let worker_device = device.clone();
        let thread = std::thread::Builder::new()
            .name(format!("{}-compiled-session", device.as_str()))
            .spawn(move || {
                let mut state = WorkerState {
                    pool: ComputePool::new(thread_count),
                    device: worker_device,
                    catalog,
                    programs,
                    slots,
                    params_ptr,
                    q8,
                    scales,
                };
                let _ = ready_tx.send(());
                while let Ok((fence, program)) = request_rx.recv() {
                    let result = state.execute(program);
                    if completion_tx.send((fence, result)).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| allocation_error_for(&device, error.to_string()))?;
        ready_rx
            .recv()
            .map_err(|_| submission_error(&device, "CPU worker failed during initialization"))?;
        Ok(Self {
            requests: Some(requests),
            completions,
            thread: Some(thread),
            params,
            next_fence: 0,
            pending: None,
            device,
        })
    }

    fn submit(
        &mut self,
        program: ProgramId,
        params: &RunParams<'_>,
    ) -> Result<FenceId, BackendError> {
        if self.pending.is_some()
            || params.token_count as usize > self.params.token_ids.len()
            || params.token_ids.len() > self.params.token_ids.len()
            || params.mrope_positions.len() > self.params.mrope_positions.len()
        {
            return Err(submission_error(
                &self.device,
                "CPU worker is busy or parameters exceed capacity",
            ));
        }
        self.params.token_count = params.token_count;
        self.params.position_start = params.position_start;
        self.params.token_ids_len = params.token_ids.len();
        self.params.token_ids[..params.token_ids.len()].copy_from_slice(params.token_ids);
        self.params.mrope_positions_len = params.mrope_positions.len();
        self.params.mrope_positions[..params.mrope_positions.len()]
            .copy_from_slice(params.mrope_positions);

        let fence = FenceId(
            self.next_fence
                .checked_add(1)
                .ok_or_else(|| submission_error(&self.device, "CPU fence overflow"))?,
        );
        match self
            .requests
            .as_ref()
            .ok_or_else(|| submission_error(&self.device, "CPU worker stopped"))?
            .try_send((fence, program))
        {
            Ok(()) => {
                self.next_fence = fence.0;
                self.pending = Some(fence);
                Ok(fence)
            }
            Err(TrySendError::Full(_)) => {
                Err(submission_error(&self.device, "CPU request queue is full"))
            }
            Err(TrySendError::Disconnected(_)) => {
                Err(submission_error(&self.device, "CPU worker stopped"))
            }
        }
    }

    fn wait(&mut self, fence: FenceId) -> Result<(), BackendError> {
        if self.pending != Some(fence) {
            return Err(BackendError::InvalidHandle);
        }
        let (completed, result) = self
            .completions
            .recv()
            .map_err(|_| submission_error(&self.device, "CPU worker stopped before completion"))?;
        self.pending = None;
        if completed != fence {
            return Err(BackendError::InvalidHandle);
        }
        result
    }

    fn shutdown(&mut self) {
        self.requests.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for CpuWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct CpuRunParams {
    token_count: u32,
    position_start: u32,
    mrope_positions: Box<[[u32; 4]]>,
    mrope_positions_len: usize,
    token_ids: Box<[u32]>,
    token_ids_len: usize,
}

impl CpuRunParams {
    fn new(capacity: usize) -> Self {
        Self {
            token_count: 0,
            position_start: 0,
            mrope_positions: vec![[0; 4]; capacity].into_boxed_slice(),
            mrope_positions_len: 0,
            token_ids: vec![0; capacity].into_boxed_slice(),
            token_ids_len: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct SlotPtr {
    ptr: usize,
    len: usize,
}

struct WorkerState {
    pool: ComputePool,
    device: DeviceId,
    catalog: Arc<TensorCatalog>,
    programs: BTreeMap<ProgramId, ProgramPlan>,
    slots: Vec<SlotPtr>,
    params_ptr: usize,
    q8: Box<[u8]>,
    scales: Box<[f32]>,
}

impl WorkerState {
    fn execute(&mut self, program: ProgramId) -> Result<(), BackendError> {
        let plan = self
            .programs
            .get(&program)
            .ok_or(BackendError::InvalidHandle)?;
        let input = plan.input;
        let output = plan.output;
        let kind = plan.kind.clone();
        match kind {
            ProgramKind::Q8Rows {
                tensor,
                rows,
                batch_capacity,
            } => self.execute_q8_rows(
                tensor,
                rows.start as usize,
                rows.end as usize,
                batch_capacity as usize,
                input,
                output,
            ),
            ProgramKind::EmbeddingRows { tensor, row_count } => {
                self.execute_embedding_rows(tensor, row_count as usize, output)
            }
            ProgramKind::LayerSegment { .. } | ProgramKind::FinalNormQ8Logits { .. } => {
                Err(BackendError::Unsupported {
                    device: self.device.clone(),
                    operation: "compiled CPU program kind",
                })
            }
        }
    }

    fn execute_q8_rows(
        &mut self,
        tensor: crate::TensorId,
        row_start: usize,
        row_end: usize,
        batch_capacity: usize,
        input: SlotId,
        output: SlotId,
    ) -> Result<(), BackendError> {
        // SAFETY: submit does not mutate the preallocated parameter box until wait clears the
        // single pending fence, and the request channel synchronizes this read with that write.
        let params = unsafe { &*(self.params_ptr as *const CpuRunParams) };
        let batch = params.token_count as usize;
        if batch > batch_capacity {
            return Err(BackendError::InvalidHandle);
        }
        let entry = self
            .catalog
            .entry(tensor)
            .ok_or(BackendError::InvalidHandle)?;
        let n_in = usize::try_from(entry.shape[0]).map_err(|_| BackendError::InvalidHandle)?;
        let local_rows = row_end
            .checked_sub(row_start)
            .ok_or(BackendError::InvalidHandle)?;
        let input_len = batch.checked_mul(n_in).ok_or(BackendError::InvalidHandle)?;
        let output_len = batch
            .checked_mul(local_rows)
            .ok_or(BackendError::InvalidHandle)?;
        let input_slot = *self
            .slots
            .get(input.0 as usize)
            .ok_or(BackendError::InvalidHandle)?;
        let output_slot = *self
            .slots
            .get(output.0 as usize)
            .ok_or(BackendError::InvalidHandle)?;
        if input_len > input_slot.len
            || output_len > output_slot.len
            || n_in > self.q8.len()
            || n_in / 32 > self.scales.len()
        {
            return Err(BackendError::InvalidHandle);
        }
        let weight = self
            .catalog
            .bytes(tensor)
            .map_err(|_| BackendError::InvalidHandle)?;
        // SAFETY: slots point into session-owned boxes that remain stable until CpuSession::drop
        // joins this worker. Host slot access is rejected while a fence is pending, and the input
        // and output plans were validated as distinct allocations during open.
        let inputs = unsafe { std::slice::from_raw_parts(input_slot.ptr as *const f32, input_len) };
        let outputs =
            unsafe { std::slice::from_raw_parts_mut(output_slot.ptr as *mut f32, output_len) };
        for item in 0..batch {
            crate::ops::quantize_q8_0_into(
                &inputs[item * n_in..(item + 1) * n_in],
                n_in,
                &mut self.q8[..n_in],
                &mut self.scales[..n_in / 32],
            );
            matmul_q8_range_with_pool(
                &self.pool,
                weight,
                &self.q8[..n_in],
                &self.scales[..n_in / 32],
                &mut outputs[item * local_rows..(item + 1) * local_rows],
                n_in,
                row_start,
                row_end,
            );
        }
        Ok(())
    }

    fn execute_embedding_rows(
        &mut self,
        tensor: crate::TensorId,
        row_count: usize,
        output: SlotId,
    ) -> Result<(), BackendError> {
        // SAFETY: identical single-pending-fence synchronization to execute_q8_rows.
        let params = unsafe { &*(self.params_ptr as *const CpuRunParams) };
        let batch = params.token_count as usize;
        if batch != params.token_ids_len {
            return Err(BackendError::InvalidHandle);
        }
        let entry = self
            .catalog
            .entry(tensor)
            .ok_or(BackendError::InvalidHandle)?;
        let width = usize::try_from(entry.shape[0]).map_err(|_| BackendError::InvalidHandle)?;
        let output_len = batch
            .checked_mul(width)
            .ok_or(BackendError::InvalidHandle)?;
        let output_slot = *self
            .slots
            .get(output.0 as usize)
            .ok_or(BackendError::InvalidHandle)?;
        if output_len > output_slot.len
            || params.token_ids[..batch]
                .iter()
                .any(|token| *token as usize >= row_count)
        {
            return Err(BackendError::InvalidHandle);
        }
        let weight = self
            .catalog
            .bytes(tensor)
            .map_err(|_| BackendError::InvalidHandle)?;
        // SAFETY: the output slot is worker-exclusive while this fence is pending.
        let outputs =
            unsafe { std::slice::from_raw_parts_mut(output_slot.ptr as *mut f32, output_len) };
        for (item, &token) in params.token_ids[..batch].iter().enumerate() {
            let row = &mut outputs[item * width..(item + 1) * width];
            match entry.ggml_type {
                GGMLType::Q8_0 => crate::ops::embedding_lookup_q8_0(weight, token, width, row),
                GGMLType::F32 => {
                    let start = token as usize * width * size_of::<f32>();
                    for (value, bytes) in row
                        .iter_mut()
                        .zip(weight[start..start + width * size_of::<f32>()].chunks_exact(4))
                    {
                        *value = f32::from_le_bytes(bytes.try_into().unwrap());
                    }
                }
                GGMLType::F16 => {
                    let start = token as usize * width * size_of::<u16>();
                    for (value, bytes) in row
                        .iter_mut()
                        .zip(weight[start..start + width * size_of::<u16>()].chunks_exact(2))
                    {
                        *value =
                            half::f16::from_bits(u16::from_le_bytes(bytes.try_into().unwrap()))
                                .to_f32();
                    }
                }
                _ => return Err(BackendError::InvalidHandle),
            }
        }
        Ok(())
    }
}

fn matmul_q8_range_with_pool(
    pool: &ComputePool,
    weight: &[u8],
    q8: &[u8],
    scales: &[f32],
    output: &mut [f32],
    n_in: usize,
    row_start: usize,
    row_end: usize,
) {
    let output_ptr = output.as_mut_ptr() as usize;
    let rows = row_end - row_start;
    pool.compute(|thread, thread_count| {
        let start = row_start + thread * rows / thread_count;
        let end = row_start + (thread + 1) * rows / thread_count;
        if start == end {
            return;
        }
        // SAFETY: each pool thread receives a disjoint portion of output.
        let local = unsafe {
            std::slice::from_raw_parts_mut(
                (output_ptr as *mut f32).add(start - row_start),
                end - start,
            )
        };
        crate::ops::matmul_q8_0_quantized_range(weight, q8, scales, local, n_in, start, end);
    });
}

fn validate_resident_ranges(
    descriptor: &DeviceDescriptor,
    catalog: &TensorCatalog,
    resident: &[ResidentTensorPlan],
) -> Result<u64, BackendError> {
    let mut total = 0_u64;
    for plan in resident {
        let entry = catalog
            .entry(plan.tensor)
            .ok_or_else(|| BackendError::Upload {
                device: descriptor.id.clone(),
                message: "resident tensor is missing from catalog".into(),
            })?;
        catalog
            .bytes(plan.tensor)
            .map_err(|error| BackendError::Upload {
                device: descriptor.id.clone(),
                message: error.to_string(),
            })?;
        let expected_start = entry
            .segment_byte_range
            .start
            .checked_add(u64::from(plan.rows.start) * entry.row_bytes);
        let expected_end = entry
            .segment_byte_range
            .start
            .checked_add(u64::from(plan.rows.end) * entry.row_bytes);
        if plan.rows.start > plan.rows.end
            || u64::from(plan.rows.end) > entry.row_count
            || expected_start != Some(plan.source_bytes.start)
            || expected_end != Some(plan.source_bytes.end)
        {
            return Err(BackendError::Upload {
                device: descriptor.id.clone(),
                message: "resident tensor range does not resolve in catalog".into(),
            });
        }
        total = total
            .checked_add(plan.source_bytes.end - plan.source_bytes.start)
            .ok_or_else(|| allocation_error(descriptor, "resident byte count overflow"))?;
    }
    Ok(total)
}

fn allocation_error(descriptor: &DeviceDescriptor, message: &'static str) -> BackendError {
    allocation_error_for(&descriptor.id, message)
}

fn allocation_error_for(device: &DeviceId, message: impl Into<String>) -> BackendError {
    BackendError::Allocation {
        device: device.clone(),
        message: message.into(),
    }
}

fn submission_error(device: &DeviceId, message: &'static str) -> BackendError {
    BackendError::Submission {
        device: device.clone(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compute::{
        DeviceDiscovery, DevicePlan, DeviceProvider, DeviceSession, LayerOp, MemoryPlan, ProgramId,
        ProgramKind, ProgramPlan, ResidentTensorPlan, RunParams, SlotId, SlotKind, SlotPlan,
        SlotStorage,
    };
    use crate::{
        ComponentId, GGMLType, MetaValue, SourceFormat, SourceTensorRecord, TensorCatalog,
        TensorId, TensorInfo, TensorSource,
    };
    use std::ops::Range;
    use std::sync::Arc;

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

    fn q8_program_fixture(
        batch: usize,
        n_in: usize,
        n_out: usize,
        rows: Range<u32>,
    ) -> (Arc<TensorCatalog>, DevicePlan, Vec<f32>) {
        let mut bytes = Vec::with_capacity(n_out * n_in / 32 * 34);
        for row in 0..n_out {
            for block in 0..n_in / 32 {
                let scale = half::f16::from_f32(0.01 * (1 + (row + block) % 7) as f32);
                bytes.extend_from_slice(&scale.to_bits().to_le_bytes());
                bytes.extend(
                    (0..32).map(|lane| ((row * 3 + block * 5 + lane) as i32 % 31 - 15) as i8 as u8),
                );
            }
        }
        let catalog = Arc::new(
            TensorCatalog::from_sources(vec![(
                ComponentId::Llm,
                Arc::new(TestSource {
                    info: TensorInfo {
                        name: "weight".into(),
                        dims: vec![n_in as u64, n_out as u64],
                        ggml_type: GGMLType::Q8_0,
                        offset: 0,
                    },
                    bytes,
                }),
            )])
            .unwrap(),
        );
        let descriptor = CpuProvider::new(2).enumerate().unwrap().remove(0);
        let row_bytes = (n_in / 32 * 34) as u64;
        let resident_bytes = u64::from(rows.end - rows.start) * row_bytes;
        let slots = vec![
            SlotPlan {
                id: SlotId(0),
                kind: SlotKind::Activation,
                storage: SlotStorage::F32,
                byte_len: (batch * n_in * size_of::<f32>()) as u64,
                alignment: 16,
                arena_offset: 0,
            },
            SlotPlan {
                id: SlotId(1),
                kind: SlotKind::Result,
                storage: SlotStorage::F32,
                byte_len: (batch * rows.len() * 4) as u64,
                alignment: 16,
                arena_offset: (batch * n_in * size_of::<f32>()) as u64,
            },
        ];
        let scratch_bytes = slots.iter().map(|slot| slot.byte_len).sum::<u64>();
        let plan = DevicePlan {
            descriptor,
            tensors: vec![ResidentTensorPlan {
                tensor: TensorId(0),
                rows: rows.clone(),
                source_bytes: u64::from(rows.start) * row_bytes..u64::from(rows.end) * row_bytes,
                arena_offset: 0,
            }],
            slots,
            programs: vec![ProgramPlan {
                id: ProgramId(0),
                kind: ProgramKind::Q8Rows {
                    tensor: TensorId(0),
                    rows,
                    batch_capacity: batch as u32,
                },
                input: SlotId(0),
                output: SlotId(1),
                layer_ops: vec![LayerOp::Q8Matmul {
                    input: SlotId(0),
                    weight: TensorId(0),
                    output: SlotId(1),
                }],
            }],
            memory: MemoryPlan {
                resident_bytes,
                scratch_bytes,
                staging_bytes: resident_bytes,
                required_bytes: resident_bytes * 2 + scratch_bytes,
                largest_allocation_bytes: resident_bytes.max(scratch_bytes),
                ..MemoryPlan::default()
            },
        };
        let input = (0..batch * n_in)
            .map(|index| (index as i32 % 19 - 9) as f32 * 0.07)
            .collect();
        (catalog, plan, input)
    }

    fn open_cpu_session(catalog: &Arc<TensorCatalog>, plan: &DevicePlan) -> Box<dyn DeviceSession> {
        CpuProvider::new(2)
            .open(&plan.descriptor, plan, Arc::clone(catalog))
            .unwrap()
    }

    fn cpu_q8_range_reference(
        catalog: &TensorCatalog,
        input: &[f32],
        rows: Range<usize>,
    ) -> Vec<f32> {
        let n_in = 64;
        let mut q8 = vec![0; n_in];
        let mut scales = vec![0.0; n_in / 32];
        let mut output = vec![0.0; input.len() / n_in * rows.len()];
        for (item, values) in input.chunks_exact(n_in).enumerate() {
            crate::ops::quantize_q8_0_into(values, n_in, &mut q8, &mut scales);
            crate::ops::matmul_q8_0_quantized_range(
                catalog.bytes(TensorId(0)).unwrap(),
                &q8,
                &scales,
                &mut output[item * rows.len()..(item + 1) * rows.len()],
                n_in,
                rows.start,
                rows.end,
            );
        }
        output
    }

    fn assert_close(actual: &[f32], expected: &[f32], atol: f32, rtol: f32) {
        assert_eq!(actual.len(), expected.len());
        for (index, (&actual, &expected)) in actual.iter().zip(expected).enumerate() {
            assert!(
                (actual - expected).abs() <= atol + rtol * expected.abs(),
                "mismatch at {index}: actual={actual}, expected={expected}"
            );
        }
    }

    #[test]
    fn compiled_q8_rows_match_existing_cpu_kernel_for_batch_and_offset() {
        let (catalog, plan, input) = q8_program_fixture(2, 64, 129, 17..113);
        let mut session = open_cpu_session(&catalog, &plan);
        session.write_f32(SlotId(0), &input).unwrap();
        let fence = session
            .submit(
                ProgramId(0),
                &RunParams {
                    token_count: 2,
                    position_start: 0,
                    mrope_positions: &[],
                    token_ids: &[],
                },
            )
            .unwrap();
        assert_eq!(fence, FenceId(1));
        assert!(matches!(
            session.wait(FenceId(2)),
            Err(BackendError::InvalidHandle)
        ));
        assert_eq!(session.stats().host_waits, 0);
        session.wait(fence).unwrap();
        let mut actual = vec![0.0; 2 * 96];
        session.read_f32(SlotId(1), &mut actual).unwrap();
        assert_close(
            &actual,
            &cpu_q8_range_reference(&catalog, &input, 17..113),
            1e-4,
            1e-4,
        );

        let before = session.stats();
        let second_fence = session
            .submit(
                ProgramId(0),
                &RunParams {
                    token_count: 2,
                    position_start: 0,
                    mrope_positions: &[],
                    token_ids: &[],
                },
            )
            .unwrap();
        assert_eq!(second_fence, FenceId(2));
        session.wait(second_fence).unwrap();
        let after = session.stats();
        assert_eq!(before.weight_uploads, after.weight_uploads);
        assert_eq!(before.resident_allocations, after.resident_allocations);
        assert_eq!(after.submissions, before.submissions + 1);
        assert_eq!(after.host_waits, before.host_waits + 1);
    }

    #[test]
    fn opening_cpu_session_borrows_each_resident_range_once() {
        let (catalog, plan, _) = q8_program_fixture(2, 64, 129, 17..113);
        let session = open_cpu_session(&catalog, &plan);
        let stats = session.stats();

        assert_eq!(stats.resident_bytes, 96 * 68);
        assert_eq!(stats.resident_allocations, 1);
        assert_eq!(stats.weight_uploads, 1);
        assert_eq!(stats.weight_upload_bytes, 96 * 68);
    }

    #[test]
    fn cpu_rejects_legacy_i8_q8_public_input() {
        let (catalog, mut plan, _) = q8_program_fixture(2, 64, 129, 17..113);
        plan.slots[0].kind = SlotKind::Scratch;
        plan.slots[0].storage = SlotStorage::I8;

        assert!(CpuProvider::new(2)
            .open(&plan.descriptor, &plan, catalog)
            .is_err());
    }

    #[test]
    fn compiled_embedding_rows_decode_requested_q8_tokens() {
        let (catalog, mut plan, _) = q8_program_fixture(2, 64, 129, 0..129);
        plan.slots[0].kind = SlotKind::Scratch;
        plan.slots[0].storage = SlotStorage::I8;
        plan.slots[0].byte_len = 2 * size_of::<u32>() as u64;
        plan.slots[1].byte_len = 2 * 64 * size_of::<f32>() as u64;
        plan.programs[0] = ProgramPlan {
            id: ProgramId(0),
            kind: ProgramKind::EmbeddingRows {
                tensor: TensorId(0),
                row_count: 129,
            },
            input: SlotId(0),
            output: SlotId(1),
            layer_ops: Vec::new(),
        };
        let mut session = open_cpu_session(&catalog, &plan);
        let fence = session
            .submit(
                ProgramId(0),
                &RunParams {
                    token_count: 2,
                    position_start: 0,
                    mrope_positions: &[],
                    token_ids: &[17, 112],
                },
            )
            .unwrap();
        session.wait(fence).unwrap();
        let mut actual = vec![0.0; 2 * 64];
        session.read_f32(SlotId(1), &mut actual).unwrap();
        let mut expected = vec![0.0; 2 * 64];
        for (item, token) in [17, 112].into_iter().enumerate() {
            crate::ops::embedding_lookup_q8_0(
                catalog.bytes(TensorId(0)).unwrap(),
                token,
                64,
                &mut expected[item * 64..(item + 1) * 64],
            );
        }
        assert_eq!(actual, expected);
    }

    #[test]
    fn cpu_descriptor_advertises_only_implemented_program_modes() {
        let descriptor = CpuProvider::new(2).enumerate().unwrap().remove(0);

        assert_eq!(
            descriptor.capabilities.modes,
            BTreeSet::from([PlacementMode::Row])
        );
        assert!(descriptor.capabilities.layer_families.is_empty());
    }
}
