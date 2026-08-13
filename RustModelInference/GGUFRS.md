# GGUFRS v1

GGUFRS is a RustModelInference package for model management and device-independent loading. It bundles exactly one LLM GGUF and optionally one mmproj GGUF while preserving component metadata and original tensor bytes. It is not readable by llama.cpp and does not replace ordinary GGUF interchange.

All integers are little-endian. Offsets and byte lengths are `u64`; counts and stable IDs are `u32`. Strings are `u64 byte_length` followed by UTF-8 bytes. GGUF metadata and GGML tensor type numeric codes are reused.

## Physical order

```text
128-byte superblock
component table
component-scoped metadata table
segment table
tensor table
zero alignment padding
64 KiB-aligned tensor segments
```

No component or directory is appended after tensor data. The last segment ends at the declared file size.

## Superblock

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | magic `b"GGUFRS\0\0"` |
| 8 | 4 | version, `1` |
| 12 | 4 | flags, `0` in v1 |
| 16 | 8 | declared file size |
| 24 | 4 | component count |
| 28 | 4 | metadata count |
| 32 | 4 | segment count |
| 36 | 4 | tensor count |
| 40 | 8 | component table offset |
| 48 | 8 | component table length |
| 56 | 8 | metadata table offset |
| 64 | 8 | metadata table length |
| 72 | 8 | segment table offset |
| 80 | 8 | segment table length |
| 88 | 8 | tensor table offset |
| 96 | 8 | tensor table length |
| 104 | 8 | tensor-data offset |
| 112 | 16 | reserved zero bytes |

Readers reject unsupported versions, nonzero flags/reserved bytes, unordered or noncontiguous tables, nonzero table padding, invalid ranges, appended data, and a declared size different from the actual file size.

## Component table

Each entry is:

```text
u32 component_id
u32 role                 # 1 = LLM, 2 = MMPROJ
string name              # canonical "llm" or "mmproj"
u32 metadata_start
u32 metadata_count
u32 tensor_start
u32 tensor_count
u32 segment_start
u32 segment_count
```

V1 requires exactly one LLM and at most one mmproj. Components are ordered by role then UTF-8 name bytes; IDs are their table indices.

## Scoped metadata table

Each entry is:

```text
u32 component_id
string key
i32 GGUF value_type
typed GGUF value
```

Array encoding is `i32 element_type`, `u64 count`, then homogeneous values. Metadata is sorted by component and key bytes. Duplicate keys inside one component are invalid; identical keys in different components remain independent.

## Segment table

Each 72-byte entry is:

```text
u32 segment_id
u32 component_id
u32 kind                 # 1 = shared, 2 = layer, 3 = component
i32 layer                # layer index, or -1
u64 absolute_offset
u64 stored_length
u32 tensor_start
u32 tensor_count
u8 sha256[32]
```

The LLM has one shared segment and one segment for every layer. The mmproj has one component segment. Segment starts and stored lengths are multiples of 64 KiB and segments are contiguous. SHA-256 covers the complete stored segment, including inter-tensor and trailing zero padding. A segment can therefore be verified, mapped, and released independently.

## Tensor table and bytes

Each entry is:

```text
u32 component_id
u32 segment_id
string tensor_name
i32 GGML type
u32 rank
u64 dims[rank]
u64 offset_within_segment
u64 exact_byte_length
```

Tensors are sorted by name bytes inside each segment. Offsets use `max(32, general.alignment)` for that component. Shapes, quantization block sizes, ranges, and overlaps are validated before mapping.

The exporter copies `GGUFLoader::tensor_slice(name)` directly. It never dequantizes, requantizes, repacks, or converts tensor data through floating point. Identical source bytes and options therefore produce byte-identical packages; source paths, timestamps, host devices, and temporary names are not serialized.

## Export and publication

```bash
cargo run --release --bin ggufrs -- \
  export \
  --llm model.gguf \
  --mmproj mmproj.gguf \
  --output model.ggufrs
```

`--mmproj` is optional. The default never replaces an output. `--overwrite` requests atomic replacement. Export writes a unique file in the output directory, retains and syncs its `create_new` handle, verifies every segment through a clone of that handle and the production reader, then publishes it. Unsupported atomic publication returns an error and never deletes the destination first.

## Runtime and load planning

`TensorSource` is the common read-only interface for GGUF and a loaded GGUFRS component. Runtime format selection uses file magic, not the extension. An explicit `--mmproj` overrides the bundled component.

`LayerSplit` keeps each layer segment whole and assigns contiguous layer ranges to caller-provided logical devices. Shared and mmproj tensors stay on the declared primary device. `TensorSplit` may divide a tensor only between complete rows; quantized rows must contain complete quantization blocks. Capacity counts tensor payload, not table or padding bytes.

GGUF and GGUFRS expose the same logical `TensorSource` contract. `compile_model`
routes the LLM component and an optional bundled mmproj component into one
catalog, then applies the same placement rules to either source format. An
explicit standalone `--mmproj` is a vision source; a package supplies it when
present. Vision is currently CPU-only, so explicit Metal/Vulkan/NPU vision
placement fails during planning before a device session opens.
