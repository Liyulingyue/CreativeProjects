# Multi-Device Vulkan and Metal Execution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the copy-per-matmul prototype with one command-line-driven execution system that supports weighted Row and Layer placement across arbitrary logical CPU, Vulkan, and Metal devices for every Q8_0 matrix and complete Qwen3/Qwen3.5 layers.

**Architecture:** Extend the existing `TensorSource` and `load_plan` paths instead of building a second loader or scheduler: GGUF and GGUFRS become one `TensorCatalog`, placement rules compile into an immutable `ExecutionPlan`, and `CompiledModel` owns long-lived backend sessions. Row programs submit all shards before waiting; Layer programs enqueue complete contiguous layer spans and transfer only hidden activations at backend boundaries.

**Tech Stack:** Rust 1.97, `memmap2`, existing CPU/NEON/AVX2 Q8_0 operations, `ash 0.38`, target-specific `metal 0.33.0`, runtime-compiled MSL, checked-in Vulkan 1.1 SPIR-V, Cargo unit/integration tests.

## Global Constraints

- Implement from `MultiDev@b454d45c4eb6f4b3134022185f711e780cb99b35`; preserve unrelated and untracked files, especially `.codegraph/`.
- Run every command from the `RustModelInference/` directory; all file paths in this plan are relative to that directory.
- Reuse `TensorSource`, `GGUFLoader`, `LoadedComponent`, Q8_0 CPU kernels, `ComputePool`, and the validation logic in `load_plan`; do not add a graph compiler, async runtime, allocator crate, command-line crate, or shader build dependency.
- Accept repeatable `--placement COMPONENT:MODE=DEVICE@WEIGHT[,DEVICE@WEIGHT...]` on both binaries. Components are `llm` and `vision`; modes are `layer` and `row`.
- Parse weights as standard-library `f64`, reject malformed, negative, non-finite, duplicate, empty, or all-zero sets, ignore individual zero-weight targets after duplicate detection, normalize automatically, and preserve declaration order for deterministic ties.
- No placement means every loaded component uses `cpu0`; enabling Cargo GPU features must not enumerate, open, allocate on, or upload to a GPU without an explicit GPU placement.
- Register arbitrary logical `cpuN`, `vulkanN`, and `metalN` IDs. Keep `BackendKind::Npu` and the provider/session contracts, but add no NPU provider, dummy runtime, driver dependency, or fallback.
- Layer placement assigns contiguous complete layers by largest remainder. Row placement assigns contiguous complete output rows of every Q8_0 matrix by the same rule. Every positive target must receive at least one layer/row.
- Validate capabilities, tensor formats, exact checked memory totals, backend alignment, maximum allocation sizes, and aggregate use by physical adapter before opening any session or allocating backend memory.
- Q8_0 uses GGML's 34-byte block: two little-endian F16 scale bytes followed by 32 signed bytes; `row_stride = (n_in / 32) * 34`, and no block or row may be split. A Q8 matrix program accepts F32 activations, quantizes each 32-value block into preallocated signed bytes plus an F32 scale, then accumulates `weight_scale * activation_scale * dot(i8, i8)` exactly like the existing CPU reference.
- The hot path may create backend command objects required by the native API, but it must allocate/upload each resident weight exactly once and must not allocate/free backend buffers or upload weights after compilation.
- Row coordination is `write all inputs -> submit all shards -> wait all fences -> read/scatter all shards`; never submit and wait one shard at a time.
- Consecutive Layer-mode layers on one device stay in one queued segment with no host wait. At a device transition, transfer only hidden F32 activation; KV, convolution, and recurrent state remain on the layer owner.
- A stateful backend error poisons that inference run. Do not retry a partially updated operation on CPU and do not reuse its KV/recurrent state.
- `vision` participates in parsing, cataloguing, placement, and capacity/capability validation. CPU placement remains valid; Vulkan, Metal, and NPU placement fail before session allocation because vision kernels are outside this delivery.
- Vulkan sources and checked-in SPIR-V target Vulkan 1.1. Runtime builds must not invoke `glslc`; regeneration uses `/opt/homebrew/bin/glslc` explicitly.
- Metal code is compiled only under `cfg(all(target_os = "macos", feature = "metal"))`; MSL is embedded with `include_str!` and compiled with `Device::new_library_with_source`, not `xcrun metal`.
- Keep tests at the approved tolerances: Q8 kernels use `abs(actual - expected) <= 1e-4 + 1e-4 * abs(expected)` and complete layers use `1e-3` absolute plus relative tolerance.
- Performance results are reports, not gates: use identical model, prompt/prefill, generated tokens, KV format, sampling, and thread count; take at least five samples and report separate prefill/decode medians with transfer/submission/wait counters.
- Do not push this branch without explicit user authorization.

## File Map

- Create `src/placement.rs`: placement grammar, component/mode/device IDs, normalization, duplicate detection.
- Create `src/tensor_catalog.rs`: one source-owning catalog for raw GGUF and GGUFRS tensors.
- Rewrite `src/load_plan.rs`: weighted ranges, immutable component/device/memory/program plans, capability and capacity compiler.
- Rewrite `src/compute/device.rs`: provider, descriptor, session, handle, error, and statistics contracts; remove the old scheduler types.
- Create `src/compute/session.rs`: compiled-model lifecycle and Row/Layer coordinator.
- Create `src/compute/program.rs`: fixed Q8 Row and Qwen layer program records; this is a typed schedule, not a general graph IR.
- Rewrite `src/compute/cpu.rs`: one logical CPU session per NUMA node and the compiled CPU reference path.
- Replace `src/compute/gpu.rs` with `src/compute/vulkan.rs`: Vulkan discovery, resident arenas, pipelines, queueing, synchronization, and readback.
- Create `src/compute/metal.rs`: Metal discovery, resident buffers, runtime MSL library, queueing, synchronization, and readback.
- Replace `src/compute/kernels/matmul_q8_0.comp` and `.spv` with focused Vulkan shaders under `src/compute/vulkan/shaders/`.
- Create `src/compute/metal/kernels.metal`: Metal equivalents of the fixed kernel set.
- Create `src/qwen3.rs`: shared Qwen3 model/runner used by CLI and server, eliminating their duplicated Q8 paths.
- Modify `src/qwen35.rs`: attach catalog tensor IDs and route Row/Layer execution through `ExecutionRun`.
- Modify `src/model.rs` and `src/ggufrs.rs`: expose owned source records without changing mmap/segment lifetimes.
- Modify `src/main.rs` and `src/bin/server.rs`: repeated placement CLI, catalog/registry/plan compilation, shared runners, explicit errors.
- Modify `src/lib.rs`, `Cargo.toml`, `README.md`, `ARCHITECTURE.md`, `OPTIMIZATION.md`, and `src/bin/micro_bench.rs`: exports, feature split, user docs, and instrumentation-aware benchmarks.
- Create `tests/support/mod.rs`, `tests/execution_lifecycle.rs`, `tests/gpu_backends.rs`, `tests/qwen_gpu_layers.rs`, and `tests/placement_e2e.rs`: deterministic fixtures, lifecycle invariants, hardware correctness, and GGUF/GGUFRS equivalence.

---

### Task 1: Parse and Normalize Repeatable Placement Rules

**Files:**
- Create: `src/placement.rs`
- Modify: `src/lib.rs`
- Test: `src/placement.rs` (`#[cfg(test)]` module)

**Interfaces:**
- Consumes: raw values collected from repeated CLI `--placement` flags.
- Produces:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComponentId { Llm, Vision }

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlacementMode { Layer, Row }

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn parse(value: &str) -> Result<Self, PlacementError>;
    pub fn as_str(&self) -> &str;
}

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedTarget {
    pub device: DeviceId,
    pub fraction: f64,
    pub ordinal: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlacementRule {
    pub component: ComponentId,
    pub mode: PlacementMode,
    pub targets: Vec<NormalizedTarget>,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum PlacementError {
    #[error("invalid placement syntax: {0}")]
    Syntax(String),
    #[error("unknown component: {0}")]
    UnknownComponent(String),
    #[error("unknown placement mode: {0}")]
    UnknownMode(String),
    #[error("invalid device id: {0}")]
    InvalidDevice(String),
    #[error("invalid placement weight: {0}")]
    InvalidWeight(String),
    #[error("duplicate target: {0:?}")]
    DuplicateDevice(DeviceId),
    #[error("duplicate component rule: {0:?}")]
    DuplicateComponent(ComponentId),
    #[error("all weights are zero for {0:?}")]
    AllZero(ComponentId),
}

pub fn parse_placement(value: &str) -> Result<PlacementRule, PlacementError>;
pub fn parse_placements(values: &[String])
    -> Result<std::collections::BTreeMap<ComponentId, PlacementRule>, PlacementError>;
```

- `DeviceId::parse` accepts only the ASCII forms `cpuN`, `vulkanN`, `metalN`, and `npuN`, where `N` is a non-negative decimal `u32`; device availability is checked by the compiler, not the parser.

- [ ] **Step 1: Add tests that pin grammar, normalization, and deterministic order**

```rust
#[test]
fn normalizes_decimal_weights_and_ignores_zero_after_duplicate_check() {
    let rule = parse_placement("llm:row=cpu0@1.5,metal0@0,vulkan0@4.5").unwrap();
    assert_eq!(rule.component, ComponentId::Llm);
    assert_eq!(rule.mode, PlacementMode::Row);
    assert_eq!(rule.targets.len(), 2);
    assert_eq!(rule.targets[0].device.as_str(), "cpu0");
    assert!((rule.targets[0].fraction - 0.25).abs() < f64::EPSILON);
    assert_eq!(rule.targets[1].device.as_str(), "vulkan0");
    assert!((rule.targets[1].fraction - 0.75).abs() < f64::EPSILON);
    assert_eq!((rule.targets[0].ordinal, rule.targets[1].ordinal), (0, 2));
}

#[test]
fn rejects_invalid_weights_and_duplicate_rules() {
    for value in [
        "llm:row=cpu0@-1",
        "llm:row=cpu0@NaN",
        "llm:row=cpu0@inf",
        "llm:row=cpu0@wat",
        "llm:row=cpu0@0,metal0@0",
        "llm:row=cpu0@1,cpu0@0",
        "audio:row=cpu0@1",
        "llm:tensor=cpu0@1",
        "llm:row=",
    ] {
        assert!(parse_placement(value).is_err(), "accepted {value}");
    }
    let duplicate_components = vec![
        "llm:row=cpu0@1".to_string(),
        "llm:layer=metal0@1".to_string(),
    ];
    assert!(parse_placements(&duplicate_components).is_err());
}
```

- [ ] **Step 2: Run the focused tests and confirm the new module is missing**

Run: `cargo test --lib placement::tests -- --nocapture`

Expected: FAIL because `src/placement.rs` and its exported symbols do not exist.

- [ ] **Step 3: Implement the parser with standard-library splitting and one normalization pass**

Use this control flow in `parse_placement`:

```rust
let (left, targets) = value
    .split_once('=')
    .ok_or_else(|| PlacementError::Syntax(value.to_owned()))?;
let (component, mode) = left
    .split_once(':')
    .ok_or_else(|| PlacementError::Syntax(value.to_owned()))?;
let component = ComponentId::parse(component)?;
let mode = PlacementMode::parse(mode)?;
let mut seen = std::collections::BTreeSet::new();
let mut parsed = Vec::new();
for (ordinal, target) in targets.split(',').enumerate() {
    let (device, weight) = target
        .split_once('@')
        .ok_or_else(|| PlacementError::Syntax(value.to_owned()))?;
    let device = DeviceId::parse(device)?;
    if !seen.insert(device.clone()) {
        return Err(PlacementError::DuplicateDevice(device));
    }
    let weight = weight
        .parse::<f64>()
        .map_err(|_| PlacementError::InvalidWeight(target.to_owned()))?;
    if !weight.is_finite() || weight < 0.0 {
        return Err(PlacementError::InvalidWeight(target.to_owned()));
    }
    parsed.push((device, weight, ordinal));
}
let sum = parsed.iter().map(|(_, weight, _)| *weight).sum::<f64>();
if !sum.is_finite() || sum <= 0.0 {
    return Err(PlacementError::AllZero(component));
}
let targets = parsed
    .into_iter()
    .filter(|(_, weight, _)| *weight > 0.0)
    .map(|(device, weight, ordinal)| NormalizedTarget {
        device,
        fraction: weight / sum,
        ordinal,
    })
    .collect();
Ok(PlacementRule { component, mode, targets })
```

Use `thiserror::Error` for stable variants `Syntax`, `UnknownComponent`, `UnknownMode`, `InvalidDevice`, `InvalidWeight`, `DuplicateDevice`, `DuplicateComponent`, and `AllZero`. Reject extra `=`, `:`, `@`, empty tokens, and numeric suffix overflow explicitly.

- [ ] **Step 4: Run placement tests and the default library suite**

Run: `cargo test --lib placement::tests -- --nocapture && cargo test --lib`

Expected: PASS, including normalized `1.5:4.5`, invalid/non-finite input, zero handling, and duplicate rules.

- [ ] **Step 5: Commit the parser only**

```bash
git add src/placement.rs src/lib.rs
git commit -m "feat: parse weighted placement rules"
```

### Task 2: Build One Source-Owned Tensor Catalog for GGUF and GGUFRS

**Files:**
- Create: `src/tensor_catalog.rs`
- Modify: `src/model.rs`
- Modify: `src/ggufrs.rs`
- Modify: `src/lib.rs`
- Test: `src/tensor_catalog.rs` (`#[cfg(test)]` module)

**Interfaces:**
- Consumes: `Arc<dyn TensorSource>` returned by `open_model_source`, `ComponentId`, existing `TensorInfo`, and GGUFRS segment metadata.
- Produces:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat { Gguf, Ggufrs }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTensorRecord {
    pub info: TensorInfo,
    pub segment_id: u32,
    pub segment_byte_range: std::ops::Range<u64>,
    pub layer: Option<u32>,
}

pub trait TensorSource: Send + Sync {
    fn metadata(&self, key: &str) -> Option<&MetaValue>;
    fn tensor_info(&self, name: &str) -> Option<&TensorInfo>;
    fn tensor_slice(&self, name: &str) -> Option<&[u8]>;
    fn source_format(&self) -> SourceFormat;
    fn tensor_records(&self) -> Vec<SourceTensorRecord>;
    fn model_config(&self) -> Result<ModelConfig, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TensorId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TensorCatalogEntry {
    pub id: TensorId,
    pub component: ComponentId,
    pub name: String,
    pub shape: Vec<u64>,
    pub ggml_type: GGMLType,
    pub byte_len: u64,
    pub segment_id: u32,
    pub segment_byte_range: std::ops::Range<u64>,
    pub layer: Option<u32>,
    pub row_count: u64,
    pub row_bytes: u64,
}

pub struct TensorCatalog {
    sources: std::collections::BTreeMap<ComponentId, Arc<dyn TensorSource>>,
    entries: Vec<TensorCatalogEntry>,
    by_name: std::collections::BTreeMap<(ComponentId, String), TensorId>,
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("duplicate component source: {0:?}")]
    DuplicateComponent(ComponentId),
    #[error("duplicate tensor {name} in {component:?}")]
    DuplicateTensor { component: ComponentId, name: String },
    #[error("invalid tensor shape or byte layout: {0}")]
    InvalidShape(String),
    #[error("tensor data is missing: {0}")]
    MissingTensor(String),
    #[error("invalid tensor id: {0:?}")]
    InvalidTensorId(TensorId),
}

impl TensorCatalog {
    pub fn from_sources(
        sources: Vec<(ComponentId, Arc<dyn TensorSource>)>,
    ) -> Result<Self, CatalogError>;
    pub fn entries(&self) -> &[TensorCatalogEntry];
    pub fn entry(&self, id: TensorId) -> Option<&TensorCatalogEntry>;
    pub fn find(&self, component: ComponentId, name: &str) -> Option<TensorId>;
    pub fn bytes(&self, id: TensorId) -> Result<&[u8], CatalogError>;
}
```

- Raw GGUF records use virtual `segment_id = 0`, byte ranges relative to the GGUF data section, and `layer` parsed from `blk.N.`. GGUFRS records preserve validated component segment IDs and segment-relative byte ranges. `TensorCatalog` owns the `Arc` sources so all returned byte slices remain valid.

- [ ] **Step 1: Add an equivalence test using the existing in-crate GGUF/GGUFRS fixture writers**

```rust
#[test]
fn raw_gguf_and_ggufrs_have_the_same_logical_tensor_contract() {
    let fixture = crate::ggufrs::test_support::test_q8_row_package(4, 32);
    let raw: Arc<dyn TensorSource> = Arc::new(GGUFLoader::from_file(&fixture.inputs.llm).unwrap());
    let packaged: Arc<dyn TensorSource> = Arc::from(
        fixture.package.load_component(ComponentRole::Llm)
            .map(|source| Box::new(source) as Box<dyn TensorSource>)
            .unwrap(),
    );
    let raw = TensorCatalog::from_sources(vec![(ComponentId::Llm, raw)]).unwrap();
    let packaged = TensorCatalog::from_sources(vec![(ComponentId::Llm, packaged)]).unwrap();
    let project = |catalog: &TensorCatalog| {
        catalog.entries().iter().map(|entry| (
            entry.component,
            entry.name.clone(),
            entry.shape.clone(),
            entry.ggml_type,
            entry.byte_len,
            entry.layer,
            entry.row_count,
            entry.row_bytes,
        )).collect::<Vec<_>>()
    };
    assert_eq!(project(&raw), project(&packaged));
}

#[test]
fn catalog_keeps_source_alive_and_reports_q8_rows() {
    let catalog = test_q8_catalog(4, 64);
    let id = catalog.find(ComponentId::Llm, "blk.0.weight").unwrap();
    let entry = catalog.entry(id).unwrap();
    assert_eq!((entry.row_count, entry.row_bytes), (4, 68));
    assert_eq!(catalog.bytes(id).unwrap().len(), 272);
}
```

- [ ] **Step 2: Run the focused catalog tests and confirm the new source methods are absent**

Run: `cargo test --lib tensor_catalog::tests -- --nocapture`

Expected: FAIL because `SourceFormat`, `SourceTensorRecord`, and `TensorCatalog` are undefined.

- [ ] **Step 3: Extend both source implementations and build the catalog without copying tensor bytes**

For every source record, calculate the logical row layout with checked arithmetic:

```rust
let row_elements = *record.info.dims.first().ok_or_else(|| {
    CatalogError::InvalidShape(record.info.name.clone())
})?;
let row_count = record.info.dims[1..].iter().try_fold(1_u64, |n, dim| {
    n.checked_mul(*dim)
        .ok_or_else(|| CatalogError::InvalidShape(record.info.name.clone()))
})?;
let (block_elements, block_bytes) = record.info.ggml_type.type_traits();
let row_bytes = row_elements
    .checked_div(block_elements as u64)
    .and_then(|blocks| blocks.checked_mul(block_bytes as u64))
    .ok_or_else(|| CatalogError::InvalidShape(record.info.name.clone()))?;
let byte_len = record.info.checked_nbytes().ok_or_else(|| {
    CatalogError::InvalidShape(record.info.name.clone())
})?;
if row_count.checked_mul(row_bytes) != Some(byte_len) {
    return Err(CatalogError::InvalidShape(record.info.name.clone()));
}
```

Assign `TensorId`s in component order (`llm`, then `vision`) and source record order. Reject duplicate component sources and duplicate `(component, tensor name)` keys. Update every in-crate `TensorSource` test double with `source_format` and `tensor_records`; do not add a default that silently returns an empty catalog.

- [ ] **Step 4: Run catalog, model, and package tests**

Run: `cargo test --lib tensor_catalog::tests && cargo test --lib model::tests && cargo test --lib ggufrs::tests`

Expected: PASS with identical logical tensor projections and source bytes still readable after the builder returns.

- [ ] **Step 5: Commit the unified catalog**

```bash
git add src/tensor_catalog.rs src/model.rs src/ggufrs.rs src/lib.rs
git commit -m "feat: catalog gguf and ggufrs tensors"
```

### Task 3: Define Device Descriptors and Session Contracts

**Files:**
- Rewrite: `src/compute/device.rs`
- Modify: `src/compute/mod.rs`
- Modify: `src/lib.rs`
- Test: `src/compute/device.rs` (`#[cfg(test)]` module)

**Interfaces:**
- Consumes: `ComponentId`, `PlacementMode`, `DeviceId`, and `GGMLType`. The provider `open` method is added in Task 4 after `DevicePlan` exists, avoiding a forward dependency.
- Produces the only backend abstraction used by CPU, Vulkan, Metal, and a future NPU:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BackendKind { Cpu, Vulkan, Metal, Npu }

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LayerFamily { Qwen3, Qwen35Dense, Qwen35Recurrent }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCapabilities {
    pub components: std::collections::BTreeSet<ComponentId>,
    pub modes: std::collections::BTreeSet<PlacementMode>,
    pub layer_families: std::collections::BTreeSet<LayerFamily>,
    pub tensor_types: std::collections::BTreeSet<GGMLType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceDescriptor {
    pub id: DeviceId,
    pub backend: BackendKind,
    pub physical_key: String,
    pub name: String,
    pub usable_bytes: u64,
    pub max_allocation_bytes: u64,
    pub buffer_alignment: u64,
    pub unified_memory: bool,
    pub capabilities: DeviceCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SlotId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProgramId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FenceId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionStats {
    pub resident_bytes: u64,
    pub resident_allocations: u64,
    pub resident_frees: u64,
    pub weight_uploads: u64,
    pub weight_upload_bytes: u64,
    pub activation_h2d_bytes: u64,
    pub activation_d2h_bytes: u64,
    pub submissions: u64,
    pub host_waits: u64,
}

#[derive(Default)]
struct LifecycleCounters {
    resident_bytes: std::sync::atomic::AtomicU64,
    resident_allocations: std::sync::atomic::AtomicU64,
    resident_frees: std::sync::atomic::AtomicU64,
    weight_uploads: std::sync::atomic::AtomicU64,
    weight_upload_bytes: std::sync::atomic::AtomicU64,
    activation_h2d_bytes: std::sync::atomic::AtomicU64,
    activation_d2h_bytes: std::sync::atomic::AtomicU64,
    submissions: std::sync::atomic::AtomicU64,
    host_waits: std::sync::atomic::AtomicU64,
}

#[derive(Clone, Default)]
pub struct LifecycleProbe(Arc<LifecycleCounters>);

impl LifecycleProbe {
    pub fn snapshot(&self) -> SessionStats;
}

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("backend is unavailable: {backend:?}")]
    BackendUnavailable { backend: BackendKind },
    #[error("device is unavailable: {device:?}")]
    DeviceUnavailable { device: DeviceId },
    #[error("duplicate backend registration: {backend:?}")]
    DuplicateBackend { backend: BackendKind },
    #[error("duplicate device id: {device:?}")]
    DuplicateDeviceId { device: DeviceId },
    #[error("descriptor {id:?} reports {actual:?}, expected {expected:?}")]
    DescriptorBackendMismatch { id: DeviceId, expected: BackendKind, actual: BackendKind },
    #[error("unsupported {operation} for {device:?}")]
    Unsupported { device: DeviceId, operation: &'static str },
    #[error("allocation failed on {device:?}: {message}")]
    Allocation { device: DeviceId, message: String },
    #[error("weight upload failed on {device:?}: {message}")]
    Upload { device: DeviceId, message: String },
    #[error("pipeline creation failed on {device:?}: {message}")]
    Pipeline { device: DeviceId, message: String },
    #[error("submission failed on {device:?}: {message}")]
    Submission { device: DeviceId, message: String },
    #[error("program is missing for tensor {tensor:?}")]
    ProgramMissing { tensor: TensorId },
    #[error("invalid compiled handle")]
    InvalidHandle,
    #[error("inference state is poisoned")]
    PoisonedRun,
}

pub struct RunParams<'a> {
    pub token_count: u32,
    pub position_start: u32,
    pub mrope_positions: &'a [[u32; 4]],
    pub token_ids: &'a [u32],
}

pub trait DeviceSession: Send {
    fn descriptor(&self) -> &DeviceDescriptor;
    fn write_f32(&mut self, slot: SlotId, values: &[f32]) -> Result<(), BackendError>;
    fn submit(&mut self, program: ProgramId, params: &RunParams<'_>)
        -> Result<FenceId, BackendError>;
    fn wait(&mut self, fence: FenceId) -> Result<(), BackendError>;
    fn read_f32(&mut self, slot: SlotId, values: &mut [f32]) -> Result<(), BackendError>;
    fn reset_state(&mut self) -> Result<(), BackendError>;
    fn stats(&self) -> SessionStats;
    fn lifecycle_probe(&self) -> LifecycleProbe;
}

pub trait DeviceDiscovery: Send + Sync {
    fn backend(&self) -> BackendKind;
    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, BackendError>;
}

pub struct DeviceRegistry {
    discoveries: std::collections::BTreeMap<BackendKind, Arc<dyn DeviceDiscovery>>,
    descriptors: std::collections::BTreeMap<DeviceId, DeviceDescriptor>,
}

impl DeviceRegistry {
    pub fn new() -> Self;
    pub fn register_discovery(
        &mut self,
        discovery: Arc<dyn DeviceDiscovery>,
    ) -> Result<(), BackendError>;
    pub fn discover(
        &mut self,
        requested: &std::collections::BTreeSet<BackendKind>,
    ) -> Result<(), BackendError>;
    pub fn get(&self, id: &DeviceId) -> Option<&DeviceDescriptor>;
    pub fn require(&self, id: &DeviceId) -> Result<&DeviceDescriptor, BackendError>;
}
```

`submit` is non-blocking by contract and never calls `wait`. `SlotId` is the only activation/result handle; backend-native buffer handles remain private to each session. Test-only `TestDiscovery` implements `DeviceDiscovery` from a fixed descriptor list and an atomic enumeration counter.

- [ ] **Step 1: Add registry tests for arbitrary IDs, lazy discovery, and duplicate rejection**

```rust
#[test]
fn discovers_only_requested_backends_and_rejects_duplicate_ids() {
    let cpu_calls = Arc::new(AtomicUsize::new(0));
    let vulkan_calls = Arc::new(AtomicUsize::new(0));
    let mut registry = DeviceRegistry::new();
    registry.register_discovery(Arc::new(TestDiscovery::new(
        BackendKind::Cpu, "cpu0", cpu_calls.clone(),
    ))).unwrap();
    registry.register_discovery(Arc::new(TestDiscovery::new(
        BackendKind::Vulkan, "vulkan0", vulkan_calls.clone(),
    ))).unwrap();
    registry.discover(&BTreeSet::from([BackendKind::Cpu])).unwrap();
    assert_eq!(cpu_calls.load(Ordering::SeqCst), 1);
    assert_eq!(vulkan_calls.load(Ordering::SeqCst), 0);
    assert!(registry.get(&DeviceId::parse("cpu0").unwrap()).is_some());
}

#[test]
fn npu_id_has_a_contract_but_no_implicit_provider() {
    let registry = DeviceRegistry::new();
    let error = registry.require(&DeviceId::parse("npu0").unwrap()).unwrap_err();
    assert!(matches!(error, BackendError::DeviceUnavailable { .. }));
}
```

- [ ] **Step 2: Run the device tests and verify the old fixed types cannot satisfy them**

Run: `cargo test --lib compute::device::tests -- --nocapture`

Expected: FAIL because `BackendKind`, `DeviceRegistry`, `DeviceDiscovery`, and `DeviceSession` do not exist.

- [ ] **Step 3: Implement registry discovery and delete scheduler ownership from this module**

Implement `DeviceRegistry` initially with only the discovery and descriptor `BTreeMap`s; Task 4 adds the provider map and `register_provider` once `DeviceProvider` can refer to `DevicePlan`. The discover loop is exact and deterministic:

```rust
pub fn discover(
    &mut self,
    requested: &BTreeSet<BackendKind>,
) -> Result<(), BackendError> {
    for backend in requested {
        let discovery = self.discoveries.get(backend).ok_or_else(|| {
            BackendError::BackendUnavailable { backend: *backend }
        })?;
        for descriptor in discovery.enumerate()? {
            if descriptor.backend != *backend {
                return Err(BackendError::DescriptorBackendMismatch {
                    id: descriptor.id.clone(),
                    expected: *backend,
                    actual: descriptor.backend,
                });
            }
            let id = descriptor.id.clone();
            if self.descriptors.insert(id.clone(), descriptor).is_some() {
                return Err(BackendError::DuplicateDeviceId {
                    device: id,
                });
            }
        }
    }
    Ok(())
}
```

Remove `ComputeDevice`, `Scheduler`, `DeviceRatio`, `DeviceConfig`, and copy-owning `WorkSpec` from `device.rs`. Do not add an NPU implementation; `BackendKind::Npu` plus `DeviceDiscovery`/`DeviceSession` is the requested interface.

- [ ] **Step 4: Run registry tests and compile the library**

Run: `cargo test --lib compute::device::tests && cargo check --lib`

Expected: PASS. Add a private CPU compatibility function in `compute/mod.rs` for the one embedding caller while the Q8 call sites migrate in Task 9; it delegates to the existing CPU Q8 operation and does not retain `Scheduler`, `DeviceRatio`, or `WorkSpec`.

- [ ] **Step 5: Commit the contracts**

```bash
git add src/compute/device.rs src/compute/mod.rs src/lib.rs src/main.rs src/bin/server.rs
git commit -m "refactor: define multi-device session contracts"
```

### Task 4: Compile Weighted Layer and Row Execution Plans

**Files:**
- Rewrite: `src/load_plan.rs`
- Create: `src/compute/program.rs`
- Modify: `src/compute/mod.rs`
- Modify: `src/lib.rs`
- Test: `src/load_plan.rs` (`#[cfg(test)]` module)

**Interfaces:**
- Consumes: `TensorCatalog`, normalized `PlacementRule`s, discovered `DeviceRegistry`, and per-component runtime requirements.
- Produces:

```rust
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
pub enum KvCacheType { F16, F32 }

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
    pub fn layer(&self) -> u32;
    pub fn family(&self) -> LayerFamily;
}

// These are fixed Qwen descriptions, not a public graph IR. Tensor shapes remain
// authoritative in TensorCatalog; these values are the metadata needed by LayerOp.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowShard {
    pub device: DeviceId,
    pub rows: std::ops::Range<u32>,
    pub tensor_bytes: std::ops::Range<u64>,
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
    pub layers: std::ops::Range<u32>,
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
pub enum TransferTarget { Span(u32), Finalization }

`after_span: None` identifies the embedding-to-first-span boundary; `Some(index)` is the zero-based completed `LayerSpan` index. `TransferTarget::Span(0)` is therefore unambiguous when embedding primary differs from the first layer device.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidentTensorPlan {
    pub tensor: TensorId,
    pub rows: std::ops::Range<u32>,
    pub source_bytes: std::ops::Range<u64>,
    pub arena_offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind { Activation, Scratch, Result, KvState, ConvState, SsmState }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotStorage { F32, F16, I8 }

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
    Q8Rows { tensor: TensorId, rows: std::ops::Range<u32>, batch_capacity: u32 },
    EmbeddingRows { tensor: TensorId, row_count: u32 },
    LayerSegment { layers: std::ops::Range<u32>, families: Vec<LayerFamily> },
    FinalNormQ8Logits { norm: TensorId, output: TensorId, epsilon_bits: u32, batch_capacity: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerOp {
    Copy { input: SlotId, output: SlotId, elements: u32 },
    Slice { input: SlotId, offset: u32, elements: u32, output: SlotId },
    RmsNorm { input: SlotId, weight: TensorId, output: SlotId, epsilon_bits: u32 },
    Q8Matmul { input: SlotId, weight: TensorId, output: SlotId },
    Rope { q: SlotId, k: SlotId, key_head_dim: u32, rope_dims: u32, freq_base_bits: u32 },
    MRope { q: SlotId, k: SlotId, sections: [i32; 4], key_head_dim: u32, rope_dims: u32, freq_base_bits: u32 },
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
    Sigmoid { values: SlotId },
    SigmoidMul { gate: SlotId, values: SlotId },
    Silu { values: SlotId },
    SiluMul { gate: SlotId, up: SlotId },
    Mul { left: SlotId, right: SlotId, output: SlotId },
    Scale { values: SlotId, scale_bits: u32 },
    Add { left: SlotId, right: SlotId, output: SlotId },
    DepthwiseCausalConv { input: SlotId, weight: TensorId, state: SlotId, width: u32, output: SlotId },
    L2Norm { values: SlotId, epsilon_bits: u32 },
    SoftplusAffine { values: SlotId, bias: TensorId, scale: TensorId },
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
    pub row_shards: std::collections::BTreeMap<TensorId, Vec<RowShard>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPlan {
    pub components: std::collections::BTreeMap<ComponentId, ComponentPlan>,
    pub devices: std::collections::BTreeMap<DeviceId, DevicePlan>,
}

pub trait DeviceProvider: DeviceDiscovery {
    fn open(
        &self,
        descriptor: &DeviceDescriptor,
        plan: &DevicePlan,
        catalog: Arc<TensorCatalog>,
    ) -> Result<Box<dyn DeviceSession>, BackendError>;
}

impl DeviceRegistry {
    pub fn register_provider(
        &mut self,
        provider: Arc<dyn DeviceProvider>,
    ) -> Result<(), BackendError>;
    pub fn provider(
        &self,
        backend: BackendKind,
    ) -> Result<Arc<dyn DeviceProvider>, BackendError>;
}

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error("{device:?} receives no units from {available} across {targets} targets")]
    InsufficientUnits { device: DeviceId, available: u32, targets: usize },
    #[error("unsupported component {component:?} on {device:?}")]
    UnsupportedComponent { component: ComponentId, device: DeviceId },
    #[error("unsupported tensor {tensor:?} on {device:?}")]
    UnsupportedTensor { tensor: TensorId, device: DeviceId },
    #[error("row mode requires a CPU primary for {component:?}, got {device:?}")]
    UnsupportedRowPrimary { component: ComponentId, device: DeviceId },
    #[error("physical device {physical_key} was selected through more than one logical id: {devices:?}")]
    DuplicatePhysicalSelection { physical_key: String, devices: Vec<DeviceId> },
    #[error("capacity exceeded on {device:?}: required {required_bytes}, available {available_bytes}, largest allocation {largest_allocation_bytes}")]
    CapacityExceeded { device: DeviceId, required_bytes: u64, available_bytes: u64, largest_allocation_bytes: u64 },
    #[error("memory-size arithmetic overflow")]
    SizeOverflow,
}

pub struct PlacementCompiler<'a> {
    pub catalog: &'a TensorCatalog,
    pub registry: &'a DeviceRegistry,
    pub requirements: &'a [ComponentRequirements],
}

impl PlacementCompiler<'_> {
    pub fn compile(
        &self,
        rules: &std::collections::BTreeMap<ComponentId, PlacementRule>,
    ) -> Result<ExecutionPlan, PlanError>;
}

pub fn weighted_ranges(
    total: u32,
    targets: &[NormalizedTarget],
) -> Result<Vec<(DeviceId, std::ops::Range<u32>)>, PlanError>;
```

- `PlacementCompiler::compile` is pure: it does not call `DeviceProvider::open`, allocate arenas, or upload weights.

- [ ] **Step 1: Replace the old capacity-first tests with weighted plan tests**

```rust
#[test]
fn largest_remainder_is_weighted_contiguous_and_stable() {
    let targets = targets(&[("cpu0", 1.0), ("vulkan0", 2.0), ("metal0", 1.0)]);
    assert_eq!(weighted_ranges(10, &targets).unwrap(), vec![
        (id("cpu0"), 0..3),
        (id("vulkan0"), 3..8),
        (id("metal0"), 8..10),
    ]);
    let tied = targets(&[("cpu0", 1.0), ("metal0", 1.0), ("vulkan0", 1.0)]);
    assert_eq!(weighted_ranges(5, &tied).unwrap(), vec![
        (id("cpu0"), 0..2),
        (id("metal0"), 2..4),
        (id("vulkan0"), 4..5),
    ]);
}

#[test]
fn positive_target_cannot_receive_zero_units() {
    let targets = targets(&[("cpu0", 1.0), ("metal0", 1.0), ("vulkan0", 1.0)]);
    assert!(matches!(weighted_ranges(2, &targets), Err(PlanError::InsufficientUnits { .. })));
}

#[test]
fn compiler_rejects_capacity_and_vision_gpu_before_open() {
    let fixture = planner_fixture();
    let allocation_count = fixture.allocation_count.clone();
    assert!(matches!(fixture.compile_too_small(), Err(PlanError::CapacityExceeded { .. })));
    assert!(matches!(fixture.compile_vision_metal(), Err(PlanError::UnsupportedComponent { .. })));
    assert_eq!(allocation_count.load(Ordering::SeqCst), 0);
}
```

Add a `providers: BTreeMap<BackendKind, Arc<dyn DeviceProvider>>` field to `DeviceRegistry`. `register_provider` inserts the same provider into the discovery and provider maps and returns `BackendError::DuplicateBackend` if either backend key already exists; `provider` clones the stored `Arc` or returns `BackendUnavailable`.

Add separate assertions for unequal two-device and three-device Row plans, exact no-gap/no-overlap coverage, layer continuity, unknown devices, a non-CPU Row primary, unsupported Q4/Q5/Q6 GPU layers, per-allocation limits, and rejection when two selected logical IDs share one `physical_key`.

- [ ] **Step 2: Run planner tests and confirm the old capacity-fill algorithm fails weighted expectations**

Run: `cargo test --lib load_plan::tests -- --nocapture`

Expected: FAIL because the old `build_load_plan` fills device capacity in order and has no normalized quotas or execution programs.

- [ ] **Step 3: Implement largest remainder without hash-map iteration**

```rust
let exact = targets
    .iter()
    .map(|target| target.fraction * f64::from(total))
    .collect::<Vec<_>>();
let mut counts = exact.iter().map(|value| value.floor() as u32).collect::<Vec<_>>();
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
```

Build contiguous ranges in target declaration order. In Row mode, require the first positive target (the primary) to have `BackendKind::Cpu`, call `weighted_ranges` independently for every Q8_0 matrix's output rows, derive `source_bytes` from the catalog row stride, and reject unsupported matrices before allocation. Add an `EmbeddingRows` program on the primary device for `token_embd.weight`; embedding lookup selects resident rows by token ID and is not represented as a matmul or one-hot input. The existing Qwen runner retains non-Q8 operators and KV/recurrent state on that CPU primary; Q8 kernels have no runner-side escape.

In LLM Layer mode, validate that `LlmRequirements::layers` is non-empty and its `LlmLayerSpec::layer()` values are exactly `0..layers.len()`, then call `weighted_ranges` once per component. For every resulting device range, copy the corresponding `family()` values into one `LayerSegment` program, so an alternating Qwen3.5 dense/recurrent range remains one queued segment. Tasks 10–12 populate the three fixed spec structs and the compiler translates each spec directly into `ProgramPlan.layer_ops`; this vector is the sole immutable schedule owner. Translation preserves separate key/value head dimensions, attention context values, explicit key/value state slots, and recurrent state/group/rank/inner values in the matching fixed `LayerOp`. The primary also receives one `FinalNormQ8Logits` program bound through `ComponentPlan::finalization`, with the final norm and output TensorIds resident there. When the last span is on primary, its output aliases `finalization.input`; otherwise create a `TransferTarget::Finalization` transfer. Create one indexed `ActivationTransfer` between each adjacent span whose device changes; no state slot appears in this list. `VisionCpu` keeps the existing CPU vision runner and exists only for component resolution/capacity accounting; any non-CPU vision target still fails before allocation.

Before calculating memory, reject a selection containing two logical IDs with the same `physical_key`; this makes one `DevicePlan` and one session exactly equal one selected physical device. Compile every activation, Q8 value/scale scratch, result, key/value KV, convolution, and SSM handle into a byte-sized/aligned `SlotPlan`; the fixed `SlotStorage` tells backends how to encode it, and the state kinds are exactly the slots cleared by `reset_state`. `KvCacheType::F16` selects `SlotStorage::F16` at two bytes per KV element and `F32` selects `F32` at four, while Q8 activation values/scales receive separate `I8`/`F32` slots. Calculate slot lengths/offsets and all memory fields using `checked_add`, `checked_mul`, and an `align_up_checked` helper. Deduplicate tied tensors within one `DevicePlan` by `(TensorId, row range)`. Validate `max_allocation_bytes` and `usable_bytes`. Error variants carry component/device/tensor-or-layer plus `required_bytes`, `available_bytes`, and `largest_allocation_bytes`.

- [ ] **Step 4: Run planner tests plus catalog/load-plan regression tests**

Run: `cargo test --lib load_plan::tests && cargo test --lib tensor_catalog::tests && cargo test --lib ggufrs::tests`

Expected: PASS with deterministic weighted coverage and every pre-allocation error leaving the allocation counter at zero.

- [ ] **Step 5: Commit the immutable plan compiler**

```bash
git add src/load_plan.rs src/compute/program.rs src/compute/mod.rs src/lib.rs
git commit -m "feat: compile weighted execution plans"
```

### Task 5: Allocate Resident Arenas and Publish Compiled Sessions Atomically

**Files:**
- Create: `src/compute/session.rs`
- Modify: `src/compute/mod.rs`
- Modify: `src/lib.rs`
- Create: `tests/execution_lifecycle.rs`

**Interfaces:**
- Consumes: `ExecutionPlan`, `DeviceRegistry`, `TensorCatalog`, and Task 4's `DeviceProvider::open`.
- Produces:

```rust
pub struct CompiledModel {
    catalog: Arc<TensorCatalog>,
    plan: Arc<ExecutionPlan>,
    sessions: std::sync::Mutex<SessionSet>,
}

struct SessionSet {
    sessions: std::collections::BTreeMap<DeviceId, Box<dyn DeviceSession>>,
    host_results: std::collections::BTreeMap<(DeviceId, SlotId), Box<[f32]>>,
    pending: Vec<(usize, FenceId)>,
    transfer_scratch: Box<[f32]>,
    poisoned: bool,
}

pub struct ExecutionRun<'a> {
    plan: Arc<ExecutionPlan>,
    sessions: std::sync::MutexGuard<'a, SessionSet>,
}

impl CompiledModel {
    pub fn compile(
        catalog: Arc<TensorCatalog>,
        plan: ExecutionPlan,
        providers: Arc<DeviceRegistry>,
    ) -> Result<Self, BackendError>;
    pub fn start_run(&self) -> Result<ExecutionRun<'_>, BackendError>;
    pub fn plan(&self) -> &ExecutionPlan;
}

impl<'a> ExecutionRun<'a> {
    pub fn plan(&self) -> &ExecutionPlan;
    pub fn execute_q8(
        &mut self,
        component: ComponentId,
        tensor: TensorId,
        input: &[f32],
        batch: u32,
        output: &mut [f32],
    ) -> Result<(), BackendError>;
    pub fn execute_embedding(
        &mut self,
        component: ComponentId,
        tensor: TensorId,
        token_ids: &[u32],
        output: &mut [f32],
    ) -> Result<(), BackendError>;
    pub fn execute_embedding_into_layers(
        &mut self,
        component: ComponentId,
        tensor: TensorId,
        token_ids: &[u32],
        params: &RunParams<'_>,
    ) -> Result<(), BackendError>;
    pub fn execute_layers(
        &mut self,
        component: ComponentId,
        hidden: &mut [f32],
        params: &RunParams<'_>,
    ) -> Result<(), BackendError>;
    pub fn execute_logits(
        &mut self,
        component: ComponentId,
        params: &RunParams<'_>,
        output: &mut [f32],
    ) -> Result<(), BackendError>;
    pub fn reset_state(&mut self) -> Result<(), BackendError>;
    pub fn stats(&self) -> std::collections::BTreeMap<DeviceId, SessionStats>;
}
```

- `CompiledModel::compile` opens one session per selected `DevicePlan`. Task 4 has already guaranteed that selected `physical_key` values are unique, so this is exactly one session per physical device. It allocates all arenas/results, uploads every resident tensor, and publishes only after every session is ready. A failed open drops already opened sessions through RAII.

```rust
let session = providers
    .provider(device_plan.descriptor.backend)?
    .open(
        &device_plan.descriptor,
        device_plan,
        Arc::clone(&catalog),
    )?;
```

`start_run` locks the resident session set, verifies it is healthy, resets its KV/convolution/recurrent state, and returns a guard; the first implementation intentionally serializes server inference runs so weights remain resident without sharing mutable state. Add per-run state arenas only after concurrent throughput is measured to require them.

- [ ] **Step 1: Add mock-session tests for residency, parallel Row submission, Layer boundaries, rollback, and poisoning**

```rust
#[test]
fn weights_upload_once_and_row_submits_all_before_any_wait() {
    let trace = Arc::new(Mutex::new(Vec::new()));
    let compiled = compile_mock_row_model(trace.clone());
    let mut run = compiled.start_run().unwrap();
    let after_compile = run.stats();
    let mut output = vec![0.0; 12];
    run.execute_q8(ComponentId::Llm, TensorId(0), &[1.0; 64], 1, &mut output).unwrap();
    let after_token = run.stats();
    for id in [id("cpu0"), id("vulkan0"), id("metal0")] {
        assert_eq!(after_token[&id].weight_uploads, after_compile[&id].weight_uploads);
        assert_eq!(after_token[&id].resident_allocations, after_compile[&id].resident_allocations);
    }
    let trace = trace.lock().unwrap();
    let first_wait = trace.iter().position(|event| event.starts_with("wait:")).unwrap();
    assert_eq!(trace[..first_wait].iter().filter(|event| event.starts_with("submit:")).count(), 3);
}

#[test]
fn layer_segments_transfer_only_hidden_and_stateful_failure_poisons_run() {
    let compiled = compile_mock_layer_model(&[("cpu0", 0..1), ("metal0", 1..3)]);
    let mut run = compiled.start_run().unwrap();
    let error = run.execute_layers(
        ComponentId::Llm,
        &mut [0.0; 64],
        &RunParams {
            token_count: 1,
            position_start: 0,
            mrope_positions: &[[0; 4]],
            token_ids: &[],
        },
    ).unwrap_err();
    assert!(matches!(error, BackendError::Submission { .. }));
    assert!(matches!(run.reset_state(), Err(BackendError::PoisonedRun)));
}
```

Add assertions that embedding plus a two-layer same-device span on primary has no host readback or intermediate wait, one CPU-to-GPU boundary transfers exactly `hidden_size * token_count * 4` bytes in each direction required by the concrete transition, KV/recurrent transfer remains zero, partial construction frees every resident allocation, and drop observes `resident_allocations == resident_frees` through a shared lifecycle probe.

Add `layer_finalization_runs_on_primary_after_last_span_transfer`: place the last layer on `metal0` with `cpu0` primary, assert one `TransferTarget::Finalization` copies only `token_count * hidden_size * 4` bytes, then assert exactly one `FinalNormQ8Logits` submit produces the logits on `cpu0`. Add the same-primary case and assert `last_span.output == finalization.input`, zero finalization transfer, and correct logits. Add a compile test showing two logical IDs with one `physical_key` fail before any provider `open` call.

Add `embedding_rows_decode_requested_q8_tokens` with token IDs `[3, 1]`; compare its two F32 output rows with the existing CPU Q8 embedding decoder and assert the recording session receives `ProgramKind::EmbeddingRows`, never `Q8Rows`.

- [ ] **Step 2: Run the lifecycle integration test and confirm the coordinator is missing**

Run: `cargo test --test execution_lifecycle -- --nocapture`

Expected: FAIL because `CompiledModel` and `ExecutionRun` do not exist.

- [ ] **Step 3: Implement session construction and Row/Layer coordination**

During `CompiledModel::compile`, open sessions in deterministic `ExecutionPlan.devices` order. Row execution must follow this shape:

```rust
let shards = component_plan.row_shards.get(&tensor)
    .ok_or(BackendError::ProgramMissing { tensor })?;
let mut pending = std::mem::take(&mut self.sessions.pending);
pending.clear();
for (index, shard) in shards.iter().enumerate() {
    let session = self.session_mut(&shard.device)?;
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
for &(index, fence) in &pending {
    self.session_mut(&shards[index].device)?.wait(fence)?;
}
for &(index, _) in &pending {
    let shard = &shards[index];
    let rows = shard.rows.end - shard.rows.start;
    let local = self.sessions.host_results
        .get_mut(&(shard.device.clone(), shard.output))
        .ok_or(BackendError::InvalidHandle)?;
    self.session_mut(&shard.device)?
        .read_f32(shard.output, &mut local[..batch as usize * rows as usize])?;
    for item in 0..batch as usize {
        let src = &local[item * rows as usize..(item + 1) * rows as usize];
        let dst = &mut output[
            item * total_rows + shard.rows.start as usize
                ..item * total_rows + shard.rows.end as usize
        ];
        dst.copy_from_slice(src);
    }
}
pending.clear();
self.sessions.pending = pending;
```

`execute_embedding` is the Row/host-output API: it validates `token_ids.len() * hidden_size == output.len()`, resolves `ComponentPlan.embedding`, submits its `EmbeddingRows` program with `RunParams.token_ids`, waits once, and reads decoded F32 rows from its output slot into `output`; it never constructs a one-hot input.

`execute_embedding_into_layers` is the Layer-only resident chain. When primary owns the first span, the compiler aliases the embedding output and first-span input `SlotId`, so this method submits embedding and then the span without a host wait/read. When the first span is on another device, the plan contains an indexed hidden `ActivationTransfer` from embedding output to span 0; only that boundary waits and copies the hidden activation. It then performs the same span walk as `execute_layers`. `ExecutionRun::plan` returns its immutable plan reference so runners can choose Row versus Layer without reaching back into `CompiledModel`.

Layer execution walks `component_plan.layer_spans` in order. It writes the initial hidden values to the first span input and submits one program per span. After each span index, it looks up the matching `ActivationTransfer::after_span`; `TransferTarget::Span(next)` waits for the current fence, reads exactly `token_count * f32_values_per_token` values into `transfer_scratch`, writes them to that next span's input, and then submits the next span. After the last span, `TransferTarget::Finalization` moves the same hidden range into `ComponentPlan::finalization.input` on primary. There is no wait inside a span, and no KV, convolution, or recurrent slot is read by the coordinator.

`execute_logits` resolves `ComponentPlan::finalization`, submits its `FinalNormQ8Logits` program on primary, waits once, and reads its preallocated F32 logits slot into `output`. Layer mode never looks in `row_shards` for the final output projection. Row mode continues to route the Q8 output matrix through `execute_q8` like every other Q8 matrix.

Plan-sized reusable host result and `transfer_scratch` slots are allocated once in `CompiledModel::compile`; token execution must not allocate. The implementation preallocates `pending` to the maximum shard count and reuses it. On any error after a stateful submit, set `poisoned = true` before returning. Dropping a healthy run preserves resident weights; the next `start_run` resets only state. A poisoned model refuses new runs rather than silently reopening/reuploading or falling back.

- [ ] **Step 4: Run lifecycle tests and allocation-sensitive regression tests**

Run: `cargo test --test execution_lifecycle && cargo test --lib compute::session::tests`

Expected: PASS; mock traces show all Row submissions precede waits and compile/token snapshots have identical allocation/upload counts.

- [ ] **Step 5: Commit the compiled lifecycle**

```bash
git add src/compute/session.rs src/compute/mod.rs src/lib.rs tests/execution_lifecycle.rs
git commit -m "feat: own resident compiled sessions"
```

### Task 6: Implement the CPU Compiled-Session Reference

**Files:**
- Rewrite: `src/compute/cpu.rs`
- Modify: `src/compute/program.rs`
- Modify: `src/compute/mod.rs`
- Modify: `src/ops.rs`
- Test: `src/compute/cpu.rs` (`#[cfg(test)]` module)

**Interfaces:**
- Consumes: Task 3's provider/session traits, Task 4's programs and memory plan, and existing `ComputePool`, `quantize_q8_0_into`, `matmul_q8_0_quantized_range`, normalization, attention, and SSM helpers.
- Produces:

```rust
pub struct CpuProvider {
    thread_count: usize,
}

impl CpuProvider {
    pub fn new(thread_count: usize) -> Self;
}

pub struct CpuSession {
    descriptor: DeviceDescriptor,
    catalog: Arc<TensorCatalog>,
    resident: Vec<ResidentTensorPlan>,
    slots: Vec<Box<[f32]>>,
    programs: std::collections::BTreeMap<ProgramId, ProgramPlan>,
    worker: CpuWorker,
    stats: SessionStats,
}
```

- On platforms where NUMA topology is not available through the standard library, `CpuProvider` registers `cpu0`; the interface allows later platform-specific enumeration without adding it now. Explicit mock providers cover arbitrary `cpuN` planner behavior.

- [ ] **Step 1: Add one batched, non-zero-row Q8 reference test and a residency test**

```rust
#[test]
fn compiled_q8_rows_match_existing_cpu_kernel_for_batch_and_offset() {
    let (catalog, plan, input) = q8_program_fixture(2, 64, 129, 17..113);
    let mut session = open_cpu_session(&catalog, &plan);
    session.write_f32(SlotId(0), &input).unwrap();
    let fence = session.submit(
        ProgramId(0),
        &RunParams {
            token_count: 2,
            position_start: 0,
            mrope_positions: &[],
            token_ids: &[],
        },
    ).unwrap();
    session.wait(fence).unwrap();
    let mut actual = vec![0.0; 2 * 96];
    session.read_f32(SlotId(1), &mut actual).unwrap();
    assert_close(&actual, &cpu_q8_range_reference(&catalog, &input, 17..113), 1e-4, 1e-4);
    let before = session.stats();
    let second_fence = session.submit(ProgramId(0), &RunParams {
        token_count: 2,
        position_start: 0,
        mrope_positions: &[],
        token_ids: &[],
    }).unwrap();
    session.wait(second_fence).unwrap();
    let after = session.stats();
    assert_eq!(before.weight_uploads, after.weight_uploads);
    assert_eq!(before.resident_allocations, after.resident_allocations);
}
```

- [ ] **Step 2: Run CPU session tests and confirm the old copy-owning path fails the contract**

Run: `cargo test --lib compute::cpu::tests -- --nocapture`

Expected: FAIL because `CpuDevice` implements the removed `ComputeDevice`/`WorkSpec` interface and cannot execute compiled handles.

- [ ] **Step 3: Implement resident borrowed CPU tensors and asynchronous fences behind the common API**

`CpuProvider::open` stores the `Arc<TensorCatalog>`, validates that every `ResidentTensorPlan` range resolves in it, preallocates quantized input/scales/result/parameter slots from `MemoryPlan`, and stores catalog-backed offsets rather than copying weights. It starts one session-owned worker thread; that worker owns the existing `ComputePool`, so pool methods and output slots are never used concurrently from two host threads. The request channel is `std::sync::mpsc::sync_channel(1)` and the completion channel is pre-created during `open`; no async runtime or per-submit thread is added.

`submit` copies the small scalar/token parameters into preallocated session parameter slots, sends only `(FenceId, ProgramId)` to that worker, and immediately returns the monotonically increasing fence. It never sends `RunParams` references across threads and never allocates. The worker uses its `ComputePool` to fill the preallocated result slot. `wait` receives the matching completion/error and increments `host_waits`. This non-blocking submit is required so CPU, Vulkan, and Metal Row shards are all in flight before the first wait; a synchronous CPU `submit` is not acceptable.

For a Q8 Row program with `batch > 1`, loop over input rows and call the existing public range kernel with the resident source slice:

```rust
for item in 0..batch {
    let input = &input_slot[item * n_in..(item + 1) * n_in];
    quantize_q8_0_into(input, n_in, &mut q8_slot[..q8_len], &mut scale_slot[..scale_len]);
    matmul_q8_0_quantized_range(
        weight_bytes,
        &q8_slot[..q8_len],
        &scale_slot[..scale_len],
        &mut output_slot[item * local_rows..(item + 1) * local_rows],
        n_in,
        global_rows.start,
        global_rows.end,
    );
}
```

Keep `ops.rs` changes limited to making the already existing range functions usable without duplicating SIMD code. Report CPU weight residency as borrowed bytes and one logical upload per resident tensor so lifecycle assertions have backend-independent meaning.

- [ ] **Step 4: Run CPU session, SIMD, and default suites**

Run: `cargo test --lib compute::cpu::tests && cargo test --lib ops::tests && cargo test --all-targets`

Expected: PASS with Q8 batch/offset output inside the approved tolerance and no x86/NEON regression.

- [ ] **Step 5: Commit the CPU reference**

```bash
git add src/compute/cpu.rs src/compute/program.rs src/compute/mod.rs src/ops.rs
git commit -m "feat: execute compiled programs on cpu"
```

### Task 7: Implement Resident Vulkan Q8_0 Row Execution

**Files:**
- Delete: `src/compute/gpu.rs`
- Create: `src/compute/vulkan.rs`
- Delete: `src/compute/kernels/matmul_q8_0.comp`
- Delete: `src/compute/kernels/matmul_q8_0.spv`
- Create: `src/compute/vulkan/shaders/q8_0_rows.comp`
- Create: `src/compute/vulkan/shaders/q8_0_rows.spv`
- Modify: `src/compute/mod.rs`
- Modify: `Cargo.toml`
- Create: `tests/support/mod.rs`
- Create: `tests/gpu_backends.rs`

**Interfaces:**
- Consumes: `DeviceProvider`, `DeviceSession`, `DevicePlan`, and the Q8 Row ABI.
- Produces `VulkanProvider::new() -> Result<Self, BackendError>` and one private `VulkanSession` per selected physical adapter. Feature names become:

```toml
[features]
default = []
parity-trace = []
vulkan = ["dep:ash"]
metal = ["dep:metal"]
gpu = ["vulkan", "metal"]
```

The `metal` feature is global, but its optional dependency is declared only under `[target.'cfg(target_os = "macos")'.dependencies]`; combined with module-level `cfg(all(target_os = "macos", feature = "metal"))`, `gpu` enables both providers on macOS without compiling or linking Metal on other targets.

- [ ] **Step 1: Add deterministic Q8 fixtures and an ignored hardware comparison**

```rust
#[test]
fn q8_fixture_exercises_signed_bytes_scales_batch_offset_and_tail() {
    let fixture = support::q8_fixture(2, 64, 129, 17..113);
    assert!(fixture.weight_blocks.iter().any(|block| block.qs.iter().any(|q| *q < 0)));
    assert_ne!(fixture.weight_blocks[0].scale, fixture.weight_blocks[1].scale);
    assert_eq!(fixture.expected.len(), 2 * 96);
}

#[test]
#[ignore = "requires a Vulkan 1.1 compute adapter"]
fn vulkan_q8_row_matches_cpu() {
    require_backend("vulkan");
    let fixture = support::q8_fixture(2, 64, 129, 17..113);
    let actual = support::run_q8_backend(BackendKind::Vulkan, &fixture).unwrap();
    support::assert_close(&actual, &fixture.expected, 1e-4, 1e-4);
    assert!(actual.iter().all(|value| value.is_finite()));
}
```

`require_backend` fails when `RMI_REQUIRE_BACKEND=vulkan` and discovery/open fails; it never converts an explicitly required missing adapter into a skipped test.

- [ ] **Step 2: Run portable tests, then the required hardware test**

Run: `cargo test --test gpu_backends q8_fixture_exercises_signed_bytes_scales_batch_offset_and_tail -- --nocapture`

Expected: FAIL because the support fixture and Vulkan provider are absent.

Run on Vulkan hardware: `RMI_REQUIRE_BACKEND=vulkan VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation cargo test --features vulkan --test gpu_backends vulkan_q8_row_matches_cpu -- --ignored --nocapture`

Expected before implementation: FAIL during Vulkan session discovery or because the old shader does not implement the GGML Q8_0 ABI.

- [ ] **Step 3: Implement the shader's exact byte ABI and regenerate checked-in SPIR-V**

The shader reads weights from `uint[]` by byte extraction, reconstructs the little-endian F16 scale, sign-extends each quantized byte, multiplies by F32 input, and accumulates F32. Its push constants include `batch`, `n_in`, `local_rows`, `global_row_start`, and `global_output_stride`; dispatch covers `batch * local_rows`, so `M=2`, nonzero rows, and tail rows are first-class.

Regenerate once and commit the artifact:

```bash
/opt/homebrew/bin/glslc \
  --target-env=vulkan1.1 \
  -O \
  -fshader-stage=compute \
  src/compute/vulkan/shaders/q8_0_rows.comp \
  -o src/compute/vulkan/shaders/q8_0_rows.spv
```

- [ ] **Step 4: Implement Vulkan discovery and long-lived resources**

Enumerate every physical adapter and every compute-capable queue family; assign `vulkanN` in enumeration order. On macOS/MoltenVK, enable `VK_KHR_portability_enumeration`/`VK_KHR_portability_subset` only when advertised. Record `maxStorageBufferRange`, `maxMemoryAllocationSize` when exposed, `nonCoherentAtomSize`, heap budget, queue family, and physical UUID in `DeviceDescriptor`.

At `open`, allocate device-local resident buffers/arena, host-visible staging, activation/scratch/result buffers, descriptors, pipeline, command pool, reusable command buffers, and fences. Split a matrix into complete-row buffer chunks when it exceeds `maxStorageBufferRange`; prebuild descriptors for each chunk. Upload each resident row slice exactly once with transfer-to-compute barriers.

At `submit`, reset only completed fences/command buffers, record all chunk dispatches, use transfer-to-compute/compute-to-compute/compute-to-transfer GPU barriers, submit, and return without host waiting. At `wait`/`read_f32`, wait only for the requested fence and invalidate non-coherent ranges aligned to `nonCoherentAtomSize`. Never call `device_wait_idle` in inference; `Drop` waits for in-flight work, then releases resources in dependency-reverse order.

- [ ] **Step 5: Run Vulkan build, unit tests, hardware correctness, and shader freshness checks**

Run: `cargo test --all-targets --features vulkan`

Expected: PASS on hosts without a requested adapter because portable tests use mocks and discovery remains lazy.

Run on Vulkan hardware:

```bash
RMI_REQUIRE_BACKEND=vulkan \
VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation \
cargo test --features vulkan --test gpu_backends \
  vulkan_q8_row_matches_cpu -- --ignored --nocapture
```

Expected: PASS within `1e-4 + 1e-4 * abs(expected)`.

Verify the checked-in shader without modifying the worktree:

```bash
tmp_spv="$(mktemp -t rmi-q8-rows.XXXXXX.spv)"
/opt/homebrew/bin/glslc --target-env=vulkan1.1 -O -fshader-stage=compute \
  src/compute/vulkan/shaders/q8_0_rows.comp -o "$tmp_spv"
cmp "$tmp_spv" src/compute/vulkan/shaders/q8_0_rows.spv
rm "$tmp_spv"
```

Expected: PASS; `cmp` exits 0.

- [ ] **Step 6: Commit Vulkan Row execution**

```bash
git add Cargo.toml src/compute/mod.rs src/compute/vulkan.rs src/compute/vulkan/shaders/q8_0_rows.comp src/compute/vulkan/shaders/q8_0_rows.spv tests/support/mod.rs tests/gpu_backends.rs
git rm src/compute/gpu.rs src/compute/kernels/matmul_q8_0.comp src/compute/kernels/matmul_q8_0.spv
git commit -m "feat: execute resident q8 rows on vulkan"
```

### Task 8: Implement Resident Metal Q8_0 Row Execution

**Files:**
- Create: `src/compute/metal.rs`
- Create: `src/compute/metal/kernels.metal`
- Modify: `src/compute/mod.rs`
- Modify: `Cargo.toml`
- Modify: `tests/gpu_backends.rs`

**Interfaces:**
- Consumes: the same Q8 fixture, plan, provider/session contract, and slot layout as Vulkan.
- Produces `MetalProvider::new() -> Result<Self, BackendError>` and a cfg-isolated `MetalSession`.

Cargo dependency placement is exact:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
metal = { version = "0.33.0", optional = true }
```

- [ ] **Step 1: Add Metal cfg/build tests and the ignored hardware comparison**

```rust
#[cfg(all(target_os = "macos", feature = "metal"))]
#[test]
#[ignore = "requires a Metal device"]
fn metal_q8_row_matches_cpu() {
    require_backend("metal");
    let fixture = support::q8_fixture(2, 64, 129, 17..113);
    let actual = support::run_q8_backend(BackendKind::Metal, &fixture).unwrap();
    support::assert_close(&actual, &fixture.expected, 1e-4, 1e-4);
    assert!(actual.iter().all(|value| value.is_finite()));
}
```

Add a portable `#[cfg(not(target_os = "macos"))]` compilation test that refers only to public non-Metal contracts, proving no Metal symbol leaks into other targets.

- [ ] **Step 2: Run the feature build and hardware test before implementation**

Run: `cargo check --all-targets --features metal`

Expected: FAIL because the feature/module/dependency are absent.

Run on macOS: `RMI_REQUIRE_BACKEND=metal MTL_DEBUG_LAYER=1 MTL_SHADER_VALIDATION=1 cargo test --features metal --test gpu_backends metal_q8_row_matches_cpu -- --ignored --nocapture`

Expected: FAIL because no Metal provider exists.

- [ ] **Step 3: Implement runtime MSL compilation and resident Metal buffers**

In `kernels.metal`, read weights from `device const uchar *`; build the F16 scale from two little-endian bytes, cast Q bytes through `char`, accumulate F32, and use global `batch`, `global_row`, and output-stride parameters matching Vulkan. Compile with:

```rust
let source = include_str!("metal/kernels.metal");
let options = metal::CompileOptions::new();
options.set_fast_math_enabled(false);
let library = device
    .new_library_with_source(source, &options)
    .map_err(|message| BackendError::Pipeline { device: id.clone(), message })?;
```

Enumerate `metal::Device::all()`, record registry IDs, recommended working-set size, `max_buffer_length`, and unified-memory status. At `open`, allocate Private resident/state/scratch buffers and reusable Shared staging/result buffers; split oversized matrices on whole rows. Create queue, library, pipeline cache, slots, and resident uploads before returning.

Each submit creates the native one-shot command buffer inside `objc::rc::autoreleasepool`, encodes all chunks, commits, stores it under a `FenceId`, and returns without `wait_until_completed`. `wait` checks completion status/error and poisons the run on failure. Native command-buffer creation is not counted as a resident allocation; `MTLBuffer`, pipeline, state, scratch, and staging allocations are. `Drop` completes in-flight buffers before releasing dependent resources.

- [ ] **Step 4: Run Metal tests, compatibility feature checks, and non-macOS cfg checking**

Run on macOS:

```bash
cargo test --all-targets --features metal
RMI_REQUIRE_BACKEND=metal MTL_DEBUG_LAYER=1 MTL_SHADER_VALIDATION=1 \
  cargo test --features metal --test gpu_backends \
  metal_q8_row_matches_cpu -- --ignored --nocapture
cargo test --all-targets --features gpu
```

Expected: PASS within the approved kernel tolerance; `gpu` builds both available providers on macOS.

Run from a host with the Linux target installed: `RUSTFLAGS='' cargo check --target x86_64-unknown-linux-gnu --all-targets --features gpu`

Expected: PASS without compiling or linking Metal code.

- [ ] **Step 5: Commit Metal Row execution**

```bash
git add Cargo.toml src/compute/mod.rs src/compute/metal.rs src/compute/metal/kernels.metal tests/gpu_backends.rs
git commit -m "feat: execute resident q8 rows on metal"
```

### Task 9: Route Every Qwen3 and Qwen3.5 Q8_0 Matrix Through the Compiled Plan

**Files:**
- Create: `src/qwen3.rs`
- Modify: `src/qwen35.rs`
- Modify: `src/main.rs`
- Modify: `src/bin/server.rs`
- Modify: `src/lib.rs`
- Create: `tests/placement_e2e.rs`

**Interfaces:**
- Consumes: `TensorCatalog`, `CompiledModel`, `ExecutionRun::{execute_embedding, execute_embedding_into_layers, execute_q8, execute_layers, execute_logits}`, existing Qwen3 generation code from both binaries, and `Qwen35Model`.
- Produces:

```rust
pub struct Qwen3Model {
    pub config: Qwen3Config,
    pub tensors: Qwen3TensorIds,
    pub auxiliary: Qwen3AuxiliaryWeights,
}

pub struct Qwen3TensorIds {
    pub token_embedding: TensorId,
    pub layers: Vec<Qwen3LayerTensorIds>,
    pub output: TensorId,
}

pub struct Qwen3LayerTensorIds {
    pub q: TensorId,
    pub k: TensorId,
    pub v: TensorId,
    pub o: TensorId,
    pub gate: TensorId,
    pub up: TensorId,
    pub down: TensorId,
}

pub struct Qwen3AuxiliaryWeights {
    pub attention_norms: Vec<TensorId>,
    pub q_norms: Vec<Option<TensorId>>,
    pub k_norms: Vec<Option<TensorId>>,
    pub ffn_norms: Vec<TensorId>,
    pub final_norm: TensorId,
}

impl Qwen3Model {
    pub fn from_catalog(catalog: &TensorCatalog) -> Result<Self, String>;
    pub fn forward(
        &self,
        run: &mut ExecutionRun,
        tokens: &[u32],
        positions: &[[u32; 4]],
        output: &mut [f32],
    ) -> Result<(), String>;
}

impl Qwen35Model {
    pub fn from_catalog(catalog: &TensorCatalog) -> Result<Self, String>;
    pub fn forward_compiled(
        &self,
        run: &mut ExecutionRun,
        tokens: &[u32],
        positions: &[[u32; 4]],
        output: &mut [f32],
    ) -> Result<(), String>;
}
```

Both forward methods require `positions.len() == tokens.len()`, convert `tokens.len()` and the current absolute position to `u32` with `try_from`, and reject overflow before submitting any program. They pass those checked values through `RunParams`; neither backend narrows an unchecked `usize`.

Each model also exposes `requirements(&self) -> ComponentRequirements`. `from_catalog` resolves the named TensorIds and metadata first, then `requirements` constructs the complete ordered `LlmLayerSpec` vector before `PlacementCompiler::compile` runs. Session opening therefore sees final `ProgramPlan::layer_ops`; no runner mutates programs or arenas after compilation.

`Qwen3TensorIds` records IDs for `token_embd.weight`, output/tied output, and every layer's Q/K/V/O and gate/up/down tensor. Qwen3.5 records IDs alongside each `QWeight`; a Q8_0 variant calls `ExecutionRun::execute_q8`, while F32/F16/Q4/Q5/Q6 CPU-only variants keep the existing CPU operation only when the compiled plan assigns that matrix to CPU.

- [ ] **Step 1: Add a recording-session coverage test for every projection family**

```rust
#[test]
fn qwen3_and_qwen35_q8_tensors_all_use_compiled_programs() {
    let fixtures = [support::tiny_qwen3(), support::tiny_qwen35_hybrid()];
    for fixture in fixtures {
        let expected_q8_matrices = fixture.catalog.entries().iter()
            .filter(|entry| entry.component == ComponentId::Llm && entry.ggml_type == GGMLType::Q8_0)
            .filter(|entry| entry.id != fixture.token_embedding_id() || entry.id == fixture.output_id())
            .map(|entry| entry.id)
            .collect::<BTreeSet<_>>();
        let trace = fixture.run_recording_forward_two_tokens().unwrap();
        assert_eq!(trace.q8_matrix_tensor_ids, expected_q8_matrices);
        assert_eq!(
            trace.embedding_tensor_ids,
            BTreeSet::from([fixture.token_embedding_id()]),
        );
    }
}

#[test]
fn qwen35_q8_dispatch_has_no_direct_cpu_escape() {
    let source = include_str!("../src/qwen35.rs");
    assert!(!source.contains("fn quantize_and_matmul_with_scratch"));
}
```

The recording session fills `q8_matrix_tensor_ids` only when it executes `ProgramKind::Q8Rows`, `LayerOp::Q8Matmul`, or the output inside `FinalNormQ8Logits`; it fills `embedding_tensor_ids` only for `ProgramKind::EmbeddingRows`. Keeping the operation kind separate means a tied embedding/output TensorId proves both uses independently. The tiny fixtures use hidden 64, Qwen3 heads 4/KV heads 2/head dim 16/intermediate 96/vocab 64/context 8/two layers, and Qwen3.5 one dense plus one recurrent layer with convolution width 4. Every supported projection is Q8_0; auxiliary norm/SSM tensors use F32.

- [ ] **Step 2: Run the routing test and confirm the duplicated/direct paths are detected**

Run: `cargo test --test placement_e2e qwen3_and_qwen35_q8_tensors_all_use_compiled_programs -- --nocapture && cargo test --test placement_e2e qwen35_q8_dispatch_has_no_direct_cpu_escape -- --nocapture`

Expected: FAIL because Qwen3 binaries call `matmul_q8_0_quantized_parallel_rows` directly and Qwen3.5 owns `quantize_and_matmul_with_scratch`.

- [ ] **Step 3: Move Qwen3 into the library and replace all Q8 dispatch sites**

Move, rather than copy, `Qwen3Config`, layer tensor loading, embedding lookup, transformer forward, output projection, and generation-support logic from `main.rs`/`server.rs` into `qwen3.rs`. Remove `LayerWeights`/`LayerWeightsOwned`, leaked static tensor slices, duplicated `generate_qwen3` math, and the unsafe `Send`/`Sync` declarations that existed to share borrowed state.

Branch only on the immutable component mode:

```rust
match run.plan().components[&ComponentId::Llm].mode {
    PlacementMode::Row => forward_row_cpu_primary(run, tokens, positions, output),
    PlacementMode::Layer => {
        run.execute_embedding_into_layers(
            ComponentId::Llm,
            self.token_embedding(),
            tokens,
            &params,
        )?;
        run.execute_logits(ComponentId::Llm, &params, output)
    }
}
```

In Row mode, each Q8 matrix—including tied/untied output head—resolves its `TensorId` and calls `run.execute_q8`; the existing runner retains only non-Q8 operators and CPU-primary KV/recurrent state, and embeddings use the host-output `execute_embedding`. In Layer mode, `execute_embedding_into_layers` chains the resident embedding slot into the first span, internal matrices execute inside the precompiled segment, and the output head executes only through `FinalNormQ8Logits`. Both embedding APIs select rows through `EmbeddingRows` and never use the temporary scheduler path in `EmbeddingWeight::project`. Qwen3.5 deletes `quantize_and_matmul_with_scratch` and removes every runner-side Q8 kernel escape.

Keep Q8 scalar/SIMD kernels only inside sessions. Row CPU-primary intentionally retains the existing host implementation for non-Q8 operators and state; GPU-primary Row remains a compile-time error until a fixed primary-stage program is implemented.

- [ ] **Step 4: Run route coverage and all CPU-only model tests**

Run: `cargo test --test placement_e2e qwen3_and_qwen35_q8_tensors_all_use_compiled_programs && cargo test --test placement_e2e qwen35_q8_dispatch_has_no_direct_cpu_escape && cargo test --all-targets`

Expected: PASS; no placement produces CPU-only logits through the compiled CPU session, and the recorded tensor IDs exactly cover all catalogued LLM Q8_0 matrices.

- [ ] **Step 5: Commit the single Q8 routing path**

```bash
git add src/qwen3.rs src/qwen35.rs src/main.rs src/bin/server.rs src/lib.rs tests/placement_e2e.rs tests/support/mod.rs
git commit -m "refactor: route all q8 matrices through execution plans"
```

### Task 10: Execute Complete Qwen3 Layers on Vulkan and Metal

**Files:**
- Modify: `src/load_plan.rs`
- Modify: `src/compute/program.rs`
- Modify: `src/compute/cpu.rs`
- Modify: `src/compute/vulkan.rs`
- Modify: `src/compute/vulkan/shaders/q8_0_rows.comp`
- Modify: `src/compute/vulkan/shaders/q8_0_rows.spv`
- Create: `src/compute/vulkan/shaders/layer_ops.comp`
- Create: `src/compute/vulkan/shaders/layer_ops.spv`
- Modify: `src/compute/metal.rs`
- Modify: `src/compute/metal/kernels.metal`
- Modify: `src/qwen3.rs`
- Create: `tests/qwen_gpu_layers.rs`

**Interfaces:**
- Consumes: `Qwen3LayerSpec`, `ProgramKind::LayerSegment` whose family entries are `LayerFamily::Qwen3`, Qwen3 tensor IDs/config, resident slots, Task 7/8 Q8 kernels, `ProgramKind::FinalNormQ8Logits`, and `ExecutionRun::{execute_layers, execute_logits}`.
- Extends the Task 4 fixed `LayerOp` schedule only by populating the already-declared Qwen3 variants; no public general graph IR or second wrapper is added:

`ProgramPlan.layer_ops: Vec<LayerOp>` is the sole owner of this schedule. Do not add a second `LayerProgram` wrapper.

- [ ] **Step 1: Add a two-token Qwen3 CPU gold test and ignored backend tests**

```rust
#[test]
fn qwen3_layer_schedule_matches_cpu_for_two_tokens_and_carries_kv() {
    let fixture = support::tiny_qwen3();
    let expected = fixture.cpu_reference_two_tokens();
    let actual = fixture.compiled_cpu_two_tokens().unwrap();
    support::assert_close(&actual.logits, &expected.logits, 1e-3, 1e-3);
    assert_ne!(actual.first_token_logits, actual.second_token_logits);
}

#[test]
#[ignore = "requires selected GPU backend"]
fn gpu_qwen3_layer_matches_cpu() {
    let backend = required_backend_from_env();
    let fixture = support::tiny_qwen3();
    let expected = fixture.compiled_cpu_two_tokens().unwrap();
    let actual = fixture.compiled_backend_two_tokens(backend).unwrap();
    support::assert_close(&actual.logits, &expected.logits, 1e-3, 1e-3);
    assert_eq!(actual.tokens, expected.tokens);
    assert_eq!(actual.same_device_internal_host_waits, 0);
    assert_eq!(actual.kv_transfer_bytes, 0);
}
```

- [ ] **Step 2: Run the CPU schedule test and confirm Layer programs are not executable**

Run: `cargo test --test qwen_gpu_layers qwen3_layer_schedule_matches_cpu_for_two_tokens_and_carries_kv -- --nocapture`

Expected: FAIL because sessions only execute Q8 Row programs.

- [ ] **Step 3: Compile the exact Qwen3 operation sequence and implement the CPU reference**

Make `Qwen3Model::requirements` fill every `Qwen3LayerSpec` field declared in Task 4 from catalog TensorIds and model metadata before placement compilation. For each spec, emit: attention RMSNorm; Q/K/V Q8 projections; optional per-head Q/K RMSNorm; RoPE using model metadata; KV append; grouped-query attention score; padded stable softmax that subtracts the maximum; attention-value reduction; O projection; residual add; FFN RMSNorm; gate/up projections; SiLU gate times up; down projection; residual add. Compile `FinalNormQ8Logits` on primary as final RMSNorm followed by the resident output Q8 matrix. Embedding stays a separate `EmbeddingRows` program on primary.

CPU `submit` interprets this fixed enum with existing `ops.rs` routines. Tests compare every named checkpoint against the pre-refactor CPU path before deleting the latter.

- [ ] **Step 4: Add equivalent Vulkan and Metal kernels and one submission per layer span**

Add activation quantization, RMSNorm, Q/K norm, RoPE, KV update/read, grouped-query score, stable padded softmax, value reduction, SiLU/SwiGLU, vector multiply/add/scale/copy, and Q8 matrix dispatch to both backends. F32/F16 auxiliary weights are resident. Keep state/scratch offsets in the compiled program; do not allocate in dispatch.

Vulkan records all ops for a span into one command buffer with compute-to-compute barriers and no host wait. Metal records all ops into one command buffer inside an autorelease pool; native encoder ordering/barriers provide device dependencies and `wait_until_completed` occurs only at a segment boundary that must cross devices or expose host output.

Regenerate both Vulkan artifacts:

```bash
/opt/homebrew/bin/glslc --target-env=vulkan1.1 -O -fshader-stage=compute \
  src/compute/vulkan/shaders/q8_0_rows.comp \
  -o src/compute/vulkan/shaders/q8_0_rows.spv
/opt/homebrew/bin/glslc --target-env=vulkan1.1 -O -fshader-stage=compute \
  src/compute/vulkan/shaders/layer_ops.comp \
  -o src/compute/vulkan/shaders/layer_ops.spv
```

After regeneration, verify `layer_ops.spv` without modifying the worktree:

```bash
tmp_spv="$(mktemp -t rmi-layer-ops.XXXXXX.spv)"
/opt/homebrew/bin/glslc --target-env=vulkan1.1 -O -fshader-stage=compute \
  src/compute/vulkan/shaders/layer_ops.comp -o "$tmp_spv"
cmp "$tmp_spv" src/compute/vulkan/shaders/layer_ops.spv
rm "$tmp_spv"
```

Expected: `cmp` exits 0.

- [ ] **Step 5: Run CPU and both real-backend Qwen3 tests**

Run: `cargo test --test qwen_gpu_layers qwen3_layer_schedule_matches_cpu_for_two_tokens_and_carries_kv`

Expected: PASS within `1e-3` absolute/relative tolerance.

Run on Metal:

```bash
RMI_REQUIRE_BACKEND=metal MTL_DEBUG_LAYER=1 MTL_SHADER_VALIDATION=1 \
  cargo test --features metal --test qwen_gpu_layers \
  gpu_qwen3_layer_matches_cpu -- --ignored --nocapture
```

Run on Vulkan:

```bash
RMI_REQUIRE_BACKEND=vulkan VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation \
  cargo test --features vulkan --test qwen_gpu_layers \
  gpu_qwen3_layer_matches_cpu -- --ignored --nocapture
```

Expected: both PASS; selected tokens match, and same-device layer spans show zero internal host waits and zero KV transfer bytes.

- [ ] **Step 6: Commit complete Qwen3 Layer execution**

```bash
git add src/load_plan.rs src/compute/program.rs src/compute/cpu.rs src/compute/vulkan.rs src/compute/vulkan/shaders/q8_0_rows.comp src/compute/vulkan/shaders/q8_0_rows.spv src/compute/vulkan/shaders/layer_ops.comp src/compute/vulkan/shaders/layer_ops.spv src/compute/metal.rs src/compute/metal/kernels.metal src/qwen3.rs tests/qwen_gpu_layers.rs tests/support/mod.rs
git commit -m "feat: execute qwen3 layers on vulkan and metal"
```

### Task 11: Execute Complete Qwen3.5 Dense Layers on Vulkan and Metal

**Files:**
- Modify: `src/load_plan.rs`
- Modify: `src/compute/program.rs`
- Modify: `src/compute/cpu.rs`
- Modify: `src/compute/vulkan.rs`
- Modify: `src/compute/vulkan/shaders/layer_ops.comp`
- Modify: `src/compute/vulkan/shaders/layer_ops.spv`
- Modify: `src/compute/metal.rs`
- Modify: `src/compute/metal/kernels.metal`
- Modify: `src/qwen35.rs`
- Modify: `tests/qwen_gpu_layers.rs`

**Interfaces:**
- Consumes: `Qwen35DenseLayerSpec`, Qwen3 dense primitives plus Qwen3.5 metadata (`rope_dimension_sections`, `rope_dimension_count`), dense Q/K/V/O tensor IDs, attention gate, and `ProgramKind::LayerSegment` entries whose family is `LayerFamily::Qwen35Dense`.
- Produces Qwen3.5 dense schedules by populating the Task 4 `LayerOp::MRope` and `LayerOp::SigmoidMul` variants:

```rust
LayerOp::MRope {
    q: SlotId,
    k: SlotId,
    sections: [i32; 4],
    key_head_dim: u32,
    rope_dims: u32,
    freq_base_bits: u32,
},
LayerOp::SigmoidMul { gate: SlotId, values: SlotId },
```

- [ ] **Step 1: Add two-token dense-layer CPU/backend comparisons**

```rust
#[test]
fn qwen35_dense_layer_matches_cpu_and_uses_metadata_mrope() {
    let fixture = support::tiny_qwen35_hybrid();
    let expected = fixture.cpu_dense_two_tokens();
    let actual = fixture.compiled_cpu_dense_two_tokens().unwrap();
    support::assert_close(&actual.logits, &expected.logits, 1e-3, 1e-3);
    assert_eq!(actual.observed_sections, fixture.config.rope_dimension_sections);
}

#[test]
#[ignore = "requires selected GPU backend"]
fn gpu_qwen35_dense_layer_matches_cpu() {
    let backend = required_backend_from_env();
    let fixture = support::tiny_qwen35_hybrid();
    let expected = fixture.compiled_cpu_dense_two_tokens().unwrap();
    let actual = fixture.compiled_backend_dense_two_tokens(backend).unwrap();
    support::assert_close(&actual.logits, &expected.logits, 1e-3, 1e-3);
    assert_eq!(actual.tokens, expected.tokens);
    assert_eq!(actual.kv_transfer_bytes, 0);
}
```

- [ ] **Step 2: Run the CPU dense test and confirm the Qwen3 schedule misses MRoPE/gating**

Run: `cargo test --test qwen_gpu_layers qwen35_dense_layer_matches_cpu_and_uses_metadata_mrope -- --nocapture`

Expected: FAIL because Qwen3.5 dense `LayerSegment` ops do not yet schedule MRoPE and the attention gate.

- [ ] **Step 3: Transcribe the current dense path in exact order into the fixed schedule**

Make `Qwen35Model::requirements` fill every dense layer's `Qwen35DenseLayerSpec` field declared in Task 4 before placement compilation. Then follow `Qwen35Model::forward_dense_attn_layer` operation-for-operation: input RMSNorm; Q/K/V projections; per-head Q/K norms; metadata-driven MRoPE or RoPE; KV append; causal grouped-query attention with padded stable softmax; attention gate sigmoid multiply; O projection; residual; post-attention RMSNorm; gate/up Q8 projections; SiLU multiply; down projection; residual. Preserve shapes, epsilon, Q's doubled gate layout, and token/batch indexing from the CPU reference.

Implement CPU, Vulkan, and Metal interpretations using the existing primitives; do not fork backend-specific model formulas. Regenerate `layer_ops.spv` with the Task 10 command.

- [ ] **Step 4: Run CPU, Metal, and Vulkan dense tests**

Run: `cargo test --test qwen_gpu_layers qwen35_dense_layer_matches_cpu_and_uses_metadata_mrope`

Run on each backend:

```bash
RMI_REQUIRE_BACKEND=metal cargo test --features metal --test qwen_gpu_layers \
  gpu_qwen35_dense_layer_matches_cpu -- --ignored --nocapture
RMI_REQUIRE_BACKEND=vulkan cargo test --features vulkan --test qwen_gpu_layers \
  gpu_qwen35_dense_layer_matches_cpu -- --ignored --nocapture
```

Expected: all PASS inside layer tolerance with matching selected tokens and zero KV transfer bytes.

- [ ] **Step 5: Commit Qwen3.5 dense execution**

```bash
git add src/load_plan.rs src/compute/program.rs src/compute/cpu.rs src/compute/vulkan.rs src/compute/vulkan/shaders/layer_ops.comp src/compute/vulkan/shaders/layer_ops.spv src/compute/metal.rs src/compute/metal/kernels.metal src/qwen35.rs tests/qwen_gpu_layers.rs
git commit -m "feat: execute qwen35 dense layers on gpu"
```

### Task 12: Execute Complete Qwen3.5 Recurrent Layers on Vulkan and Metal

**Files:**
- Modify: `src/load_plan.rs`
- Modify: `src/compute/program.rs`
- Modify: `src/compute/cpu.rs`
- Modify: `src/compute/vulkan.rs`
- Modify: `src/compute/vulkan/shaders/layer_ops.comp`
- Modify: `src/compute/vulkan/shaders/layer_ops.spv`
- Modify: `src/compute/metal.rs`
- Modify: `src/compute/metal/kernels.metal`
- Modify: `src/qwen35.rs`
- Modify: `tests/qwen_gpu_layers.rs`

**Interfaces:**
- Consumes: `Qwen35RecurrentLayerSpec`, `ProgramKind::LayerSegment` entries whose family is `LayerFamily::Qwen35Recurrent`, recurrent Q8 tensor IDs, F32 convolution/bias/A/norm tensors, and per-layer resident convolution/SSM state slots.
- Produces recurrent schedules by populating the Task 4 `DepthwiseCausalConv`, `L2Norm`, `SoftplusAffine`, and `SsmUpdate` variants:

```rust
LayerOp::DepthwiseCausalConv {
    input: SlotId,
    weight: TensorId,
    state: SlotId,
    width: u32,
    output: SlotId,
},
LayerOp::L2Norm { values: SlotId, epsilon_bits: u32 },
LayerOp::SoftplusAffine { values: SlotId, bias: TensorId, scale: TensorId },
LayerOp::SsmUpdate {
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
```

- [ ] **Step 1: Add two-token state-carry and reset tests for CPU and real backends**

```rust
#[test]
fn qwen35_recurrent_layer_carries_and_resets_conv_and_ssm_state() {
    let fixture = support::tiny_qwen35_hybrid();
    let mut run = fixture.compiled_cpu_run().unwrap();
    let first = fixture.forward_recurrent_token(&mut run, 0).unwrap();
    let second = fixture.forward_recurrent_token(&mut run, 1).unwrap();
    assert_ne!(first, second);
    run.reset_state().unwrap();
    let first_after_reset = fixture.forward_recurrent_token(&mut run, 0).unwrap();
    support::assert_close(&first_after_reset, &first, 1e-3, 1e-3);
}

#[test]
#[ignore = "requires selected GPU backend"]
fn gpu_qwen35_recurrent_layer_matches_cpu_and_carries_state() {
    let backend = required_backend_from_env();
    let fixture = support::tiny_qwen35_hybrid();
    let expected = fixture.compiled_cpu_recurrent_two_tokens().unwrap();
    let actual = fixture.compiled_backend_recurrent_two_tokens(backend).unwrap();
    support::assert_close(&actual.logits, &expected.logits, 1e-3, 1e-3);
    assert_eq!(actual.tokens, expected.tokens);
    assert_eq!(actual.recurrent_transfer_bytes, 0);
}
```

- [ ] **Step 2: Run the CPU state test and confirm recurrent spans are unsupported**

Run: `cargo test --test qwen_gpu_layers qwen35_recurrent_layer_carries_and_resets_conv_and_ssm_state -- --nocapture`

Expected: FAIL because the compiled session has no resident convolution/SSM state program.

- [ ] **Step 3: Transcribe the current recurrent path exactly and make state ownership explicit**

Make `Qwen35Model::requirements` fill every recurrent layer's `Qwen35RecurrentLayerSpec` field declared in Task 4 before placement compilation. Then follow `Qwen35Model::forward_recurrent_layer` in its current order: QKV/gate/beta/alpha Q8 projections; beta sigmoid; alpha bias, softplus, and A scaling; shift/update depthwise convolution state; F32 causal convolution; SiLU; Q/K/V split; per-head L2 norm; SSM decay; state-times-K; beta correction; outer-product update; scaled state-times-Q; per-head RMSNorm; gate SiLU multiply; SSM output Q8 projection; residual; post-attention norm; FFN; residual.

Allocate convolution and SSM state in the owning device's `MemoryPlan`. Every kernel reads/writes those slots in queue order. `reset_state` zeroes them without uploading weights. If any submit/wait/read reports failure after a state mutation, mark `ExecutionRun` poisoned and reject all further forward/reset calls.

Implement the same schedule in CPU/Vulkan/Metal; preserve the CPU reference's dimensions, epsilon, head mapping, gate order, and state update order. Regenerate `layer_ops.spv` with the Task 10 command.

- [ ] **Step 4: Run CPU, Metal, Vulkan, lifecycle, and poison tests**

Run: `cargo test --test qwen_gpu_layers qwen35_recurrent_layer_carries_and_resets_conv_and_ssm_state && cargo test --test execution_lifecycle`

Run on each backend:

```bash
RMI_REQUIRE_BACKEND=metal cargo test --features metal --test qwen_gpu_layers \
  gpu_qwen35_recurrent_layer_matches_cpu_and_carries_state -- --ignored --nocapture
RMI_REQUIRE_BACKEND=vulkan cargo test --features vulkan --test qwen_gpu_layers \
  gpu_qwen35_recurrent_layer_matches_cpu_and_carries_state -- --ignored --nocapture
```

Expected: PASS; the second token proves state carry, reset reproduces the first token, state transfers remain zero, and injected stateful failures poison the run.

- [ ] **Step 5: Commit Qwen3.5 recurrent execution**

```bash
git add src/load_plan.rs src/compute/program.rs src/compute/cpu.rs src/compute/vulkan.rs src/compute/vulkan/shaders/layer_ops.comp src/compute/vulkan/shaders/layer_ops.spv src/compute/metal.rs src/compute/metal/kernels.metal src/qwen35.rs tests/qwen_gpu_layers.rs
git commit -m "feat: execute qwen35 recurrent layers on gpu"
```

### Task 13: Integrate CLI and Server, Prove GGUF/GGUFRS E2E, Remove the Old Path, and Document Benchmarks

**Files:**
- Modify: `src/main.rs`
- Modify: `src/bin/server.rs`
- Modify: `src/bin/micro_bench.rs`
- Modify: `src/compute/mod.rs`
- Modify: `src/lib.rs`
- Modify: `README.md`
- Modify: `ARCHITECTURE.md`
- Modify: `OPTIMIZATION.md`
- Modify: `GGUFRS.md`
- Modify: `tests/placement_e2e.rs`
- Modify: `tests/execution_lifecycle.rs`
- Modify: `tests/gpu_backends.rs`
- Modify: `tests/qwen_gpu_layers.rs`
- Modify: `tests/support/mod.rs`

**Interfaces:**
- Consumes: `parse_placements`, `TensorCatalog::from_sources`, providers/registry, `PlacementCompiler`, `CompiledModel`, shared Qwen3/Qwen3.5 runners, and existing tokenizer/server request types.
- Produces one reusable configuration entry point called by both binaries:

```rust
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

pub fn compile_model(
    sources: Vec<(ComponentId, Arc<dyn TensorSource>)>,
    options: &ExecutionOptions,
) -> Result<(CompiledModel, QwenRunner), CompileModelError>;
```

- `QwenRunner` is the existing concrete `Qwen3Model`/`Qwen35Model` dispatch enum, not a backend abstraction. `compile_model` always registers CPU discovery, derives required backends from parsed positive targets, lazily registers/discovers only those providers, injects default `component:layer=cpu0@1` rules for loaded components without a rule, constructs the selected Qwen model and its requirements, compiles the plan, opens resident sessions once, and returns that same parsed runner with the compiled model.

- [ ] **Step 1: Add CLI parser, lazy GPU, capability, and raw/package E2E tests**

```rust
#[test]
fn repeated_placement_flags_preserve_values_for_both_binaries() {
    let args = [
        "--model", "model.gguf",
        "--placement", "llm:layer=cpu0@1,metal0@3",
        "--placement", "vision:layer=cpu0@1",
    ];
    assert_eq!(parse_inference_options(&args).unwrap().placements, vec![
        "llm:layer=cpu0@1,metal0@3",
        "vision:layer=cpu0@1",
    ]);
    assert_eq!(parse_server_options(&args).unwrap().execution.placements.len(), 2);
}

#[test]
fn default_cpu_does_not_discover_or_open_gpu_with_features_enabled() {
    let probes = support::ProviderProbes::default();
    let compiled = compile_fixture_with_probes(&support::tiny_qwen3(), &[], &probes).unwrap();
    assert_eq!(compiled.plan().components[&ComponentId::Llm].primary.as_str(), "cpu0");
    assert_eq!(probes.vulkan_discover.load(Ordering::SeqCst), 0);
    assert_eq!(probes.vulkan_open.load(Ordering::SeqCst), 0);
    assert_eq!(probes.metal_discover.load(Ordering::SeqCst), 0);
    assert_eq!(probes.metal_open.load(Ordering::SeqCst), 0);
}

#[test]
fn gguf_and_ggufrs_cpu_plans_produce_equivalent_logits_and_tokens() {
    let fixture = support::tiny_qwen3_sources();
    let raw = run_short_prompt(&fixture.gguf, &["llm:layer=cpu0@1"]).unwrap();
    let package = run_short_prompt(&fixture.ggufrs, &["llm:layer=cpu0@1"]).unwrap();
    support::assert_close(&raw.logits, &package.logits, 1e-3, 1e-3);
    assert_eq!(raw.tokens, package.tokens);
}
```

`tests/support/mod.rs` defines `ProviderProbes` with public `Arc<AtomicUsize>` counters named `vulkan_discover`, `vulkan_open`, `metal_discover`, and `metal_open`. Its mock providers increment the matching counter in `enumerate`/`open`, so this test checks the real lazy-registration boundary rather than a boolean flag.

Add tests for Qwen3.5 raw/package equivalence, `vision:layer=cpu0@1` success, explicit vision Vulkan/Metal/NPU failure before `open`, explicit unavailable Vulkan/Metal/NPU failure, insufficient capacity, and an unsupported Q4/Q5/Q6 layer failure. Add parser tests proving `--gpu-ratio` is rejected with “use --placement” rather than silently ignored.

- [ ] **Step 2: Run CLI/E2E tests and confirm the binaries do not accept the new contract yet**

Run: `cargo test --test placement_e2e -- --nocapture`

Expected: FAIL because the CLI/server do not collect `--placement`, default compilation is absent, and the server still accepts `--gpu-ratio`.

- [ ] **Step 3: Add strict shared CLI parsing and compile once per process**

Extract small library parsers that consume `&[String]`/`&[&str]` and return errors for unknown flags or missing values; keep `std::env::args` in the binaries. Each `--placement` pushes one string in declaration order. Parse `--kv-cache f16|f32` into the existing two-format `KvCacheType` and carry it through `LlmRequirements` so slot capacity matches runtime storage. Remove `gpu_ratio`, `--gpu-ratio`, `compute::set_device_ratio`, `DeviceKind::Gpu(0)`, the transitional CPU helper from Task 3, and any direct scheduler initialization.

Load `--model` as `llm` and `--mmproj` as `vision`; when a GGUFRS model contains both roles, open both components from the package. `compile_model` builds the catalog, parses the Qwen model/config and its complete `ComponentRequirements`/`LlmLayerSpec` list, compiles the immutable plan, then opens resident sessions; the same parsed Qwen model is returned as the runner, so it does not re-resolve or mutate the frozen plan. The CLI holds one `ExecutionRun` for its inference/interactive sequence. The server holds `Arc<CompiledModel>` and calls `start_run` inside `tokio::task::spawn_blocking`; the deliberate serialized session lock prevents request state contamination while keeping weights resident.

Return placement/registry/catalog/plan/session errors with component, device, and tensor/layer context. Never continue on CPU after an explicit placement error.

- [ ] **Step 4: Add ignored real-model E2E tests for every backend and source format**

The hardware suite reads these exact environment variables:

```text
RMI_QWEN3_GGUF
RMI_QWEN3_GGUFRS
RMI_QWEN35_GGUF
RMI_QWEN35_GGUFRS
RMI_REQUIRE_BACKEND=vulkan|metal
```

One ignored test `explicit_layer_backend_matches_cpu_for_all_model_sources` runs the same short deterministic prompt with temperature 0 and F16 KV for Qwen3/Qwen3.5 × GGUF/GGUFRS. For each model it compares CPU placement with explicit backend Layer placement, asserts chosen tokens are equal, logits satisfy `1e-3` absolute/relative tolerance, upload counts do not change between compile and two generated tokens, and same-device spans obey wait/transfer invariants.

Run on Metal:

```bash
RMI_REQUIRE_BACKEND=metal \
RMI_QWEN3_GGUF=/absolute/path/Qwen3-Q8_0.gguf \
RMI_QWEN3_GGUFRS=/absolute/path/Qwen3-Q8_0.ggufrs \
RMI_QWEN35_GGUF=/absolute/path/Qwen3.5-Q8_0.gguf \
RMI_QWEN35_GGUFRS=/absolute/path/Qwen3.5-Q8_0.ggufrs \
cargo test --features metal --test placement_e2e \
  explicit_layer_backend_matches_cpu_for_all_model_sources -- --ignored --nocapture
```

Run on Vulkan with the same four model paths:

```bash
RMI_REQUIRE_BACKEND=vulkan \
RMI_QWEN3_GGUF=/absolute/path/Qwen3-Q8_0.gguf \
RMI_QWEN3_GGUFRS=/absolute/path/Qwen3-Q8_0.ggufrs \
RMI_QWEN35_GGUF=/absolute/path/Qwen3.5-Q8_0.gguf \
RMI_QWEN35_GGUFRS=/absolute/path/Qwen3.5-Q8_0.ggufrs \
cargo test --features vulkan --test placement_e2e \
  explicit_layer_backend_matches_cpu_for_all_model_sources -- --ignored --nocapture
```

Expected: PASS on required hardware. Missing paths or an explicitly required backend are test failures, not skips.

- [ ] **Step 5: Extend benchmarks and document the exact user/runtime contract**

Extend `micro-bench` with `--model`, repeatable `--placement`, `--prompt`, `--max-tokens`, `--samples`, and `--kv-cache`. For each sample print machine-readable `BENCH:` rows for prefill tokens/s, decode tokens/s, resident bytes, weight upload count/bytes, activation H2D/D2H bytes, submissions, and host waits. The reporting command is:

```bash
for backend in cpu vulkan metal; do
  case "$backend" in
    cpu) placement='llm:layer=cpu0@1' ;;
    vulkan) placement='llm:layer=vulkan0@1' ;;
    metal) placement='llm:layer=metal0@1' ;;
  esac
  cargo run --release --features gpu --bin micro-bench -- \
    --model /absolute/path/Qwen3-Q8_0.gguf \
    --placement "$placement" \
    --prompt '2 + 3 =' --max-tokens 32 --kv-cache f16 --samples 5
done
```

Do not impose a throughput threshold. `README.md` documents syntax/defaults/examples/errors; `ARCHITECTURE.md` documents catalog → compiler → compiled sessions and Row versus Layer sync; `OPTIMIZATION.md` documents residency counters and fair five-sample medians; `GGUFRS.md` documents logical equivalence and component routing.

- [ ] **Step 6: Run the complete portable matrix and static old-path audit**

Run on macOS:

```bash
cargo fmt --all -- --check
cargo build --all-targets
cargo test --all-targets
cargo build --all-targets --features vulkan
cargo test --all-targets --features vulkan
cargo build --all-targets --features metal
cargo test --all-targets --features metal
cargo build --all-targets --features gpu
cargo test --all-targets --features gpu
```

Expected: every command exits 0; hardware tests remain ignored unless explicitly selected.

Run from a host with the target installed:

```bash
RUSTFLAGS='' cargo check --target x86_64-unknown-linux-gnu --all-targets --features gpu
```

Expected: PASS and Metal is not compiled or linked.

Audit removed primary paths:

```bash
if rg -n -- 'gpu_ratio|Scheduler|DeviceRatio|WorkSpec|set_device_ratio|enable_gpu' src; then
  exit 1
fi
```

Expected: no matches. Also run the two shader `mktemp`/`cmp` freshness checks from Tasks 7 and 10.

- [ ] **Step 7: Run hardware lifecycle invariants and collect five-sample reports**

Run `explicit_layer_backend_matches_cpu_for_all_model_sources` for Metal and Vulkan with the commands in Step 4. Run Row-mode hardware tests with unequal two-target plans (`cpu0@1,backend0@3`) and a three-target mock plan, then assert exact row ownership, zero token-time weight allocation/upload, and submit-before-wait ordering. Finally run the Step 5 benchmark command and save raw console output in the change/PR notes; do not commit machine-specific numbers as universal baselines.

Expected: all correctness/invariant tests PASS. Benchmark output contains five samples plus prefill/decode medians and lifecycle counters for each available backend.

- [ ] **Step 8: Commit the integrated product path and documentation**

```bash
git add src/main.rs src/bin/server.rs src/bin/micro_bench.rs src/compute/mod.rs src/lib.rs README.md ARCHITECTURE.md OPTIMIZATION.md GGUFRS.md tests/placement_e2e.rs tests/execution_lifecycle.rs tests/gpu_backends.rs tests/qwen_gpu_layers.rs tests/support/mod.rs
git commit -m "feat: integrate multi-device q8 execution"
```

## Final Self-Review Checklist

- [ ] Spec coverage: Tasks 1–4 cover CLI grammar, normalization, arbitrary logical devices, GGUF/GGUFRS cataloguing, weighted Row/Layer planning, component routing, capacity, and pre-allocation errors.
- [ ] Spec coverage: Tasks 5–8 cover compiled lifecycle, resident weights, preallocated buffers, instrumentation, CPU reference, Vulkan Row, Metal Row, lazy discovery, and NPU interfaces without an NPU runtime.
- [ ] Spec coverage: Tasks 9–12 route every Qwen3/Qwen3.5 Q8_0 matrix and implement complete Qwen3, Qwen3.5 dense, and Qwen3.5 recurrent Layer execution with resident state.
- [ ] Spec coverage: Task 13 covers CLI/server integration, default CPU behavior, vision capability failure, old scheduler removal, raw/package E2E, platform builds, documentation, and five-sample benchmarks.
- [ ] Lifecycle: compilation, not request start, opens sessions and uploads weights; repeated requests reuse resident weights, and the initial serialized-run lock prevents mutable state sharing.
- [ ] Synchronization: Row tests prove every submit precedes the first wait; Layer tests prove embedding chains into a same-device first span without host readback, no host wait occurs within a same-device span, and only hidden activation transfers at boundaries.
- [ ] Platform: Metal remains target-specific and runtime-compiles MSL; Vulkan builds from checked-in SPIR-V and both shader artifacts have reproducible freshness checks.
- [ ] Error policy: explicit unavailable/unsupported/capacity errors happen before allocation; a stateful runtime failure poisons the run and never silently falls back.
- [ ] Type consistency: `ComponentId`, `PlacementMode`, `DeviceId`, `TensorId`, `SlotId`, `SlotPlan`, `ProgramId`, `FenceId`, `LlmLayerSpec`, `DeviceDescriptor`, `DevicePlan`, `ProgramPlan`, `ProgramBinding`, `ActivationTransfer`, `ExecutionPlan`, `SessionStats`, `DeviceDiscovery`, `DeviceProvider`, `DeviceSession`, `CompiledModel`, and `ExecutionRun` use the same spelling and ownership in every task.
- [ ] Layer finalization: Task 4 compiles a primary `FinalNormQ8Logits` binding, aliases a same-primary last-span output to its input or transfers only hidden activation otherwise, Task 5 exposes `execute_logits`, and Tasks 9–10 never query Row shards for Layer logits.
- [ ] Row primary: this delivery rejects non-CPU Row primaries before `open`; Q8 matrices still use submit-all-before-wait while existing host code owns only non-Q8 operators and state.
- [ ] Physical ownership: selected `physical_key` values are unique, so every physical device has exactly one `DevicePlan`, arena, resident upload set, and session.
- [ ] Placeholder scan: construct the forbidden phrases without embedding them literally, then require no matches:

```bash
bad='T''BD|TO''DO|implement la''ter|fill in de''tails|appropriate error hand''ling|similar to Ta''sk|write tests for the ab''ove'
! rg -n "$bad" docs/superpowers/plans/2026-08-12-multi-device-vulkan-metal-execution.md
```
- [ ] Scope: stage only files named by each task; never stage `.codegraph/`, and never push without explicit authorization.
