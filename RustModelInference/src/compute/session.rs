use super::{
    ActivationTransfer, BackendError, DeviceRegistry, DeviceSession, ExecutionPlan, FenceId,
    ProgramKind, RunParams, SessionStats, SlotId, TransferTarget,
};
use crate::{ComponentId, DeviceId, TensorCatalog, TensorId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard};

pub struct CompiledModel {
    catalog: Arc<TensorCatalog>,
    plan: Arc<ExecutionPlan>,
    sessions: Mutex<SessionSet>,
}

struct SessionSet {
    sessions: BTreeMap<DeviceId, Box<dyn DeviceSession>>,
    host_results: BTreeMap<(DeviceId, SlotId), Box<[f32]>>,
    pending: Vec<(usize, FenceId)>,
    transfer_scratch: Box<[f32]>,
    poisoned: bool,
}

pub struct ExecutionRun<'a> {
    plan: Arc<ExecutionPlan>,
    sessions: MutexGuard<'a, SessionSet>,
}

impl CompiledModel {
    pub fn compile(
        catalog: Arc<TensorCatalog>,
        plan: ExecutionPlan,
        providers: Arc<DeviceRegistry>,
    ) -> Result<Self, BackendError> {
        let mut physical = BTreeSet::new();
        for device_plan in plan.devices.values() {
            if !physical.insert(device_plan.descriptor.physical_key.as_str()) {
                return Err(BackendError::InvalidHandle);
            }
        }

        let mut sessions = BTreeMap::new();
        for (device, device_plan) in &plan.devices {
            if device != &device_plan.descriptor.id {
                return Err(BackendError::InvalidHandle);
            }
            let session = providers.provider(device_plan.descriptor.backend)?.open(
                &device_plan.descriptor,
                device_plan,
                Arc::clone(&catalog),
            )?;
            sessions.insert(device.clone(), session);
        }

        let mut host_results = BTreeMap::new();
        let mut max_shards = 0;
        for component in plan.components.values() {
            for shards in component.row_shards.values() {
                max_shards = max_shards.max(shards.len());
                for shard in shards {
                    let values = slot_values(&plan, &shard.device, shard.output)?;
                    host_results
                        .entry((shard.device.clone(), shard.output))
                        .or_insert_with(|| vec![0.0; values].into_boxed_slice());
                }
            }
        }
        let mut transfer_values = 0;
        for transfer in plan
            .components
            .values()
            .flat_map(|component| &component.activation_transfers)
        {
            transfer_values = transfer_values.max(slot_values(
                &plan,
                &transfer.from_device,
                transfer.from_slot,
            )?);
        }

        Ok(Self {
            catalog,
            plan: Arc::new(plan),
            sessions: Mutex::new(SessionSet {
                sessions,
                host_results,
                pending: Vec::with_capacity(max_shards),
                transfer_scratch: vec![0.0; transfer_values].into_boxed_slice(),
                poisoned: false,
            }),
        })
    }

    pub fn start_run(&self) -> Result<ExecutionRun<'_>, BackendError> {
        let _ = &self.catalog;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| BackendError::PoisonedRun)?;
        reset_sessions(&mut sessions)?;
        Ok(ExecutionRun {
            plan: Arc::clone(&self.plan),
            sessions,
        })
    }

    pub fn plan(&self) -> &ExecutionPlan {
        &self.plan
    }
}

impl<'a> ExecutionRun<'a> {
    pub fn plan(&self) -> &ExecutionPlan {
        &self.plan
    }

    pub fn execute_q8(
        &mut self,
        component: ComponentId,
        tensor: TensorId,
        input: &[f32],
        batch: u32,
        output: &mut [f32],
    ) -> Result<(), BackendError> {
        self.require_healthy()?;
        let component_plan = self
            .plan
            .components
            .get(&component)
            .filter(|plan| plan.component == component)
            .ok_or(BackendError::InvalidHandle)?;
        let shards = component_plan
            .row_shards
            .get(&tensor)
            .ok_or(BackendError::ProgramMissing { tensor })?;
        let total_rows = shards.iter().map(|shard| shard.rows.end).max().unwrap_or(0) as usize;
        let output_len = (batch as usize)
            .checked_mul(total_rows)
            .ok_or(BackendError::InvalidHandle)?;
        if output.len() != output_len {
            return Err(BackendError::InvalidHandle);
        }

        let result = (|| {
            let SessionSet {
                sessions,
                host_results,
                pending,
                ..
            } = &mut *self.sessions;
            pending.clear();
            for (index, shard) in shards.iter().enumerate() {
                let session = session_mut(sessions, &shard.device)?;
                session.write_f32(shard.input, input)?;
                let fence = session.submit(
                    shard.program,
                    &RunParams {
                        token_count: batch,
                        position_start: 0,
                        mrope_positions: &[],
                        token_ids: &[],
                    },
                )?;
                pending.push((index, fence));
            }
            for &(index, fence) in pending.iter() {
                session_mut(sessions, &shards[index].device)?.wait(fence)?;
            }
            for &(index, _) in pending.iter() {
                let shard = &shards[index];
                let rows = (shard.rows.end - shard.rows.start) as usize;
                let local = host_result_mut(host_results, &shard.device, shard.output)?;
                let local_len = (batch as usize)
                    .checked_mul(rows)
                    .ok_or(BackendError::InvalidHandle)?;
                if local_len > local.len() {
                    return Err(BackendError::InvalidHandle);
                }
                session_mut(sessions, &shard.device)?
                    .read_f32(shard.output, &mut local[..local_len])?;
                for item in 0..batch as usize {
                    let src = &local[item * rows..(item + 1) * rows];
                    let dst = &mut output[item * total_rows + shard.rows.start as usize
                        ..item * total_rows + shard.rows.end as usize];
                    dst.copy_from_slice(src);
                }
            }
            pending.clear();
            Ok(())
        })();
        if result.is_err() {
            self.sessions.poisoned = true;
        }
        result
    }

    pub fn execute_embedding(
        &mut self,
        component: ComponentId,
        tensor: TensorId,
        token_ids: &[u32],
        output: &mut [f32],
    ) -> Result<(), BackendError> {
        self.require_healthy()?;
        let component_plan = component_plan(&self.plan, component)?;
        let embedding = component_plan
            .embedding
            .as_ref()
            .ok_or(BackendError::ProgramMissing { tensor })?;
        validate_embedding_program(&self.plan, &embedding.device, embedding.program, tensor)?;
        let hidden = embedding_width(&self.plan, component_plan)?;
        if token_ids
            .len()
            .checked_mul(hidden)
            .filter(|length| *length == output.len())
            .is_none()
        {
            return Err(BackendError::InvalidHandle);
        }
        let token_count =
            u32::try_from(token_ids.len()).map_err(|_| BackendError::InvalidHandle)?;
        let result = (|| {
            let session = session_mut(&mut self.sessions.sessions, &embedding.device)?;
            let fence = session.submit(
                embedding.program,
                &RunParams {
                    token_count,
                    position_start: 0,
                    mrope_positions: &[],
                    token_ids,
                },
            )?;
            session.wait(fence)?;
            session.read_f32(embedding.output, output)
        })();
        if result.is_err() {
            self.sessions.poisoned = true;
        }
        result
    }

    pub fn execute_embedding_into_layers(
        &mut self,
        component: ComponentId,
        tensor: TensorId,
        token_ids: &[u32],
        params: &RunParams<'_>,
    ) -> Result<(), BackendError> {
        self.require_healthy()?;
        let component_plan = component_plan(&self.plan, component)?;
        let embedding = component_plan
            .embedding
            .as_ref()
            .ok_or(BackendError::ProgramMissing { tensor })?;
        validate_embedding_program(&self.plan, &embedding.device, embedding.program, tensor)?;
        if params.token_count as usize != token_ids.len() {
            return Err(BackendError::InvalidHandle);
        }
        let result = (|| {
            let fence = session_mut(&mut self.sessions.sessions, &embedding.device)?.submit(
                embedding.program,
                &RunParams {
                    token_count: params.token_count,
                    position_start: params.position_start,
                    mrope_positions: params.mrope_positions,
                    token_ids,
                },
            )?;
            if let Some(first) = component_plan.layer_spans.first() {
                if first.device == embedding.device && first.input == embedding.output {
                    if component_plan
                        .activation_transfers
                        .iter()
                        .any(|transfer| transfer.after_span.is_none())
                    {
                        return Err(BackendError::InvalidHandle);
                    }
                } else {
                    let transfer = component_plan
                        .activation_transfers
                        .iter()
                        .find(|transfer| transfer.after_span.is_none())
                        .filter(|transfer| transfer.target == TransferTarget::Span(0))
                        .ok_or(BackendError::InvalidHandle)?;
                    copy_transfer(&mut self.sessions, transfer, params.token_count, fence)?;
                }
            }
            execute_spans(
                &mut self.sessions,
                &component_plan.layer_spans,
                &component_plan.activation_transfers,
                params,
            )
        })();
        if result.is_err() {
            self.sessions.poisoned = true;
        }
        result
    }

    pub fn execute_layers(
        &mut self,
        component: ComponentId,
        hidden: &mut [f32],
        params: &RunParams<'_>,
    ) -> Result<(), BackendError> {
        self.require_healthy()?;
        let component_plan = component_plan(&self.plan, component)?;
        let first = component_plan
            .layer_spans
            .first()
            .ok_or(BackendError::InvalidHandle)?;
        let result = (|| {
            session_mut(&mut self.sessions.sessions, &first.device)?
                .write_f32(first.input, hidden)?;
            execute_spans(
                &mut self.sessions,
                &component_plan.layer_spans,
                &component_plan.activation_transfers,
                params,
            )
        })();
        if result.is_err() {
            self.sessions.poisoned = true;
        }
        result
    }

    pub fn execute_logits(
        &mut self,
        component: ComponentId,
        params: &RunParams<'_>,
        output: &mut [f32],
    ) -> Result<(), BackendError> {
        self.require_healthy()?;
        let component_plan = component_plan(&self.plan, component)?;
        let finalization = component_plan
            .finalization
            .as_ref()
            .ok_or(BackendError::InvalidHandle)?;
        let result = (|| {
            let session = session_mut(&mut self.sessions.sessions, &finalization.device)?;
            let fence = session.submit(finalization.program, params)?;
            session.wait(fence)?;
            let offset = usize::try_from(params.token_count)
                .ok()
                .and_then(|batch| batch.checked_sub(1))
                .and_then(|last| last.checked_mul(output.len()))
                .ok_or(BackendError::InvalidHandle)?;
            session.read_f32_at(finalization.output, offset, output)
        })();
        if result.is_err() {
            self.sessions.poisoned = true;
        }
        result
    }

    pub fn reset_state(&mut self) -> Result<(), BackendError> {
        reset_sessions(&mut self.sessions)
    }

    pub fn batch_capacity(&self, component: ComponentId) -> Result<u32, BackendError> {
        let binding = component_plan(&self.plan, component)?
            .embedding
            .as_ref()
            .ok_or(BackendError::InvalidHandle)?;
        let slot = self
            .plan
            .devices
            .get(&binding.device)
            .and_then(|device| device.slots.iter().find(|slot| slot.id == binding.input))
            .filter(|slot| slot.storage == super::SlotStorage::I8 && slot.byte_len % 4 == 0)
            .ok_or(BackendError::InvalidHandle)?;
        u32::try_from(slot.byte_len / 4)
            .ok()
            .filter(|capacity| *capacity != 0)
            .ok_or(BackendError::InvalidHandle)
    }

    pub fn stats(&self) -> BTreeMap<DeviceId, SessionStats> {
        self.sessions
            .sessions
            .iter()
            .map(|(device, session)| (device.clone(), session.stats()))
            .collect()
    }

    fn require_healthy(&self) -> Result<(), BackendError> {
        (!self.sessions.poisoned)
            .then_some(())
            .ok_or(BackendError::PoisonedRun)
    }
}

fn component_plan(
    plan: &ExecutionPlan,
    component: ComponentId,
) -> Result<&super::ComponentPlan, BackendError> {
    plan.components
        .get(&component)
        .filter(|plan| plan.component == component)
        .ok_or(BackendError::InvalidHandle)
}

fn session_mut<'a>(
    sessions: &'a mut BTreeMap<DeviceId, Box<dyn DeviceSession>>,
    device: &DeviceId,
) -> Result<&'a mut (dyn DeviceSession + 'static), BackendError> {
    sessions
        .get_mut(device)
        .map(Box::as_mut)
        .ok_or(BackendError::InvalidHandle)
}

fn slot_values(
    plan: &ExecutionPlan,
    device: &DeviceId,
    slot: SlotId,
) -> Result<usize, BackendError> {
    let byte_len = plan
        .devices
        .get(device)
        .and_then(|plan| plan.slots.iter().find(|candidate| candidate.id == slot))
        .filter(|slot| slot.byte_len % 4 == 0)
        .map(|slot| slot.byte_len)
        .ok_or(BackendError::InvalidHandle)?;
    usize::try_from(byte_len / 4).map_err(|_| BackendError::InvalidHandle)
}

fn validate_embedding_program(
    plan: &ExecutionPlan,
    device: &DeviceId,
    program: super::ProgramId,
    tensor: TensorId,
) -> Result<(), BackendError> {
    match plan
        .devices
        .get(device)
        .and_then(|plan| {
            plan.programs
                .iter()
                .find(|candidate| candidate.id == program)
        })
        .map(|program| &program.kind)
    {
        Some(ProgramKind::EmbeddingRows { tensor: actual, .. }) if *actual == tensor => Ok(()),
        _ => Err(BackendError::ProgramMissing { tensor }),
    }
}

fn host_result_mut<'a>(
    results: &'a mut BTreeMap<(DeviceId, SlotId), Box<[f32]>>,
    device: &DeviceId,
    slot: SlotId,
) -> Result<&'a mut Box<[f32]>, BackendError> {
    // ponytail: linear scan avoids allocating a cloned DeviceId in the hot path;
    // add a borrowed composite-key index only if row-shard counts become large.
    results
        .iter_mut()
        .find(|((candidate, candidate_slot), _)| candidate == device && *candidate_slot == slot)
        .map(|(_, values)| values)
        .ok_or(BackendError::InvalidHandle)
}

fn embedding_width(
    plan: &ExecutionPlan,
    component: &super::ComponentPlan,
) -> Result<usize, BackendError> {
    let embedding = component
        .embedding
        .as_ref()
        .ok_or(BackendError::InvalidHandle)?;
    let device = plan
        .devices
        .get(&embedding.device)
        .ok_or(BackendError::InvalidHandle)?;
    let batch_capacity = device
        .slots
        .iter()
        .find(|slot| slot.id == embedding.input)
        .filter(|slot| slot.byte_len > 0 && slot.byte_len % 4 == 0)
        .map(|slot| slot.byte_len as usize / 4)
        .ok_or(BackendError::InvalidHandle)?;
    let values = slot_values(plan, &embedding.device, embedding.output)?;
    if values % batch_capacity != 0 {
        return Err(BackendError::InvalidHandle);
    }
    Ok(values / batch_capacity)
}

fn copy_transfer(
    session_set: &mut SessionSet,
    transfer: &ActivationTransfer,
    token_count: u32,
    fence: FenceId,
) -> Result<(), BackendError> {
    let values = (token_count as usize)
        .checked_mul(transfer.f32_values_per_token as usize)
        .filter(|values| *values <= session_set.transfer_scratch.len())
        .ok_or(BackendError::InvalidHandle)?;
    let SessionSet {
        sessions,
        transfer_scratch,
        ..
    } = session_set;
    session_mut(sessions, &transfer.from_device)?.wait(fence)?;
    session_mut(sessions, &transfer.from_device)?
        .read_f32(transfer.from_slot, &mut transfer_scratch[..values])?;
    session_mut(sessions, &transfer.to_device)?
        .write_f32(transfer.to_slot, &transfer_scratch[..values])
}

fn execute_spans(
    session_set: &mut SessionSet,
    spans: &[super::LayerSpan],
    transfers: &[ActivationTransfer],
    params: &RunParams<'_>,
) -> Result<(), BackendError> {
    for (index, span) in spans.iter().enumerate() {
        let fence = match session_mut(&mut session_set.sessions, &span.device)
            .and_then(|session| session.submit(span.program, params))
        {
            Ok(fence) => fence,
            Err(error) => {
                session_set.poisoned = true;
                return Err(error);
            }
        };
        if let Some(transfer) = transfers
            .iter()
            .find(|transfer| transfer.after_span == Some(index as u32))
        {
            let target_matches = match transfer.target {
                TransferTarget::Span(next) => next as usize == index + 1,
                TransferTarget::Finalization => index + 1 == spans.len(),
            };
            if !target_matches {
                session_set.poisoned = true;
                return Err(BackendError::InvalidHandle);
            }
            if let Err(error) = copy_transfer(session_set, transfer, params.token_count, fence) {
                session_set.poisoned = true;
                return Err(error);
            }
        } else if let Some(next) = spans.get(index + 1) {
            if span.device != next.device || span.output != next.input {
                session_set.poisoned = true;
                return Err(BackendError::InvalidHandle);
            }
        }
    }
    Ok(())
}

fn reset_sessions(session_set: &mut SessionSet) -> Result<(), BackendError> {
    if session_set.poisoned {
        return Err(BackendError::PoisonedRun);
    }
    for session in session_set.sessions.values_mut() {
        if let Err(error) = session.reset_state() {
            session_set.poisoned = true;
            return Err(error);
        }
    }
    Ok(())
}
