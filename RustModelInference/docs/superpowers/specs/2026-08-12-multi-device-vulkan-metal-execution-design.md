# Multi-Device Vulkan and Metal Execution Design

**Date:** 2026-08-12
**Status:** Approved
**Base:** `MultiDev` at `b454d45c4eb6f4b3134022185f711e780cb99b35`

## Summary

Replace the current copy-per-matmul GPU prototype with one placement and execution system for arbitrary logical CPU, Vulkan GPU, Metal GPU, and future NPU devices. The system compiles command-line placement rules into an immutable execution plan, uploads selected weights once, preallocates device memory, and executes either row-sharded Q8_0 matrix multiplications or complete device-resident Qwen3/Qwen3.5 transformer layers.

No placement option means CPU-only execution. NPU discovery and session interfaces are included, but no NPU driver is implemented in this delivery. CLIP/vision can be represented and validated as a component, but Vulkan and Metal vision kernels are outside this delivery.

## Existing Problems

The `MultiDev` baseline has two disconnected systems:

- `load_plan` already models arbitrary logical device IDs, capacities, layer placement, tensor row placement, and `.ggufrs` segment ownership, but it only loads logical CPU mappings and is not connected to inference execution.
- `compute::Scheduler` creates `CpuDevice(0)` and at most `GpuDevice(0)`. `DeviceRatio` only decides whether a device is active; execution then splits output rows equally and ignores configured ratios.

The Vulkan path is a compileable prototype rather than a usable offload implementation:

- Only the Q8_0 embedding projection in `src/main.rs` calls the scheduler. Ordinary Qwen3 generation and all Qwen3.5 projections remain on the CPU.
- Each matrix multiplication allocates Vulkan buffers, uploads weights and activation data, submits one command, waits for the queue, reads the result, and frees every resource.
- Weights are not resident, scratch memory is not preallocated, and the host synchronizes at every call.
- The current shader does not implement the complete GGML Q8_0 row layout and has no end-to-end device correctness coverage.

Adding Metal beside that path without changing ownership would reproduce the same defects. This design makes the existing load plan the single source of placement truth and replaces the temporary `WorkSpec` execution path.

## Goals

- Discover and register any number of logical CPU, Vulkan GPU, and Metal GPU devices with stable IDs such as `cpu0`, `vulkan0`, and `metal0`.
- Reserve provider and session interfaces for future NPU devices without implementing an NPU runtime.
- Accept both raw `.gguf` inputs and packaged `.ggufrs` inputs.
- Configure placement only through repeatable command-line `--placement` arguments.
- Support both contiguous transformer-layer placement and Q8_0 output-row sharding, including normalized device weights.
- Allow different components to have independent placements.
- Keep device weights resident for the lifetime of the compiled model.
- Preallocate weight, activation, scratch, KV-cache, recurrent-state, result, and synchronization resources before inference.
- Run complete Qwen3 and Qwen3.5 dense/recurrent layers on Vulkan or Metal in layer mode, with state resident on the owning device.
- Route every Q8_0 matrix multiplication used by Qwen3, Qwen3.5, embeddings, and output logits through the compiled execution plan.
- Preserve existing CPU-only behavior when no placement is supplied.
- Fail explicitly when a requested device, component, tensor format, capacity, or kernel is unsupported.

## Non-Goals

- No CoreML, Apple Neural Engine, RKNN, or other concrete NPU backend.
- No Vulkan or Metal CLIP/vision kernels. The component can be catalogued and planned, but a GPU placement request fails capability validation.
- No general-purpose graph compiler or graph optimization IR.
- No Q4_K, Q5_K, or Q6_K GPU matrix kernels in this delivery. A layer containing an unsupported matrix format cannot be placed on a GPU.
- No implicit GPU selection. GPU initialization and use require an explicit placement.
- No silent runtime CPU fallback after a plan has started executing.
- No unmeasured performance threshold.

## Core Model

### Tensor Catalog

`TensorCatalog` presents the same component and tensor contract for both file formats:

- A raw `.gguf` passed as `--model` becomes the logical `llm` component.
- A raw `.gguf` passed as `--mmproj` becomes the logical `vision` component.
- A `.ggufrs` file exposes its `Llm` component as `llm` and its `Mmproj` component as `vision`.
- Each catalog entry contains the component, tensor name, shape, GGML type, byte range, layer index when applicable, row layout, and source lifetime.

CPU placements retain borrowed mmap slices. GPU placements upload only the whole tensor or row slice assigned by the plan. `.ggufrs` continues to use its validated segment mappings; a raw `.gguf` uses equivalent virtual component and segment records over its mmap.

### Device Registry

`DeviceRegistry` owns the discovered device descriptors and provider factories. A descriptor contains:

- Stable device ID and backend kind.
- Human-readable adapter name.
- Usable memory budget and maximum allocation size.
- Supported components, operations, tensor formats, and execution modes.
- Queue and unified-memory properties needed by the placement compiler.

The CPU provider registers one logical device per usable NUMA node on platforms that expose NUMA topology, and otherwise registers `cpu0`. Each logical CPU owns its execution pool rather than sharing a global pool. Vulkan and Metal providers enumerate every compatible adapter as `vulkanN` and `metalN` in provider enumeration order. NPU providers implement the same discovery interface later; the registry itself does not contain NPU-specific branches.

Duplicate IDs are rejected. Device IDs are resolved before any model allocation, so an unavailable backend produces a plan error rather than a partial model load.

### Placement Compiler

`PlacementCompiler` combines the tensor catalog, device registry, placement arguments, model dimensions, context/KV requirements, and the existing `load_plan` validation rules. It emits an immutable `ExecutionPlan` containing:

- One component plan per loaded component.
- The primary device for shared tensors, embeddings, final normalization, and output head.
- Contiguous device-owned layer spans or per-matrix row shards.
- Resident tensor allocations and their source byte ranges.
- Per-device KV cache, recurrent state, scratch, activation, result, and staging allocations.
- Cross-device activation-transfer edges.
- Backend kernel and pipeline requirements.

The compiler validates all capabilities and memory before opening device sessions. Allocation or upload failure rolls back the partially constructed compiled model through RAII.

### Device Sessions

Each selected physical device has one long-lived `DeviceSession`. The session owns its device/queue handles, memory arenas, resident tensor handles, pipeline cache, reusable command resources, and synchronization objects.

The device contract has two execution entry points:

- Row execution consumes a precompiled Q8_0 shard, an activation handle, and a fixed output range.
- Layer execution consumes a precompiled Qwen3 or Qwen3.5 layer and device-resident state, and produces the next hidden activation.

CPU, Vulkan, and Metal implement the same contract. The future NPU backend plugs into provider discovery, capability reporting, model compilation, and execution without changing placement syntax.

### Compiled Model

`CompiledModel` owns the execution plan and all selected device sessions. It replaces the global mutable scheduler in the inference hot path. Its layer and tensor handles remain valid until the compiled model is dropped.

The existing `Scheduler`, `DeviceRatio`, and copy-owning `WorkSpec` are removed from the primary path. `--gpu-ratio` is removed because it cannot identify a backend, component, split policy, or multiple devices. Users express the same intent with `--placement`.

## Command-Line Contract

Both the inference CLI and server accept repeatable placement arguments:

```text
--placement COMPONENT:MODE=DEVICE@WEIGHT[,DEVICE@WEIGHT...]
```

Supported component names in this delivery are:

- `llm`: Qwen3 or Qwen3.5 model component.
- `vision`: CLIP/mmproj component.

Supported modes are:

- `layer`: distribute complete component layers.
- `row`: distribute the output rows of every supported Q8_0 matrix.

Examples:

```bash
--placement llm:layer=cpu0@1,metal0@3
--placement llm:row=cpu0@1,vulkan0@3
--placement vision:layer=cpu0@1
```

Weights are finite, non-negative decimal numbers parsed by the standard library. They do not have to sum to 100. The compiler divides every weight by the sum of the component's weights. Negative, non-finite, malformed, or all-zero weights are rejected. An individual zero-weight target is ignored. Duplicate targets, duplicate component rules, unknown components, unknown devices, and unsupported modes are rejected.

The first positive target in a component rule is its primary device. Declaration order also breaks equal largest-remainder ties, making every plan deterministic.

Any loaded component without an explicit rule defaults to `cpu0`. When no placement is supplied, every component therefore uses the existing CPU-only behavior. Merely compiling Vulkan or Metal support never initializes a GPU.

## Placement Semantics

### Layer Mode

For a component with `N` layers, each positive target receives the floor of its normalized quota. Remaining layers are assigned by descending fractional remainder, with declaration order as the tie-breaker. Each target receives one contiguous layer span in declaration order.

A positive target that would receive zero layers makes the plan invalid. This prevents a placement from appearing active while doing no work. Shared, rank-one, embedding, final-normalization, and output-head tensors reside on the primary device. The embedding output chains directly into the first layer span when that span is on primary; otherwise it crosses one hidden-only transfer boundary, without an unconditional host readback.

Every transformer layer is indivisible in layer mode:

- All layer weights are resident on one device.
- Qwen3 KV cache and Qwen3.5 KV/recurrent state for that layer reside on the same device.
- Norm, matrix, positional, attention, activation, residual, and state-update operations run on that device.
- Consecutive layers on the same device remain queued without a host wait.
- When ownership changes, only the hidden activation crosses the boundary.
- The output of the last layer moves to the primary device only when final normalization/output logits require it.

Host waits are limited to backend transitions, final host-visible output, and explicit diagnostics. Queue ordering within one device does not create a host synchronization point.

### Row Mode

Every supported Q8_0 matrix is split into contiguous output-row shards using the same normalized quota and largest-remainder rule. Shards align to complete GGML rows; no 34-byte Q8_0 block is split. Every positive target must receive at least one complete output row for every routed matrix, otherwise plan compilation fails.

Each target stores only its assigned weight rows. The primary device owns non-Q8 operators and the component's KV/recurrent state. The input activation is broadcast from the primary device to the participating devices, devices execute concurrently, and results land in preallocated shard result slots associated with their final output offsets. The coordinator gathers or exposes those ranges on the primary device at the dependent operation boundary without allocating a new weight or result buffer.

In this delivery, Row mode requires a CPU primary because the existing Qwen runners own the non-Q8 operators and KV/recurrent state on the host. A Row rule whose first positive target is Vulkan, Metal, or NPU is rejected during plan compilation, before any session is opened. This restriction does not affect Q8_0 row sharding: every Q8_0 matrix, including the output head, still submits all device shards before the first wait. A future non-CPU Row primary requires a fixed primary-stage program contract rather than an implicit host fallback.

Row mode necessarily synchronizes at each dependent matrix result and transfers activation/result data more frequently than layer mode. It exists for heterogeneous capacity sharing and experiments; layer mode is the preferred performance path.

### Component Routing

Each component has an independent placement rule and mode. `llm` placement is fully executable in this delivery. `vision` placement participates in parsing, device resolution, capacity planning, and capability validation.

Because CLIP/vision GPU kernels are not included, `--placement vision:...` targeting Vulkan, Metal, or NPU fails with a clear unsupported-component error. It does not fall back to CPU. A CPU vision placement remains valid.

## Resident Memory and Loading

Model compilation has four phases:

1. Build and validate the tensor catalog.
2. Compile placement and calculate exact per-device memory requirements.
3. Open selected device sessions, allocate arenas, create pipelines, and upload resident weights once.
4. Publish the `CompiledModel` only after every device is ready.

The memory estimate includes resident weights, quantized activations and scales, hidden activations, row results, KV cache, Qwen3.5 recurrent state, scratch space, staging ranges, command resources, and required alignment. Capacity failure identifies the device, required bytes, available bytes, and largest allocations.

Vulkan chooses host-visible staging plus device-local resident buffers where available. Metal selects shared or private storage according to the adapter's unified-memory properties. Both expose the same logical handles and preserve the same lifetime rules.

The inference hot path may update activation, KV, recurrent, and output buffers, but it does not allocate, free, or upload weight buffers. A debug statistics snapshot records resident bytes, weight allocations, weight uploads, activation-transfer bytes, queue submissions, and host waits.

## Backend Implementation

Cargo features are separated by backend:

- `vulkan` enables `ash` and the Vulkan provider.
- `metal` enables the target-specific Metal dependency and provider on macOS.
- `gpu` remains a compatibility feature enabling the GPU providers available for the build target.

Non-macOS builds do not compile or link Metal code. A macOS build may enable both Vulkan and Metal when both runtimes are available, and the registry exposes them as separate selectable devices.

### Q8_0 Representation

Resident Q8_0 weights preserve the GGML row layout: each block is a little-endian F16 scale followed by 32 signed quantized bytes, for 34 bytes per block. Row stride is `(n_in / 32) * 34` bytes.

Layer-mode activations stay in F32 device memory until a matrix requires them. A device quantization kernel writes signed Q8 values and F32 activation scales into preallocated buffers. The Q8_0 matrix kernel multiplies each resident weight block by the matching activation block and accumulates into the designated output row. Weight row offsets are part of the compiled tensor handle, not recalculated from copied vectors.

### Vulkan

Vulkan retains `ash`, replaces the current per-call object lifecycle, and uses one session per selected physical adapter. It creates reusable descriptor/pipeline layouts, command pools, command buffers, fences, and aligned arenas during model compilation. GLSL compute sources are kept beside their checked-in SPIR-V artifacts.

### Metal

Metal uses a target-specific Rust Metal binding and native MSL compute kernels. It creates one command queue, compiled library, pipeline cache, and reusable buffers per selected adapter during model compilation. MSL sources are embedded and compiled when the session is created; pipeline creation never occurs in the token loop.

### Layer Kernels

Vulkan and Metal implement equivalent kernels and dispatch sequences for the operations required by Q8_0 Qwen3 and Qwen3.5 models:

- F32 to Q8_0 activation quantization.
- Q8_0 matrix multiplication for embedding/output, Q/K/V/O, FFN gate/up/down, and Qwen3.5 dense/recurrent projections.
- RMSNorm and Q/K norm.
- RoPE and MRoPE.
- Attention score, padded softmax, attention value, and KV-cache update/read.
- SiLU/SwiGLU, vector multiplication, residual addition, and scaling.
- Qwen3.5 recurrent convolution/state update and gating operations.

F32/F16 auxiliary tensors required by those layers are resident and consumed by the relevant vector kernels. A requested GPU layer containing a Q4_K, Q5_K, Q6_K, or otherwise unsupported matrix is rejected before allocation.

## Error and Fallback Policy

Errors before execution include invalid placement syntax, unavailable features or adapters, duplicate or unknown targets, unsupported component/format/operator combinations, invalid row boundaries, and insufficient capacity. They are returned with component, device, and tensor/layer context.

CPU fallback is a placement decision, not an exception handler:

- No placement means CPU-only.
- An explicit CPU target executes its assigned work on CPU.
- An explicit unavailable or unsupported GPU/NPU target fails plan compilation.
- A device failure after stateful inference begins terminates that inference. It never retries a partially updated layer on CPU.

All session and partial-model resources are released through RAII after an error.

## Verification

### Portable Checks

- Default `cargo build --all-targets` and `cargo test --all-targets` remain passing.
- Vulkan builds and tests under `--features vulkan`.
- Metal builds and tests under `--features metal` on macOS.
- The compatibility `gpu` feature builds on macOS and a non-macOS target.
- CPU-only behavior is covered with GPU features both disabled and enabled.

### Planner and Lifecycle Tests

- CLI parsing covers repeated rules, automatic normalization, decimals, zero targets, malformed/negative/non-finite weights, duplicates, and deterministic tie-breaking.
- Layer and row allocations cover two or more CPU/GPU device IDs, unequal weights, insufficient layers/rows, capacity errors, and exact coverage without gaps or overlap.
- `.gguf` virtual components and `.ggufrs` real components produce equivalent tensor placement contracts.
- Mock sessions prove that every resident weight is allocated and uploaded exactly once, and that token execution performs no weight allocation/upload.
- Unsupported NPU and vision GPU placements fail before session allocation.

### Kernel and Model Correctness

- Each Vulkan and Metal kernel is compared with the CPU reference on deterministic fixtures, including tails, zero input, negative values, multiple rows, and multiple Q8_0 blocks.
- Kernel values satisfy `abs(actual - expected) <= 1e-4 + 1e-4 * abs(expected)`.
- Row-sharded output is compared with unsplit CPU Q8_0 output for unequal two- and three-device plans.
- A complete Qwen3 layer, Qwen3.5 dense layer, and Qwen3.5 recurrent layer are compared with CPU using `1e-3` absolute and relative tolerance.
- Short-prompt Qwen3 and Qwen3.5 logits and selected tokens are compared across CPU, Vulkan, and Metal for both `.gguf` and `.ggufrs`; selected tokens must match and logits use the layer tolerance.
- A tolerance change requires a recorded hardware fixture and explanation; tests are not relaxed silently.

### Runtime Invariants and Benchmarking

On real Vulkan and Metal hardware, instrumentation verifies:

- Device enumeration returns the selected adapter.
- Weight upload count is unchanged after model compilation.
- Token execution performs zero weight allocations and uploads.
- A same-device layer span causes no host wait between its layers.
- Layer-mode host waits are bounded by backend transitions plus final output readback.
- Row-mode row ownership and configured normalized proportions match the compiled plan.

CPU, Vulkan, and Metal benchmarks use the same model, prompt/prefill length, generated-token count, KV format, and sampling controls. At least five samples are reported using medians, with prefill and decode rates separated. Transfer bytes, queue submissions, and host waits accompany throughput results. Performance is reported rather than gated until stable device-specific baselines exist.

## Delivery Boundary

The delivery is complete when:

- Both Vulkan and Metal can execute Q8_0 Qwen3 and Qwen3.5 end to end in explicit layer placement.
- All Q8_0 matrices in Qwen3/Qwen3.5 generation, embedding, and logits are routed through the compiled plan.
- Row placement honors arbitrary normalized weights across multiple registered devices.
- Layer placement keeps weights and state resident and transfers only hidden activations at device boundaries.
- Raw `.gguf` and `.ggufrs` inputs pass equivalent placement and inference coverage.
- Default execution remains CPU-only.
- NPU and vision GPU requests fail explicitly at the documented capability boundary.
