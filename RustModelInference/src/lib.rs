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

use std::collections::BTreeSet;
use std::sync::Arc;

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

/// Process-wide execution settings shared by the CLI, server, and benchmarks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionOptions {
    pub placements: Vec<String>,
    pub thread_count: usize,
    pub max_batch_tokens: u32,
    pub kv_cache: KvCacheType,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            placements: Vec::new(),
            thread_count: std::thread::available_parallelism().map_or(1, usize::from),
            max_batch_tokens: 512,
            kv_cache: KvCacheType::F16,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CompileModelError {
    #[error(transparent)]
    Placement(#[from] PlacementError),
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error("invalid model metadata: {0}")]
    Model(String),
}

pub enum QwenRunner {
    Qwen3(Qwen3Model),
    Qwen35(Qwen35Model),
}

impl QwenRunner {
    pub fn requirements(&self) -> ComponentRequirements {
        match self {
            Self::Qwen3(model) => model.requirements(),
            Self::Qwen35(model) => model.requirements(),
        }
    }
}

/// Build the immutable plan and open resident device sessions once.
pub fn compile_model(
    sources: Vec<(ComponentId, Arc<dyn TensorSource>)>,
    options: &ExecutionOptions,
) -> Result<(CompiledModel, QwenRunner), CompileModelError> {
    let catalog = Arc::new(TensorCatalog::from_sources(sources)?);
    let llm = catalog
        .source(ComponentId::Llm)
        .ok_or_else(|| CompileModelError::Model("missing LLM source".into()))?;
    let architecture = llm
        .metadata("general.architecture")
        .and_then(MetaValue::to_string_val)
        .unwrap_or_default();
    let runner = if architecture == "qwen35" {
        QwenRunner::Qwen35(Qwen35Model::from_catalog(&catalog).map_err(CompileModelError::Model)?)
    } else {
        QwenRunner::Qwen3(Qwen3Model::from_catalog(&catalog).map_err(CompileModelError::Model)?)
    };
    let mut requirements = vec![runner.requirements()];
    if catalog.entries().iter().any(|entry| entry.component == ComponentId::Vision) {
        let layers = catalog
            .entries()
            .iter()
            .filter_map(|entry| entry.layer)
            .max()
            .map_or(1, |layer| layer + 1);
        requirements.push(ComponentRequirements {
            component: ComponentId::Vision,
            workload: ComponentWorkload::VisionCpu { layer_count: layers },
        });
    }
    for requirement in &mut requirements {
        if let ComponentWorkload::Llm(llm) = &mut requirement.workload {
            llm.max_batch_tokens = options.max_batch_tokens;
            llm.kv_cache = options.kv_cache;
        }
    }

    let mut rules = parse_placements(&options.placements)?;
    for requirement in &requirements {
        rules.entry(requirement.component).or_insert_with(|| PlacementRule {
            component: requirement.component,
            mode: PlacementMode::Layer,
            targets: vec![NormalizedTarget {
                device: DeviceId::parse("cpu0").expect("static CPU device id is valid"),
                fraction: 1.0,
                ordinal: 0,
            }],
        });
    }
    let mut backends = BTreeSet::from([BackendKind::Cpu]);
    for rule in rules.values() {
        for target in &rule.targets {
            backends.insert(match target.device.as_str().trim_end_matches(char::is_numeric) {
                "cpu" => BackendKind::Cpu,
                "vulkan" => BackendKind::Vulkan,
                "metal" => BackendKind::Metal,
                "npu" => BackendKind::Npu,
                _ => return Err(CompileModelError::Model(format!("invalid device {}", target.device.as_str()))),
            });
        }
    }
    let mut registry = DeviceRegistry::new();
    compute::register_requested_providers(&mut registry, &backends, options.thread_count.max(1))?;
    registry.discover(&backends)?;
    let registry = Arc::new(registry);
    let plan = PlacementCompiler {
        catalog: &catalog,
        registry: &registry,
        requirements: &requirements,
    }
    .compile(&rules)?;
    Ok((CompiledModel::compile(catalog, plan, registry)?, runner))
}
