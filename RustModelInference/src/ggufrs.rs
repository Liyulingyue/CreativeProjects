use crate::model::{
    ByteReader, GGMLType, GGUFLoader, MetaValue, MetaValueType, TensorInfo, TensorSource,
};
use memmap2::{Mmap, MmapOptions};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub const GGUFRS_VERSION: u32 = 1;
pub const GGUFRS_SEGMENT_ALIGNMENT: u64 = 64 * 1024;
const GGUFRS_MAGIC: &[u8; 8] = b"GGUFRS\0\0";
const SUPERBLOCK_LEN: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ComponentRole {
    Llm = 1,
    Mmproj = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SegmentKind {
    Shared = 1,
    Layer = 2,
    Component = 3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentInfo {
    pub id: u32,
    pub role: ComponentRole,
    pub name: String,
    pub metadata_range: Range<u32>,
    pub tensor_range: Range<u32>,
    pub segment_range: Range<u32>,
}

#[derive(Debug)]
pub enum GgufrsError {
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    InvalidFormat {
        context: String,
    },
    SourceGguf {
        role: ComponentRole,
        path: PathBuf,
        message: String,
    },
    ChecksumMismatch {
        component_id: u32,
        segment_id: u32,
        expected: [u8; 32],
        actual: [u8; 32],
    },
    OutputExists {
        path: PathBuf,
    },
    CapacityExceeded {
        device_id: String,
        required: u64,
        available: u64,
        context: String,
    },
    UnsplittableTensor {
        component_id: u32,
        tensor: String,
        row_bytes: u64,
        remaining: Vec<(String, u64)>,
        reason: String,
    },
    InvalidPlan {
        context: String,
    },
    UnsupportedPublish {
        path: PathBuf,
        operation: &'static str,
        source: io::Error,
    },
}

impl fmt::Display for GgufrsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                operation,
                path,
                source,
            } => write!(formatter, "{operation} {}: {source}", path.display()),
            Self::InvalidFormat { context } => write!(formatter, "invalid ggufrs: {context}"),
            Self::SourceGguf {
                role,
                path,
                message,
            } => write!(
                formatter,
                "failed to load source GGUF {:?} {}: {message}",
                role,
                path.display()
            ),
            Self::ChecksumMismatch {
                component_id,
                segment_id,
                expected,
                actual,
            } => write!(
                formatter,
                "checksum mismatch for component {component_id} segment {segment_id}: expected {expected:02x?}, actual {actual:02x?}"
            ),
            Self::OutputExists { path } => {
                write!(formatter, "output already exists: {}", path.display())
            }
            Self::CapacityExceeded {
                device_id,
                required,
                available,
                context,
            } => write!(
                formatter,
                "device {device_id} capacity exceeded for {context}: required {required}, available {available}"
            ),
            Self::UnsplittableTensor {
                component_id,
                tensor,
                row_bytes,
                remaining,
                reason,
            } => write!(
                formatter,
                "component {component_id} tensor {tensor} cannot be split: row bytes {row_bytes}, remaining capacities {remaining:?}: {reason}"
            ),
            Self::InvalidPlan { context } => write!(formatter, "invalid ggufrs plan: {context}"),
            Self::UnsupportedPublish {
                path,
                operation,
                source,
            } => write!(
                formatter,
                "unsupported publish operation {operation} for {}: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for GgufrsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } | Self::UnsupportedPublish { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TableRange {
    offset: u64,
    length: u64,
}

#[derive(Debug)]
struct Superblock {
    declared_file_size: u64,
    component_count: u32,
    metadata_count: u32,
    segment_count: u32,
    tensor_count: u32,
    component_table: TableRange,
    metadata_table: TableRange,
    segment_table: TableRange,
    tensor_table: TableRange,
    tensor_data_offset: u64,
}

struct IndexTables {
    components: Vec<u8>,
    metadata: Vec<u8>,
    segments: Vec<u8>,
    tensors: Vec<u8>,
}

#[derive(Debug, Clone)]
struct ScopedMetadata {
    component_id: u32,
    key: String,
    value: MetaValue,
}

#[derive(Debug, Clone)]
pub(crate) struct SegmentInfo {
    pub id: u32,
    pub component_id: u32,
    pub kind: SegmentKind,
    pub layer: Option<u32>,
    pub absolute_offset: u64,
    pub stored_len: u64,
    pub tensor_range: Range<u32>,
    pub sha256: [u8; 32],
}

#[derive(Debug, Clone)]
pub(crate) struct TensorRecord {
    pub component_id: u32,
    pub segment_id: u32,
    pub info: TensorInfo,
    pub segment_offset: u64,
    pub byte_len: u64,
}

#[derive(Debug)]
struct PackageIndex {
    components: Vec<ComponentInfo>,
    metadata: Vec<ScopedMetadata>,
    segments: Vec<SegmentInfo>,
    tensors: Vec<TensorRecord>,
    component_by_role: BTreeMap<ComponentRole, u32>,
    metadata_lookup: BTreeMap<(u32, String), usize>,
    tensor_lookup: BTreeMap<(u32, String), usize>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExportOptions {
    pub overwrite: bool,
}

struct ExportSource {
    role: ComponentRole,
    path: PathBuf,
    name: &'static str,
    loader: GGUFLoader,
    tensor_alignment: u64,
}

struct PlannedExport {
    index: PackageIndex,
    component_table: TableRange,
    metadata_table: TableRange,
    segment_table: TableRange,
    tensor_table: TableRange,
    tensor_data_offset: u64,
    declared_file_size: u64,
}

#[derive(Clone)]
pub struct GgufrsFile {
    file: Arc<File>,
    path: Arc<PathBuf>,
    index: Arc<PackageIndex>,
}

pub struct LoadedComponent {
    file: Arc<File>,
    path: Arc<PathBuf>,
    index: Arc<PackageIndex>,
    component_id: u32,
    mappings: BTreeMap<u32, Arc<MappedSegment>>,
    tensor_infos: BTreeMap<String, TensorInfo>,
}

pub(crate) struct MappedSegment {
    pub segment_id: u32,
    pub bytes: Mmap,
}

fn invalid(context: impl Into<String>) -> GgufrsError {
    GgufrsError::InvalidFormat {
        context: context.into(),
    }
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    if alignment == 0 {
        return None;
    }
    value
        .checked_add(alignment.checked_sub(1)?)
        .map(|value| value / alignment * alignment)
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_string(out: &mut Vec<u8>, value: &str) -> Result<(), GgufrsError> {
    put_u64(
        out,
        u64::try_from(value.len()).map_err(|_| invalid("string length does not fit u64"))?,
    );
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

fn meta_value_type(value: &MetaValue) -> MetaValueType {
    match value {
        MetaValue::Uint8(_) => MetaValueType::Uint8,
        MetaValue::Int8(_) => MetaValueType::Int8,
        MetaValue::Uint16(_) => MetaValueType::Uint16,
        MetaValue::Int16(_) => MetaValueType::Int16,
        MetaValue::Uint32(_) => MetaValueType::Uint32,
        MetaValue::Int32(_) => MetaValueType::Int32,
        MetaValue::Float32(_) => MetaValueType::Float32,
        MetaValue::Bool(_) => MetaValueType::Bool,
        MetaValue::String(_) => MetaValueType::String,
        MetaValue::Uint64(_) => MetaValueType::Uint64,
        MetaValue::Int64(_) => MetaValueType::Int64,
        MetaValue::Float64(_) => MetaValueType::Float64,
        MetaValue::Array(_, _) => MetaValueType::Array,
    }
}

fn put_meta_value(out: &mut Vec<u8>, value: &MetaValue) -> Result<(), GgufrsError> {
    match value {
        MetaValue::Uint8(value) => out.push(*value),
        MetaValue::Int8(value) => out.push(*value as u8),
        MetaValue::Uint16(value) => out.extend_from_slice(&value.to_le_bytes()),
        MetaValue::Int16(value) => out.extend_from_slice(&value.to_le_bytes()),
        MetaValue::Uint32(value) => put_u32(out, *value),
        MetaValue::Int32(value) => put_i32(out, *value),
        MetaValue::Float32(value) => out.extend_from_slice(&value.to_le_bytes()),
        MetaValue::Bool(value) => out.push(u8::from(*value)),
        MetaValue::String(value) => put_string(out, value)?,
        MetaValue::Uint64(value) => put_u64(out, *value),
        MetaValue::Int64(value) => out.extend_from_slice(&value.to_le_bytes()),
        MetaValue::Float64(value) => out.extend_from_slice(&value.to_le_bytes()),
        MetaValue::Array(element_type, values) => {
            if *element_type == MetaValueType::Array {
                return Err(invalid("metadata arrays cannot contain arrays"));
            }
            for child in values {
                if matches!(child, MetaValue::Array(_, _))
                    || meta_value_type(child) != *element_type
                {
                    return Err(invalid(format!(
                        "metadata array declares {element_type:?} but contains {:?}",
                        meta_value_type(child)
                    )));
                }
            }
            put_i32(out, *element_type as i32);
            put_u64(
                out,
                u64::try_from(values.len())
                    .map_err(|_| invalid("metadata array length does not fit u64"))?,
            );
            for value in values {
                put_meta_value(out, value)?;
            }
        }
    }
    Ok(())
}

fn source_error(source: &ExportSource, message: impl Into<String>) -> GgufrsError {
    GgufrsError::SourceGguf {
        role: source.role,
        path: source.path.clone(),
        message: message.into(),
    }
}

fn layer_index(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("blk.")?;
    let (index, suffix) = rest.split_once('.')?;
    if suffix.is_empty() {
        return None;
    }
    index.parse().ok()
}

fn load_export_source(role: ComponentRole, path: &Path) -> Result<ExportSource, GgufrsError> {
    let loader = GGUFLoader::from_file(path).map_err(|message| GgufrsError::SourceGguf {
        role,
        path: path.to_path_buf(),
        message,
    })?;
    let source = ExportSource {
        role,
        path: path.to_path_buf(),
        name: match role {
            ComponentRole::Llm => "llm",
            ComponentRole::Mmproj => "mmproj",
        },
        tensor_alignment: 32,
        loader,
    };
    let alignment = source
        .loader
        .metadata("general.alignment")
        .map(|value| {
            value
                .to_u64()
                .ok_or_else(|| source_error(&source, "general.alignment is not an integer"))
        })
        .transpose()?
        .unwrap_or(32);
    if alignment == 0 || !alignment.is_power_of_two() {
        return Err(source_error(
            &source,
            format!("general.alignment {alignment} is not a nonzero power of two"),
        ));
    }

    match role {
        ComponentRole::Llm => {
            let architecture = source
                .loader
                .metadata("general.architecture")
                .and_then(MetaValue::to_string_val)
                .ok_or_else(|| source_error(&source, "missing or invalid general.architecture"))?;
            if !matches!(architecture, "qwen2" | "qwen3" | "qwen35" | "llama") {
                return Err(source_error(
                    &source,
                    format!("unsupported general.architecture {architecture}"),
                ));
            }
            let block_key = format!("{architecture}.block_count");
            let block_count = source
                .loader
                .metadata(&block_key)
                .and_then(MetaValue::to_u64)
                .and_then(|value| u32::try_from(value).ok())
                .filter(|value| *value != 0 && *value <= i32::MAX as u32)
                .ok_or_else(|| source_error(&source, format!("missing or invalid {block_key}")))?;
            let required_tensors = u64::from(block_count).checked_add(1).ok_or_else(|| {
                source_error(&source, format!("{block_key} tensor count overflow"))
            })?;
            let source_tensors = u64::try_from(source.loader.n_tensors())
                .map_err(|_| source_error(&source, "source tensor count does not fit u64"))?;
            if required_tensors > source_tensors {
                return Err(source_error(
                    &source,
                    format!(
                        "{block_key}={block_count} requires at least {required_tensors} tensors, source has {source_tensors}"
                    ),
                ));
            }
        }
        ComponentRole::Mmproj => {
            for key in [
                "clip.vision.projection_dim",
                "clip.vision.image_size",
                "clip.vision.patch_size",
                "clip.vision.embedding_length",
                "clip.vision.feed_forward_length",
                "clip.vision.block_count",
                "clip.vision.attention.head_count",
            ] {
                if source
                    .loader
                    .metadata(key)
                    .and_then(MetaValue::to_u64)
                    .is_none()
                {
                    return Err(source_error(&source, format!("missing or invalid {key}")));
                }
            }
            let epsilon = "clip.vision.attention.layer_norm_epsilon";
            if source
                .loader
                .metadata(epsilon)
                .and_then(MetaValue::to_f64)
                .is_none()
            {
                return Err(source_error(
                    &source,
                    format!("missing or invalid {epsilon}"),
                ));
            }
            for tensor in ["v.patch_embd.weight", "mm.0.weight", "mm.2.weight"] {
                if source.loader.tensor_info(tensor).is_none() {
                    return Err(source_error(
                        &source,
                        format!("missing required tensor {tensor}"),
                    ));
                }
            }
        }
    }
    Ok(ExportSource {
        tensor_alignment: alignment.max(32),
        ..source
    })
}

fn vec_len_u32<T>(values: &[T], context: &str) -> Result<u32, GgufrsError> {
    u32::try_from(values.len()).map_err(|_| invalid(format!("{context} count does not fit u32")))
}

fn plan_export(sources: &[ExportSource]) -> Result<PlannedExport, GgufrsError> {
    if !matches!(
        sources
            .iter()
            .map(|source| source.role)
            .collect::<Vec<_>>()
            .as_slice(),
        [ComponentRole::Llm] | [ComponentRole::Llm, ComponentRole::Mmproj]
    ) {
        return Err(invalid("export sources must be [LLM] or [LLM, MMPROJ]"));
    }

    let mut index = PackageIndex {
        components: Vec::new(),
        metadata: Vec::new(),
        segments: Vec::new(),
        tensors: Vec::new(),
        component_by_role: BTreeMap::new(),
        metadata_lookup: BTreeMap::new(),
        tensor_lookup: BTreeMap::new(),
    };

    for source in sources {
        let component_id = vec_len_u32(&index.components, "component")?;
        let metadata_start = vec_len_u32(&index.metadata, "metadata")?;
        let mut metadata = source.loader.metadata_entries().to_vec();
        metadata.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
        for pair in metadata.windows(2) {
            if pair[0].0 == pair[1].0 {
                return Err(invalid(format!(
                    "component {} has duplicate metadata key {}",
                    source.name, pair[0].0
                )));
            }
        }
        index
            .metadata
            .extend(metadata.into_iter().map(|(key, value)| ScopedMetadata {
                component_id,
                key,
                value,
            }));
        let metadata_end = vec_len_u32(&index.metadata, "metadata")?;

        let segment_start = vec_len_u32(&index.segments, "segment")?;
        let tensor_start = vec_len_u32(&index.tensors, "tensor")?;
        let groups = match source.role {
            ComponentRole::Llm => {
                let architecture = source
                    .loader
                    .metadata("general.architecture")
                    .and_then(MetaValue::to_string_val)
                    .expect("validated LLM architecture");
                let block_key = format!("{architecture}.block_count");
                let block_count = source
                    .loader
                    .metadata(&block_key)
                    .and_then(MetaValue::to_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .expect("validated LLM block count");
                let group_count = usize::try_from(block_count)
                    .map_err(|_| {
                        invalid(format!(
                            "component {} block count does not fit usize",
                            source.name
                        ))
                    })?
                    .checked_add(1)
                    .ok_or_else(|| {
                        invalid(format!("component {} segment count overflow", source.name))
                    })?;
                let mut groups: Vec<(SegmentKind, Option<u32>, Vec<TensorInfo>)> = Vec::new();
                groups.try_reserve_exact(group_count).map_err(|_| {
                    invalid(format!(
                        "component {} failed to allocate {group_count} segment groups",
                        source.name
                    ))
                })?;
                groups.push((SegmentKind::Shared, None, Vec::new()));
                groups.extend(
                    (0..block_count).map(|layer| (SegmentKind::Layer, Some(layer), Vec::new())),
                );
                for tensor in source.loader.tensors() {
                    let group = if let Some(layer) = layer_index(&tensor.name) {
                        if layer >= block_count {
                            return Err(invalid(format!(
                                "component {} tensor {} layer {layer} is outside block count {block_count}",
                                source.name, tensor.name
                            )));
                        }
                        usize::try_from(layer)
                            .ok()
                            .and_then(|layer| layer.checked_add(1))
                            .ok_or_else(|| {
                                invalid(format!(
                                    "component {} tensor {} layer index overflow",
                                    source.name, tensor.name
                                ))
                            })?
                    } else {
                        0
                    };
                    groups[group].2.push(tensor.clone());
                }
                groups
            }
            ComponentRole::Mmproj => vec![(
                SegmentKind::Component,
                None,
                source.loader.tensors().to_vec(),
            )],
        };

        let mut component_tensor_names = BTreeSet::new();
        for (kind, layer, mut tensors) in groups {
            if tensors.is_empty() {
                let label = match layer {
                    Some(layer) => format!("layer {layer}"),
                    None => format!("{kind:?}"),
                };
                return Err(invalid(format!(
                    "component {} {label} segment has no tensors",
                    source.name
                )));
            }
            tensors.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
            let segment_id = vec_len_u32(&index.segments, "segment")?;
            let segment_tensor_start = vec_len_u32(&index.tensors, "tensor")?;
            let mut cursor = 0u64;
            for tensor in tensors {
                if !component_tensor_names.insert(tensor.name.clone()) {
                    return Err(invalid(format!(
                        "component {} has duplicate tensor name {}",
                        source.name, tensor.name
                    )));
                }
                let segment_offset =
                    align_up(cursor, source.tensor_alignment).ok_or_else(|| {
                        invalid(format!(
                            "component {} tensor {} aligned offset overflow",
                            source.name, tensor.name
                        ))
                    })?;
                let byte_len = tensor.checked_nbytes().ok_or_else(|| {
                    invalid(format!(
                        "component {} tensor {} dimensions/type do not form complete GGML blocks",
                        source.name, tensor.name
                    ))
                })?;
                let actual_len = source
                    .loader
                    .tensor_slice(&tensor.name)
                    .and_then(|bytes| u64::try_from(bytes.len()).ok())
                    .ok_or_else(|| {
                        invalid(format!(
                            "component {} tensor {} source bytes are unavailable",
                            source.name, tensor.name
                        ))
                    })?;
                if actual_len != byte_len {
                    return Err(invalid(format!(
                        "component {} tensor {} source length {actual_len} differs from checked size {byte_len}",
                        source.name, tensor.name
                    )));
                }
                cursor = segment_offset.checked_add(byte_len).ok_or_else(|| {
                    invalid(format!(
                        "component {} tensor {} end overflow",
                        source.name, tensor.name
                    ))
                })?;
                index.tensors.push(TensorRecord {
                    component_id,
                    segment_id,
                    info: TensorInfo {
                        name: tensor.name,
                        dims: tensor.dims,
                        ggml_type: tensor.ggml_type,
                        offset: segment_offset,
                    },
                    segment_offset,
                    byte_len,
                });
            }
            let stored_len = align_up(cursor, GGUFRS_SEGMENT_ALIGNMENT).ok_or_else(|| {
                invalid(format!(
                    "component {} segment {segment_id} stored length overflow",
                    source.name
                ))
            })?;
            let segment_tensor_end = vec_len_u32(&index.tensors, "tensor")?;
            index.segments.push(SegmentInfo {
                id: segment_id,
                component_id,
                kind,
                layer,
                absolute_offset: 0,
                stored_len,
                tensor_range: segment_tensor_start..segment_tensor_end,
                sha256: [0; 32],
            });
        }

        let tensor_end = vec_len_u32(&index.tensors, "tensor")?;
        let segment_end = vec_len_u32(&index.segments, "segment")?;
        index.components.push(ComponentInfo {
            id: component_id,
            role: source.role,
            name: source.name.into(),
            metadata_range: metadata_start..metadata_end,
            tensor_range: tensor_start..tensor_end,
            segment_range: segment_start..segment_end,
        });
    }

    let component_bytes = serialize_component_table(&index)?;
    let metadata_bytes = serialize_metadata_table(&index)?;
    let segment_bytes = serialize_segment_table(&index)?;
    let tensor_bytes = serialize_tensor_table(&index)?;
    let table_range = |offset: u64, bytes: &[u8], name: &str| -> Result<TableRange, GgufrsError> {
        let length = u64::try_from(bytes.len())
            .map_err(|_| invalid(format!("{name} length does not fit u64")))?;
        offset
            .checked_add(length)
            .ok_or_else(|| invalid(format!("{name} range overflow")))?;
        Ok(TableRange { offset, length })
    };
    let component_table = table_range(SUPERBLOCK_LEN as u64, &component_bytes, "component table")?;
    let metadata_table = table_range(
        component_table
            .offset
            .checked_add(component_table.length)
            .ok_or_else(|| invalid("component table range overflow"))?,
        &metadata_bytes,
        "metadata table",
    )?;
    let segment_table = table_range(
        metadata_table
            .offset
            .checked_add(metadata_table.length)
            .ok_or_else(|| invalid("metadata table range overflow"))?,
        &segment_bytes,
        "segment table",
    )?;
    let tensor_table = table_range(
        segment_table
            .offset
            .checked_add(segment_table.length)
            .ok_or_else(|| invalid("segment table range overflow"))?,
        &tensor_bytes,
        "tensor table",
    )?;
    let table_end = tensor_table
        .offset
        .checked_add(tensor_table.length)
        .ok_or_else(|| invalid("tensor table range overflow"))?;
    let tensor_data_offset = align_up(table_end, GGUFRS_SEGMENT_ALIGNMENT)
        .ok_or_else(|| invalid("tensor data offset overflow"))?;
    let mut declared_file_size = tensor_data_offset;
    for segment in &mut index.segments {
        segment.absolute_offset = declared_file_size;
        declared_file_size = declared_file_size
            .checked_add(segment.stored_len)
            .ok_or_else(|| invalid(format!("segment {} file range overflow", segment.id)))?;
    }

    Ok(PlannedExport {
        index,
        component_table,
        metadata_table,
        segment_table,
        tensor_table,
        tensor_data_offset,
        declared_file_size,
    })
}

fn serialize_component_table(index: &PackageIndex) -> Result<Vec<u8>, GgufrsError> {
    let mut out = Vec::new();
    for component in &index.components {
        put_u32(&mut out, component.id);
        put_u32(&mut out, component.role as u32);
        put_string(&mut out, &component.name)?;
        for range in [
            &component.metadata_range,
            &component.tensor_range,
            &component.segment_range,
        ] {
            put_u32(&mut out, range.start);
            put_u32(&mut out, range.end - range.start);
        }
    }
    Ok(out)
}

fn serialize_metadata_table(index: &PackageIndex) -> Result<Vec<u8>, GgufrsError> {
    let mut out = Vec::new();
    for entry in &index.metadata {
        put_u32(&mut out, entry.component_id);
        put_string(&mut out, &entry.key)?;
        put_i32(&mut out, meta_value_type(&entry.value) as i32);
        put_meta_value(&mut out, &entry.value)?;
    }
    Ok(out)
}

fn serialize_segment_table(index: &PackageIndex) -> Result<Vec<u8>, GgufrsError> {
    let mut out = Vec::new();
    for segment in &index.segments {
        put_u32(&mut out, segment.id);
        put_u32(&mut out, segment.component_id);
        put_u32(&mut out, segment.kind as u32);
        let layer = segment
            .layer
            .map(|value| {
                i32::try_from(value)
                    .map_err(|_| invalid(format!("segment {} layer does not fit i32", segment.id)))
            })
            .transpose()?
            .unwrap_or(-1);
        put_i32(&mut out, layer);
        put_u64(&mut out, segment.absolute_offset);
        put_u64(&mut out, segment.stored_len);
        put_u32(&mut out, segment.tensor_range.start);
        put_u32(
            &mut out,
            segment.tensor_range.end - segment.tensor_range.start,
        );
        out.extend_from_slice(&segment.sha256);
    }
    Ok(out)
}

fn serialize_tensor_table(index: &PackageIndex) -> Result<Vec<u8>, GgufrsError> {
    let mut out = Vec::new();
    for tensor in &index.tensors {
        put_u32(&mut out, tensor.component_id);
        put_u32(&mut out, tensor.segment_id);
        put_string(&mut out, &tensor.info.name)?;
        put_i32(&mut out, tensor.info.ggml_type as i32);
        put_u32(
            &mut out,
            u32::try_from(tensor.info.dims.len()).map_err(|_| {
                invalid(format!("tensor {} rank does not fit u32", tensor.info.name))
            })?,
        );
        for dimension in &tensor.info.dims {
            put_u64(&mut out, *dimension);
        }
        put_u64(&mut out, tensor.segment_offset);
        put_u64(&mut out, tensor.byte_len);
    }
    Ok(out)
}

fn serialize_superblock(plan: &PlannedExport) -> Result<[u8; 128], GgufrsError> {
    let mut out = Vec::with_capacity(SUPERBLOCK_LEN);
    out.extend_from_slice(GGUFRS_MAGIC);
    put_u32(&mut out, GGUFRS_VERSION);
    put_u32(&mut out, 0);
    put_u64(&mut out, plan.declared_file_size);
    for count in [
        vec_len_u32(&plan.index.components, "component")?,
        vec_len_u32(&plan.index.metadata, "metadata")?,
        vec_len_u32(&plan.index.segments, "segment")?,
        vec_len_u32(&plan.index.tensors, "tensor")?,
    ] {
        put_u32(&mut out, count);
    }
    for table in [
        plan.component_table,
        plan.metadata_table,
        plan.segment_table,
        plan.tensor_table,
    ] {
        put_u64(&mut out, table.offset);
        put_u64(&mut out, table.length);
    }
    put_u64(&mut out, plan.tensor_data_offset);
    out.extend_from_slice(&[0; 16]);
    out.try_into()
        .map_err(|_| invalid("superblock is not exactly 128 bytes"))
}

fn checked_range(
    offset: u64,
    len: u64,
    file_len: u64,
    context: &str,
) -> Result<Range<usize>, GgufrsError> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| invalid(format!("{context} range overflow")))?;
    if end > file_len {
        return Err(invalid(format!(
            "{context} range {offset}..{end} exceeds file length {file_len}"
        )));
    }
    Ok(usize::try_from(offset)
        .map_err(|_| invalid(format!("{context} offset does not fit usize")))?
        ..usize::try_from(end).map_err(|_| invalid(format!("{context} end does not fit usize")))?)
}

fn counted_range(start: u32, count: u32, context: &str) -> Result<Range<u32>, GgufrsError> {
    Ok(start
        ..start
            .checked_add(count)
            .ok_or_else(|| invalid(format!("{context} count overflow")))?)
}

fn component_role(value: u32) -> Result<ComponentRole, GgufrsError> {
    match value {
        1 => Ok(ComponentRole::Llm),
        2 => Ok(ComponentRole::Mmproj),
        _ => Err(invalid(format!("unknown component role {value}"))),
    }
}

fn segment_kind(value: u32) -> Result<SegmentKind, GgufrsError> {
    match value {
        1 => Ok(SegmentKind::Shared),
        2 => Ok(SegmentKind::Layer),
        3 => Ok(SegmentKind::Component),
        _ => Err(invalid(format!("unknown segment kind {value}"))),
    }
}

fn read_superblock(bytes: &[u8]) -> Result<Superblock, GgufrsError> {
    let header = bytes
        .get(..SUPERBLOCK_LEN)
        .ok_or_else(|| invalid("file is shorter than the 128-byte superblock"))?;
    let mut reader = ByteReader::new(header);
    if reader.read_exact(8, "ggufrs magic").map_err(invalid)? != GGUFRS_MAGIC {
        return Err(invalid("invalid ggufrs magic"));
    }
    let version = reader.read_u32().map_err(invalid)?;
    let flags = reader.read_u32().map_err(invalid)?;
    if version != GGUFRS_VERSION || flags != 0 {
        return Err(invalid(format!(
            "unsupported ggufrs version/flags: version={version}, flags={flags}"
        )));
    }
    let declared_file_size = reader.read_u64().map_err(invalid)?;
    let component_count = reader.read_u32().map_err(invalid)?;
    let metadata_count = reader.read_u32().map_err(invalid)?;
    let segment_count = reader.read_u32().map_err(invalid)?;
    let tensor_count = reader.read_u32().map_err(invalid)?;
    let mut table = || -> Result<TableRange, GgufrsError> {
        Ok(TableRange {
            offset: reader.read_u64().map_err(invalid)?,
            length: reader.read_u64().map_err(invalid)?,
        })
    };
    let component_table = table()?;
    let metadata_table = table()?;
    let segment_table = table()?;
    let tensor_table = table()?;
    let tensor_data_offset = reader.read_u64().map_err(invalid)?;
    if reader
        .read_exact(16, "reserved superblock bytes")
        .map_err(invalid)?
        != [0u8; 16]
    {
        return Err(invalid("reserved superblock bytes must be zero"));
    }
    Ok(Superblock {
        declared_file_size,
        component_count,
        metadata_count,
        segment_count,
        tensor_count,
        component_table,
        metadata_table,
        segment_table,
        tensor_table,
        tensor_data_offset,
    })
}

fn validate_table_layout(
    superblock: &Superblock,
    file_len: u64,
) -> Result<Range<u64>, GgufrsError> {
    if superblock.declared_file_size != file_len {
        return Err(invalid(format!(
            "declared file size {} does not match actual file size {file_len}",
            superblock.declared_file_size
        )));
    }
    if superblock.tensor_data_offset % GGUFRS_SEGMENT_ALIGNMENT != 0 {
        return Err(invalid(format!(
            "tensor data offset {} is not aligned to {GGUFRS_SEGMENT_ALIGNMENT}",
            superblock.tensor_data_offset
        )));
    }

    let tables = [
        ("component table", superblock.component_table),
        ("metadata table", superblock.metadata_table),
        ("segment table", superblock.segment_table),
        ("tensor table", superblock.tensor_table),
    ];
    let mut expected_offset = SUPERBLOCK_LEN as u64;
    for (name, table) in tables {
        checked_range(table.offset, table.length, file_len, name)?;
        if table.offset != expected_offset {
            return Err(invalid(format!(
                "{name} begins at {}, expected contiguous offset {expected_offset}",
                table.offset
            )));
        }
        expected_offset = table
            .offset
            .checked_add(table.length)
            .ok_or_else(|| invalid(format!("{name} range overflow")))?;
    }
    if expected_offset > superblock.tensor_data_offset {
        return Err(invalid(format!(
            "tensor table ends at {expected_offset}, after tensor data offset {}",
            superblock.tensor_data_offset
        )));
    }
    checked_range(
        expected_offset,
        superblock.tensor_data_offset - expected_offset,
        file_len,
        "index padding",
    )?;
    Ok(expected_offset..superblock.tensor_data_offset)
}

fn read_file_range(
    file: &mut File,
    path: &Path,
    table: TableRange,
    context: &str,
) -> Result<Vec<u8>, GgufrsError> {
    let len = usize::try_from(table.length)
        .map_err(|_| invalid(format!("{context} length does not fit usize")))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| invalid(format!("failed to allocate {context}")))?;
    bytes.resize(len, 0);
    file.seek(SeekFrom::Start(table.offset))
        .and_then(|_| file.read_exact(&mut bytes))
        .map_err(|source| GgufrsError::Io {
            operation: "read ggufrs index table",
            path: path.to_path_buf(),
            source,
        })?;
    Ok(bytes)
}

fn verify_zero_file_range(
    file: &mut File,
    path: &Path,
    range: Range<u64>,
) -> Result<(), GgufrsError> {
    file.seek(SeekFrom::Start(range.start))
        .map_err(|source| GgufrsError::Io {
            operation: "seek ggufrs index padding",
            path: path.to_path_buf(),
            source,
        })?;
    let mut remaining = range.end - range.start;
    let mut buffer = [0u8; 8192];
    while remaining != 0 {
        let len = usize::try_from(remaining.min(buffer.len() as u64))
            .expect("bounded padding read length fits usize");
        file.read_exact(&mut buffer[..len])
            .map_err(|source| GgufrsError::Io {
                operation: "read ggufrs index padding",
                path: path.to_path_buf(),
                source,
            })?;
        if buffer[..len].iter().any(|byte| *byte != 0) {
            return Err(invalid("index padding before tensor data must be zero"));
        }
        remaining -= len as u64;
    }
    Ok(())
}

fn read_index_tables(
    file: &mut File,
    path: &Path,
    superblock: &Superblock,
    file_len: u64,
) -> Result<IndexTables, GgufrsError> {
    let padding = validate_table_layout(superblock, file_len)?;
    let tables = IndexTables {
        components: read_file_range(file, path, superblock.component_table, "component table")?,
        metadata: read_file_range(file, path, superblock.metadata_table, "metadata table")?,
        segments: read_file_range(file, path, superblock.segment_table, "segment table")?,
        tensors: read_file_range(file, path, superblock.tensor_table, "tensor table")?,
    };
    verify_zero_file_range(file, path, padding)?;
    Ok(tables)
}

fn table_vec<T>(
    count: u32,
    table_len: usize,
    minimum_entry_bytes: usize,
    context: &str,
) -> Result<Vec<T>, GgufrsError> {
    let count = usize::try_from(count)
        .map_err(|_| invalid(format!("{context} count does not fit usize")))?;
    if count > table_len / minimum_entry_bytes {
        return Err(invalid(format!("{context} count exceeds remaining bytes")));
    }
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| invalid(format!("failed to allocate {context} entries")))?;
    Ok(values)
}

fn parse_index(superblock: &Superblock, tables: &IndexTables) -> Result<PackageIndex, GgufrsError> {
    let component_bytes = tables.components.as_slice();

    let mut reader = ByteReader::new(component_bytes);
    let mut components = table_vec(
        superblock.component_count,
        component_bytes.len(),
        40,
        "component table",
    )?;
    for entry in 0..superblock.component_count {
        let context = format!("component {entry}");
        let id = reader
            .read_u32()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let role = component_role(
            reader
                .read_u32()
                .map_err(|message| invalid(format!("{context}: {message}")))?,
        )?;
        let name = reader
            .read_string()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let metadata_start = reader
            .read_u32()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let metadata_count = reader
            .read_u32()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let tensor_start = reader
            .read_u32()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let tensor_count = reader
            .read_u32()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let segment_start = reader
            .read_u32()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let segment_count = reader
            .read_u32()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        components.push(ComponentInfo {
            id,
            role,
            name,
            metadata_range: counted_range(metadata_start, metadata_count, &context)?,
            tensor_range: counted_range(tensor_start, tensor_count, &context)?,
            segment_range: counted_range(segment_start, segment_count, &context)?,
        });
    }
    if reader.pos() != component_bytes.len() {
        return Err(invalid(format!(
            "component table has {} trailing bytes",
            component_bytes.len() - reader.pos()
        )));
    }

    let metadata_bytes = tables.metadata.as_slice();
    let mut reader = ByteReader::new(metadata_bytes);
    let mut metadata = table_vec(
        superblock.metadata_count,
        metadata_bytes.len(),
        17,
        "metadata table",
    )?;
    for entry in 0..superblock.metadata_count {
        let context = format!("metadata {entry}");
        let component_id = reader
            .read_u32()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let key = reader
            .read_string()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let value_type_raw = reader
            .read_i32()
            .map_err(|message| invalid(format!("{context} key {key}: {message}")))?;
        let value_type = MetaValueType::from_i32(value_type_raw).ok_or_else(|| {
            invalid(format!(
                "{context} key {key}: unknown metadata value type {value_type_raw}"
            ))
        })?;
        if value_type == MetaValueType::Array {
            let mut peek = ByteReader::new(&metadata_bytes[reader.pos()..]);
            let element_type = peek
                .read_i32()
                .map_err(|message| invalid(format!("{context} key {key}: {message}")))?;
            if element_type == MetaValueType::Array as i32 {
                return Err(invalid(format!(
                    "{context} key {key}: nested metadata arrays are not supported"
                )));
            }
        }
        let value = reader
            .read_meta_value(value_type)
            .map_err(|message| invalid(format!("{context} key {key}: {message}")))?;
        metadata.push(ScopedMetadata {
            component_id,
            key,
            value,
        });
    }
    if reader.pos() != metadata_bytes.len() {
        return Err(invalid(format!(
            "metadata table has {} trailing bytes",
            metadata_bytes.len() - reader.pos()
        )));
    }

    let segment_bytes = tables.segments.as_slice();
    let mut reader = ByteReader::new(segment_bytes);
    let mut segments = table_vec(
        superblock.segment_count,
        segment_bytes.len(),
        72,
        "segment table",
    )?;
    for entry in 0..superblock.segment_count {
        let context = format!("segment {entry}");
        let id = reader
            .read_u32()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let component_id = reader
            .read_u32()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let kind = segment_kind(
            reader
                .read_u32()
                .map_err(|message| invalid(format!("{context}: {message}")))?,
        )?;
        let layer_raw = reader
            .read_i32()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let layer = match layer_raw {
            -1 => None,
            value if value >= 0 => Some(value as u32),
            _ => {
                return Err(invalid(format!(
                    "{context}: invalid layer value {layer_raw}"
                )))
            }
        };
        let absolute_offset = reader
            .read_u64()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let stored_len = reader
            .read_u64()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let tensor_start = reader
            .read_u32()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let tensor_count = reader
            .read_u32()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let mut sha256 = [0u8; 32];
        sha256.copy_from_slice(
            reader
                .read_exact(32, "segment sha256")
                .map_err(|message| invalid(format!("{context}: {message}")))?,
        );
        segments.push(SegmentInfo {
            id,
            component_id,
            kind,
            layer,
            absolute_offset,
            stored_len,
            tensor_range: counted_range(tensor_start, tensor_count, &context)?,
            sha256,
        });
    }
    if reader.pos() != segment_bytes.len() {
        return Err(invalid(format!(
            "segment table has {} trailing bytes",
            segment_bytes.len() - reader.pos()
        )));
    }

    let tensor_bytes = tables.tensors.as_slice();
    let mut reader = ByteReader::new(tensor_bytes);
    let mut tensors = table_vec(
        superblock.tensor_count,
        tensor_bytes.len(),
        40,
        "tensor table",
    )?;
    for entry in 0..superblock.tensor_count {
        let context = format!("tensor {entry}");
        let component_id = reader
            .read_u32()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let segment_id = reader
            .read_u32()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let name = reader
            .read_string()
            .map_err(|message| invalid(format!("{context}: {message}")))?;
        let tensor_context = format!("component {component_id} segment {segment_id} tensor {name}");
        let type_raw = reader
            .read_i32()
            .map_err(|message| invalid(format!("{tensor_context}: {message}")))?;
        let ggml_type = GGMLType::from_i32(type_raw)
            .ok_or_else(|| invalid(format!("{tensor_context}: unknown GGML type {type_raw}")))?;
        let rank = reader
            .read_u32()
            .map_err(|message| invalid(format!("{tensor_context}: {message}")))?;
        let rank = usize::try_from(rank)
            .map_err(|_| invalid(format!("{tensor_context}: rank does not fit usize")))?;
        let required = rank
            .checked_mul(8)
            .and_then(|bytes| bytes.checked_add(16))
            .ok_or_else(|| invalid(format!("{tensor_context}: rank byte count overflow")))?;
        if required > tensor_bytes.len().saturating_sub(reader.pos()) {
            return Err(invalid(format!(
                "{tensor_context}: rank exceeds remaining bytes including trailing offsets"
            )));
        }
        let mut dims = Vec::new();
        dims.try_reserve_exact(rank)
            .map_err(|_| invalid(format!("{tensor_context}: failed to allocate dimensions")))?;
        for _ in 0..rank {
            dims.push(
                reader
                    .read_u64()
                    .map_err(|message| invalid(format!("{tensor_context}: {message}")))?,
            );
        }
        let segment_offset = reader
            .read_u64()
            .map_err(|message| invalid(format!("{tensor_context}: {message}")))?;
        let byte_len = reader
            .read_u64()
            .map_err(|message| invalid(format!("{tensor_context}: {message}")))?;
        tensors.push(TensorRecord {
            component_id,
            segment_id,
            info: TensorInfo {
                name,
                dims,
                ggml_type,
                offset: segment_offset,
            },
            segment_offset,
            byte_len,
        });
    }
    if reader.pos() != tensor_bytes.len() {
        return Err(invalid(format!(
            "tensor table has {} trailing bytes",
            tensor_bytes.len() - reader.pos()
        )));
    }

    let mut index = PackageIndex {
        components,
        metadata,
        segments,
        tensors,
        component_by_role: BTreeMap::new(),
        metadata_lookup: BTreeMap::new(),
        tensor_lookup: BTreeMap::new(),
    };
    validate_index(superblock, &mut index)?;
    Ok(index)
}

fn range_end_within(range: &Range<u32>, len: usize, context: &str) -> Result<(), GgufrsError> {
    if u64::from(range.end) > len as u64 {
        return Err(invalid(format!(
            "{context} range {}..{} exceeds table count {len}",
            range.start, range.end
        )));
    }
    Ok(())
}

fn metadata_value<'a>(
    index: &'a PackageIndex,
    component_id: u32,
    key: &str,
) -> Option<&'a MetaValue> {
    index
        .metadata
        .iter()
        .find(|entry| entry.component_id == component_id && entry.key == key)
        .map(|entry| &entry.value)
}

fn validate_index(superblock: &Superblock, index: &mut PackageIndex) -> Result<(), GgufrsError> {
    let mut roles = BTreeSet::new();
    let mut next_metadata = 0u32;
    let mut next_tensor = 0u32;
    let mut next_segment = 0u32;
    let mut previous_component: Option<&ComponentInfo> = None;

    for (position, component) in index.components.iter().enumerate() {
        let context = format!(
            "component {} ({:?} {})",
            component.id, component.role, component.name
        );
        if component.id != position as u32 {
            return Err(invalid(format!(
                "{context}: id is {}, expected {position}",
                component.id
            )));
        }
        if let Some(previous) = previous_component {
            if (previous.role, previous.name.as_bytes())
                >= (component.role, component.name.as_bytes())
            {
                return Err(invalid(format!(
                    "{context}: components are not sorted by role and UTF-8 name bytes"
                )));
            }
        }
        previous_component = Some(component);
        if !roles.insert(component.role) {
            return Err(invalid(format!("{context}: duplicate component role")));
        }

        for (label, range, expected, len) in [
            (
                "metadata",
                &component.metadata_range,
                &mut next_metadata,
                index.metadata.len(),
            ),
            (
                "tensor",
                &component.tensor_range,
                &mut next_tensor,
                index.tensors.len(),
            ),
            (
                "segment",
                &component.segment_range,
                &mut next_segment,
                index.segments.len(),
            ),
        ] {
            range_end_within(range, len, &format!("{context} {label}"))?;
            if range.start != *expected {
                return Err(invalid(format!(
                    "{context}: {label} range begins at {}, expected exclusive coverage from {}",
                    range.start, *expected
                )));
            }
            *expected = range.end;
        }

        let mut previous_key: Option<&[u8]> = None;
        let mut metadata_keys = BTreeSet::new();
        for metadata_position in component.metadata_range.clone() {
            let entry = &index.metadata[metadata_position as usize];
            if entry.component_id != component.id {
                return Err(invalid(format!(
                    "{context}: metadata {metadata_position} belongs to component {}",
                    entry.component_id
                )));
            }
            if !metadata_keys.insert(entry.key.as_str()) {
                return Err(invalid(format!(
                    "{context}: duplicate metadata key {}",
                    entry.key
                )));
            }
            if let Some(previous) = previous_key {
                match previous.cmp(entry.key.as_bytes()) {
                    std::cmp::Ordering::Equal => unreachable!("duplicate checked above"),
                    std::cmp::Ordering::Greater => {
                        return Err(invalid(format!(
                            "{context}: metadata keys are not sorted at {}",
                            entry.key
                        )))
                    }
                    std::cmp::Ordering::Less => {}
                }
            }
            previous_key = Some(entry.key.as_bytes());
        }

        let mut tensor_names = BTreeSet::new();
        for tensor_position in component.tensor_range.clone() {
            let tensor = &index.tensors[tensor_position as usize];
            if tensor.component_id != component.id {
                return Err(invalid(format!(
                    "{context}: tensor {} belongs to component {}",
                    tensor.info.name, tensor.component_id
                )));
            }
            if !tensor_names.insert(tensor.info.name.as_str()) {
                return Err(invalid(format!(
                    "{context}: duplicate tensor name {}",
                    tensor.info.name
                )));
            }
        }

        for segment_position in component.segment_range.clone() {
            let segment = &index.segments[segment_position as usize];
            if segment.component_id != component.id {
                return Err(invalid(format!(
                    "{context}: segment {} belongs to component {}",
                    segment.id, segment.component_id
                )));
            }
        }
    }

    if next_metadata != superblock.metadata_count
        || next_tensor != superblock.tensor_count
        || next_segment != superblock.segment_count
    {
        return Err(invalid(format!(
            "component ranges do not cover all tables: metadata {next_metadata}/{}, tensors {next_tensor}/{}, segments {next_segment}/{}",
            superblock.metadata_count,
            superblock.tensor_count,
            superblock.segment_count
        )));
    }
    if index
        .components
        .iter()
        .filter(|component| component.role == ComponentRole::Llm)
        .count()
        != 1
    {
        return Err(invalid("package must contain exactly one LLM component"));
    }
    if index
        .components
        .iter()
        .filter(|component| component.role == ComponentRole::Mmproj)
        .count()
        > 1
    {
        return Err(invalid("package may contain at most one mmproj component"));
    }

    let mut expected_segment_offset = superblock.tensor_data_offset;
    for (position, segment) in index.segments.iter().enumerate() {
        let context = format!("component {} segment {}", segment.component_id, segment.id);
        if segment.id != position as u32 {
            return Err(invalid(format!(
                "{context}: segment id is {}, expected {position}",
                segment.id
            )));
        }
        if segment.absolute_offset == 0 || segment.stored_len == 0 {
            return Err(invalid(format!(
                "{context}: segment offset and length must be nonzero"
            )));
        }
        if segment.absolute_offset % GGUFRS_SEGMENT_ALIGNMENT != 0
            || segment.stored_len % GGUFRS_SEGMENT_ALIGNMENT != 0
        {
            return Err(invalid(format!(
                "{context}: segment offset {} and length {} must be aligned to {GGUFRS_SEGMENT_ALIGNMENT}",
                segment.absolute_offset, segment.stored_len
            )));
        }
        if segment.absolute_offset < superblock.tensor_data_offset {
            return Err(invalid(format!(
                "{context}: offset {} is before tensor data offset {}",
                segment.absolute_offset, superblock.tensor_data_offset
            )));
        }
        let end = segment
            .absolute_offset
            .checked_add(segment.stored_len)
            .ok_or_else(|| invalid(format!("{context}: segment range overflow")))?;
        if end > superblock.declared_file_size {
            return Err(invalid(format!(
                "{context}: segment end {end} exceeds declared file size {}",
                superblock.declared_file_size
            )));
        }
        if segment.absolute_offset != expected_segment_offset {
            let relation = if segment.absolute_offset < expected_segment_offset {
                "overlaps the previous segment"
            } else {
                "is not contiguous with the previous segment"
            };
            return Err(invalid(format!(
                "{context}: offset {} {relation}; expected {expected_segment_offset}",
                segment.absolute_offset
            )));
        }
        expected_segment_offset = end;
    }
    if expected_segment_offset != superblock.declared_file_size {
        return Err(invalid(format!(
            "last segment ends at {expected_segment_offset}, expected declared file size {}",
            superblock.declared_file_size
        )));
    }

    for component in &index.components {
        let context = format!(
            "component {} ({:?} {})",
            component.id, component.role, component.name
        );
        let alignment = metadata_value(index, component.id, "general.alignment")
            .map(|value| {
                value.to_u64().ok_or_else(|| {
                    invalid(format!("{context}: general.alignment is not an integer"))
                })
            })
            .transpose()?
            .unwrap_or(32);
        if alignment == 0 || !alignment.is_power_of_two() {
            return Err(invalid(format!(
                "{context}: general.alignment {alignment} is not a nonzero power of two"
            )));
        }
        let tensor_alignment = alignment.max(32);
        let mut next_component_tensor = component.tensor_range.start;
        let mut previous_segment_key: Option<(u32, u32)> = None;
        let mut layer_indices = BTreeSet::new();
        let mut shared_count = 0usize;
        let mut component_segment_count = 0usize;

        for segment_position in component.segment_range.clone() {
            let segment = &index.segments[segment_position as usize];
            range_end_within(
                &segment.tensor_range,
                index.tensors.len(),
                &format!("{context} segment {} tensor", segment.id),
            )?;
            if segment.tensor_range.start != next_component_tensor
                || segment.tensor_range.end > component.tensor_range.end
            {
                return Err(invalid(format!(
                    "{context} segment {} tensor range {}..{} does not exclusively cover component tensor range from {next_component_tensor}",
                    segment.id, segment.tensor_range.start, segment.tensor_range.end
                )));
            }
            next_component_tensor = segment.tensor_range.end;

            match (component.role, segment.kind) {
                (ComponentRole::Llm, SegmentKind::Shared) => shared_count += 1,
                (ComponentRole::Llm, SegmentKind::Layer) => {}
                (ComponentRole::Mmproj, SegmentKind::Component) => component_segment_count += 1,
                _ => {
                    return Err(invalid(format!(
                        "{context} segment {}: {:?} kind is invalid for {:?}",
                        segment.id, segment.kind, component.role
                    )))
                }
            }
            match (segment.kind, segment.layer) {
                (SegmentKind::Layer, Some(layer)) => {
                    if !layer_indices.insert(layer) {
                        return Err(invalid(format!(
                            "{context} segment {}: duplicate layer index {layer}",
                            segment.id
                        )));
                    }
                }
                (SegmentKind::Layer, None) => {
                    return Err(invalid(format!(
                        "{context} segment {}: layer segment lacks a nonnegative layer index",
                        segment.id
                    )))
                }
                (_, Some(layer)) => {
                    return Err(invalid(format!(
                        "{context} segment {}: non-layer segment has layer index {layer}",
                        segment.id
                    )))
                }
                (_, None) => {}
            }

            let segment_key = (segment.kind as u32, segment.layer.unwrap_or(0));
            if previous_segment_key.is_some_and(|previous| previous >= segment_key) {
                return Err(invalid(format!(
                    "{context} segment {}: segment kind/layer order is noncanonical",
                    segment.id
                )));
            }
            previous_segment_key = Some(segment_key);

            let mut previous_tensor_name: Option<&[u8]> = None;
            let mut tensor_ranges = Vec::new();
            for tensor_position in segment.tensor_range.clone() {
                let tensor = &index.tensors[tensor_position as usize];
                let tensor_context = format!(
                    "component {} segment {} tensor {}",
                    component.id, segment.id, tensor.info.name
                );
                if tensor.component_id != component.id || tensor.segment_id != segment.id {
                    return Err(invalid(format!(
                        "{tensor_context}: references component {} segment {}",
                        tensor.component_id, tensor.segment_id
                    )));
                }
                if let Some(previous) = previous_tensor_name {
                    if previous >= tensor.info.name.as_bytes() {
                        return Err(invalid(format!(
                            "{tensor_context}: tensor names are not byte-sorted inside segment"
                        )));
                    }
                }
                previous_tensor_name = Some(tensor.info.name.as_bytes());

                if tensor.info.dims.is_empty() {
                    return Err(invalid(format!("{tensor_context}: tensor rank is zero")));
                }
                tensor.info.checked_n_elements().ok_or_else(|| {
                    invalid(format!("{tensor_context}: tensor dimensions overflow"))
                })?;
                let expected_len = tensor.info.checked_nbytes().ok_or_else(|| {
                    invalid(format!(
                        "{tensor_context}: tensor dimensions/type do not form complete GGML blocks"
                    ))
                })?;
                if tensor.byte_len != expected_len {
                    return Err(invalid(format!(
                        "{tensor_context}: byte length {} differs from checked size {expected_len}",
                        tensor.byte_len
                    )));
                }

                if tensor.segment_offset % tensor_alignment != 0 {
                    return Err(invalid(format!(
                        "{tensor_context}: segment offset {} is not aligned to {tensor_alignment}",
                        tensor.segment_offset
                    )));
                }
                let tensor_end = tensor
                    .segment_offset
                    .checked_add(tensor.byte_len)
                    .ok_or_else(|| invalid(format!("{tensor_context}: tensor range overflow")))?;
                if tensor_end > segment.stored_len {
                    return Err(invalid(format!(
                        "{tensor_context}: range {}..{tensor_end} exceeds segment length {}",
                        tensor.segment_offset, segment.stored_len
                    )));
                }
                tensor_ranges.push((tensor.segment_offset, tensor_end, tensor.info.name.as_str()));
            }
            tensor_ranges.sort_unstable_by_key(|range| (range.0, range.1));
            for pair in tensor_ranges.windows(2) {
                if pair[1].0 < pair[0].1 {
                    return Err(invalid(format!(
                        "{context} segment {}: tensors {} and {} overlap",
                        segment.id, pair[0].2, pair[1].2
                    )));
                }
            }
        }
        if next_component_tensor != component.tensor_range.end {
            return Err(invalid(format!(
                "{context}: segment tensor ranges end at {next_component_tensor}, expected {}",
                component.tensor_range.end
            )));
        }

        match component.role {
            ComponentRole::Llm => {
                if shared_count != 1 {
                    return Err(invalid(format!(
                        "{context}: LLM must have exactly one shared segment"
                    )));
                }
                let architecture = metadata_value(index, component.id, "general.architecture")
                    .and_then(MetaValue::to_string_val)
                    .ok_or_else(|| {
                        invalid(format!(
                            "{context}: missing or invalid general.architecture"
                        ))
                    })?;
                let block_key = format!("{architecture}.block_count");
                let block_count = metadata_value(index, component.id, &block_key)
                    .and_then(MetaValue::to_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| invalid(format!("{context}: missing or invalid {block_key}")))?;
                if layer_indices.len() != block_count as usize
                    || !layer_indices.iter().copied().eq(0..block_count)
                {
                    return Err(invalid(format!(
                        "{context}: layer indices must be exactly 0..{block_count}, got {layer_indices:?}"
                    )));
                }
            }
            ComponentRole::Mmproj => {
                if component_segment_count != 1 || component.segment_range.len() != 1 {
                    return Err(invalid(format!(
                        "{context}: mmproj must have exactly one component segment"
                    )));
                }
            }
        }
    }

    for component in &index.components {
        index.component_by_role.insert(component.role, component.id);
    }
    for (position, entry) in index.metadata.iter().enumerate() {
        index
            .metadata_lookup
            .insert((entry.component_id, entry.key.clone()), position);
    }
    for (position, tensor) in index.tensors.iter().enumerate() {
        index
            .tensor_lookup
            .insert((tensor.component_id, tensor.info.name.clone()), position);
    }
    Ok(())
}

impl GgufrsFile {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, GgufrsError> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path).map_err(|source| GgufrsError::Io {
            operation: "open ggufrs",
            path: path.clone(),
            source,
        })?;
        Self::from_file(file, path)
    }

    fn from_file(mut file: File, path: PathBuf) -> Result<Self, GgufrsError> {
        file.seek(SeekFrom::Start(0))
            .map_err(|source| io_error("seek to ggufrs start", &path, source))?;
        let file_len = file
            .metadata()
            .map_err(|source| GgufrsError::Io {
                operation: "read ggufrs metadata",
                path: path.clone(),
                source,
            })?
            .len();
        if file_len < SUPERBLOCK_LEN as u64 {
            return Err(invalid("file is shorter than the 128-byte superblock"));
        }
        let mut header = [0u8; SUPERBLOCK_LEN];
        file.read_exact(&mut header)
            .map_err(|source| GgufrsError::Io {
                operation: "read ggufrs superblock",
                path: path.clone(),
                source,
            })?;
        let superblock = read_superblock(&header)?;
        let tables = read_index_tables(&mut file, &path, &superblock, file_len)?;
        let index = parse_index(&superblock, &tables)?;
        Ok(Self {
            file: Arc::new(file),
            path: Arc::new(path),
            index: Arc::new(index),
        })
    }

    pub fn components(&self) -> &[ComponentInfo] {
        &self.index.components
    }

    pub fn component_id(&self, role: ComponentRole) -> Option<u32> {
        self.index.component_by_role.get(&role).copied()
    }

    pub fn load_component(&self, role: ComponentRole) -> Result<LoadedComponent, GgufrsError> {
        let component_id = self
            .component_id(role)
            .ok_or_else(|| invalid(format!("package has no {role:?} component")))?;
        self.load_component_id(component_id)
    }

    pub fn load_component_id(&self, component_id: u32) -> Result<LoadedComponent, GgufrsError> {
        let component = self
            .index
            .components
            .get(component_id as usize)
            .filter(|component| component.id == component_id)
            .ok_or_else(|| invalid(format!("unknown component id {component_id}")))?;
        let mut mappings = BTreeMap::new();
        for segment_id in component.segment_range.clone() {
            mappings.insert(segment_id, self.map_segment_shared(segment_id)?);
        }
        let tensor_infos = component
            .tensor_range
            .clone()
            .map(|position| {
                let info = self.tensors()[position as usize].info.clone();
                (info.name.clone(), info)
            })
            .collect();
        Ok(LoadedComponent {
            file: Arc::clone(&self.file),
            path: Arc::clone(&self.path),
            index: Arc::clone(&self.index),
            component_id,
            mappings,
            tensor_infos,
        })
    }

    pub fn verify_all(&self) -> Result<(), GgufrsError> {
        for component in &self.index.components {
            drop(self.load_component_id(component.id)?);
        }
        Ok(())
    }

    pub(crate) fn segment(&self, id: u32) -> Option<&SegmentInfo> {
        self.index
            .segments
            .get(id as usize)
            .filter(|segment| segment.id == id)
    }

    pub(crate) fn segments_for_component(
        &self,
        component_id: u32,
    ) -> impl Iterator<Item = &SegmentInfo> {
        self.index
            .segments
            .iter()
            .filter(move |segment| segment.component_id == component_id)
    }

    pub(crate) fn tensors(&self) -> &[TensorRecord] {
        &self.index.tensors
    }

    pub(crate) fn tensors_for_segment(
        &self,
        segment_id: u32,
    ) -> impl Iterator<Item = &TensorRecord> {
        self.index
            .tensors
            .iter()
            .filter(move |tensor| tensor.segment_id == segment_id)
    }

    #[allow(dead_code)]
    pub(crate) fn layer_segment_id(&self, component_id: u32, layer: u32) -> Option<u32> {
        self.index
            .segments
            .iter()
            .find(|segment| {
                segment.component_id == component_id
                    && segment.kind == SegmentKind::Layer
                    && segment.layer == Some(layer)
            })
            .map(|segment| segment.id)
    }

    pub(crate) fn map_segment_shared(
        &self,
        segment_id: u32,
    ) -> Result<Arc<MappedSegment>, GgufrsError> {
        let segment = self
            .segment(segment_id)
            .ok_or_else(|| invalid(format!("unknown segment id {segment_id}")))?;
        let len = usize::try_from(segment.stored_len).map_err(|_| {
            invalid(format!(
                "component {} segment {} length does not fit usize",
                segment.component_id, segment.id
            ))
        })?;
        let bytes = unsafe {
            MmapOptions::new()
                .offset(segment.absolute_offset)
                .len(len)
                .map(&*self.file)
        }
        .map_err(|source| GgufrsError::Io {
            operation: "map ggufrs segment",
            path: (*self.path).clone(),
            source,
        })?;
        let actual: [u8; 32] = Sha256::digest(&bytes).into();
        if actual != segment.sha256 {
            return Err(GgufrsError::ChecksumMismatch {
                component_id: segment.component_id,
                segment_id: segment.id,
                expected: segment.sha256,
                actual,
            });
        }
        let mut used_ranges: Vec<Range<usize>> = segment
            .tensor_range
            .clone()
            .map(|position| {
                let tensor = &self.index.tensors[position as usize];
                let start = usize::try_from(tensor.segment_offset).map_err(|_| {
                    invalid(format!(
                        "component {} segment {} tensor {} offset does not fit usize",
                        segment.component_id, segment.id, tensor.info.name
                    ))
                })?;
                let len = usize::try_from(tensor.byte_len).map_err(|_| {
                    invalid(format!(
                        "component {} segment {} tensor {} length does not fit usize",
                        segment.component_id, segment.id, tensor.info.name
                    ))
                })?;
                let end = start.checked_add(len).ok_or_else(|| {
                    invalid(format!(
                        "component {} segment {} tensor {} range overflow",
                        segment.component_id, segment.id, tensor.info.name
                    ))
                })?;
                Ok(start..end)
            })
            .collect::<Result<_, GgufrsError>>()?;
        used_ranges.sort_unstable_by_key(|range| range.start);
        let mut padding_start = 0usize;
        for range in used_ranges {
            if bytes[padding_start..range.start]
                .iter()
                .any(|byte| *byte != 0)
            {
                return Err(invalid(format!(
                    "component {} segment {} has nonzero padding before byte {}",
                    segment.component_id, segment.id, range.start
                )));
            }
            padding_start = range.end;
        }
        if bytes[padding_start..].iter().any(|byte| *byte != 0) {
            return Err(invalid(format!(
                "component {} segment {} has nonzero padding after byte {padding_start}",
                segment.component_id, segment.id
            )));
        }
        Ok(Arc::new(MappedSegment { segment_id, bytes }))
    }
}

impl LoadedComponent {
    pub fn component_id(&self) -> u32 {
        self.component_id
    }

    pub fn component_metadata_entries(&self) -> impl Iterator<Item = (&String, &MetaValue)> + '_ {
        let range = self.index.components[self.component_id as usize]
            .metadata_range
            .clone();
        self.index.metadata[range.start as usize..range.end as usize]
            .iter()
            .map(|entry| (&entry.key, &entry.value))
    }

    pub fn map_segment(&mut self, segment_id: u32) -> Result<(), GgufrsError> {
        let segment = self
            .index
            .segments
            .get(segment_id as usize)
            .filter(|segment| segment.id == segment_id)
            .ok_or_else(|| invalid(format!("unknown segment id {segment_id}")))?;
        if segment.component_id != self.component_id {
            return Err(invalid(format!(
                "component {} does not own segment {segment_id}",
                self.component_id
            )));
        }
        if self.mappings.contains_key(&segment_id) {
            return Ok(());
        }
        let package = GgufrsFile {
            file: Arc::clone(&self.file),
            path: Arc::clone(&self.path),
            index: Arc::clone(&self.index),
        };
        self.mappings
            .insert(segment_id, package.map_segment_shared(segment_id)?);
        Ok(())
    }

    pub fn unmap_segment(&mut self, segment_id: u32) -> Result<bool, GgufrsError> {
        let segment = self
            .index
            .segments
            .get(segment_id as usize)
            .filter(|segment| segment.id == segment_id)
            .ok_or_else(|| invalid(format!("unknown segment id {segment_id}")))?;
        if segment.component_id != self.component_id {
            return Err(invalid(format!(
                "component {} does not own segment {segment_id}",
                self.component_id
            )));
        }
        Ok(self.mappings.remove(&segment_id).is_some())
    }

    pub fn is_segment_mapped(&self, segment_id: u32) -> bool {
        self.mappings.contains_key(&segment_id)
    }
}

impl TensorSource for LoadedComponent {
    fn metadata(&self, key: &str) -> Option<&MetaValue> {
        let index = *self
            .index
            .metadata_lookup
            .get(&(self.component_id, key.to_string()))?;
        Some(&self.index.metadata[index].value)
    }

    fn tensor_info(&self, name: &str) -> Option<&TensorInfo> {
        self.tensor_infos.get(name)
    }

    fn tensor_slice(&self, name: &str) -> Option<&[u8]> {
        let record_index = *self
            .index
            .tensor_lookup
            .get(&(self.component_id, name.to_string()))?;
        let record = &self.index.tensors[record_index];
        let mapping = self.mappings.get(&record.segment_id)?;
        debug_assert_eq!(mapping.segment_id, record.segment_id);
        let start = usize::try_from(record.segment_offset).ok()?;
        let len = usize::try_from(record.byte_len).ok()?;
        mapping.bytes.get(start..start.checked_add(len)?)
    }
}

fn io_error(operation: &'static str, path: &Path, source: io::Error) -> GgufrsError {
    GgufrsError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    }
}

fn write_zeros(
    file: &mut File,
    path: &Path,
    mut hasher: Option<&mut Sha256>,
    mut len: u64,
) -> Result<(), GgufrsError> {
    const ZEROES: [u8; 8192] = [0; 8192];
    while len != 0 {
        let count_u64 = len.min(8192);
        let count = usize::try_from(count_u64).map_err(|_| {
            invalid(format!(
                "padding chunk length {count_u64} does not fit usize"
            ))
        })?;
        file.write_all(&ZEROES[..count])
            .map_err(|source| GgufrsError::Io {
                operation: "write package padding",
                path: path.to_path_buf(),
                source,
            })?;
        if let Some(hasher) = hasher.as_deref_mut() {
            hasher.update(&ZEROES[..count]);
        }
        len = len
            .checked_sub(count_u64)
            .ok_or_else(|| invalid("package padding length underflow"))?;
    }
    Ok(())
}

fn write_planned_package(
    file: &mut File,
    path: &Path,
    sources: &[ExportSource],
    plan: &mut PlannedExport,
) -> Result<(), GgufrsError> {
    let superblock = serialize_superblock(plan)?;
    let component_table = serialize_component_table(&plan.index)?;
    let metadata_table = serialize_metadata_table(&plan.index)?;
    let segment_table = serialize_segment_table(&plan.index)?;
    let tensor_table = serialize_tensor_table(&plan.index)?;
    for (name, bytes, range) in [
        (
            "component",
            component_table.as_slice(),
            plan.component_table,
        ),
        ("metadata", metadata_table.as_slice(), plan.metadata_table),
        ("segment", segment_table.as_slice(), plan.segment_table),
        ("tensor", tensor_table.as_slice(), plan.tensor_table),
    ] {
        let actual = u64::try_from(bytes.len())
            .map_err(|_| invalid(format!("{name} table length does not fit u64")))?;
        if actual != range.length {
            return Err(invalid(format!(
                "{name} table length {actual} differs from planned {}",
                range.length
            )));
        }
    }

    file.seek(SeekFrom::Start(0))
        .map_err(|source| io_error("seek to package start", path, source))?;
    file.write_all(&superblock)
        .map_err(|source| io_error("write package superblock", path, source))?;
    for bytes in [
        component_table.as_slice(),
        metadata_table.as_slice(),
        segment_table.as_slice(),
        tensor_table.as_slice(),
    ] {
        file.write_all(bytes)
            .map_err(|source| io_error("write package index table", path, source))?;
    }
    let table_end = file
        .stream_position()
        .map_err(|source| io_error("read package table end position", path, source))?;
    let padding = plan
        .tensor_data_offset
        .checked_sub(table_end)
        .ok_or_else(|| invalid("index tables extend past tensor data offset"))?;
    write_zeros(file, path, None, padding)?;

    for segment_index in 0..plan.index.segments.len() {
        let (segment_id, component_id, stored_len, tensor_range) = {
            let segment = &plan.index.segments[segment_index];
            (
                segment.id,
                segment.component_id,
                segment.stored_len,
                segment.tensor_range.clone(),
            )
        };
        let tensor_start = usize::try_from(tensor_range.start).map_err(|_| {
            invalid(format!(
                "segment {segment_id} tensor start does not fit usize"
            ))
        })?;
        let tensor_end = usize::try_from(tensor_range.end).map_err(|_| {
            invalid(format!(
                "segment {segment_id} tensor end does not fit usize"
            ))
        })?;
        let records = plan
            .index
            .tensors
            .get(tensor_start..tensor_end)
            .ok_or_else(|| {
                invalid(format!(
                    "segment {segment_id} tensor range {:?} is outside the tensor table",
                    tensor_range
                ))
            })?;
        let mut hasher = Sha256::new();
        let mut cursor = 0u64;
        for record in records {
            if record.component_id != component_id {
                return Err(invalid(format!(
                    "segment {segment_id} contains tensor {} from component {}",
                    record.info.name, record.component_id
                )));
            }
            let gap = record.segment_offset.checked_sub(cursor).ok_or_else(|| {
                invalid(format!(
                    "component {component_id} segment {segment_id} tensor {} starts before cursor {cursor}",
                    record.info.name
                ))
            })?;
            write_zeros(file, path, Some(&mut hasher), gap)?;
            let source_index = usize::try_from(record.component_id).map_err(|_| {
                invalid(format!(
                    "component {} does not fit the source index type",
                    record.component_id
                ))
            })?;
            let source = sources.get(source_index).ok_or_else(|| {
                invalid(format!(
                    "missing export source for component {}",
                    record.component_id
                ))
            })?;
            let bytes = source
                .loader
                .tensor_slice(&record.info.name)
                .ok_or_else(|| {
                    invalid(format!(
                        "source tensor disappeared: component {} tensor {}",
                        record.component_id, record.info.name
                    ))
                })?;
            let actual_len = u64::try_from(bytes.len()).map_err(|_| {
                invalid(format!(
                    "source tensor length does not fit u64: {}",
                    record.info.name
                ))
            })?;
            if actual_len != record.byte_len {
                return Err(invalid(format!(
                    "source tensor length changed: component {} tensor {} expected {}, got {actual_len}",
                    record.component_id, record.info.name, record.byte_len
                )));
            }
            file.write_all(bytes)
                .map_err(|source| io_error("write source tensor", path, source))?;
            hasher.update(bytes);
            cursor = record
                .segment_offset
                .checked_add(record.byte_len)
                .ok_or_else(|| {
                    invalid(format!(
                        "component {} segment {segment_id} tensor {} end overflow",
                        record.component_id, record.info.name
                    ))
                })?;
        }
        let trailing = stored_len.checked_sub(cursor).ok_or_else(|| {
            invalid(format!(
                "component {component_id} segment {segment_id} payload {cursor} exceeds stored length {stored_len}"
            ))
        })?;
        write_zeros(file, path, Some(&mut hasher), trailing)?;
        plan.index.segments[segment_index].sha256 = hasher.finalize().into();
    }

    let tensor_data_end = file
        .stream_position()
        .map_err(|source| io_error("read tensor-data end position", path, source))?;
    if tensor_data_end != plan.declared_file_size {
        return Err(invalid(format!(
            "tensor data ended at {tensor_data_end}, expected {}",
            plan.declared_file_size
        )));
    }

    let segment_table = serialize_segment_table(&plan.index)?;
    let segment_table_len = u64::try_from(segment_table.len())
        .map_err(|_| invalid("segment table length does not fit u64"))?;
    if segment_table_len != plan.segment_table.length {
        return Err(invalid(format!(
            "final segment table length {segment_table_len} differs from planned {}",
            plan.segment_table.length
        )));
    }
    file.seek(SeekFrom::Start(plan.segment_table.offset))
        .map_err(|source| io_error("seek to segment table", path, source))?;
    file.write_all(&segment_table)
        .map_err(|source| io_error("rewrite segment table", path, source))?;
    let superblock = serialize_superblock(plan)?;
    file.seek(SeekFrom::Start(0))
        .map_err(|source| io_error("seek to superblock", path, source))?;
    file.write_all(&superblock)
        .map_err(|source| io_error("rewrite superblock", path, source))?;
    file.flush()
        .map_err(|source| io_error("flush temporary package", path, source))?;
    file.sync_all()
        .map_err(|source| io_error("sync temporary package", path, source))?;
    let actual_file_size = file
        .seek(SeekFrom::End(0))
        .map_err(|source| io_error("read final package size", path, source))?;
    if actual_file_size != plan.declared_file_size {
        return Err(invalid(format!(
            "final package size {actual_file_size} differs from declared {}",
            plan.declared_file_size
        )));
    }
    Ok(())
}

static PENDING_OUTPUT_ID: AtomicU64 = AtomicU64::new(0);

struct PendingOutput {
    file: File,
    path: PathBuf,
    output: PathBuf,
    overwrite: bool,
    published: bool,
}

impl PendingOutput {
    fn create(output: &Path, overwrite: bool) -> Result<Self, GgufrsError> {
        if output.file_name().filter(|name| !name.is_empty()).is_none() {
            return Err(invalid(format!(
                "output path has no file name: {}",
                output.display()
            )));
        }
        if !overwrite && output.exists() {
            return Err(GgufrsError::OutputExists {
                path: output.to_path_buf(),
            });
        }
        let parent = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        loop {
            let id = PENDING_OUTPUT_ID.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".ggufrs-{}-{id}.tmp", std::process::id()));
            match OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .open(&path)
            {
                Ok(file) => {
                    return Ok(Self {
                        file,
                        path,
                        output: output.to_path_buf(),
                        overwrite,
                        published: false,
                    });
                }
                Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => {
                    return Err(GgufrsError::Io {
                        operation: "create temporary package",
                        path,
                        source,
                    });
                }
            }
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    #[cfg(unix)]
    fn path_matches_file(&self) -> Result<bool, GgufrsError> {
        use std::os::unix::fs::MetadataExt;

        let file = self
            .file
            .metadata()
            .map_err(|source| io_error("read temporary package identity", &self.path, source))?;
        let path = std::fs::symlink_metadata(&self.path).map_err(|source| {
            io_error("read temporary package path identity", &self.path, source)
        })?;
        Ok(file.dev() == path.dev() && file.ino() == path.ino())
    }

    #[cfg(windows)]
    fn path_matches_file(&self) -> Result<bool, GgufrsError> {
        use std::os::windows::fs::MetadataExt;

        let file = self
            .file
            .metadata()
            .map_err(|source| io_error("read temporary package identity", &self.path, source))?;
        let path = std::fs::symlink_metadata(&self.path).map_err(|source| {
            io_error("read temporary package path identity", &self.path, source)
        })?;
        Ok(file.volume_serial_number() == path.volume_serial_number()
            && file.file_index() == path.file_index())
    }

    #[cfg(not(any(unix, windows)))]
    fn path_matches_file(&self) -> Result<bool, GgufrsError> {
        Err(invalid(
            "temporary package identity checks are unsupported on this platform",
        ))
    }

    fn publish(mut self) -> Result<(), GgufrsError> {
        self.file
            .flush()
            .map_err(|source| io_error("flush temporary package", &self.path, source))?;
        let verification_file = self
            .file
            .try_clone()
            .map_err(|source| io_error("clone temporary package handle", &self.path, source))?;
        GgufrsFile::from_file(verification_file, self.path.clone())?.verify_all()?;
        if !self.path_matches_file()? {
            return Err(invalid(format!(
                "temporary package identity changed: {}",
                self.path.display()
            )));
        }
        if self.overwrite {
            std::fs::rename(&self.path, &self.output).map_err(|source| {
                GgufrsError::UnsupportedPublish {
                    path: self.output.clone(),
                    operation: "atomic replacement rename",
                    source,
                }
            })?;
        } else {
            match std::fs::hard_link(&self.path, &self.output) {
                Ok(()) => std::fs::remove_file(&self.path).map_err(|source| GgufrsError::Io {
                    operation: "remove published temporary link",
                    path: self.path.clone(),
                    source,
                })?,
                Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
                    return Err(GgufrsError::OutputExists {
                        path: self.output.clone(),
                    });
                }
                Err(source) => {
                    return Err(GgufrsError::UnsupportedPublish {
                        path: self.output.clone(),
                        operation: "no-replace hard link",
                        source,
                    });
                }
            }
        }
        self.published = true;
        let parent = self
            .output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|source| GgufrsError::Io {
                operation: "sync output directory",
                path: parent.to_path_buf(),
                source,
            })
    }
}

impl Drop for PendingOutput {
    fn drop(&mut self) {
        if !self.published && self.path_matches_file().unwrap_or(false) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub fn export_ggufrs(
    output: &Path,
    llm: &Path,
    mmproj: Option<&Path>,
    options: ExportOptions,
) -> Result<(), GgufrsError> {
    if !options.overwrite && output.exists() {
        return Err(GgufrsError::OutputExists {
            path: output.to_path_buf(),
        });
    }
    let mut sources = vec![load_export_source(ComponentRole::Llm, llm)?];
    if let Some(path) = mmproj {
        sources.push(load_export_source(ComponentRole::Mmproj, path)?);
    }
    let mut plan = plan_export(&sources)?;
    let mut pending = PendingOutput::create(output, options.overwrite)?;
    let pending_path = pending.path().to_path_buf();
    write_planned_package(pending.file_mut(), &pending_path, &sources, &mut plan)?;
    pending.publish()
}

pub fn open_model_source(
    path: &Path,
    role: ComponentRole,
) -> Result<Box<dyn TensorSource>, GgufrsError> {
    let mut file = File::open(path).map_err(|source| GgufrsError::Io {
        operation: "open model source",
        path: path.to_path_buf(),
        source,
    })?;
    let mut magic = [0u8; 8];
    file.read_exact(&mut magic)
        .map_err(|source| GgufrsError::Io {
            operation: "read model magic",
            path: path.to_path_buf(),
            source,
        })?;
    // GGUFRS starts with GGUF, so the exact eight-byte magic must win.
    if &magic == GGUFRS_MAGIC {
        return GgufrsFile::open(path)?
            .load_component(role)
            .map(|component| Box::new(component) as Box<dyn TensorSource>);
    }
    if &magic[..4] == b"GGUF" {
        return GGUFLoader::from_file(path)
            .map(|loader| Box::new(loader) as Box<dyn TensorSource>)
            .map_err(|message| GgufrsError::SourceGguf {
                role,
                path: path.to_path_buf(),
                message,
            });
    }
    Err(invalid(format!(
        "unknown model magic in {}",
        path.display()
    )))
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    fn test_put_u32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn test_put_i32(out: &mut Vec<u8>, value: i32) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn test_put_u64(out: &mut Vec<u8>, value: u64) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn test_put_string(out: &mut Vec<u8>, value: &str) {
        test_put_u64(out, value.len() as u64);
        out.extend_from_slice(value.as_bytes());
    }

    fn test_put_component(
        out: &mut Vec<u8>,
        id: u32,
        role: ComponentRole,
        name: &str,
        metadata_start: u32,
        metadata_count: u32,
        tensor_start: u32,
        tensor_count: u32,
        segment_start: u32,
        segment_count: u32,
    ) {
        test_put_u32(out, id);
        test_put_u32(out, role as u32);
        test_put_string(out, name);
        for value in [
            metadata_start,
            metadata_count,
            tensor_start,
            tensor_count,
            segment_start,
            segment_count,
        ] {
            test_put_u32(out, value);
        }
    }

    fn test_put_tensor(
        out: &mut Vec<u8>,
        component_id: u32,
        segment_id: u32,
        name: &str,
        ggml_type: GGMLType,
        dims: &[u64],
        segment_offset: u64,
        byte_len: u64,
    ) {
        test_put_u32(out, component_id);
        test_put_u32(out, segment_id);
        test_put_string(out, name);
        test_put_i32(out, ggml_type as i32);
        test_put_u32(out, dims.len() as u32);
        for dimension in dims {
            test_put_u64(out, *dimension);
        }
        test_put_u64(out, segment_offset);
        test_put_u64(out, byte_len);
    }

    fn test_put_segment(
        out: &mut Vec<u8>,
        id: u32,
        component_id: u32,
        kind: SegmentKind,
        layer: Option<u32>,
        absolute_offset: u64,
        tensor_start: u32,
        tensor_count: u32,
        sha256: [u8; 32],
    ) {
        test_put_u32(out, id);
        test_put_u32(out, component_id);
        test_put_u32(out, kind as u32);
        test_put_i32(out, layer.map(|value| value as i32).unwrap_or(-1));
        test_put_u64(out, absolute_offset);
        test_put_u64(out, GGUFRS_SEGMENT_ALIGNMENT);
        test_put_u32(out, tensor_start);
        test_put_u32(out, tensor_count);
        out.extend_from_slice(&sha256);
    }

    fn package_fixture_bytes_with(
        second_tensor_name: &str,
        second_segment_id: u32,
        second_segment_offset: u64,
        segment0_tensor_count: u32,
        segment1_tensor_start: u32,
        segment1_tensor_count: u32,
    ) -> Vec<u8> {
        let mut components = Vec::new();
        test_put_component(
            &mut components,
            0,
            ComponentRole::Llm,
            "llm",
            0,
            4,
            0,
            2,
            0,
            2,
        );
        test_put_component(
            &mut components,
            1,
            ComponentRole::Mmproj,
            "mmproj",
            4,
            1,
            2,
            1,
            2,
            1,
        );

        let mut metadata = Vec::new();
        test_put_u32(&mut metadata, 0);
        test_put_string(&mut metadata, "general.alignment");
        test_put_i32(&mut metadata, MetaValueType::Uint32 as i32);
        test_put_u32(&mut metadata, 32);
        test_put_u32(&mut metadata, 0);
        test_put_string(&mut metadata, "general.architecture");
        test_put_i32(&mut metadata, MetaValueType::String as i32);
        test_put_string(&mut metadata, "qwen3");
        test_put_u32(&mut metadata, 0);
        test_put_string(&mut metadata, "general.name");
        test_put_i32(&mut metadata, MetaValueType::String as i32);
        test_put_string(&mut metadata, "test-llm");
        test_put_u32(&mut metadata, 0);
        test_put_string(&mut metadata, "qwen3.block_count");
        test_put_i32(&mut metadata, MetaValueType::Uint32 as i32);
        test_put_u32(&mut metadata, 1);
        test_put_u32(&mut metadata, 1);
        test_put_string(&mut metadata, "general.name");
        test_put_i32(&mut metadata, MetaValueType::String as i32);
        test_put_string(&mut metadata, "test-mmproj");

        let mut tensors = Vec::new();
        test_put_tensor(
            &mut tensors,
            0,
            0,
            "token_embd.weight",
            GGMLType::F32,
            &[32],
            0,
            128,
        );
        test_put_tensor(
            &mut tensors,
            0,
            second_segment_id,
            second_tensor_name,
            GGMLType::Q8_0,
            &[32],
            second_segment_offset,
            34,
        );
        test_put_tensor(
            &mut tensors,
            1,
            2,
            "mm.0.weight",
            GGMLType::F16,
            &[32],
            0,
            64,
        );

        const SEGMENT_TABLE_LEN: u64 = 3 * 72;
        let component_table = TableRange {
            offset: SUPERBLOCK_LEN as u64,
            length: components.len() as u64,
        };
        let metadata_table = TableRange {
            offset: component_table.offset + component_table.length,
            length: metadata.len() as u64,
        };
        let segment_table = TableRange {
            offset: metadata_table.offset + metadata_table.length,
            length: SEGMENT_TABLE_LEN,
        };
        let tensor_table = TableRange {
            offset: segment_table.offset + segment_table.length,
            length: tensors.len() as u64,
        };
        let table_end = tensor_table.offset + tensor_table.length;
        let tensor_data_offset = (table_end + GGUFRS_SEGMENT_ALIGNMENT - 1)
            / GGUFRS_SEGMENT_ALIGNMENT
            * GGUFRS_SEGMENT_ALIGNMENT;

        let mut payloads = vec![
            vec![0u8; GGUFRS_SEGMENT_ALIGNMENT as usize],
            vec![0u8; GGUFRS_SEGMENT_ALIGNMENT as usize],
            vec![0u8; GGUFRS_SEGMENT_ALIGNMENT as usize],
        ];
        let second_payload = &mut payloads[second_segment_id as usize];
        let second_start = second_segment_offset as usize;
        second_payload[second_start..second_start + 2]
            .copy_from_slice(&half::f16::from_f32(1.0).to_le_bytes());
        second_payload[second_start + 2..second_start + 34].fill(1);
        payloads[2][..64].fill(0x3c);

        let hashes: Vec<[u8; 32]> = payloads
            .iter()
            .map(|payload| Sha256::digest(payload).into())
            .collect();
        let mut segments = Vec::new();
        test_put_segment(
            &mut segments,
            0,
            0,
            SegmentKind::Shared,
            None,
            tensor_data_offset,
            0,
            segment0_tensor_count,
            hashes[0],
        );
        test_put_segment(
            &mut segments,
            1,
            0,
            SegmentKind::Layer,
            Some(0),
            tensor_data_offset + GGUFRS_SEGMENT_ALIGNMENT,
            segment1_tensor_start,
            segment1_tensor_count,
            hashes[1],
        );
        test_put_segment(
            &mut segments,
            2,
            1,
            SegmentKind::Component,
            None,
            tensor_data_offset + 2 * GGUFRS_SEGMENT_ALIGNMENT,
            2,
            1,
            hashes[2],
        );
        assert_eq!(segments.len() as u64, SEGMENT_TABLE_LEN);

        let declared_file_size = tensor_data_offset + 3 * GGUFRS_SEGMENT_ALIGNMENT;
        let mut output = Vec::new();
        output.extend_from_slice(GGUFRS_MAGIC);
        test_put_u32(&mut output, GGUFRS_VERSION);
        test_put_u32(&mut output, 0);
        test_put_u64(&mut output, declared_file_size);
        for count in [2, 5, 3, 3] {
            test_put_u32(&mut output, count);
        }
        for table in [component_table, metadata_table, segment_table, tensor_table] {
            test_put_u64(&mut output, table.offset);
            test_put_u64(&mut output, table.length);
        }
        test_put_u64(&mut output, tensor_data_offset);
        output.extend_from_slice(&[0u8; 16]);
        assert_eq!(output.len(), SUPERBLOCK_LEN);
        output.extend_from_slice(&components);
        output.extend_from_slice(&metadata);
        output.extend_from_slice(&segments);
        output.extend_from_slice(&tensors);
        output.resize(tensor_data_offset as usize, 0);
        for payload in payloads {
            output.extend_from_slice(&payload);
        }
        assert_eq!(output.len() as u64, declared_file_size);
        output
    }

    pub(crate) fn package_fixture_bytes() -> Vec<u8> {
        package_fixture_bytes_with("blk.0.weight", 1, 0, 1, 1, 1)
    }

    pub(crate) fn package_fixture_with_second_tensor(
        name: &str,
        segment_id: u32,
        segment_offset: u64,
        segment0_tensor_count: u32,
        segment1_tensor_start: u32,
        segment1_tensor_count: u32,
    ) -> Vec<u8> {
        package_fixture_bytes_with(
            name,
            segment_id,
            segment_offset,
            segment0_tensor_count,
            segment1_tensor_start,
            segment1_tensor_count,
        )
    }

    static TEST_FILE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    pub(crate) fn write_package_bytes(bytes: &[u8]) -> PathBuf {
        let id = TEST_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("rmi-ggufrs-{}-{id}.ggufrs", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    pub(crate) fn test_package() -> (PathBuf, GgufrsFile) {
        let path = write_package_bytes(&package_fixture_bytes());
        let package = GgufrsFile::open(&path).unwrap();
        (path, package)
    }

    #[derive(Clone)]
    pub(crate) struct SourceTensor {
        pub(crate) name: &'static str,
        pub(crate) ggml_type: GGMLType,
        pub(crate) dims: Vec<u64>,
        pub(crate) bytes: Vec<u8>,
    }

    fn test_meta_type(value: &MetaValue) -> MetaValueType {
        match value {
            MetaValue::Uint8(_) => MetaValueType::Uint8,
            MetaValue::Int8(_) => MetaValueType::Int8,
            MetaValue::Uint16(_) => MetaValueType::Uint16,
            MetaValue::Int16(_) => MetaValueType::Int16,
            MetaValue::Uint32(_) => MetaValueType::Uint32,
            MetaValue::Int32(_) => MetaValueType::Int32,
            MetaValue::Float32(_) => MetaValueType::Float32,
            MetaValue::Bool(_) => MetaValueType::Bool,
            MetaValue::String(_) => MetaValueType::String,
            MetaValue::Uint64(_) => MetaValueType::Uint64,
            MetaValue::Int64(_) => MetaValueType::Int64,
            MetaValue::Float64(_) => MetaValueType::Float64,
            MetaValue::Array(_, _) => MetaValueType::Array,
        }
    }

    fn test_put_meta_value(out: &mut Vec<u8>, value: &MetaValue) {
        match value {
            MetaValue::Uint8(value) => out.push(*value),
            MetaValue::Int8(value) => out.push(*value as u8),
            MetaValue::Uint16(value) => out.extend_from_slice(&value.to_le_bytes()),
            MetaValue::Int16(value) => out.extend_from_slice(&value.to_le_bytes()),
            MetaValue::Uint32(value) => test_put_u32(out, *value),
            MetaValue::Int32(value) => test_put_i32(out, *value),
            MetaValue::Float32(value) => out.extend_from_slice(&value.to_le_bytes()),
            MetaValue::Bool(value) => out.push(u8::from(*value)),
            MetaValue::String(value) => test_put_string(out, value),
            MetaValue::Uint64(value) => test_put_u64(out, *value),
            MetaValue::Int64(value) => out.extend_from_slice(&value.to_le_bytes()),
            MetaValue::Float64(value) => out.extend_from_slice(&value.to_le_bytes()),
            MetaValue::Array(element_type, values) => {
                test_put_i32(out, *element_type as i32);
                test_put_u64(out, values.len() as u64);
                for value in values {
                    test_put_meta_value(out, value);
                }
            }
        }
    }

    pub(crate) fn write_test_gguf(
        path: &Path,
        metadata: &[(String, MetaValue)],
        tensors: &[SourceTensor],
    ) {
        let mut output = Vec::new();
        output.extend_from_slice(b"GGUF");
        test_put_u32(&mut output, 3);
        test_put_u64(&mut output, tensors.len() as u64);
        test_put_u64(&mut output, metadata.len() as u64);
        for (key, value) in metadata {
            test_put_string(&mut output, key);
            test_put_i32(&mut output, test_meta_type(value) as i32);
            test_put_meta_value(&mut output, value);
        }

        let mut relative_offset = 0u64;
        for tensor in tensors {
            assert_eq!(
                tensor.bytes.len() as u64,
                TensorInfo {
                    name: tensor.name.into(),
                    dims: tensor.dims.clone(),
                    ggml_type: tensor.ggml_type,
                    offset: relative_offset,
                }
                .checked_nbytes()
                .unwrap()
            );
            test_put_string(&mut output, tensor.name);
            test_put_u32(&mut output, tensor.dims.len() as u32);
            for dimension in &tensor.dims {
                test_put_u64(&mut output, *dimension);
            }
            test_put_i32(&mut output, tensor.ggml_type as i32);
            test_put_u64(&mut output, relative_offset);
            relative_offset = align_up(relative_offset + tensor.bytes.len() as u64, 32).unwrap();
        }

        output.resize(align_up(output.len() as u64, 32).unwrap() as usize, 0);
        for tensor in tensors {
            output.extend_from_slice(&tensor.bytes);
            output.resize(align_up(output.len() as u64, 32).unwrap() as usize, 0);
        }
        std::fs::write(path, output).unwrap();
    }

    pub(crate) struct TestInputs {
        pub(crate) dir: PathBuf,
        pub(crate) llm: PathBuf,
        pub(crate) mmproj: PathBuf,
        pub(crate) llm_shared: Vec<u8>,
        pub(crate) llm_blk0: Vec<u8>,
        pub(crate) llm_blk1: Vec<u8>,
        pub(crate) mmproj_weight: Vec<u8>,
    }

    pub(crate) struct LoadFixture {
        pub(crate) package: GgufrsFile,
        pub(crate) llm_component: u32,
        #[allow(dead_code)]
        pub(crate) inputs: TestInputs,
    }

    impl Drop for TestInputs {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    pub(crate) fn test_gguf_pair() -> TestInputs {
        let id = TEST_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("rmi-ggufrs-export-{}-{id}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let llm = dir.join("llm.gguf");
        let mmproj = dir.join("mmproj.gguf");
        let llm_shared = vec![0x11; 128];
        let mut llm_blk0 = vec![0x22; 34];
        llm_blk0[..2].copy_from_slice(&half::f16::from_f32(1.0).to_bits().to_le_bytes());
        let mut llm_blk1 = vec![0x33; 34];
        llm_blk1[..2].copy_from_slice(&half::f16::from_f32(0.5).to_bits().to_le_bytes());
        let mmproj_weight = vec![0x44; 64];

        write_test_gguf(
            &llm,
            &[
                ("general.alignment".into(), MetaValue::Uint32(32)),
                (
                    "general.architecture".into(),
                    MetaValue::String("qwen3".into()),
                ),
                ("general.name".into(), MetaValue::String("test-llm".into())),
                ("qwen3.block_count".into(), MetaValue::Uint32(2)),
            ],
            &[
                SourceTensor {
                    name: "token_embd.weight",
                    ggml_type: GGMLType::F32,
                    dims: vec![32],
                    bytes: llm_shared.clone(),
                },
                SourceTensor {
                    name: "blk.0.weight",
                    ggml_type: GGMLType::Q8_0,
                    dims: vec![32],
                    bytes: llm_blk0.clone(),
                },
                SourceTensor {
                    name: "blk.1.weight",
                    ggml_type: GGMLType::Q8_0,
                    dims: vec![32],
                    bytes: llm_blk1.clone(),
                },
            ],
        );
        write_test_gguf(
            &mmproj,
            &[
                (
                    "clip.vision.attention.head_count".into(),
                    MetaValue::Uint32(1),
                ),
                (
                    "clip.vision.attention.layer_norm_epsilon".into(),
                    MetaValue::Float32(1e-6),
                ),
                ("clip.vision.block_count".into(), MetaValue::Uint32(1)),
                ("clip.vision.embedding_length".into(), MetaValue::Uint32(32)),
                (
                    "clip.vision.feed_forward_length".into(),
                    MetaValue::Uint32(32),
                ),
                ("clip.vision.image_size".into(), MetaValue::Uint32(32)),
                ("clip.vision.patch_size".into(), MetaValue::Uint32(16)),
                ("clip.vision.projection_dim".into(), MetaValue::Uint32(32)),
                ("general.alignment".into(), MetaValue::Uint32(32)),
                (
                    "general.name".into(),
                    MetaValue::String("test-mmproj".into()),
                ),
            ],
            &[
                SourceTensor {
                    name: "v.patch_embd.weight",
                    ggml_type: GGMLType::F16,
                    dims: vec![32],
                    bytes: vec![0x55; 64],
                },
                SourceTensor {
                    name: "mm.0.weight",
                    ggml_type: GGMLType::F16,
                    dims: vec![32],
                    bytes: mmproj_weight.clone(),
                },
                SourceTensor {
                    name: "mm.2.weight",
                    ggml_type: GGMLType::F16,
                    dims: vec![32],
                    bytes: vec![0x66; 64],
                },
            ],
        );
        TestInputs {
            dir,
            llm,
            mmproj,
            llm_shared,
            llm_blk0,
            llm_blk1,
            mmproj_weight,
        }
    }

    pub(crate) fn test_q8_row_package(rows: u64, row_elements: u64) -> LoadFixture {
        let inputs = test_gguf_pair();
        let row_bytes = (row_elements / 32) * 34;
        write_test_gguf(
            &inputs.llm,
            &[
                ("general.alignment".into(), MetaValue::Uint32(32)),
                (
                    "general.architecture".into(),
                    MetaValue::String("qwen3".into()),
                ),
                ("qwen3.block_count".into(), MetaValue::Uint32(1)),
            ],
            &[
                SourceTensor {
                    name: "token_embd.weight",
                    ggml_type: GGMLType::F32,
                    dims: vec![32],
                    bytes: vec![0x11; 128],
                },
                SourceTensor {
                    name: "blk.0.weight",
                    ggml_type: GGMLType::Q8_0,
                    dims: vec![row_elements, rows],
                    bytes: vec![0x22; (row_bytes * rows) as usize],
                },
            ],
        );
        let output = inputs.dir.join("q8-rows.ggufrs");
        export_ggufrs(&output, &inputs.llm, None, ExportOptions::default()).unwrap();
        let package = GgufrsFile::open(output).unwrap();
        let llm_component = package.component_id(ComponentRole::Llm).unwrap();
        LoadFixture {
            package,
            llm_component,
            inputs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_is_deterministic_scoped_and_byte_exact() {
        use super::test_support::*;

        let inputs = test_gguf_pair();
        let a = inputs.dir.join("a.ggufrs");
        let b = inputs.dir.join("b.ggufrs");
        export_ggufrs(
            &a,
            &inputs.llm,
            Some(&inputs.mmproj),
            ExportOptions::default(),
        )
        .unwrap();
        export_ggufrs(
            &b,
            &inputs.llm,
            Some(&inputs.mmproj),
            ExportOptions::default(),
        )
        .unwrap();
        assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());

        let package = GgufrsFile::open(&a).unwrap();
        package.verify_all().unwrap();
        let llm = package.load_component(ComponentRole::Llm).unwrap();
        let mmproj = package.load_component(ComponentRole::Mmproj).unwrap();
        assert_eq!(
            llm.tensor_slice("token_embd.weight").unwrap(),
            inputs.llm_shared
        );
        assert_eq!(llm.tensor_slice("blk.0.weight").unwrap(), inputs.llm_blk0);
        assert_eq!(llm.tensor_slice("blk.1.weight").unwrap(), inputs.llm_blk1);
        assert_eq!(
            mmproj.tensor_slice("mm.0.weight").unwrap(),
            inputs.mmproj_weight
        );
        assert_eq!(
            llm.metadata("general.name")
                .and_then(MetaValue::to_string_val),
            Some("test-llm")
        );
        assert_eq!(
            mmproj
                .metadata("general.name")
                .and_then(MetaValue::to_string_val),
            Some("test-mmproj")
        );
    }

    #[test]
    fn gguf_and_ggufrs_expose_the_same_tensor_contract() {
        use super::test_support::*;

        let fixture = test_gguf_pair();
        let output = fixture.dir.join("model.ggufrs");
        export_ggufrs(
            &output,
            &fixture.llm,
            Some(&fixture.mmproj),
            ExportOptions::default(),
        )
        .unwrap();

        let gguf = GGUFLoader::from_file(&fixture.llm).unwrap();
        let package = GgufrsFile::open(&output).unwrap();
        let loaded = package.load_component(ComponentRole::Llm).unwrap();
        for source_info in gguf.tensors() {
            let loaded_info = loaded.tensor_info(&source_info.name).unwrap();
            assert_eq!(loaded_info.name, source_info.name);
            assert_eq!(loaded_info.dims, source_info.dims);
            assert_eq!(loaded_info.ggml_type, source_info.ggml_type);
            assert_eq!(
                loaded.tensor_slice(&source_info.name).unwrap(),
                gguf.tensor_slice(&source_info.name).unwrap()
            );
        }

        let mut gguf_metadata = gguf
            .metadata_entries()
            .iter()
            .map(|(key, value)| (key, value))
            .collect::<Vec<_>>();
        gguf_metadata.sort_by(|(left, _), (right, _)| left.as_bytes().cmp(right.as_bytes()));
        assert_eq!(
            gguf_metadata,
            loaded.component_metadata_entries().collect::<Vec<_>>(),
        );
    }

    #[test]
    fn export_supports_llm_without_mmproj() {
        let inputs = test_support::test_gguf_pair();
        let output = inputs.dir.join("llm-only.ggufrs");

        export_ggufrs(&output, &inputs.llm, None, ExportOptions::default()).unwrap();

        let package = GgufrsFile::open(output).unwrap();
        package.verify_all().unwrap();
        assert_eq!(package.components().len(), 1);
        assert!(package.component_id(ComponentRole::Llm).is_some());
        assert!(package.component_id(ComponentRole::Mmproj).is_none());
    }

    #[test]
    fn export_never_clobbers_without_explicit_overwrite() {
        use super::test_support::*;

        let inputs = test_gguf_pair();
        let output = inputs.dir.join("model.ggufrs");
        std::fs::write(&output, b"keep-me").unwrap();
        let error = export_ggufrs(
            &output,
            &inputs.llm,
            Some(&inputs.mmproj),
            ExportOptions::default(),
        )
        .unwrap_err();
        assert!(matches!(error, GgufrsError::OutputExists { .. }));
        assert_eq!(std::fs::read(&output).unwrap(), b"keep-me");

        export_ggufrs(
            &output,
            &inputs.llm,
            Some(&inputs.mmproj),
            ExportOptions { overwrite: true },
        )
        .unwrap();
        GgufrsFile::open(output).unwrap().verify_all().unwrap();
    }

    #[test]
    fn failed_validation_removes_only_the_new_temp_file() {
        use super::test_support::*;

        let inputs = test_gguf_pair();
        let output = inputs.dir.join("model.ggufrs");
        let temp = PendingOutput::create(&output, false).unwrap();
        std::fs::write(temp.path(), b"not-a-package").unwrap();
        let path = temp.path().to_path_buf();
        assert!(temp.publish().is_err());
        assert!(!path.exists());
        assert!(!output.exists());
        assert!(inputs.llm.exists());
        assert!(inputs.mmproj.exists());
    }

    #[test]
    fn replaced_temp_path_cannot_truncate_or_publish_an_unrelated_inode() {
        let inputs = test_support::test_gguf_pair();
        let output = inputs.dir.join("model.ggufrs");
        let unrelated = inputs.dir.join("unrelated.ggufrs");
        let unrelated_bytes = test_support::package_fixture_bytes();
        std::fs::write(&unrelated, &unrelated_bytes).unwrap();

        let mut temp = PendingOutput::create(&output, false).unwrap();
        let temp_path = temp.path().to_path_buf();
        std::fs::remove_file(&temp_path).unwrap();
        std::fs::hard_link(&unrelated, &temp_path).unwrap();

        temp.file_mut()
            .write_all(&test_support::package_fixture_bytes())
            .unwrap();
        assert_eq!(std::fs::read(&unrelated).unwrap(), unrelated_bytes);

        let error = temp.publish().unwrap_err();

        match error {
            GgufrsError::InvalidFormat { context } => {
                assert!(
                    context.contains("temporary package identity changed"),
                    "{context}"
                )
            }
            other => panic!("expected InvalidFormat, got {other}"),
        }
        assert!(!output.exists());
        assert_eq!(std::fs::read(&unrelated).unwrap(), unrelated_bytes);
        assert_eq!(std::fs::read(&temp_path).unwrap(), unrelated_bytes);
    }

    #[test]
    fn no_clobber_publish_loses_race_without_replacing_winner() {
        let inputs = test_support::test_gguf_pair();
        let output = inputs.dir.join("model.ggufrs");
        let temp = PendingOutput::create(&output, false).unwrap();
        std::fs::write(temp.path(), test_support::package_fixture_bytes()).unwrap();
        let temp_path = temp.path().to_path_buf();
        std::fs::write(&output, b"race-winner").unwrap();

        let error = temp.publish().unwrap_err();

        assert!(matches!(error, GgufrsError::OutputExists { .. }));
        assert_eq!(std::fs::read(&output).unwrap(), b"race-winner");
        assert!(!temp_path.exists());
    }

    fn rewrite_test_llm(inputs: &test_support::TestInputs, metadata: &[(String, MetaValue)]) {
        use super::test_support::{write_test_gguf, SourceTensor};

        write_test_gguf(
            &inputs.llm,
            metadata,
            &[
                SourceTensor {
                    name: "token_embd.weight",
                    ggml_type: GGMLType::F32,
                    dims: vec![32],
                    bytes: inputs.llm_shared.clone(),
                },
                SourceTensor {
                    name: "blk.0.weight",
                    ggml_type: GGMLType::Q8_0,
                    dims: vec![32],
                    bytes: inputs.llm_blk0.clone(),
                },
                SourceTensor {
                    name: "blk.1.weight",
                    ggml_type: GGMLType::Q8_0,
                    dims: vec![32],
                    bytes: inputs.llm_blk1.clone(),
                },
            ],
        );
    }

    #[test]
    fn export_rejects_invalid_source_alignment() {
        let inputs = test_support::test_gguf_pair();
        rewrite_test_llm(
            &inputs,
            &[
                ("general.alignment".into(), MetaValue::Uint32(3)),
                (
                    "general.architecture".into(),
                    MetaValue::String("qwen3".into()),
                ),
                ("qwen3.block_count".into(), MetaValue::Uint32(2)),
            ],
        );

        let error = export_ggufrs(
            &inputs.dir.join("model.ggufrs"),
            &inputs.llm,
            None,
            ExportOptions::default(),
        )
        .unwrap_err();

        match error {
            GgufrsError::SourceGguf { message, .. } => {
                assert!(message.contains("nonzero power of two"), "{message}")
            }
            other => panic!("expected SourceGguf, got {other}"),
        }
    }

    #[test]
    fn export_rejects_block_count_larger_than_source_tensor_budget() {
        let inputs = test_support::test_gguf_pair();
        rewrite_test_llm(
            &inputs,
            &[
                (
                    "general.architecture".into(),
                    MetaValue::String("qwen3".into()),
                ),
                ("qwen3.block_count".into(), MetaValue::Uint32(10)),
            ],
        );

        let error = export_ggufrs(
            &inputs.dir.join("model.ggufrs"),
            &inputs.llm,
            None,
            ExportOptions::default(),
        )
        .unwrap_err();

        match error {
            GgufrsError::SourceGguf { message, .. } => {
                assert!(
                    message.contains("requires at least 11 tensors"),
                    "{message}"
                )
            }
            other => panic!("expected SourceGguf, got {other}"),
        }
    }

    #[test]
    fn export_rejects_nested_metadata_arrays() {
        let inputs = test_support::test_gguf_pair();
        rewrite_test_llm(
            &inputs,
            &[
                (
                    "general.architecture".into(),
                    MetaValue::String("qwen3".into()),
                ),
                ("qwen3.block_count".into(), MetaValue::Uint32(2)),
                (
                    "test.nested".into(),
                    MetaValue::Array(
                        MetaValueType::Array,
                        vec![MetaValue::Array(
                            MetaValueType::Uint32,
                            vec![MetaValue::Uint32(1)],
                        )],
                    ),
                ),
            ],
        );

        let error = export_ggufrs(
            &inputs.dir.join("model.ggufrs"),
            &inputs.llm,
            None,
            ExportOptions::default(),
        )
        .unwrap_err();

        match error {
            GgufrsError::InvalidFormat { context } => {
                assert!(context.contains("metadata array"), "{context}")
            }
            other => panic!("expected InvalidFormat, got {other}"),
        }
    }

    #[test]
    fn metadata_writer_rejects_empty_nested_array_before_writing() {
        let mut out = vec![0xaa];

        let error = put_meta_value(
            &mut out,
            &MetaValue::Array(MetaValueType::Array, Vec::new()),
        )
        .unwrap_err();

        assert!(matches!(error, GgufrsError::InvalidFormat { .. }));
        assert_eq!(out, [0xaa]);
    }

    #[test]
    fn export_rejects_out_of_range_llm_layer() {
        let inputs = test_support::test_gguf_pair();
        use super::test_support::{write_test_gguf, SourceTensor};
        write_test_gguf(
            &inputs.llm,
            &[
                (
                    "general.architecture".into(),
                    MetaValue::String("qwen3".into()),
                ),
                ("qwen3.block_count".into(), MetaValue::Uint32(2)),
            ],
            &[
                SourceTensor {
                    name: "token_embd.weight",
                    ggml_type: GGMLType::F32,
                    dims: vec![32],
                    bytes: inputs.llm_shared.clone(),
                },
                SourceTensor {
                    name: "blk.0.weight",
                    ggml_type: GGMLType::Q8_0,
                    dims: vec![32],
                    bytes: inputs.llm_blk0.clone(),
                },
                SourceTensor {
                    name: "blk.2.weight",
                    ggml_type: GGMLType::Q8_0,
                    dims: vec![32],
                    bytes: inputs.llm_blk1.clone(),
                },
            ],
        );

        let error = export_ggufrs(
            &inputs.dir.join("model.ggufrs"),
            &inputs.llm,
            None,
            ExportOptions::default(),
        )
        .unwrap_err();

        match error {
            GgufrsError::InvalidFormat { context } => {
                assert!(context.contains("outside block count 2"), "{context}")
            }
            other => panic!("expected InvalidFormat, got {other}"),
        }
    }

    fn read_u64_at(bytes: &[u8], offset: usize) -> u64 {
        u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
    }

    fn put_u32_at(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u64_at(bytes: &mut [u8], offset: usize, value: u64) {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn find_once(bytes: &[u8], needle: &[u8]) -> usize {
        let mut matches = bytes
            .windows(needle.len())
            .enumerate()
            .filter_map(|(offset, window)| (window == needle).then_some(offset));
        let offset = matches.next().expect("fixture field exists");
        assert!(matches.next().is_none(), "fixture field is unique");
        offset
    }

    fn assert_invalid(bytes: Vec<u8>, expected: &str) {
        use super::test_support::write_package_bytes;

        let path = write_package_bytes(&bytes);
        let error = match GgufrsFile::open(&path) {
            Ok(_) => panic!("invalid fixture was accepted"),
            Err(error) => error,
        };
        std::fs::remove_file(path).unwrap();
        match error {
            GgufrsError::InvalidFormat { context } => assert!(
                context.contains(expected),
                "expected {expected:?} in {context:?}"
            ),
            other => panic!("expected InvalidFormat, got {other}"),
        }
    }

    fn assert_checksum_mismatch(bytes: Vec<u8>, segment_id: u32) {
        use super::test_support::write_package_bytes;

        let path = write_package_bytes(&bytes);
        let package = GgufrsFile::open(&path).unwrap();
        let error = match package.load_component(ComponentRole::Llm) {
            Ok(_) => panic!("corrupt segment was accepted"),
            Err(error) => error,
        };
        drop(package);
        std::fs::remove_file(path).unwrap();
        match error {
            GgufrsError::ChecksumMismatch {
                component_id: 0,
                segment_id: actual_segment,
                ..
            } => assert_eq!(actual_segment, segment_id),
            other => panic!("expected ChecksumMismatch, got {other}"),
        }
    }

    fn assert_count_exceeds_table(
        mut bytes: Vec<u8>,
        count_offset: usize,
        table_length_offset: usize,
        minimum_entry_bytes: u64,
        table: &str,
    ) {
        let count = read_u64_at(&bytes, table_length_offset) / minimum_entry_bytes + 1;
        put_u32_at(&mut bytes, count_offset, count as u32);
        assert_invalid(bytes, &format!("{table} count exceeds remaining bytes"));
    }

    #[test]
    fn loaded_component_scopes_metadata_and_releases_segments() {
        use super::test_support::test_package;

        let (path, package) = test_package();
        let mut llm = package.load_component(ComponentRole::Llm).unwrap();
        let layer = package.layer_segment_id(llm.component_id(), 0).unwrap();

        assert_eq!(
            llm.metadata("general.name")
                .and_then(MetaValue::to_string_val),
            Some("test-llm")
        );
        assert!(llm.tensor_slice("blk.0.weight").is_some());
        assert!(llm.unmap_segment(layer).unwrap());
        assert!(llm.tensor_slice("blk.0.weight").is_none());
        llm.map_segment(layer).unwrap();
        assert!(llm.tensor_slice("blk.0.weight").is_some());

        let mmproj = package.load_component(ComponentRole::Mmproj).unwrap();
        assert_eq!(
            mmproj
                .metadata("general.name")
                .and_then(MetaValue::to_string_val),
            Some("test-mmproj")
        );
        drop(mmproj);
        drop(llm);
        drop(package);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_unsupported_version() {
        let mut bytes = test_support::package_fixture_bytes();
        put_u32_at(&mut bytes, 8, GGUFRS_VERSION + 1);
        assert_invalid(bytes, "version=2");
    }

    #[test]
    fn rejects_nonzero_flags() {
        let mut bytes = test_support::package_fixture_bytes();
        put_u32_at(&mut bytes, 12, 1);
        assert_invalid(bytes, "flags=1");
    }

    #[test]
    fn rejects_declared_size_mismatch() {
        let mut bytes = test_support::package_fixture_bytes();
        let declared = read_u64_at(&bytes, 16);
        put_u64_at(&mut bytes, 16, declared + 1);
        assert_invalid(bytes, "does not match actual file size");
    }

    #[test]
    fn rejects_nonzero_reserved_byte() {
        let mut bytes = test_support::package_fixture_bytes();
        bytes[SUPERBLOCK_LEN - 1] = 1;
        assert_invalid(bytes, "reserved superblock bytes must be zero");
    }

    #[test]
    fn rejects_table_outside_file() {
        let mut bytes = test_support::package_fixture_bytes();
        let outside = bytes.len() as u64 + 1;
        put_u64_at(&mut bytes, 40, outside);
        assert_invalid(bytes, "component table range");
    }

    #[test]
    fn rejects_component_count_exceeding_table() {
        assert_count_exceeds_table(
            test_support::package_fixture_bytes(),
            24,
            48,
            40,
            "component table",
        );
    }

    #[test]
    fn rejects_metadata_count_exceeding_table() {
        assert_count_exceeds_table(
            test_support::package_fixture_bytes(),
            28,
            64,
            17,
            "metadata table",
        );
    }

    #[test]
    fn rejects_segment_count_exceeding_table() {
        assert_count_exceeds_table(
            test_support::package_fixture_bytes(),
            32,
            80,
            72,
            "segment table",
        );
    }

    #[test]
    fn rejects_tensor_count_exceeding_table() {
        assert_count_exceeds_table(
            test_support::package_fixture_bytes(),
            36,
            96,
            40,
            "tensor table",
        );
    }

    #[test]
    fn rejects_tensor_rank_exceeding_remaining_entry_bytes() {
        let mut bytes = test_support::package_fixture_bytes();
        let tensor_name = find_once(&bytes, b"token_embd.weight");
        let rank_offset = tensor_name + b"token_embd.weight".len() + 4;
        put_u32_at(&mut bytes, rank_offset, u32::MAX);
        assert_invalid(
            bytes,
            "rank exceeds remaining bytes including trailing offsets",
        );
    }

    #[test]
    fn rejects_nested_metadata_arrays_before_recursive_decode() {
        let mut bytes = test_support::package_fixture_bytes();
        let key = find_once(&bytes, b"general.architecture");
        let value_type = key + b"general.architecture".len();
        put_u32_at(&mut bytes, value_type, MetaValueType::Array as u32);
        put_u32_at(&mut bytes, value_type + 4, MetaValueType::Array as u32);
        put_u64_at(&mut bytes, value_type + 8, 1);
        assert_invalid(bytes, "nested metadata arrays are not supported");
    }

    #[test]
    fn rejects_duplicate_component_metadata_key() {
        let mut bytes = test_support::package_fixture_bytes();
        let offset = find_once(&bytes, b"qwen3.block_count");
        bytes[offset..offset + b"general.alignment".len()].copy_from_slice(b"general.alignment");
        assert_invalid(bytes, "duplicate metadata key general.alignment");
    }

    #[test]
    fn rejects_duplicate_component_tensor_name() {
        let mut bytes =
            test_support::package_fixture_with_second_tensor("other_embd.weight", 1, 0, 1, 1, 1);
        let offset = find_once(&bytes, b"other_embd.weight");
        bytes[offset..offset + b"token_embd.weight".len()].copy_from_slice(b"token_embd.weight");
        assert_invalid(bytes, "duplicate tensor name token_embd.weight");
    }

    #[test]
    fn rejects_bad_tensor_component_reference() {
        let mut bytes = test_support::package_fixture_bytes();
        let tensor_name = find_once(&bytes, b"token_embd.weight");
        put_u32_at(&mut bytes, tensor_name - 16, 1);
        assert_invalid(bytes, "tensor token_embd.weight belongs to component 1");
    }

    #[test]
    fn rejects_bad_tensor_segment_reference() {
        let mut bytes = test_support::package_fixture_bytes();
        let tensor_name = find_once(&bytes, b"token_embd.weight");
        put_u32_at(&mut bytes, tensor_name - 12, 1);
        assert_invalid(bytes, "references component 0 segment 1");
    }

    #[test]
    fn rejects_unaligned_segment() {
        let mut bytes = test_support::package_fixture_bytes();
        let segment_table = read_u64_at(&bytes, 72) as usize;
        let offset = read_u64_at(&bytes, segment_table + 16);
        put_u64_at(&mut bytes, segment_table + 16, offset + 1);
        assert_invalid(bytes, "must be aligned");
    }

    #[test]
    fn rejects_overlapping_segments() {
        let mut bytes = test_support::package_fixture_bytes();
        let segment_table = read_u64_at(&bytes, 72) as usize;
        let first_offset = read_u64_at(&bytes, segment_table + 16);
        put_u64_at(&mut bytes, segment_table + 72 + 16, first_offset);
        assert_invalid(bytes, "overlaps the previous segment");
    }

    #[test]
    fn rejects_tensor_length_mismatch() {
        let mut bytes = test_support::package_fixture_bytes();
        let tensor_name = find_once(&bytes, b"token_embd.weight");
        let byte_len_offset = tensor_name + b"token_embd.weight".len() + 24;
        put_u64_at(&mut bytes, byte_len_offset, 129);
        assert_invalid(bytes, "byte length 129 differs from checked size 128");
    }

    #[test]
    fn rejects_overlapping_tensors() {
        let mut bytes =
            test_support::package_fixture_with_second_tensor("zzzzz_embd.weight", 0, 128, 2, 2, 0);
        let tensor_name = find_once(&bytes, b"zzzzz_embd.weight");
        let segment_offset = tensor_name + b"zzzzz_embd.weight".len() + 16;
        put_u64_at(&mut bytes, segment_offset, 96);
        assert_invalid(
            bytes,
            "tensors token_embd.weight and zzzzz_embd.weight overlap",
        );
    }

    #[test]
    fn changed_tensor_byte_fails_checksum() {
        let mut bytes = test_support::package_fixture_bytes();
        let tensor_data_offset = read_u64_at(&bytes, 104) as usize;
        bytes[tensor_data_offset] ^= 1;
        assert_checksum_mismatch(bytes, 0);
    }

    #[test]
    fn changed_trailing_padding_byte_fails_checksum() {
        let mut bytes = test_support::package_fixture_bytes();
        let tensor_data_offset = read_u64_at(&bytes, 104) as usize;
        bytes[tensor_data_offset + 128] ^= 1;
        assert_checksum_mismatch(bytes, 0);
    }

    #[test]
    fn rejects_nonzero_segment_padding_with_matching_checksum() {
        let mut bytes = test_support::package_fixture_bytes();
        let segment_table = read_u64_at(&bytes, 72) as usize;
        let tensor_data_offset = read_u64_at(&bytes, 104) as usize;
        bytes[tensor_data_offset + 128] = 1;
        let hash: [u8; 32] = Sha256::digest(
            &bytes[tensor_data_offset..tensor_data_offset + GGUFRS_SEGMENT_ALIGNMENT as usize],
        )
        .into();
        bytes[segment_table + 40..segment_table + 72].copy_from_slice(&hash);

        let path = test_support::write_package_bytes(&bytes);
        let package = GgufrsFile::open(&path).unwrap();
        let result = package.load_component(ComponentRole::Llm);
        drop(package);
        std::fs::remove_file(path).unwrap();
        match result {
            Err(GgufrsError::InvalidFormat { context }) => assert!(
                context.contains("nonzero padding"),
                "expected nonzero padding context, got {context:?}"
            ),
            Err(other) => panic!("expected InvalidFormat, got {other}"),
            Ok(_) => panic!("segment padding with a matching checksum was accepted"),
        }
    }

    #[test]
    fn open_does_not_map_sparse_segment_region() {
        const SPARSE_FILE_LEN: u64 = 1 << 48;

        let mut bytes = test_support::package_fixture_bytes();
        let segment_table = read_u64_at(&bytes, 72) as usize;
        let third_segment = segment_table + 2 * 72;
        let third_offset = read_u64_at(&bytes, third_segment + 16);
        put_u64_at(&mut bytes, 16, SPARSE_FILE_LEN);
        put_u64_at(
            &mut bytes,
            third_segment + 24,
            SPARSE_FILE_LEN - third_offset,
        );

        let path = test_support::write_package_bytes(&bytes);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(SPARSE_FILE_LEN)
            .unwrap();
        let result = GgufrsFile::open(&path).map(|package| package.components().len());
        std::fs::remove_file(path).unwrap();
        assert_eq!(result.unwrap(), 2);
    }

    #[test]
    fn open_model_source_prefers_exact_ggufrs_magic() {
        let path = test_support::write_package_bytes(&test_support::package_fixture_bytes());
        let source = open_model_source(&path, ComponentRole::Llm).unwrap();
        assert_eq!(
            source
                .metadata("general.name")
                .and_then(MetaValue::to_string_val),
            Some("test-llm")
        );
        drop(source);
        std::fs::remove_file(path).unwrap();
    }
}
