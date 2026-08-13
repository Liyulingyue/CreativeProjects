pub mod clip_config;
pub mod compute;
pub mod ggufrs;
pub mod load_plan;
pub mod memory;
pub mod model;
pub mod ops;
#[cfg(feature = "parity-trace")]
#[doc(hidden)]
pub mod parity_trace;
pub mod placement;
pub mod prompt;
pub mod quant;
pub mod qwen3;
pub mod qwen35;
pub mod scratchpad;
pub mod tensor_catalog;
pub mod thread_pool;
pub mod tokenizer;
pub mod traits;
pub mod vision;

pub use clip_config::{ClipVisionConfig, Qwen35Config};
pub use compute::{
    ActivationTransfer, BackendError, BackendKind, CompiledModel, ComponentPlan,
    DeviceCapabilities, DeviceDescriptor, DeviceDiscovery, DevicePlan, DeviceProvider,
    DeviceRegistry, DeviceSession, ExecutionPlan, ExecutionRun, FenceId, LayerFamily, LayerOp,
    LayerSpan, LifecycleProbe, MemoryPlan, ProgramBinding, ProgramId, ProgramKind, ProgramPlan,
    ResidentTensorPlan, RowShard, RunParams, SessionStats, SlotId, SlotKind, SlotPlan, SlotStorage,
    TransferTarget,
};
pub use ggufrs::{
    export_ggufrs, open_model_source, ComponentInfo, ComponentRole, ExportOptions, GgufrsError,
    GgufrsFile, LoadedComponent, SegmentKind, GGUFRS_SEGMENT_ALIGNMENT, GGUFRS_VERSION,
};
pub use load_plan::{
    weighted_ranges, ComponentRequirements, ComponentWorkload, KvCacheType, LlmLayerSpec,
    LlmRequirements, PlacementCompiler, PlanError, Qwen35DenseLayerSpec, Qwen35RecurrentLayerSpec,
    Qwen3LayerSpec,
};
pub use memory::{BlockAllocator, KVCacheView, MemoryArena, PagedKVBlock};
pub use model::{
    model_config_from_source, GGMLType, GGUFLoader, MetaValue, MetaValueType, ModelGraph,
    QuantizedLinear, TensorInfo, TensorSource,
};
pub use ops::*;
pub use placement::{
    parse_placement, parse_placements, parse_requested_placements, ComponentId, DeviceId,
    NormalizedTarget, PlacementError, PlacementMode, PlacementRule,
};
pub use prompt::{
    append_qwen_assistant_prefix, append_qwen_message_tokens, build_qwen_chat_prompt, QwenMessage,
};
pub use quant::dequant_weight_q4k;
pub use qwen3::{
    Qwen3AuxiliaryWeights, Qwen3Config, Qwen3EmbeddingConfig, Qwen3EmbeddingPooling,
    Qwen3LayerTensorIds, Qwen3Model, Qwen3TensorIds,
};
pub use qwen35::{build_qwen35_positions, Qwen35Model};
pub use scratchpad::{ExecutionScratchpad, KvCache, KvCacheF16, KvCacheF32};
pub use tensor_catalog::{
    CatalogError, SourceFormat, SourceTensorRecord, TensorCatalog, TensorCatalogEntry, TensorId,
};
pub use tokenizer::{BPETokenizer, EncodeOptions, StreamingDecoder};
pub use traits::{ExecContext, Layer, ModelConfig};
pub use vision::{qwen_smart_resize, VisionEncoder, VisionGrid, VisionScratchpad};
