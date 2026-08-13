use super::{DeviceDescriptor, LayerFamily, ProgramId, SlotId};
use crate::{ComponentId, DeviceId, PlacementMode, TensorId};
use std::collections::BTreeMap;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowShard {
    pub device: DeviceId,
    pub rows: Range<u32>,
    pub tensor_bytes: Range<u64>,
    pub program: ProgramId,
    pub input: SlotId,
    pub output: SlotId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramBinding {
    pub device: DeviceId,
    pub program: ProgramId,
    pub input: SlotId,
    pub output: SlotId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerSpan {
    pub device: DeviceId,
    pub layers: Range<u32>,
    pub program: ProgramId,
    pub input: SlotId,
    pub output: SlotId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationTransfer {
    pub after_span: Option<u32>,
    pub target: TransferTarget,
    pub from_device: DeviceId,
    pub from_slot: SlotId,
    pub to_device: DeviceId,
    pub to_slot: SlotId,
    pub f32_values_per_token: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferTarget {
    Span(u32),
    Finalization,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidentTensorPlan {
    pub tensor: TensorId,
    pub rows: Range<u32>,
    pub source_bytes: Range<u64>,
    pub arena_offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    Activation,
    Scratch,
    Result,
    KvState,
    ConvState,
    SsmState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotStorage {
    F32,
    F16,
    I8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotPlan {
    pub id: SlotId,
    pub kind: SlotKind,
    pub storage: SlotStorage,
    pub byte_len: u64,
    pub alignment: u64,
    pub arena_offset: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryPlan {
    pub resident_bytes: u64,
    pub state_bytes: u64,
    pub scratch_bytes: u64,
    pub staging_bytes: u64,
    pub required_bytes: u64,
    pub largest_allocation_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgramKind {
    Q8Rows {
        tensor: TensorId,
        rows: Range<u32>,
        batch_capacity: u32,
    },
    EmbeddingRows {
        tensor: TensorId,
        row_count: u32,
    },
    LayerSegment {
        layers: Range<u32>,
        families: Vec<LayerFamily>,
    },
    FinalNormQ8Logits {
        norm: TensorId,
        output: TensorId,
        epsilon_bits: u32,
        batch_capacity: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerOp {
    Copy {
        input: SlotId,
        output: SlotId,
        elements: u32,
    },
    Slice {
        input: SlotId,
        offset: u32,
        elements: u32,
        output: SlotId,
    },
    RmsNorm {
        input: SlotId,
        weight: TensorId,
        output: SlotId,
        epsilon_bits: u32,
    },
    Q8Matmul {
        input: SlotId,
        weight: TensorId,
        output: SlotId,
    },
    Rope {
        q: SlotId,
        k: SlotId,
        key_head_dim: u32,
        rope_dims: u32,
        freq_base_bits: u32,
    },
    MRope {
        q: SlotId,
        k: SlotId,
        sections: [i32; 4],
        key_head_dim: u32,
        rope_dims: u32,
        freq_base_bits: u32,
    },
    KvAppend {
        layer: u32,
        k: SlotId,
        v: SlotId,
        key_state: SlotId,
        value_state: SlotId,
    },
    Attention {
        layer: u32,
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
    Sigmoid {
        values: SlotId,
    },
    SigmoidMul {
        gate: SlotId,
        values: SlotId,
    },
    Silu {
        values: SlotId,
    },
    SiluMul {
        gate: SlotId,
        up: SlotId,
    },
    Mul {
        left: SlotId,
        right: SlotId,
        output: SlotId,
    },
    Scale {
        values: SlotId,
        scale_bits: u32,
    },
    Add {
        left: SlotId,
        right: SlotId,
        output: SlotId,
    },
    DepthwiseCausalConv {
        input: SlotId,
        weight: TensorId,
        state: SlotId,
        width: u32,
        output: SlotId,
    },
    L2Norm {
        values: SlotId,
        epsilon_bits: u32,
    },
    SoftplusAffine {
        values: SlotId,
        bias: TensorId,
        scale: TensorId,
    },
    SsmUpdate {
        q: SlotId,
        k: SlotId,
        v: SlotId,
        alpha: SlotId,
        beta: SlotId,
        state: SlotId,
        output: SlotId,
        state_size: u32,
        group_count: u32,
        dt_rank: u32,
        inner_size: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramPlan {
    pub id: ProgramId,
    pub kind: ProgramKind,
    pub input: SlotId,
    pub output: SlotId,
    pub layer_ops: Vec<LayerOp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevicePlan {
    pub descriptor: DeviceDescriptor,
    pub tensors: Vec<ResidentTensorPlan>,
    pub slots: Vec<SlotPlan>,
    pub programs: Vec<ProgramPlan>,
    pub memory: MemoryPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentPlan {
    pub component: ComponentId,
    pub mode: PlacementMode,
    pub primary: DeviceId,
    pub embedding: Option<ProgramBinding>,
    pub finalization: Option<ProgramBinding>,
    pub layer_spans: Vec<LayerSpan>,
    pub activation_transfers: Vec<ActivationTransfer>,
    pub row_shards: BTreeMap<TensorId, Vec<RowShard>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPlan {
    pub components: BTreeMap<ComponentId, ComponentPlan>,
    pub devices: BTreeMap<DeviceId, DevicePlan>,
}
