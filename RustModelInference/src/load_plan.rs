use crate::compute::{
    ActivationTransfer, BackendError, BackendKind, ComponentPlan, DevicePlan, DeviceRegistry,
    ExecutionPlan, LayerFamily, LayerOp, LayerSpan, MemoryPlan, ProgramBinding, ProgramId,
    ProgramKind, ProgramPlan, ResidentTensorPlan, RowShard, SlotId, SlotKind, SlotPlan,
    SlotStorage, TransferTarget,
};
use crate::{
    ComponentId, DeviceId, GGMLType, NormalizedTarget, PlacementMode, PlacementRule, TensorCatalog,
    TensorId,
};
use std::collections::BTreeMap;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentRequirements {
    pub component: ComponentId,
    pub workload: ComponentWorkload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentWorkload {
    Llm(LlmRequirements),
    VisionCpu { layer_count: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KvCacheType {
    F16,
    F32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmRequirements {
    pub layers: Vec<LlmLayerSpec>,
    pub hidden_size: u32,
    pub context_length: u32,
    pub max_batch_tokens: u32,
    pub kv_cache: KvCacheType,
    pub final_norm: TensorId,
    pub output: TensorId,
    pub norm_epsilon_bits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmLayerSpec {
    Qwen3(Qwen3LayerSpec),
    Qwen35Dense(Qwen35DenseLayerSpec),
    Qwen35Recurrent(Qwen35RecurrentLayerSpec),
}

impl LlmLayerSpec {
    pub fn layer(&self) -> u32 {
        match self {
            Self::Qwen3(spec) => spec.layer,
            Self::Qwen35Dense(spec) => spec.layer,
            Self::Qwen35Recurrent(spec) => spec.layer,
        }
    }

    pub fn family(&self) -> LayerFamily {
        match self {
            Self::Qwen3(_) => LayerFamily::Qwen3,
            Self::Qwen35Dense(_) => LayerFamily::Qwen35Dense,
            Self::Qwen35Recurrent(_) => LayerFamily::Qwen35Recurrent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Qwen3LayerSpec {
    pub layer: u32,
    pub attn_norm: TensorId,
    pub q_norm: Option<TensorId>,
    pub k_norm: Option<TensorId>,
    pub q: TensorId,
    pub k: TensorId,
    pub v: TensorId,
    pub o: TensorId,
    pub ffn_norm: TensorId,
    pub ffn_gate: TensorId,
    pub ffn_up: TensorId,
    pub ffn_down: TensorId,
    pub head_count: u32,
    pub kv_head_count: u32,
    pub key_head_dim: u32,
    pub value_head_dim: u32,
    pub rope_dims: u32,
    pub rope_freq_base_bits: u32,
    pub norm_epsilon_bits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Qwen35DenseLayerSpec {
    pub layer: u32,
    pub attn_norm: TensorId,
    pub post_attn_norm: TensorId,
    pub q_norm: TensorId,
    pub k_norm: TensorId,
    pub q: TensorId,
    pub k: TensorId,
    pub v: TensorId,
    pub o: TensorId,
    pub ffn_gate: TensorId,
    pub ffn_up: TensorId,
    pub ffn_down: TensorId,
    pub head_count: u32,
    pub kv_head_count: u32,
    pub key_head_dim: u32,
    pub value_head_dim: u32,
    pub rope_dims: u32,
    pub rope_sections: [i32; 4],
    pub rope_freq_base_bits: u32,
    pub norm_epsilon_bits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Qwen35RecurrentLayerSpec {
    pub layer: u32,
    pub attn_norm: TensorId,
    pub post_attn_norm: TensorId,
    pub qkv: TensorId,
    pub gate: TensorId,
    pub beta: TensorId,
    pub alpha: TensorId,
    pub conv_weight: TensorId,
    pub dt_bias: TensorId,
    pub ssm_a: TensorId,
    pub ssm_norm: TensorId,
    pub ssm_output: TensorId,
    pub ffn_gate: TensorId,
    pub ffn_up: TensorId,
    pub ffn_down: TensorId,
    pub conv_width: u32,
    pub state_size: u32,
    pub group_count: u32,
    pub dt_rank: u32,
    pub inner_size: u32,
    pub norm_epsilon_bits: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error("{device:?} receives no units from {available} across {targets} targets")]
    InsufficientUnits {
        device: DeviceId,
        available: u32,
        targets: usize,
    },
    #[error("unsupported component {component:?} on {device:?}")]
    UnsupportedComponent {
        component: ComponentId,
        device: DeviceId,
    },
    #[error("unsupported tensor {tensor:?} on {device:?}")]
    UnsupportedTensor { tensor: TensorId, device: DeviceId },
    #[error("row mode requires a CPU primary for {component:?}, got {device:?}")]
    UnsupportedRowPrimary {
        component: ComponentId,
        device: DeviceId,
    },
    #[error(
        "physical device {physical_key} was selected through more than one logical id: {devices:?}"
    )]
    DuplicatePhysicalSelection {
        physical_key: String,
        devices: Vec<DeviceId>,
    },
    #[error("capacity exceeded on {device:?}: required {required_bytes}, available {available_bytes}, largest allocation {largest_allocation_bytes}")]
    CapacityExceeded {
        device: DeviceId,
        required_bytes: u64,
        available_bytes: u64,
        largest_allocation_bytes: u64,
    },
    #[error("memory-size arithmetic overflow")]
    SizeOverflow,
}

pub struct PlacementCompiler<'a> {
    pub catalog: &'a TensorCatalog,
    pub registry: &'a DeviceRegistry,
    pub requirements: &'a [ComponentRequirements],
}

pub fn weighted_ranges(
    total: u32,
    targets: &[NormalizedTarget],
) -> Result<Vec<(DeviceId, Range<u32>)>, PlanError> {
    let exact = targets
        .iter()
        .map(|target| target.fraction * f64::from(total))
        .collect::<Vec<_>>();
    let mut counts = exact
        .iter()
        .map(|value| value.floor() as u32)
        .collect::<Vec<_>>();
    let assigned = counts.iter().copied().sum::<u32>();
    let mut order = (0..targets.len()).collect::<Vec<_>>();
    order.sort_by(|left, right| {
        let left_remainder = exact[*left] - f64::from(counts[*left]);
        let right_remainder = exact[*right] - f64::from(counts[*right]);
        right_remainder
            .total_cmp(&left_remainder)
            .then_with(|| targets[*left].ordinal.cmp(&targets[*right].ordinal))
    });
    for index in order.into_iter().take((total - assigned) as usize) {
        counts[index] += 1;
    }
    if let Some(index) = counts.iter().position(|count| *count == 0) {
        return Err(PlanError::InsufficientUnits {
            device: targets[index].device.clone(),
            available: total,
            targets: targets.len(),
        });
    }
    let mut start = 0;
    Ok(targets
        .iter()
        .zip(counts)
        .map(|(target, count)| {
            let range = start..start + count;
            start += count;
            (target.device.clone(), range)
        })
        .collect())
}

fn checked_mul(left: u64, right: u64) -> Result<u64, PlanError> {
    left.checked_mul(right).ok_or(PlanError::SizeOverflow)
}

fn align_up_checked(value: u64, alignment: u64) -> Result<u64, PlanError> {
    let alignment = alignment.max(1);
    value
        .checked_add(alignment - 1)
        .map(|value| value / alignment * alignment)
        .ok_or(PlanError::SizeOverflow)
}

struct DeviceBuilder {
    plan: DevicePlan,
    resident_end: u64,
    arena_end: u64,
}

impl DeviceBuilder {
    fn new(descriptor: crate::DeviceDescriptor) -> Self {
        Self {
            plan: DevicePlan {
                descriptor,
                tensors: Vec::new(),
                slots: Vec::new(),
                programs: Vec::new(),
                memory: MemoryPlan::default(),
            },
            resident_end: 0,
            arena_end: 0,
        }
    }

    fn tensor(
        &mut self,
        tensor: TensorId,
        rows: Range<u32>,
        bytes: Range<u64>,
    ) -> Result<(), PlanError> {
        if self
            .plan
            .tensors
            .iter()
            .any(|item| item.tensor == tensor && item.rows == rows)
        {
            return Ok(());
        }
        let len = bytes
            .end
            .checked_sub(bytes.start)
            .ok_or(PlanError::SizeOverflow)?;
        let offset = align_up_checked(self.resident_end, self.plan.descriptor.buffer_alignment)?;
        self.resident_end = offset.checked_add(len).ok_or(PlanError::SizeOverflow)?;
        self.plan.tensors.push(ResidentTensorPlan {
            tensor,
            rows,
            source_bytes: bytes,
            arena_offset: offset,
        });
        self.plan.memory.largest_allocation_bytes =
            self.plan.memory.largest_allocation_bytes.max(len);
        Ok(())
    }

    fn slot(
        &mut self,
        kind: SlotKind,
        storage: SlotStorage,
        byte_len: u64,
    ) -> Result<SlotId, PlanError> {
        let alignment = self.plan.descriptor.buffer_alignment.max(1);
        let offset = align_up_checked(self.arena_end, alignment)?;
        self.arena_end = offset
            .checked_add(byte_len)
            .ok_or(PlanError::SizeOverflow)?;
        let id = SlotId(self.plan.slots.len() as u32);
        self.plan.slots.push(SlotPlan {
            id,
            kind,
            storage,
            byte_len,
            alignment,
            arena_offset: offset,
        });
        self.plan.memory.largest_allocation_bytes =
            self.plan.memory.largest_allocation_bytes.max(byte_len);
        match kind {
            SlotKind::KvState | SlotKind::ConvState | SlotKind::SsmState => {
                self.plan.memory.state_bytes = self
                    .plan
                    .memory
                    .state_bytes
                    .checked_add(byte_len)
                    .ok_or(PlanError::SizeOverflow)?
            }
            _ => {
                self.plan.memory.scratch_bytes = self
                    .plan
                    .memory
                    .scratch_bytes
                    .checked_add(byte_len)
                    .ok_or(PlanError::SizeOverflow)?
            }
        }
        Ok(id)
    }

    fn program(
        &mut self,
        kind: ProgramKind,
        input: SlotId,
        output: SlotId,
        layer_ops: Vec<LayerOp>,
    ) -> ProgramId {
        let id = ProgramId(self.plan.programs.len() as u32);
        self.plan.programs.push(ProgramPlan {
            id,
            kind,
            input,
            output,
            layer_ops,
        });
        id
    }

    fn finish(mut self) -> Result<DevicePlan, PlanError> {
        self.plan.memory.resident_bytes = self.resident_end;
        self.plan.memory.staging_bytes = self.plan.memory.largest_allocation_bytes;
        self.plan.memory.largest_allocation_bytes = self
            .plan
            .memory
            .largest_allocation_bytes
            .max(self.resident_end)
            .max(self.arena_end);
        self.plan.memory.required_bytes = self
            .resident_end
            .checked_add(self.arena_end)
            .and_then(|v| v.checked_add(self.plan.memory.staging_bytes))
            .ok_or(PlanError::SizeOverflow)?;
        let available = self.plan.descriptor.usable_bytes;
        if self.plan.memory.required_bytes > available
            || self.plan.memory.largest_allocation_bytes > self.plan.descriptor.max_allocation_bytes
        {
            return Err(PlanError::CapacityExceeded {
                device: self.plan.descriptor.id.clone(),
                required_bytes: self.plan.memory.required_bytes,
                available_bytes: available,
                largest_allocation_bytes: self.plan.memory.largest_allocation_bytes,
            });
        }
        Ok(self.plan)
    }
}

impl PlacementCompiler<'_> {
    fn ensure_tensor(
        &self,
        tensor: TensorId,
        device: &DeviceId,
    ) -> Result<&crate::TensorCatalogEntry, PlanError> {
        let entry = self
            .catalog
            .entry(tensor)
            .ok_or(PlanError::UnsupportedTensor {
                tensor,
                device: device.clone(),
            })?;
        if !self
            .registry
            .require(device)?
            .capabilities
            .tensor_types
            .contains(&entry.ggml_type)
        {
            return Err(PlanError::UnsupportedTensor {
                tensor,
                device: device.clone(),
            });
        }
        Ok(entry)
    }

    pub fn compile(
        &self,
        rules: &BTreeMap<ComponentId, PlacementRule>,
    ) -> Result<ExecutionPlan, PlanError> {
        let mut selected = BTreeMap::<DeviceId, crate::DeviceDescriptor>::new();
        let mut physical = BTreeMap::<String, Vec<DeviceId>>::new();
        for rule in rules.values() {
            for target in &rule.targets {
                let descriptor = self.registry.require(&target.device)?.clone();
                physical
                    .entry(descriptor.physical_key.clone())
                    .or_default()
                    .push(descriptor.id.clone());
                selected.insert(descriptor.id.clone(), descriptor);
            }
        }
        for (physical_key, mut devices) in physical {
            devices.sort();
            devices.dedup();
            if devices.len() > 1 {
                return Err(PlanError::DuplicatePhysicalSelection {
                    physical_key,
                    devices,
                });
            }
        }
        let mut builders = selected
            .into_iter()
            .map(|(id, descriptor)| (id, DeviceBuilder::new(descriptor)))
            .collect::<BTreeMap<_, _>>();
        let mut components = BTreeMap::new();
        for (component, rule) in rules {
            let requirement = self
                .requirements
                .iter()
                .find(|item| item.component == *component)
                .ok_or_else(|| PlanError::UnsupportedComponent {
                    component: *component,
                    device: rule.targets[0].device.clone(),
                })?;
            let primary = rule.targets[0].device.clone();
            match &requirement.workload {
                ComponentWorkload::VisionCpu { .. } => {
                    for target in &rule.targets {
                        let descriptor = &builders[&target.device].plan.descriptor;
                        if descriptor.backend != BackendKind::Cpu
                            || !descriptor.capabilities.components.contains(component)
                        {
                            return Err(PlanError::UnsupportedComponent {
                                component: *component,
                                device: target.device.clone(),
                            });
                        }
                    }
                    for entry in self
                        .catalog
                        .entries()
                        .iter()
                        .filter(|entry| entry.component == *component)
                    {
                        let rows = 0..u32::try_from(entry.row_count)
                            .map_err(|_| PlanError::SizeOverflow)?;
                        builders.get_mut(&primary).unwrap().tensor(
                            entry.id,
                            rows,
                            entry.segment_byte_range.clone(),
                        )?;
                    }
                    components.insert(
                        *component,
                        ComponentPlan {
                            component: *component,
                            mode: rule.mode,
                            primary,
                            embedding: None,
                            finalization: None,
                            layer_spans: Vec::new(),
                            activation_transfers: Vec::new(),
                            row_shards: BTreeMap::new(),
                        },
                    );
                }
                ComponentWorkload::Llm(llm) => match rule.mode {
                    PlacementMode::Row => {
                        self.compile_row(*component, rule, llm, &mut builders, &mut components)?
                    }
                    PlacementMode::Layer => {
                        self.compile_layer(*component, rule, llm, &mut builders, &mut components)?
                    }
                },
            }
        }
        let devices = builders
            .into_iter()
            .map(|(id, builder)| builder.finish().map(|plan| (id, plan)))
            .collect::<Result<_, _>>()?;
        Ok(ExecutionPlan {
            components,
            devices,
        })
    }

    fn ensure_device(
        &self,
        component: ComponentId,
        device: &DeviceId,
        mode: PlacementMode,
        family: Option<LayerFamily>,
    ) -> Result<(), PlanError> {
        let descriptor = self.registry.require(device)?;
        if !descriptor.capabilities.components.contains(&component)
            || !descriptor.capabilities.modes.contains(&mode)
            || family
                .is_some_and(|family| !descriptor.capabilities.layer_families.contains(&family))
        {
            return Err(PlanError::UnsupportedComponent {
                component,
                device: device.clone(),
            });
        }
        Ok(())
    }

    fn compile_row(
        &self,
        component: ComponentId,
        rule: &PlacementRule,
        llm: &LlmRequirements,
        builders: &mut BTreeMap<DeviceId, DeviceBuilder>,
        components: &mut BTreeMap<ComponentId, ComponentPlan>,
    ) -> Result<(), PlanError> {
        let primary = rule.targets[0].device.clone();
        if self.registry.require(&primary)?.backend != BackendKind::Cpu {
            return Err(PlanError::UnsupportedRowPrimary {
                component,
                device: primary,
            });
        }
        for target in &rule.targets {
            self.ensure_device(component, &target.device, PlacementMode::Row, None)?;
        }
        self.ensure_tensor(llm.final_norm, &primary)?;
        let logits = self.ensure_tensor(llm.output, &primary)?;
        if logits.ggml_type != GGMLType::Q8_0 {
            return Err(PlanError::UnsupportedTensor {
                tensor: llm.output,
                device: primary,
            });
        }
        let mut row_shards = BTreeMap::new();
        let activation_bytes = checked_mul(
            u64::from(llm.hidden_size),
            checked_mul(u64::from(llm.max_batch_tokens), 4)?,
        )?;
        let mut embedding = None;
        for entry in self
            .catalog
            .entries()
            .iter()
            .filter(|entry| entry.component == component)
        {
            let row_count = u32::try_from(entry.row_count).map_err(|_| PlanError::SizeOverflow)?;
            if entry.name == "token_embd.weight" {
                self.ensure_tensor(entry.id, &primary)?;
                let builder = builders.get_mut(&primary).unwrap();
                builder.tensor(entry.id, 0..row_count, entry.segment_byte_range.clone())?;
                let input = builder.slot(
                    SlotKind::Scratch,
                    SlotStorage::I8,
                    checked_mul(u64::from(llm.max_batch_tokens), 4)?,
                )?;
                let output =
                    builder.slot(SlotKind::Activation, SlotStorage::F32, activation_bytes)?;
                let program = builder.program(
                    ProgramKind::EmbeddingRows {
                        tensor: entry.id,
                        row_count,
                    },
                    input,
                    output,
                    Vec::new(),
                );
                embedding = Some(ProgramBinding {
                    device: primary.clone(),
                    program,
                    input,
                    output,
                });
            } else if entry.shape.len() >= 2 {
                if entry.ggml_type != GGMLType::Q8_0 {
                    self.ensure_tensor(entry.id, &primary)?;
                    builders.get_mut(&primary).unwrap().tensor(
                        entry.id,
                        0..row_count,
                        entry.segment_byte_range.clone(),
                    )?;
                    continue;
                }
                let mut shards = Vec::new();
                for (device, rows) in weighted_ranges(row_count, &rule.targets)? {
                    let descriptor = self.registry.require(&device)?;
                    if !descriptor
                        .capabilities
                        .tensor_types
                        .contains(&entry.ggml_type)
                    {
                        return Err(PlanError::UnsupportedTensor {
                            tensor: entry.id,
                            device,
                        });
                    }
                    let start = entry
                        .segment_byte_range
                        .start
                        .checked_add(checked_mul(u64::from(rows.start), entry.row_bytes)?)
                        .ok_or(PlanError::SizeOverflow)?;
                    let end = entry
                        .segment_byte_range
                        .start
                        .checked_add(checked_mul(u64::from(rows.end), entry.row_bytes)?)
                        .ok_or(PlanError::SizeOverflow)?;
                    let builder = builders.get_mut(&device).unwrap();
                    builder.tensor(entry.id, rows.clone(), start..end)?;
                    let input_elements = *entry.shape.first().ok_or(PlanError::SizeOverflow)?;
                    let blocks = input_elements
                        .checked_add(31)
                        .ok_or(PlanError::SizeOverflow)?
                        / 32;
                    let input = builder.slot(
                        SlotKind::Scratch,
                        SlotStorage::I8,
                        checked_mul(input_elements, u64::from(llm.max_batch_tokens))?,
                    )?;
                    let _scales = builder.slot(
                        SlotKind::Scratch,
                        SlotStorage::F32,
                        checked_mul(checked_mul(blocks, u64::from(llm.max_batch_tokens))?, 4)?,
                    )?;
                    let output = builder.slot(
                        SlotKind::Result,
                        SlotStorage::F32,
                        checked_mul(
                            checked_mul(
                                u64::from(rows.end - rows.start),
                                u64::from(llm.max_batch_tokens),
                            )?,
                            4,
                        )?,
                    )?;
                    let program = builder.program(
                        ProgramKind::Q8Rows {
                            tensor: entry.id,
                            rows: rows.clone(),
                            batch_capacity: llm.max_batch_tokens,
                        },
                        input,
                        output,
                        vec![LayerOp::Q8Matmul {
                            input,
                            weight: entry.id,
                            output,
                        }],
                    );
                    shards.push(RowShard {
                        device,
                        rows,
                        tensor_bytes: start..end,
                        program,
                        input,
                        output,
                    });
                }
                row_shards.insert(entry.id, shards);
            } else {
                let builder = builders.get_mut(&primary).unwrap();
                builder.tensor(entry.id, 0..row_count, entry.segment_byte_range.clone())?;
            }
        }
        components.insert(
            component,
            ComponentPlan {
                component,
                mode: PlacementMode::Row,
                primary,
                embedding,
                finalization: None,
                layer_spans: Vec::new(),
                activation_transfers: Vec::new(),
                row_shards,
            },
        );
        Ok(())
    }

    fn compile_layer(
        &self,
        component: ComponentId,
        rule: &PlacementRule,
        llm: &LlmRequirements,
        builders: &mut BTreeMap<DeviceId, DeviceBuilder>,
        components: &mut BTreeMap<ComponentId, ComponentPlan>,
    ) -> Result<(), PlanError> {
        if llm.layers.is_empty()
            || llm
                .layers
                .iter()
                .enumerate()
                .any(|(index, layer)| layer.layer() != index as u32)
        {
            return Err(PlanError::UnsupportedComponent {
                component,
                device: rule.targets[0].device.clone(),
            });
        }
        let primary = rule.targets[0].device.clone();
        let ranges = weighted_ranges(llm.layers.len() as u32, &rule.targets)?;
        for (device, layers) in &ranges {
            for layer in &llm.layers[layers.start as usize..layers.end as usize] {
                self.ensure_device(
                    component,
                    device,
                    PlacementMode::Layer,
                    Some(layer.family()),
                )?;
            }
        }
        let activation_bytes = checked_mul(
            checked_mul(u64::from(llm.hidden_size), u64::from(llm.max_batch_tokens))?,
            4,
        )?;
        let embedding_id = self.catalog.find(component, "token_embd.weight");
        let embedding = if let Some(tensor) = embedding_id {
            let entry = self.ensure_tensor(tensor, &primary)?;
            let builder = builders.get_mut(&primary).unwrap();
            builder.tensor(
                tensor,
                0..entry.row_count as u32,
                entry.segment_byte_range.clone(),
            )?;
            let input = builder.slot(
                SlotKind::Scratch,
                SlotStorage::I8,
                checked_mul(u64::from(llm.max_batch_tokens), 4)?,
            )?;
            let output = builder.slot(SlotKind::Activation, SlotStorage::F32, activation_bytes)?;
            let program = builder.program(
                ProgramKind::EmbeddingRows {
                    tensor,
                    row_count: entry.row_count as u32,
                },
                input,
                output,
                Vec::new(),
            );
            Some(ProgramBinding {
                device: primary.clone(),
                program,
                input,
                output,
            })
        } else {
            None
        };
        let mut spans = Vec::new();
        for (span_index, (device, layers)) in ranges.into_iter().enumerate() {
            let builder = builders.get_mut(&device).unwrap();
            for entry in self.catalog.entries().iter().filter(|entry| {
                entry.component == component
                    && entry.layer.is_some_and(|layer| layers.contains(&layer))
            }) {
                if !builder
                    .plan
                    .descriptor
                    .capabilities
                    .tensor_types
                    .contains(&entry.ggml_type)
                {
                    return Err(PlanError::UnsupportedTensor {
                        tensor: entry.id,
                        device: device.clone(),
                    });
                }
                builder.tensor(
                    entry.id,
                    0..entry.row_count as u32,
                    entry.segment_byte_range.clone(),
                )?;
            }
            let input = if let Some(binding) = embedding
                .as_ref()
                .filter(|binding| span_index == 0 && binding.device == device)
            {
                binding.output
            } else {
                builder.slot(SlotKind::Activation, SlotStorage::F32, activation_bytes)?
            };
            let alternate =
                builder.slot(SlotKind::Activation, SlotStorage::F32, activation_bytes)?;
            let mut ops = Vec::new();
            let mut layer_input = input;
            for (layer_index, layer) in llm.layers[layers.start as usize..layers.end as usize]
                .iter()
                .enumerate()
            {
                let layer_output = if layer_index % 2 == 0 {
                    alternate
                } else {
                    input
                };
                append_layer_ops(
                    builder,
                    self.catalog,
                    layer,
                    llm,
                    layer_input,
                    layer_output,
                    &mut ops,
                )?;
                layer_input = layer_output;
            }
            let output = layer_input;
            let families = llm.layers[layers.start as usize..layers.end as usize]
                .iter()
                .map(LlmLayerSpec::family)
                .collect();
            let program = builder.program(
                ProgramKind::LayerSegment {
                    layers: layers.clone(),
                    families,
                },
                input,
                output,
                ops,
            );
            spans.push(LayerSpan {
                device,
                layers,
                program,
                input,
                output,
            });
        }
        let final_builder = builders.get_mut(&primary).unwrap();
        for tensor in [llm.final_norm, llm.output] {
            let entry = self.ensure_tensor(tensor, &primary)?;
            if tensor == llm.output && entry.ggml_type != GGMLType::Q8_0 {
                return Err(PlanError::UnsupportedTensor {
                    tensor,
                    device: primary.clone(),
                });
            }
            final_builder.tensor(
                tensor,
                0..entry.row_count as u32,
                entry.segment_byte_range.clone(),
            )?;
        }
        let final_input = if spans.last().is_some_and(|span| span.device == primary) {
            spans.last().unwrap().output
        } else {
            final_builder.slot(SlotKind::Activation, SlotStorage::F32, activation_bytes)?
        };
        let output_rows = self.catalog.entry(llm.output).unwrap().row_count;
        let final_output = final_builder.slot(
            SlotKind::Result,
            SlotStorage::F32,
            checked_mul(
                checked_mul(output_rows, u64::from(llm.max_batch_tokens))?,
                4,
            )?,
        )?;
        let final_program = final_builder.program(
            ProgramKind::FinalNormQ8Logits {
                norm: llm.final_norm,
                output: llm.output,
                epsilon_bits: llm.norm_epsilon_bits,
                batch_capacity: llm.max_batch_tokens,
            },
            final_input,
            final_output,
            vec![
                LayerOp::RmsNorm {
                    input: final_input,
                    weight: llm.final_norm,
                    output: final_input,
                    epsilon_bits: llm.norm_epsilon_bits,
                },
                LayerOp::Q8Matmul {
                    input: final_input,
                    weight: llm.output,
                    output: final_output,
                },
            ],
        );
        let finalization = Some(ProgramBinding {
            device: primary.clone(),
            program: final_program,
            input: final_input,
            output: final_output,
        });
        let mut transfers = Vec::new();
        if let (Some(embedding), Some(first)) = (&embedding, spans.first()) {
            if embedding.device != first.device {
                transfers.push(ActivationTransfer {
                    after_span: None,
                    target: TransferTarget::Span(0),
                    from_device: embedding.device.clone(),
                    from_slot: embedding.output,
                    to_device: first.device.clone(),
                    to_slot: first.input,
                    f32_values_per_token: llm.hidden_size,
                });
            }
        }
        for (index, pair) in spans.windows(2).enumerate() {
            if pair[0].device != pair[1].device {
                transfers.push(ActivationTransfer {
                    after_span: Some(index as u32),
                    target: TransferTarget::Span(index as u32 + 1),
                    from_device: pair[0].device.clone(),
                    from_slot: pair[0].output,
                    to_device: pair[1].device.clone(),
                    to_slot: pair[1].input,
                    f32_values_per_token: llm.hidden_size,
                });
            }
        }
        if let Some(last) = spans.last() {
            if last.device != primary {
                transfers.push(ActivationTransfer {
                    after_span: Some(spans.len() as u32 - 1),
                    target: TransferTarget::Finalization,
                    from_device: last.device.clone(),
                    from_slot: last.output,
                    to_device: primary.clone(),
                    to_slot: final_input,
                    f32_values_per_token: llm.hidden_size,
                });
            }
        }
        components.insert(
            component,
            ComponentPlan {
                component,
                mode: PlacementMode::Layer,
                primary,
                embedding,
                finalization,
                layer_spans: spans,
                activation_transfers: transfers,
                row_shards: BTreeMap::new(),
            },
        );
        Ok(())
    }
}

fn append_layer_ops(
    builder: &mut DeviceBuilder,
    catalog: &TensorCatalog,
    layer: &LlmLayerSpec,
    llm: &LlmRequirements,
    input: SlotId,
    output: SlotId,
    ops: &mut Vec<LayerOp>,
) -> Result<(), PlanError> {
    let f32_slot = |builder: &mut DeviceBuilder, elements: u64| {
        builder.slot(
            SlotKind::Scratch,
            SlotStorage::F32,
            checked_mul(checked_mul(elements, u64::from(llm.max_batch_tokens))?, 4)?,
        )
    };
    let state_storage = match llm.kv_cache {
        KvCacheType::F16 => SlotStorage::F16,
        KvCacheType::F32 => SlotStorage::F32,
    };
    let state_bytes = match llm.kv_cache {
        KvCacheType::F16 => 2,
        KvCacheType::F32 => 4,
    };
    let tensor_rows = |tensor: TensorId| {
        catalog
            .entry(tensor)
            .map(|entry| entry.row_count)
            .ok_or(PlanError::SizeOverflow)
    };
    match layer {
        LlmLayerSpec::Qwen3(spec) => {
            let norm = f32_slot(builder, u64::from(llm.hidden_size))?;
            let q = f32_slot(
                builder,
                u64::from(spec.head_count) * u64::from(spec.key_head_dim),
            )?;
            let k = f32_slot(
                builder,
                u64::from(spec.kv_head_count) * u64::from(spec.key_head_dim),
            )?;
            let v = f32_slot(
                builder,
                u64::from(spec.kv_head_count) * u64::from(spec.value_head_dim),
            )?;
            let ffn_gate = f32_slot(builder, tensor_rows(spec.ffn_gate)?)?;
            let ffn_up = f32_slot(builder, tensor_rows(spec.ffn_up)?)?;
            let key_state = builder.slot(
                SlotKind::KvState,
                state_storage,
                checked_mul(
                    checked_mul(
                        u64::from(llm.context_length),
                        u64::from(spec.kv_head_count) * u64::from(spec.key_head_dim),
                    )?,
                    state_bytes,
                )?,
            )?;
            let value_state = builder.slot(
                SlotKind::KvState,
                state_storage,
                checked_mul(
                    checked_mul(
                        u64::from(llm.context_length),
                        u64::from(spec.kv_head_count) * u64::from(spec.value_head_dim),
                    )?,
                    state_bytes,
                )?,
            )?;
            ops.extend([
                LayerOp::RmsNorm {
                    input,
                    weight: spec.attn_norm,
                    output: norm,
                    epsilon_bits: spec.norm_epsilon_bits,
                },
                LayerOp::Q8Matmul {
                    input: norm,
                    weight: spec.q,
                    output: q,
                },
                LayerOp::Q8Matmul {
                    input: norm,
                    weight: spec.k,
                    output: k,
                },
                LayerOp::Q8Matmul {
                    input: norm,
                    weight: spec.v,
                    output: v,
                },
            ]);
            if let Some(weight) = spec.q_norm {
                ops.push(LayerOp::RmsNorm {
                    input: q,
                    weight,
                    output: q,
                    epsilon_bits: spec.norm_epsilon_bits,
                });
            }
            if let Some(weight) = spec.k_norm {
                ops.push(LayerOp::RmsNorm {
                    input: k,
                    weight,
                    output: k,
                    epsilon_bits: spec.norm_epsilon_bits,
                });
            }
            ops.extend([
                LayerOp::Rope {
                    q,
                    k,
                    key_head_dim: spec.key_head_dim,
                    rope_dims: spec.rope_dims,
                    freq_base_bits: spec.rope_freq_base_bits,
                },
                LayerOp::KvAppend {
                    layer: spec.layer,
                    k,
                    v,
                    key_state,
                    value_state,
                },
                LayerOp::Attention {
                    layer: spec.layer,
                    q,
                    output: norm,
                    head_count: spec.head_count,
                    kv_head_count: spec.kv_head_count,
                    key_state,
                    value_state,
                    key_head_dim: spec.key_head_dim,
                    value_head_dim: spec.value_head_dim,
                    context_capacity: llm.context_length,
                },
                LayerOp::Q8Matmul {
                    input: norm,
                    weight: spec.o,
                    output,
                },
                LayerOp::Add {
                    left: input,
                    right: output,
                    output,
                },
                LayerOp::RmsNorm {
                    input: output,
                    weight: spec.ffn_norm,
                    output: norm,
                    epsilon_bits: spec.norm_epsilon_bits,
                },
                LayerOp::Q8Matmul {
                    input: norm,
                    weight: spec.ffn_gate,
                    output: ffn_gate,
                },
                LayerOp::Q8Matmul {
                    input: norm,
                    weight: spec.ffn_up,
                    output: ffn_up,
                },
                LayerOp::SiluMul {
                    gate: ffn_gate,
                    up: ffn_up,
                },
                LayerOp::Q8Matmul {
                    input: ffn_up,
                    weight: spec.ffn_down,
                    output: norm,
                },
                LayerOp::Add {
                    left: output,
                    right: norm,
                    output,
                },
            ]);
        }
        LlmLayerSpec::Qwen35Dense(spec) => {
            let norm = f32_slot(builder, u64::from(llm.hidden_size))?;
            let q_elements = u64::from(spec.head_count) * u64::from(spec.key_head_dim);
            let q_projection = f32_slot(builder, q_elements * 2)?;
            let q = f32_slot(builder, q_elements)?;
            let q_gate = f32_slot(builder, q_elements)?;
            let k = f32_slot(
                builder,
                u64::from(spec.kv_head_count) * u64::from(spec.key_head_dim),
            )?;
            let v = f32_slot(
                builder,
                u64::from(spec.kv_head_count) * u64::from(spec.value_head_dim),
            )?;
            let ffn_gate = f32_slot(builder, tensor_rows(spec.ffn_gate)?)?;
            let ffn_up = f32_slot(builder, tensor_rows(spec.ffn_up)?)?;
            let key_state = builder.slot(
                SlotKind::KvState,
                state_storage,
                checked_mul(
                    checked_mul(
                        u64::from(llm.context_length),
                        u64::from(spec.kv_head_count) * u64::from(spec.key_head_dim),
                    )?,
                    state_bytes,
                )?,
            )?;
            let value_state = builder.slot(
                SlotKind::KvState,
                state_storage,
                checked_mul(
                    checked_mul(
                        u64::from(llm.context_length),
                        u64::from(spec.kv_head_count) * u64::from(spec.value_head_dim),
                    )?,
                    state_bytes,
                )?,
            )?;
            ops.extend([
                LayerOp::RmsNorm {
                    input,
                    weight: spec.attn_norm,
                    output: norm,
                    epsilon_bits: spec.norm_epsilon_bits,
                },
                LayerOp::Q8Matmul {
                    input: norm,
                    weight: spec.q,
                    output: q_projection,
                },
                LayerOp::Slice {
                    input: q_projection,
                    offset: 0,
                    elements: u32::try_from(q_elements).map_err(|_| PlanError::SizeOverflow)?,
                    output: q,
                },
                LayerOp::Slice {
                    input: q_projection,
                    offset: u32::try_from(q_elements).map_err(|_| PlanError::SizeOverflow)?,
                    elements: u32::try_from(q_elements).map_err(|_| PlanError::SizeOverflow)?,
                    output: q_gate,
                },
                LayerOp::Q8Matmul {
                    input: norm,
                    weight: spec.k,
                    output: k,
                },
                LayerOp::Q8Matmul {
                    input: norm,
                    weight: spec.v,
                    output: v,
                },
                LayerOp::RmsNorm {
                    input: q,
                    weight: spec.q_norm,
                    output: q,
                    epsilon_bits: spec.norm_epsilon_bits,
                },
                LayerOp::RmsNorm {
                    input: k,
                    weight: spec.k_norm,
                    output: k,
                    epsilon_bits: spec.norm_epsilon_bits,
                },
                LayerOp::MRope {
                    q,
                    k,
                    sections: spec.rope_sections,
                    key_head_dim: spec.key_head_dim,
                    rope_dims: spec.rope_dims,
                    freq_base_bits: spec.rope_freq_base_bits,
                },
                LayerOp::KvAppend {
                    layer: spec.layer,
                    k,
                    v,
                    key_state,
                    value_state,
                },
                LayerOp::Attention {
                    layer: spec.layer,
                    q,
                    output: norm,
                    head_count: spec.head_count,
                    kv_head_count: spec.kv_head_count,
                    key_state,
                    value_state,
                    key_head_dim: spec.key_head_dim,
                    value_head_dim: spec.value_head_dim,
                    context_capacity: llm.context_length,
                },
                LayerOp::SigmoidMul {
                    gate: q_gate,
                    values: norm,
                },
                LayerOp::Q8Matmul {
                    input: norm,
                    weight: spec.o,
                    output,
                },
                LayerOp::RmsNorm {
                    input: output,
                    weight: spec.post_attn_norm,
                    output: output,
                    epsilon_bits: spec.norm_epsilon_bits,
                },
                LayerOp::Add {
                    left: input,
                    right: output,
                    output,
                },
                LayerOp::Q8Matmul {
                    input: output,
                    weight: spec.ffn_gate,
                    output: ffn_gate,
                },
                LayerOp::Q8Matmul {
                    input: output,
                    weight: spec.ffn_up,
                    output: ffn_up,
                },
                LayerOp::SiluMul {
                    gate: ffn_gate,
                    up: ffn_up,
                },
                LayerOp::Q8Matmul {
                    input: ffn_up,
                    weight: spec.ffn_down,
                    output: norm,
                },
                LayerOp::Add {
                    left: output,
                    right: norm,
                    output,
                },
            ]);
        }
        LlmLayerSpec::Qwen35Recurrent(spec) => {
            let norm = f32_slot(builder, u64::from(llm.hidden_size))?;
            let key_elements = u64::from(spec.state_size) * u64::from(spec.group_count);
            let qkv = f32_slot(
                builder,
                key_elements
                    .checked_mul(2)
                    .and_then(|value| value.checked_add(u64::from(spec.inner_size)))
                    .ok_or(PlanError::SizeOverflow)?,
            )?;
            let q = f32_slot(builder, key_elements)?;
            let k = f32_slot(builder, key_elements)?;
            let v = f32_slot(builder, u64::from(spec.inner_size))?;
            let alpha = f32_slot(builder, u64::from(spec.inner_size))?;
            let beta = f32_slot(builder, u64::from(spec.inner_size))?;
            let gate = f32_slot(builder, u64::from(spec.inner_size))?;
            let ffn_gate = f32_slot(builder, tensor_rows(spec.ffn_gate)?)?;
            let ffn_up = f32_slot(builder, tensor_rows(spec.ffn_up)?)?;
            let conv_state = builder.slot(
                SlotKind::ConvState,
                SlotStorage::F32,
                checked_mul(
                    checked_mul(u64::from(spec.conv_width), u64::from(spec.inner_size))?,
                    4,
                )?,
            )?;
            let ssm_state = builder.slot(
                SlotKind::SsmState,
                SlotStorage::F32,
                checked_mul(
                    checked_mul(u64::from(spec.state_size), u64::from(spec.inner_size))?,
                    4,
                )?,
            )?;
            ops.extend([
                LayerOp::RmsNorm {
                    input,
                    weight: spec.attn_norm,
                    output: norm,
                    epsilon_bits: spec.norm_epsilon_bits,
                },
                LayerOp::Q8Matmul {
                    input: norm,
                    weight: spec.qkv,
                    output: qkv,
                },
                LayerOp::Q8Matmul {
                    input: norm,
                    weight: spec.gate,
                    output: gate,
                },
                LayerOp::Slice {
                    input: qkv,
                    offset: 0,
                    elements: u32::try_from(key_elements).map_err(|_| PlanError::SizeOverflow)?,
                    output: q,
                },
                LayerOp::Slice {
                    input: qkv,
                    offset: u32::try_from(key_elements).map_err(|_| PlanError::SizeOverflow)?,
                    elements: u32::try_from(key_elements).map_err(|_| PlanError::SizeOverflow)?,
                    output: k,
                },
                LayerOp::Slice {
                    input: qkv,
                    offset: u32::try_from(
                        key_elements.checked_mul(2).ok_or(PlanError::SizeOverflow)?,
                    )
                    .map_err(|_| PlanError::SizeOverflow)?,
                    elements: spec.inner_size,
                    output: v,
                },
                LayerOp::Q8Matmul {
                    input: norm,
                    weight: spec.beta,
                    output: beta,
                },
                LayerOp::Sigmoid { values: beta },
                LayerOp::Q8Matmul {
                    input: norm,
                    weight: spec.alpha,
                    output: alpha,
                },
                LayerOp::DepthwiseCausalConv {
                    input: q,
                    weight: spec.conv_weight,
                    state: conv_state,
                    width: spec.conv_width,
                    output: q,
                },
                LayerOp::Silu { values: q },
                LayerOp::L2Norm {
                    values: q,
                    epsilon_bits: spec.norm_epsilon_bits,
                },
                LayerOp::L2Norm {
                    values: k,
                    epsilon_bits: spec.norm_epsilon_bits,
                },
                LayerOp::SoftplusAffine {
                    values: alpha,
                    bias: spec.dt_bias,
                    scale: spec.ssm_a,
                },
                LayerOp::SsmUpdate {
                    q,
                    k,
                    v,
                    alpha,
                    beta,
                    state: ssm_state,
                    output: norm,
                    state_size: spec.state_size,
                    group_count: spec.group_count,
                    dt_rank: spec.dt_rank,
                    inner_size: spec.inner_size,
                },
                LayerOp::RmsNorm {
                    input: norm,
                    weight: spec.ssm_norm,
                    output: norm,
                    epsilon_bits: spec.norm_epsilon_bits,
                },
                LayerOp::SigmoidMul { gate, values: norm },
                LayerOp::Q8Matmul {
                    input: norm,
                    weight: spec.ssm_output,
                    output,
                },
                LayerOp::RmsNorm {
                    input: output,
                    weight: spec.post_attn_norm,
                    output,
                    epsilon_bits: spec.norm_epsilon_bits,
                },
                LayerOp::Add {
                    left: input,
                    right: output,
                    output,
                },
                LayerOp::Q8Matmul {
                    input: output,
                    weight: spec.ffn_gate,
                    output: ffn_gate,
                },
                LayerOp::Q8Matmul {
                    input: output,
                    weight: spec.ffn_up,
                    output: ffn_up,
                },
                LayerOp::SiluMul {
                    gate: ffn_gate,
                    up: ffn_up,
                },
                LayerOp::Q8Matmul {
                    input: ffn_up,
                    weight: spec.ffn_down,
                    output: norm,
                },
                LayerOp::Add {
                    left: output,
                    right: norm,
                    output,
                },
            ]);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compute::{
        DeviceCapabilities, DeviceDescriptor, DeviceDiscovery, DeviceProvider, DeviceSession,
    };
    use crate::{MetaValue, SourceFormat, SourceTensorRecord, TensorInfo, TensorSource};
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn id(value: &str) -> DeviceId {
        DeviceId::parse(value).unwrap()
    }

    fn targets(values: &[(&str, f64)]) -> Vec<NormalizedTarget> {
        let total = values.iter().map(|(_, weight)| weight).sum::<f64>();
        values
            .iter()
            .enumerate()
            .map(|(ordinal, (device, weight))| NormalizedTarget {
                device: id(device),
                fraction: weight / total,
                ordinal,
            })
            .collect()
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

    struct TestDiscovery(Vec<DeviceDescriptor>);

    impl DeviceDiscovery for TestDiscovery {
        fn backend(&self) -> BackendKind {
            self.0[0].backend
        }

        fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, BackendError> {
            Ok(self.0.clone())
        }
    }

    struct TestProvider {
        descriptor: DeviceDescriptor,
        opens: Arc<AtomicUsize>,
    }

    impl DeviceDiscovery for TestProvider {
        fn backend(&self) -> BackendKind {
            self.descriptor.backend
        }

        fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, BackendError> {
            Ok(vec![self.descriptor.clone()])
        }
    }

    impl DeviceProvider for TestProvider {
        fn open(
            &self,
            descriptor: &DeviceDescriptor,
            _plan: &DevicePlan,
            _catalog: Arc<TensorCatalog>,
        ) -> Result<Box<dyn DeviceSession>, BackendError> {
            self.opens.fetch_add(1, Ordering::SeqCst);
            Err(BackendError::Allocation {
                device: descriptor.id.clone(),
                message: "test provider must not open while compiling".into(),
            })
        }
    }

    fn descriptor(id: &str, backend: BackendKind, physical_key: &str) -> DeviceDescriptor {
        DeviceDescriptor {
            id: DeviceId::parse(id).unwrap(),
            backend,
            physical_key: physical_key.into(),
            name: id.into(),
            usable_bytes: 64 * 1024 * 1024,
            max_allocation_bytes: 64 * 1024 * 1024,
            buffer_alignment: 16,
            unified_memory: backend == BackendKind::Cpu,
            capabilities: DeviceCapabilities {
                components: BTreeSet::from([ComponentId::Llm, ComponentId::Vision]),
                modes: BTreeSet::from([PlacementMode::Layer, PlacementMode::Row]),
                layer_families: BTreeSet::from([
                    LayerFamily::Qwen3,
                    LayerFamily::Qwen35Dense,
                    LayerFamily::Qwen35Recurrent,
                ]),
                tensor_types: BTreeSet::from([GGMLType::F32, GGMLType::Q8_0]),
            },
        }
    }

    fn registry(descriptors: Vec<DeviceDescriptor>) -> DeviceRegistry {
        let mut by_backend = BTreeMap::<BackendKind, Vec<DeviceDescriptor>>::new();
        for descriptor in descriptors {
            by_backend
                .entry(descriptor.backend)
                .or_default()
                .push(descriptor);
        }
        let requested = by_backend.keys().copied().collect::<BTreeSet<_>>();
        let mut registry = DeviceRegistry::new();
        for descriptors in by_backend.into_values() {
            registry
                .register_discovery(Arc::new(TestDiscovery(descriptors)))
                .unwrap();
        }
        registry.discover(&requested).unwrap();
        registry
    }

    fn test_catalog(layer_count: u32, weight_type: GGMLType) -> TensorCatalog {
        let mut tensors = vec![(
            "token_embd.weight".to_string(),
            vec![32, 16],
            GGMLType::F32,
            None,
        )];
        let row_elements = weight_type.type_traits().0 as u64;
        for layer in 0..layer_count {
            tensors.push((
                format!("blk.{layer}.weight"),
                vec![row_elements, 10],
                weight_type,
                Some(layer),
            ));
        }
        tensors.extend([
            (
                "output_norm.weight".to_string(),
                vec![32],
                GGMLType::F32,
                None,
            ),
            (
                "output.weight".to_string(),
                vec![32, 12],
                GGMLType::Q8_0,
                None,
            ),
        ]);
        catalog_from_tensors(tensors)
    }

    fn catalog_from_tensors(
        tensors: Vec<(String, Vec<u64>, GGMLType, Option<u32>)>,
    ) -> TensorCatalog {
        let mut offset = 0_u64;
        let mut records = Vec::new();
        let mut bytes = BTreeMap::new();
        let mut push = |name: String, dims: Vec<u64>, ggml_type: GGMLType, layer| {
            let info = TensorInfo {
                name: name.clone(),
                dims,
                ggml_type,
                offset,
            };
            let len = info.checked_nbytes().unwrap();
            records.push(SourceTensorRecord {
                info,
                segment_id: 0,
                segment_byte_range: offset..offset + len,
                layer,
            });
            bytes.insert(name, vec![0; len as usize]);
            offset += len;
        };
        for (name, dims, ggml_type, layer) in tensors {
            push(name, dims, ggml_type, layer);
        }
        TensorCatalog::from_sources(vec![(
            ComponentId::Llm,
            Arc::new(TestSource { records, bytes }),
        )])
        .unwrap()
    }

    fn llm_requirements(catalog: &TensorCatalog, layer_count: u32) -> ComponentRequirements {
        let layer_weights = (0..layer_count)
            .map(|layer| {
                catalog
                    .find(ComponentId::Llm, &format!("blk.{layer}.weight"))
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let layers = layer_weights
            .iter()
            .enumerate()
            .map(|(layer, &weight)| {
                LlmLayerSpec::Qwen3(Qwen3LayerSpec {
                    layer: layer as u32,
                    attn_norm: weight,
                    q_norm: None,
                    k_norm: None,
                    q: weight,
                    k: weight,
                    v: weight,
                    o: weight,
                    ffn_norm: weight,
                    ffn_gate: weight,
                    ffn_up: weight,
                    ffn_down: weight,
                    head_count: 2,
                    kv_head_count: 1,
                    key_head_dim: 4,
                    value_head_dim: 3,
                    rope_dims: 4,
                    rope_freq_base_bits: 10_000_f32.to_bits(),
                    norm_epsilon_bits: 1e-6_f32.to_bits(),
                })
            })
            .collect();
        ComponentRequirements {
            component: ComponentId::Llm,
            workload: ComponentWorkload::Llm(LlmRequirements {
                layers,
                hidden_size: 32,
                context_length: 16,
                max_batch_tokens: 2,
                kv_cache: KvCacheType::F16,
                final_norm: catalog
                    .find(ComponentId::Llm, "output_norm.weight")
                    .unwrap(),
                output: catalog.find(ComponentId::Llm, "output.weight").unwrap(),
                norm_epsilon_bits: 1e-6_f32.to_bits(),
            }),
        }
    }

    fn compile(
        catalog: &TensorCatalog,
        registry: &DeviceRegistry,
        requirement: &ComponentRequirements,
        rule: PlacementRule,
    ) -> Result<ExecutionPlan, PlanError> {
        PlacementCompiler {
            catalog,
            registry,
            requirements: std::slice::from_ref(requirement),
        }
        .compile(&BTreeMap::from([(rule.component, rule)]))
    }

    #[test]
    fn largest_remainder_is_weighted_contiguous_and_stable() {
        let weighted = targets(&[("cpu0", 1.0), ("vulkan0", 2.0), ("metal0", 1.0)]);
        assert_eq!(
            weighted_ranges(10, &weighted).unwrap(),
            vec![
                (id("cpu0"), 0..3),
                (id("vulkan0"), 3..8),
                (id("metal0"), 8..10)
            ]
        );
        let tied = targets(&[("cpu0", 1.0), ("metal0", 1.0), ("vulkan0", 1.0)]);
        assert_eq!(
            weighted_ranges(5, &tied).unwrap(),
            vec![
                (id("cpu0"), 0..2),
                (id("metal0"), 2..4),
                (id("vulkan0"), 4..5)
            ]
        );
    }

    #[test]
    fn positive_target_cannot_receive_zero_units() {
        assert!(matches!(
            weighted_ranges(
                2,
                &targets(&[("cpu0", 1.0), ("metal0", 1.0), ("vulkan0", 1.0)])
            ),
            Err(PlanError::InsufficientUnits { .. })
        ));
    }

    #[test]
    fn unequal_row_plan_shards_every_matrix_without_gaps_or_overlap() {
        let catalog = test_catalog(1, GGMLType::Q8_0);
        let registry = registry(vec![
            descriptor("cpu0", BackendKind::Cpu, "cpu"),
            descriptor("metal0", BackendKind::Metal, "metal"),
            descriptor("vulkan0", BackendKind::Vulkan, "vulkan"),
        ]);
        let requirement = llm_requirements(&catalog, 1);
        let rule = PlacementRule {
            component: ComponentId::Llm,
            mode: PlacementMode::Row,
            targets: targets(&[("cpu0", 1.0), ("metal0", 2.0), ("vulkan0", 1.0)]),
        };
        let plan = compile(&catalog, &registry, &requirement, rule).unwrap();
        let component = &plan.components[&ComponentId::Llm];
        assert!(component.embedding.is_some());
        for (&tensor, shards) in &component.row_shards {
            let row_count = catalog.entry(tensor).unwrap().row_count as u32;
            assert_eq!(shards.first().unwrap().rows.start, 0);
            assert_eq!(shards.last().unwrap().rows.end, row_count);
            assert!(shards
                .windows(2)
                .all(|pair| pair[0].rows.end == pair[1].rows.start));
        }
        let layer = catalog.find(ComponentId::Llm, "blk.0.weight").unwrap();
        assert_eq!(
            component.row_shards[&layer]
                .iter()
                .map(|shard| shard.rows.clone())
                .collect::<Vec<_>>(),
            vec![0..3, 3..8, 8..10]
        );

        let two = PlacementRule {
            component: ComponentId::Llm,
            mode: PlacementMode::Row,
            targets: targets(&[("cpu0", 1.0), ("metal0", 3.0)]),
        };
        let plan = compile(&catalog, &registry, &requirement, two).unwrap();
        assert_eq!(
            plan.components[&ComponentId::Llm].row_shards[&layer]
                .iter()
                .map(|shard| shard.rows.clone())
                .collect::<Vec<_>>(),
            vec![0..3, 3..10]
        );
    }

    #[test]
    fn layer_plan_is_contiguous_and_compiles_explicit_transfers() {
        let catalog = test_catalog(5, GGMLType::Q8_0);
        let registry = registry(vec![
            descriptor("cpu0", BackendKind::Cpu, "cpu"),
            descriptor("metal0", BackendKind::Metal, "metal"),
        ]);
        let requirement = llm_requirements(&catalog, 5);
        let rule = PlacementRule {
            component: ComponentId::Llm,
            mode: PlacementMode::Layer,
            targets: targets(&[("cpu0", 1.0), ("metal0", 2.0)]),
        };
        let plan = compile(&catalog, &registry, &requirement, rule).unwrap();
        let component = &plan.components[&ComponentId::Llm];
        assert_eq!(
            component
                .layer_spans
                .iter()
                .map(|span| span.layers.clone())
                .collect::<Vec<_>>(),
            vec![0..2, 2..5]
        );
        assert_eq!(component.activation_transfers.len(), 2);
        assert!(matches!(
            component.activation_transfers[0].target,
            TransferTarget::Span(1)
        ));
        assert!(matches!(
            component.activation_transfers[1].target,
            TransferTarget::Finalization
        ));
        assert!(plan.devices[&id("metal0")].programs[0]
            .layer_ops
            .iter()
            .any(|op| matches!(
                op,
                LayerOp::Attention {
                    key_head_dim: 4,
                    value_head_dim: 3,
                    ..
                }
            )));
    }

    #[test]
    fn same_device_layer_span_chains_embedding_and_each_layer_output() {
        let catalog = test_catalog(2, GGMLType::Q8_0);
        let registry = registry(vec![descriptor("cpu0", BackendKind::Cpu, "cpu")]);
        let requirement = llm_requirements(&catalog, 2);
        let rule = PlacementRule {
            component: ComponentId::Llm,
            mode: PlacementMode::Layer,
            targets: targets(&[("cpu0", 1.0)]),
        };
        let plan = compile(&catalog, &registry, &requirement, rule).unwrap();
        let component = &plan.components[&ComponentId::Llm];
        let embedding = component.embedding.as_ref().unwrap();
        let span = &component.layer_spans[0];
        assert_eq!(embedding.output, span.input);
        assert!(component.activation_transfers.is_empty());

        let program = &plan.devices[&id("cpu0")].programs[span.program.0 as usize];
        let layers = match &requirement.workload {
            ComponentWorkload::Llm(llm) => &llm.layers,
            _ => unreachable!(),
        };
        let layer_inputs = layers
            .iter()
            .map(|layer| {
                let weight = match layer {
                    LlmLayerSpec::Qwen3(spec) => spec.attn_norm,
                    _ => unreachable!(),
                };
                program
                    .layer_ops
                    .iter()
                    .find_map(|op| match op {
                        LayerOp::RmsNorm {
                            input,
                            weight: found,
                            ..
                        } if *found == weight => Some(*input),
                        _ => None,
                    })
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(layer_inputs[0], span.input);
        assert_ne!(layer_inputs[1], span.input);
        let second_weight = match &layers[1] {
            LlmLayerSpec::Qwen3(spec) => spec.attn_norm,
            _ => unreachable!(),
        };
        let second_layer_start = program
            .layer_ops
            .iter()
            .position(|op| matches!(op, LayerOp::RmsNorm { input, weight, .. } if *input == layer_inputs[1] && *weight == second_weight))
            .unwrap();
        assert!(program.layer_ops[..second_layer_start]
            .iter()
            .rev()
            .any(|op| matches!(op, LayerOp::Add { output, .. } if *output == layer_inputs[1])));
        assert!(matches!(
            program.layer_ops.last(),
            Some(LayerOp::Add { output, .. }) if *output == span.output
        ));
    }

    #[test]
    fn row_q8_scratch_uses_each_matrix_input_width_and_block_count() {
        let catalog = catalog_from_tensors(vec![
            (
                "token_embd.weight".into(),
                vec![32, 16],
                GGMLType::F32,
                None,
            ),
            ("narrow.weight".into(), vec![32, 4], GGMLType::Q8_0, Some(0)),
            ("wide.weight".into(), vec![64, 4], GGMLType::Q8_0, Some(0)),
            ("output_norm.weight".into(), vec![32], GGMLType::F32, None),
            ("output.weight".into(), vec![32, 12], GGMLType::Q8_0, None),
        ]);
        let requirement = llm_requirements(&catalog, 0);
        let registry = registry(vec![descriptor("cpu0", BackendKind::Cpu, "cpu")]);
        let plan = compile(
            &catalog,
            &registry,
            &requirement,
            PlacementRule {
                component: ComponentId::Llm,
                mode: PlacementMode::Row,
                targets: targets(&[("cpu0", 1.0)]),
            },
        )
        .unwrap();
        let device = &plan.devices[&id("cpu0")];
        for (name, input_bytes, scale_bytes) in [("narrow.weight", 64, 8), ("wide.weight", 128, 16)]
        {
            let tensor = catalog.find(ComponentId::Llm, name).unwrap();
            let shard = &plan.components[&ComponentId::Llm].row_shards[&tensor][0];
            assert_eq!(device.slots[shard.input.0 as usize].byte_len, input_bytes);
            assert_eq!(
                device.slots[shard.input.0 as usize + 1].byte_len,
                scale_bytes
            );
        }
    }

    #[test]
    fn layer_ffn_intermediates_follow_tensor_rows_not_attention_width() {
        let tensors = vec![
            (
                "token_embd.weight".into(),
                vec![32, 16],
                GGMLType::F32,
                None,
            ),
            ("attn_norm".into(), vec![32], GGMLType::F32, Some(0)),
            ("q".into(), vec![32, 8], GGMLType::Q8_0, Some(0)),
            ("k".into(), vec![32, 4], GGMLType::Q8_0, Some(0)),
            ("v".into(), vec![32, 3], GGMLType::Q8_0, Some(0)),
            ("o".into(), vec![32, 32], GGMLType::Q8_0, Some(0)),
            ("ffn_norm".into(), vec![32], GGMLType::F32, Some(0)),
            ("ffn_gate".into(), vec![32, 64], GGMLType::Q8_0, Some(0)),
            ("ffn_up".into(), vec![32, 64], GGMLType::Q8_0, Some(0)),
            ("ffn_down".into(), vec![64, 32], GGMLType::Q8_0, Some(0)),
            ("output_norm.weight".into(), vec![32], GGMLType::F32, None),
            ("output.weight".into(), vec![32, 12], GGMLType::Q8_0, None),
        ];
        let catalog = catalog_from_tensors(tensors);
        let find = |name| catalog.find(ComponentId::Llm, name).unwrap();
        let requirement = ComponentRequirements {
            component: ComponentId::Llm,
            workload: ComponentWorkload::Llm(LlmRequirements {
                layers: vec![LlmLayerSpec::Qwen3(Qwen3LayerSpec {
                    layer: 0,
                    attn_norm: find("attn_norm"),
                    q_norm: None,
                    k_norm: None,
                    q: find("q"),
                    k: find("k"),
                    v: find("v"),
                    o: find("o"),
                    ffn_norm: find("ffn_norm"),
                    ffn_gate: find("ffn_gate"),
                    ffn_up: find("ffn_up"),
                    ffn_down: find("ffn_down"),
                    head_count: 2,
                    kv_head_count: 1,
                    key_head_dim: 4,
                    value_head_dim: 3,
                    rope_dims: 4,
                    rope_freq_base_bits: 10_000_f32.to_bits(),
                    norm_epsilon_bits: 1e-6_f32.to_bits(),
                })],
                hidden_size: 32,
                context_length: 16,
                max_batch_tokens: 2,
                kv_cache: KvCacheType::F16,
                final_norm: find("output_norm.weight"),
                output: find("output.weight"),
                norm_epsilon_bits: 1e-6_f32.to_bits(),
            }),
        };
        let registry = registry(vec![descriptor("cpu0", BackendKind::Cpu, "cpu")]);
        let plan = compile(
            &catalog,
            &registry,
            &requirement,
            PlacementRule {
                component: ComponentId::Llm,
                mode: PlacementMode::Layer,
                targets: targets(&[("cpu0", 1.0)]),
            },
        )
        .unwrap();
        let device = &plan.devices[&id("cpu0")];
        let span = &plan.components[&ComponentId::Llm].layer_spans[0];
        let program = &device.programs[span.program.0 as usize];
        let output_slot = |weight| {
            program
                .layer_ops
                .iter()
                .find_map(|op| match op {
                    LayerOp::Q8Matmul {
                        weight: found,
                        output,
                        ..
                    } if *found == weight => Some(*output),
                    _ => None,
                })
                .unwrap()
        };
        let q = output_slot(find("q"));
        for weight in [find("ffn_gate"), find("ffn_up")] {
            let slot = output_slot(weight);
            assert_ne!(slot, q);
            assert_eq!(device.slots[slot.0 as usize].byte_len, 64 * 2 * 4);
        }
    }

    #[test]
    fn row_keeps_non_q8_matrix_on_capable_cpu_primary() {
        let catalog = catalog_from_tensors(vec![
            (
                "token_embd.weight".into(),
                vec![32, 16],
                GGMLType::F32,
                None,
            ),
            (
                "cpu_only.weight".into(),
                vec![32, 4],
                GGMLType::F32,
                Some(0),
            ),
            ("blk.0.weight".into(), vec![32, 4], GGMLType::Q8_0, Some(0)),
            ("output_norm.weight".into(), vec![32], GGMLType::F32, None),
            ("output.weight".into(), vec![32, 12], GGMLType::Q8_0, None),
        ]);
        let requirement = llm_requirements(&catalog, 1);
        let registry = registry(vec![
            descriptor("cpu0", BackendKind::Cpu, "cpu"),
            descriptor("metal0", BackendKind::Metal, "metal"),
        ]);
        let plan = compile(
            &catalog,
            &registry,
            &requirement,
            PlacementRule {
                component: ComponentId::Llm,
                mode: PlacementMode::Row,
                targets: targets(&[("cpu0", 1.0), ("metal0", 1.0)]),
            },
        )
        .unwrap();
        let tensor = catalog.find(ComponentId::Llm, "cpu_only.weight").unwrap();
        assert!(plan.devices[&id("cpu0")]
            .tensors
            .iter()
            .any(|item| item.tensor == tensor));
        assert!(!plan.devices[&id("metal0")]
            .tensors
            .iter()
            .any(|item| item.tensor == tensor));
    }

    #[test]
    fn special_llm_tensors_require_primary_capability_and_q8_logits() {
        let catalog = test_catalog(1, GGMLType::Q8_0);
        let requirement = llm_requirements(&catalog, 1);
        let mut cpu = descriptor("cpu0", BackendKind::Cpu, "cpu");
        cpu.capabilities.tensor_types = BTreeSet::from([GGMLType::Q8_0]);
        let limited = registry(vec![cpu]);
        assert!(matches!(
            compile(
                &catalog,
                &limited,
                &requirement,
                PlacementRule {
                    component: ComponentId::Llm,
                    mode: PlacementMode::Layer,
                    targets: targets(&[("cpu0", 1.0)]),
                },
            ),
            Err(PlanError::UnsupportedTensor { tensor, .. })
                if tensor == catalog.find(ComponentId::Llm, "token_embd.weight").unwrap()
        ));

        let wrong_logits = catalog_from_tensors(vec![
            (
                "token_embd.weight".into(),
                vec![32, 16],
                GGMLType::F32,
                None,
            ),
            ("blk.0.weight".into(), vec![32, 4], GGMLType::Q8_0, Some(0)),
            ("output_norm.weight".into(), vec![32], GGMLType::F32, None),
            ("output.weight".into(), vec![32, 12], GGMLType::F32, None),
        ]);
        let requirement = llm_requirements(&wrong_logits, 1);
        let registry = registry(vec![descriptor("cpu0", BackendKind::Cpu, "cpu")]);
        assert!(matches!(
            compile(
                &wrong_logits,
                &registry,
                &requirement,
                PlacementRule {
                    component: ComponentId::Llm,
                    mode: PlacementMode::Layer,
                    targets: targets(&[("cpu0", 1.0)]),
                },
            ),
            Err(PlanError::UnsupportedTensor { tensor, .. })
                if tensor == wrong_logits.find(ComponentId::Llm, "output.weight").unwrap()
        ));
    }

    #[test]
    fn compiler_rejects_unknown_non_cpu_row_and_duplicate_physical_devices() {
        let catalog = test_catalog(1, GGMLType::Q8_0);
        let requirement = llm_requirements(&catalog, 1);
        let only_cpu = registry(vec![descriptor("cpu0", BackendKind::Cpu, "cpu")]);
        let unknown = PlacementRule {
            component: ComponentId::Llm,
            mode: PlacementMode::Layer,
            targets: targets(&[("metal0", 1.0)]),
        };
        assert!(matches!(
            compile(&catalog, &only_cpu, &requirement, unknown),
            Err(PlanError::Backend(BackendError::DeviceUnavailable { .. }))
        ));

        let devices = registry(vec![
            descriptor("cpu0", BackendKind::Cpu, "shared"),
            descriptor("metal0", BackendKind::Metal, "shared"),
        ]);
        let row = PlacementRule {
            component: ComponentId::Llm,
            mode: PlacementMode::Row,
            targets: targets(&[("metal0", 1.0)]),
        };
        assert!(matches!(
            compile(&catalog, &devices, &requirement, row),
            Err(PlanError::UnsupportedRowPrimary { .. })
        ));
        let duplicate = PlacementRule {
            component: ComponentId::Llm,
            mode: PlacementMode::Layer,
            targets: targets(&[("cpu0", 1.0), ("metal0", 1.0)]),
        };
        assert!(matches!(
            compile(&catalog, &devices, &requirement, duplicate),
            Err(PlanError::DuplicatePhysicalSelection { .. })
        ));
    }

    #[test]
    fn compiler_rejects_gpu_quantization_and_capacity_before_runtime_open() {
        for ggml_type in [GGMLType::Q4K, GGMLType::Q5K, GGMLType::Q6K] {
            let catalog = test_catalog(2, ggml_type);
            let requirement = llm_requirements(&catalog, 2);
            let mut metal = descriptor("metal0", BackendKind::Metal, "metal");
            metal.capabilities.tensor_types = BTreeSet::from([GGMLType::Q8_0]);
            let devices = registry(vec![descriptor("cpu0", BackendKind::Cpu, "cpu"), metal]);
            let layer = PlacementRule {
                component: ComponentId::Llm,
                mode: PlacementMode::Layer,
                targets: targets(&[("cpu0", 1.0), ("metal0", 1.0)]),
            };
            assert!(matches!(
                compile(&catalog, &devices, &requirement, layer),
                Err(PlanError::UnsupportedTensor { .. })
            ));
        }

        let catalog = test_catalog(1, GGMLType::Q8_0);
        let requirement = llm_requirements(&catalog, 1);
        let mut cpu = descriptor("cpu0", BackendKind::Cpu, "cpu");
        cpu.max_allocation_bytes = 8;
        let opens = Arc::new(AtomicUsize::new(0));
        let mut provider_registry = DeviceRegistry::new();
        provider_registry
            .register_provider(Arc::new(TestProvider {
                descriptor: cpu,
                opens: opens.clone(),
            }))
            .unwrap();
        provider_registry
            .discover(&BTreeSet::from([BackendKind::Cpu]))
            .unwrap();
        let row = PlacementRule {
            component: ComponentId::Llm,
            mode: PlacementMode::Row,
            targets: targets(&[("cpu0", 1.0)]),
        };
        assert!(matches!(
            compile(&catalog, &provider_registry, &requirement, row),
            Err(PlanError::CapacityExceeded {
                largest_allocation_bytes,
                ..
            }) if largest_allocation_bytes > 8
        ));
        assert_eq!(opens.load(Ordering::SeqCst), 0);

        let vision = ComponentRequirements {
            component: ComponentId::Vision,
            workload: ComponentWorkload::VisionCpu { layer_count: 1 },
        };
        let metal = registry(vec![descriptor("metal0", BackendKind::Metal, "metal")]);
        let vision_rule = PlacementRule {
            component: ComponentId::Vision,
            mode: PlacementMode::Layer,
            targets: targets(&[("metal0", 1.0)]),
        };
        assert!(matches!(
            compile(&catalog, &metal, &vision, vision_rule),
            Err(PlanError::UnsupportedComponent { .. })
        ));

        let mut cpu = descriptor("cpu0", BackendKind::Cpu, "cpu");
        cpu.usable_bytes = 8;
        let too_small = registry(vec![cpu]);
        let row = PlacementRule {
            component: ComponentId::Llm,
            mode: PlacementMode::Row,
            targets: targets(&[("cpu0", 1.0)]),
        };
        assert!(matches!(
            compile(&catalog, &too_small, &requirement, row),
            Err(PlanError::CapacityExceeded {
                available_bytes: 8,
                ..
            })
        ));
    }
}
