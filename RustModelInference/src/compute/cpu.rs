use super::device::{
    BackendError, BackendKind, DeviceCapabilities, DeviceDescriptor, DeviceDiscovery,
    DeviceProvider, DeviceSession, FenceId, LifecycleProbe, ProgramId, RunParams, SessionStats,
    SlotId,
};
use super::program::{
    DevicePlan, LayerOp, ProgramKind, ProgramPlan, ResidentTensorPlan, SlotKind, SlotPlan,
    SlotStorage,
};
use crate::thread_pool::ComputePool;
use crate::{ComponentId, DeviceId, GGMLType, LayerFamily, PlacementMode, TensorCatalog};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
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
                modes: BTreeSet::from([PlacementMode::Layer, PlacementMode::Row]),
                layer_families: BTreeSet::from([LayerFamily::Qwen3]),
                tensor_types: BTreeSet::from([
                    GGMLType::F32,
                    GGMLType::F16,
                    GGMLType::Q4K,
                    GGMLType::Q5K,
                    GGMLType::Q6K,
                    GGMLType::Q8_0,
                ]),
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
        let mut max_attention_context = 0_usize;
        let mut auxiliary = BTreeMap::new();
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
            for op in &program.layer_ops {
                match op {
                    LayerOp::Q8Matmul { weight, .. } => {
                        let entry = catalog.entry(*weight).ok_or(BackendError::InvalidHandle)?;
                        if entry.ggml_type != GGMLType::Q8_0 {
                            return Err(BackendError::InvalidHandle);
                        }
                        max_q8_input = max_q8_input.max(
                            usize::try_from(entry.shape[0])
                                .map_err(|_| BackendError::InvalidHandle)?,
                        );
                    }
                    LayerOp::RmsNorm { weight, .. } => {
                        if !auxiliary.contains_key(weight) {
                            let entry =
                                catalog.entry(*weight).ok_or(BackendError::InvalidHandle)?;
                            let bytes = catalog
                                .bytes(*weight)
                                .map_err(|_| BackendError::InvalidHandle)?;
                            let values = match entry.ggml_type {
                                GGMLType::F32 => bytes
                                    .chunks_exact(4)
                                    .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
                                    .collect(),
                                GGMLType::F16 => bytes
                                    .chunks_exact(2)
                                    .map(|bytes| {
                                        crate::ops::f16_to_f32(u16::from_le_bytes(
                                            bytes.try_into().unwrap(),
                                        ))
                                    })
                                    .collect(),
                                _ => return Err(BackendError::InvalidHandle),
                            };
                            auxiliary.insert(*weight, values);
                        }
                    }
                    LayerOp::Attention {
                        context_capacity, ..
                    } => {
                        max_attention_context = max_attention_context
                            .max((*context_capacity as usize).div_ceil(32) * 32);
                    }
                    _ => {}
                }
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
            auxiliary,
            max_batch,
            max_q8_input,
            max_attention_context,
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
        if !self.worker.has_pending(fence) {
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
        if !self.worker.pending.is_empty() {
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
    requests: Option<SyncSender<(FenceId, ProgramId, usize)>>,
    completions: Receiver<(FenceId, Result<(), BackendError>)>,
    thread: Option<JoinHandle<()>>,
    params: Vec<Box<CpuRunParams>>,
    next_fence: u64,
    pending: VecDeque<(FenceId, usize)>,
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
        auxiliary: BTreeMap<crate::TensorId, Box<[f32]>>,
        parameter_capacity: usize,
        q8_len: usize,
        attention_context: usize,
    ) -> Result<Self, BackendError> {
        let in_flight_capacity = programs.len().max(1);
        let (requests, request_rx) = mpsc::sync_channel(in_flight_capacity);
        let (completion_tx, completions) = mpsc::sync_channel(in_flight_capacity);
        let (ready_tx, ready_rx) = mpsc::sync_channel(0);
        let mut params = (0..in_flight_capacity)
            .map(|_| Box::new(CpuRunParams::new(parameter_capacity)))
            .collect::<Vec<_>>();
        let param_ptrs = params
            .iter_mut()
            .map(|params| (&mut **params as *mut CpuRunParams) as usize)
            .collect::<Vec<_>>();
        let q8 = vec![0; q8_len].into_boxed_slice();
        let scales = vec![0.0; q8_len / 32].into_boxed_slice();
        let attention_scores = vec![0.0; attention_context].into_boxed_slice();
        let attention_values = vec![0.0; attention_context].into_boxed_slice();
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
                    auxiliary,
                    q8,
                    scales,
                    attention_scores,
                    attention_values,
                };
                let _ = ready_tx.send(());
                while let Ok((fence, program, params_index)) = request_rx.recv() {
                    let result = param_ptrs
                        .get(params_index)
                        .ok_or(BackendError::InvalidHandle)
                        .and_then(|params| state.execute(program, *params));
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
            pending: VecDeque::with_capacity(in_flight_capacity),
            device,
        })
    }

    fn submit(
        &mut self,
        program: ProgramId,
        params: &RunParams<'_>,
    ) -> Result<FenceId, BackendError> {
        let Some(params_index) = (0..self.params.len())
            .find(|index| self.pending.iter().all(|(_, pending)| pending != index))
        else {
            return Err(submission_error(
                &self.device,
                "CPU in-flight queue is full",
            ));
        };
        let destination = &mut self.params[params_index];
        if params.token_count as usize > destination.token_ids.len()
            || params.token_ids.len() > destination.token_ids.len()
            || params.mrope_positions.len() > destination.mrope_positions.len()
        {
            return Err(submission_error(
                &self.device,
                "CPU parameters exceed capacity",
            ));
        }
        destination.token_count = params.token_count;
        destination.position_start = params.position_start;
        destination.token_ids_len = params.token_ids.len();
        destination.token_ids[..params.token_ids.len()].copy_from_slice(params.token_ids);
        destination.mrope_positions_len = params.mrope_positions.len();
        destination.mrope_positions[..params.mrope_positions.len()]
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
            .try_send((fence, program, params_index))
        {
            Ok(()) => {
                self.next_fence = fence.0;
                self.pending.push_back((fence, params_index));
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
        if !self.has_pending(fence) {
            return Err(BackendError::InvalidHandle);
        }
        let mut first_error = None;
        loop {
            let (completed, result) = self.completions.recv().map_err(|_| {
                submission_error(&self.device, "CPU worker stopped before completion")
            })?;
            let (expected, _) = self
                .pending
                .pop_front()
                .ok_or(BackendError::InvalidHandle)?;
            if completed != expected {
                return Err(BackendError::InvalidHandle);
            }
            if first_error.is_none() {
                first_error = result.err();
            }
            if completed == fence {
                return first_error.map_or(Ok(()), Err);
            }
        }
    }

    fn has_pending(&self, fence: FenceId) -> bool {
        self.pending.iter().any(|(pending, _)| *pending == fence)
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
    auxiliary: BTreeMap<crate::TensorId, Box<[f32]>>,
    q8: Box<[u8]>,
    scales: Box<[f32]>,
    attention_scores: Box<[f32]>,
    attention_values: Box<[f32]>,
}

impl WorkerState {
    fn execute(&mut self, program: ProgramId, params_ptr: usize) -> Result<(), BackendError> {
        let params = unsafe { &*(params_ptr as *const CpuRunParams) };
        let plan = self
            .programs
            .get(&program)
            .ok_or(BackendError::InvalidHandle)?;
        let input = plan.input;
        let output = plan.output;
        let kind = plan.kind.clone();
        let layer_ops = plan.layer_ops.clone();
        match kind {
            ProgramKind::Q8Rows {
                tensor,
                rows,
                batch_capacity,
            } => self.execute_q8_rows(
                params,
                tensor,
                rows.start as usize,
                rows.end as usize,
                batch_capacity as usize,
                input,
                output,
            ),
            ProgramKind::EmbeddingRows { tensor, row_count } => {
                self.execute_embedding_rows(params, tensor, row_count as usize, output)
            }
            ProgramKind::LayerSegment { .. } | ProgramKind::FinalNormQ8Logits { .. } => {
                self.execute_layer_ops(params, &layer_ops)
            }
        }
    }

    fn execute_layer_ops(
        &mut self,
        params: &CpuRunParams,
        ops: &[LayerOp],
    ) -> Result<(), BackendError> {
        let batch = params.token_count as usize;
        if batch == 0 {
            return Err(BackendError::InvalidHandle);
        }
        for op in ops {
            match *op {
                LayerOp::RmsNorm {
                    input,
                    weight,
                    output,
                    epsilon_bits,
                } => self.execute_rms_norm(input, weight, output, f32::from_bits(epsilon_bits))?,
                LayerOp::Q8Matmul {
                    input,
                    weight,
                    output,
                } => self.execute_q8_matmul(input, weight, output, batch)?,
                LayerOp::Rope {
                    q,
                    k,
                    key_head_dim,
                    rope_dims,
                    freq_base_bits,
                } => {
                    if key_head_dim != rope_dims {
                        return Err(BackendError::InvalidHandle);
                    }
                    for slot in [q, k] {
                        let values = self.slot_mut(slot)?;
                        let width = values.len() / batch;
                        for (item, values) in
                            values[..batch * width].chunks_exact_mut(width).enumerate()
                        {
                            let position = self.position(params, item)?;
                            crate::ops::rope_neox(
                                values,
                                position,
                                key_head_dim as usize,
                                f32::from_bits(freq_base_bits),
                            );
                        }
                    }
                }
                LayerOp::KvAppend {
                    k,
                    v,
                    key_state,
                    value_state,
                    ..
                } => {
                    self.append_kv(params, batch, k, key_state)?;
                    self.append_kv(params, batch, v, value_state)?;
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
                } => self.execute_attention(
                    params,
                    batch,
                    q,
                    output,
                    head_count as usize,
                    kv_head_count as usize,
                    key_state,
                    value_state,
                    key_head_dim as usize,
                    value_head_dim as usize,
                    context_capacity as usize,
                )?,
                LayerOp::SiluMul { gate, up } => {
                    let gate = self.slot(gate)?;
                    let up = self.slot_mut(up)?;
                    let len = gate.len().min(up.len());
                    crate::ops::silu_mul_inplace(&gate[..len], &mut up[..len]);
                }
                LayerOp::Add {
                    left,
                    right,
                    output,
                } => self.execute_add(left, right, output)?,
                _ => {
                    return Err(BackendError::Unsupported {
                        device: self.device.clone(),
                        operation: "compiled CPU layer operation",
                    })
                }
            }
        }
        Ok(())
    }

    fn execute_rms_norm(
        &self,
        input: SlotId,
        weight: crate::TensorId,
        output: SlotId,
        epsilon: f32,
    ) -> Result<(), BackendError> {
        let weights = self
            .auxiliary
            .get(&weight)
            .ok_or(BackendError::InvalidHandle)?;
        let width = weights.len();
        let input_slot = *self
            .slots
            .get(input.0 as usize)
            .ok_or(BackendError::InvalidHandle)?;
        let output_slot = *self
            .slots
            .get(output.0 as usize)
            .ok_or(BackendError::InvalidHandle)?;
        if width == 0 || input_slot.len != output_slot.len || input_slot.len % width != 0 {
            return Err(BackendError::InvalidHandle);
        }
        let output_values =
            unsafe { std::slice::from_raw_parts_mut(output_slot.ptr as *mut f32, output_slot.len) };
        if input == output {
            for values in output_values.chunks_exact_mut(width) {
                crate::ops::rms_norm_inplace(values, weights, epsilon);
            }
        } else {
            let input =
                unsafe { std::slice::from_raw_parts(input_slot.ptr as *const f32, input_slot.len) };
            for (input, output) in input
                .chunks_exact(width)
                .zip(output_values.chunks_exact_mut(width))
            {
                crate::ops::rms_norm(input, weights, output, epsilon);
            }
        }
        Ok(())
    }

    fn execute_q8_matmul(
        &mut self,
        input: SlotId,
        weight: crate::TensorId,
        output: SlotId,
        batch: usize,
    ) -> Result<(), BackendError> {
        let entry = self
            .catalog
            .entry(weight)
            .ok_or(BackendError::InvalidHandle)?;
        let n_in = usize::try_from(entry.shape[0]).map_err(|_| BackendError::InvalidHandle)?;
        let rows = usize::try_from(entry.row_count).map_err(|_| BackendError::InvalidHandle)?;
        let input_slot = *self
            .slots
            .get(input.0 as usize)
            .ok_or(BackendError::InvalidHandle)?;
        let output_slot = *self
            .slots
            .get(output.0 as usize)
            .ok_or(BackendError::InvalidHandle)?;
        if batch * n_in > input_slot.len
            || batch * rows > output_slot.len
            || n_in > self.q8.len()
            || n_in / 32 > self.scales.len()
        {
            return Err(BackendError::InvalidHandle);
        }
        let weight_bytes = self
            .catalog
            .bytes(weight)
            .map_err(|_| BackendError::InvalidHandle)?;
        for item in 0..batch {
            let input = unsafe {
                std::slice::from_raw_parts((input_slot.ptr as *const f32).add(item * n_in), n_in)
            };
            crate::ops::quantize_q8_0_into(
                input,
                n_in,
                &mut self.q8[..n_in],
                &mut self.scales[..n_in / 32],
            );
            let output = unsafe {
                std::slice::from_raw_parts_mut((output_slot.ptr as *mut f32).add(item * rows), rows)
            };
            matmul_q8_range_with_pool(
                &self.pool,
                weight_bytes,
                &self.q8[..n_in],
                &self.scales[..n_in / 32],
                output,
                n_in,
                0,
                rows,
            );
        }
        Ok(())
    }

    fn append_kv(
        &self,
        params: &CpuRunParams,
        batch: usize,
        values: SlotId,
        state: SlotId,
    ) -> Result<(), BackendError> {
        let values = self.slot(values)?;
        let state = self.slot_mut(state)?;
        let width = values.len() / batch;
        if width == 0 {
            return Err(BackendError::InvalidHandle);
        }
        for item in 0..batch {
            let position = self.position(params, item)?;
            let start = position
                .checked_mul(width)
                .ok_or(BackendError::InvalidHandle)?;
            if start + width > state.len() {
                return Err(BackendError::InvalidHandle);
            }
            for index in 0..width {
                state[start + index] = half::f16::from_f32(values[item * width + index]).to_f32();
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_attention(
        &mut self,
        params: &CpuRunParams,
        batch: usize,
        q: SlotId,
        output: SlotId,
        head_count: usize,
        kv_head_count: usize,
        key_state: SlotId,
        value_state: SlotId,
        key_head_dim: usize,
        value_head_dim: usize,
        context_capacity: usize,
    ) -> Result<(), BackendError> {
        if kv_head_count == 0 || head_count % kv_head_count != 0 {
            return Err(BackendError::InvalidHandle);
        }
        let queries = *self
            .slots
            .get(q.0 as usize)
            .ok_or(BackendError::InvalidHandle)?;
        let keys = *self
            .slots
            .get(key_state.0 as usize)
            .ok_or(BackendError::InvalidHandle)?;
        let values = *self
            .slots
            .get(value_state.0 as usize)
            .ok_or(BackendError::InvalidHandle)?;
        let output = *self
            .slots
            .get(output.0 as usize)
            .ok_or(BackendError::InvalidHandle)?;
        let q_width = head_count * key_head_dim;
        let key_width = kv_head_count * key_head_dim;
        let value_width = kv_head_count * value_head_dim;
        let output_width = head_count * value_head_dim;
        if batch * q_width > queries.len
            || context_capacity * key_width > keys.len
            || context_capacity * value_width > values.len
            || batch * output_width > output.len
        {
            return Err(BackendError::InvalidHandle);
        }
        let queries = unsafe { std::slice::from_raw_parts(queries.ptr as *const f32, queries.len) };
        let keys = unsafe { std::slice::from_raw_parts(keys.ptr as *const f32, keys.len) };
        let values = unsafe { std::slice::from_raw_parts(values.ptr as *const f32, values.len) };
        let output = unsafe { std::slice::from_raw_parts_mut(output.ptr as *mut f32, output.len) };
        let group = head_count / kv_head_count;
        let scale = 1.0 / (key_head_dim as f32).sqrt();
        output[..batch * output_width].fill(0.0);
        for item in 0..batch {
            let position = self.position(params, item)?;
            let cached = position + 1;
            let padded = cached.div_ceil(32) * 32;
            for head in 0..head_count {
                let kv_head = head / group;
                let query = &queries[item * q_width + head * key_head_dim
                    ..item * q_width + (head + 1) * key_head_dim];
                let scores = &mut self.attention_scores[..padded];
                scores.fill(f32::NEG_INFINITY);
                for (prior, score) in scores[..cached].iter_mut().enumerate() {
                    let start = prior * key_width + kv_head * key_head_dim;
                    *score = crate::ops::dot_f32(
                        query,
                        &keys[start..start + key_head_dim],
                        key_head_dim,
                    ) * scale;
                }
                crate::ops::softmax(scores);
                for lane in 0..value_head_dim {
                    let attention_values = &mut self.attention_values[..padded];
                    attention_values.fill(0.0);
                    for (prior, value) in attention_values[..cached].iter_mut().enumerate() {
                        *value = values[prior * value_width + kv_head * value_head_dim + lane];
                    }
                    output[item * output_width + head * value_head_dim + lane] =
                        crate::ops::attention_value_f32(attention_values, scores, cached, padded);
                }
            }
        }
        Ok(())
    }

    fn execute_add(&self, left: SlotId, right: SlotId, output: SlotId) -> Result<(), BackendError> {
        if output == right {
            let left = self.slot(left)?;
            let output = self.slot_mut(output)?;
            if left.len() != output.len() {
                return Err(BackendError::InvalidHandle);
            }
            crate::ops::vec_add_into(left, output);
            return Ok(());
        }
        if output == left {
            let right = self.slot(right)?;
            let output = self.slot_mut(output)?;
            if right.len() != output.len() {
                return Err(BackendError::InvalidHandle);
            }
            crate::ops::vec_add_into(right, output);
            return Ok(());
        }
        let right_values = self.slot(right)?;
        let output_values = self.slot_mut(output)?;
        if right_values.len() != output_values.len() {
            return Err(BackendError::InvalidHandle);
        }
        output_values.copy_from_slice(right_values);
        let left_values = self.slot(left)?;
        if left_values.len() != output_values.len() {
            return Err(BackendError::InvalidHandle);
        }
        crate::ops::vec_add_into(left_values, output_values);
        Ok(())
    }

    fn slot(&self, slot: SlotId) -> Result<&[f32], BackendError> {
        let slot = *self
            .slots
            .get(slot.0 as usize)
            .ok_or(BackendError::InvalidHandle)?;
        Ok(unsafe { std::slice::from_raw_parts(slot.ptr as *const f32, slot.len) })
    }

    fn slot_mut(&self, slot: SlotId) -> Result<&mut [f32], BackendError> {
        let slot = *self
            .slots
            .get(slot.0 as usize)
            .ok_or(BackendError::InvalidHandle)?;
        Ok(unsafe { std::slice::from_raw_parts_mut(slot.ptr as *mut f32, slot.len) })
    }

    fn position(&self, params: &CpuRunParams, item: usize) -> Result<usize, BackendError> {
        let position = params
            .mrope_positions
            .get(item)
            .filter(|_| item < params.mrope_positions_len)
            .map(|position| position[0])
            .unwrap_or_else(|| params.position_start + item as u32);
        usize::try_from(position).map_err(|_| BackendError::InvalidHandle)
    }

    fn execute_q8_rows(
        &mut self,
        params: &CpuRunParams,
        tensor: crate::TensorId,
        row_start: usize,
        row_end: usize,
        batch_capacity: usize,
        input: SlotId,
        output: SlotId,
    ) -> Result<(), BackendError> {
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
        params: &CpuRunParams,
        tensor: crate::TensorId,
        row_count: usize,
        output: SlotId,
    ) -> Result<(), BackendError> {
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
            BTreeSet::from([PlacementMode::Layer, PlacementMode::Row])
        );
        assert_eq!(
            descriptor.capabilities.layer_families,
            BTreeSet::from([LayerFamily::Qwen3])
        );
    }

    #[test]
    fn qwen3_kv_append_persists_both_positions_for_attention() {
        let (catalog, _, _) = q8_program_fixture(1, 64, 1, 0..1);
        let mut buffers = [1, 1, 1, 2, 2, 1]
            .map(|len| vec![0.0; len].into_boxed_slice())
            .into_iter()
            .collect::<Vec<_>>();
        let slots = buffers
            .iter_mut()
            .map(|slot| SlotPtr {
                ptr: slot.as_mut_ptr() as usize,
                len: slot.len(),
            })
            .collect();
        let mut worker = WorkerState {
            pool: ComputePool::new(1),
            device: DeviceId::parse("cpu0").unwrap(),
            catalog,
            programs: BTreeMap::new(),
            slots,
            auxiliary: BTreeMap::new(),
            q8: Box::new([]),
            scales: Box::new([]),
            attention_scores: vec![0.0; 32].into_boxed_slice(),
            attention_values: vec![0.0; 32].into_boxed_slice(),
        };
        let ops = [
            LayerOp::KvAppend {
                layer: 0,
                k: SlotId(0),
                v: SlotId(1),
                key_state: SlotId(3),
                value_state: SlotId(4),
            },
            LayerOp::Attention {
                layer: 0,
                q: SlotId(2),
                output: SlotId(5),
                head_count: 1,
                kv_head_count: 1,
                key_state: SlotId(3),
                value_state: SlotId(4),
                key_head_dim: 1,
                value_head_dim: 1,
                context_capacity: 2,
            },
        ];
        for (position, value) in [2.0, 6.0].into_iter().enumerate() {
            worker.slot_mut(SlotId(0)).unwrap()[0] = 0.0;
            worker.slot_mut(SlotId(1)).unwrap()[0] = value;
            worker.slot_mut(SlotId(2)).unwrap()[0] = 1.0;
            let params = CpuRunParams {
                token_count: 1,
                position_start: position as u32,
                mrope_positions: vec![[position as u32, 0, 0, 0]].into_boxed_slice(),
                mrope_positions_len: 1,
                token_ids: Box::new([]),
                token_ids_len: 0,
            };
            worker.execute_layer_ops(&params, &ops).unwrap();
        }
        assert_eq!(&worker.slot(SlotId(4)).unwrap()[..2], &[2.0, 6.0]);
        assert_eq!(worker.slot(SlotId(5)).unwrap()[0], 4.0);
    }
}
