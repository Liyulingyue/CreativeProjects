use crate::model::{
    ByteReader, GGMLType, GGUFLoader, MetaValue, MetaValueType, TensorInfo, TensorSource,
};
use memmap2::{Mmap, MmapOptions};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::{Path, PathBuf};
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
        let mut file = File::open(&path).map_err(|source| GgufrsError::Io {
            operation: "open ggufrs",
            path: path.clone(),
            source,
        })?;
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

    pub(crate) fn tensors(&self) -> &[TensorRecord] {
        &self.index.tensors
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
