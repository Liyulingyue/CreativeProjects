pub mod asr;
pub mod clip_config;
#[cfg(feature = "gpu")]
pub mod compute;
pub mod ggufrs;
pub mod load_plan;
pub mod memory;
pub mod model;
pub mod ops;
#[cfg(feature = "parity-trace")]
#[doc(hidden)]
pub mod parity_trace;
pub mod prompt;
pub mod quant;
pub mod qwen3;
pub mod qwen35;
pub mod qwen3a;
pub mod scratchpad;
pub mod thread_pool;
pub mod tokenizer;
pub mod traits;
pub mod vision;

pub use asr::*;
pub use clip_config::{ClipVisionConfig, Qwen35Config};
pub use ggufrs::{
    export_ggufrs, open_model_source, ComponentInfo, ComponentRole, ExportOptions, GgufrsError,
    GgufrsFile, LoadedComponent, SegmentKind, GGUFRS_SEGMENT_ALIGNMENT, GGUFRS_VERSION,
};
pub use load_plan::{
    build_load_plan, load_logical_cpu, LoadPlan, LogicalCpuDeviceLoad, LogicalCpuLoad,
    LogicalCpuPlacement, LogicalDevice, Placement, PlacementPolicy, PlacementSlice,
};
pub use memory::{BlockAllocator, KVCacheView, MemoryArena, PagedKVBlock};
pub use model::{
    model_config_from_source, GGMLType, GGUFLoader, MetaValue, MetaValueType, ModelGraph,
    QuantizedLinear, TensorInfo, TensorSource,
};
pub use ops::*;
pub use prompt::{
    append_qwen_assistant_prefix, append_qwen_message_tokens, build_qwen_chat_prompt, QwenMessage,
};
pub use quant::dequant_weight_q4k;
pub use qwen3::*;
pub use qwen35::{build_qwen35_positions, Qwen35Model};
pub use scratchpad::{ExecutionScratchpad, KvCache, KvCacheF16, KvCacheF32};
pub use tokenizer::{BPETokenizer, EncodeOptions, StreamingDecoder};
pub use traits::{ExecContext, Layer, ModelConfig};
pub use vision::{qwen_smart_resize, VisionEncoder, VisionGrid, VisionScratchpad};
