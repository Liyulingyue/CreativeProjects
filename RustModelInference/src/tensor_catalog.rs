use crate::{ComponentId, GGMLType, TensorInfo, TensorSource};
use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Gguf,
    Ggufrs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTensorRecord {
    pub info: TensorInfo,
    pub segment_id: u32,
    pub segment_byte_range: Range<u64>,
    pub layer: Option<u32>,
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
    pub segment_byte_range: Range<u64>,
    pub layer: Option<u32>,
    pub row_count: u64,
    pub row_bytes: u64,
}

pub struct TensorCatalog {
    sources: BTreeMap<ComponentId, Arc<dyn TensorSource>>,
    entries: Vec<TensorCatalogEntry>,
    by_name: BTreeMap<(ComponentId, String), TensorId>,
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("duplicate component source: {0:?}")]
    DuplicateComponent(ComponentId),
    #[error("duplicate tensor {name} in {component:?}")]
    DuplicateTensor {
        component: ComponentId,
        name: String,
    },
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
    ) -> Result<Self, CatalogError> {
        let mut owned = BTreeMap::new();
        for (component, source) in sources {
            if owned.insert(component, source).is_some() {
                return Err(CatalogError::DuplicateComponent(component));
            }
        }

        let mut entries = Vec::new();
        let mut by_name = BTreeMap::new();
        for (&component, source) in &owned {
            for record in source.tensor_records() {
                let name = record.info.name.clone();
                let row_elements = *record
                    .info
                    .dims
                    .first()
                    .ok_or_else(|| CatalogError::InvalidShape(name.clone()))?;
                let row_count =
                    record.info.dims[1..]
                        .iter()
                        .try_fold(1_u64, |count, dimension| {
                            count
                                .checked_mul(*dimension)
                                .ok_or_else(|| CatalogError::InvalidShape(name.clone()))
                        })?;
                let (block_elements, block_bytes) = record.info.ggml_type.type_traits();
                let row_bytes = row_elements
                    .checked_div(block_elements as u64)
                    .and_then(|blocks| blocks.checked_mul(block_bytes as u64))
                    .ok_or_else(|| CatalogError::InvalidShape(name.clone()))?;
                let byte_len = record
                    .info
                    .checked_nbytes()
                    .ok_or_else(|| CatalogError::InvalidShape(name.clone()))?;
                if row_count.checked_mul(row_bytes) != Some(byte_len)
                    || record
                        .segment_byte_range
                        .end
                        .checked_sub(record.segment_byte_range.start)
                        != Some(byte_len)
                {
                    return Err(CatalogError::InvalidShape(name));
                }
                let bytes = source
                    .tensor_slice(&name)
                    .ok_or_else(|| CatalogError::MissingTensor(name.clone()))?;
                if u64::try_from(bytes.len()) != Ok(byte_len) {
                    return Err(CatalogError::InvalidShape(name));
                }
                let id = TensorId(
                    u32::try_from(entries.len())
                        .map_err(|_| CatalogError::InvalidShape("too many tensors".into()))?,
                );
                if by_name.insert((component, name.clone()), id).is_some() {
                    return Err(CatalogError::DuplicateTensor { component, name });
                }
                entries.push(TensorCatalogEntry {
                    id,
                    component,
                    name,
                    shape: record.info.dims,
                    ggml_type: record.info.ggml_type,
                    byte_len,
                    segment_id: record.segment_id,
                    segment_byte_range: record.segment_byte_range,
                    layer: record.layer,
                    row_count,
                    row_bytes,
                });
            }
        }
        Ok(Self {
            sources: owned,
            entries,
            by_name,
        })
    }

    pub fn entries(&self) -> &[TensorCatalogEntry] {
        &self.entries
    }

    pub fn source(&self, component: ComponentId) -> Option<&Arc<dyn TensorSource>> {
        self.sources.get(&component)
    }

    pub fn entry(&self, id: TensorId) -> Option<&TensorCatalogEntry> {
        self.entries
            .get(id.0 as usize)
            .filter(|entry| entry.id == id)
    }

    pub fn find(&self, component: ComponentId, name: &str) -> Option<TensorId> {
        self.by_name.get(&(component, name.to_owned())).copied()
    }

    pub fn bytes(&self, id: TensorId) -> Result<&[u8], CatalogError> {
        let entry = self.entry(id).ok_or(CatalogError::InvalidTensorId(id))?;
        self.sources[&entry.component]
            .tensor_slice(&entry.name)
            .ok_or_else(|| CatalogError::MissingTensor(entry.name.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ggufrs::{
        export_ggufrs,
        test_support::{test_gguf_pair, write_test_gguf, SourceTensor},
        ComponentRole, ExportOptions, GgufrsFile,
    };
    use crate::{ComponentId, GGMLType, GGUFLoader, TensorSource};
    use std::sync::Arc;

    #[test]
    fn raw_gguf_and_ggufrs_have_the_same_logical_tensor_contract() {
        let fixture = test_gguf_pair();
        let output = fixture.dir.join("catalog.ggufrs");
        export_ggufrs(
            &output,
            &fixture.llm,
            Some(&fixture.mmproj),
            ExportOptions::default(),
        )
        .unwrap();

        let raw: Arc<dyn TensorSource> = Arc::new(GGUFLoader::from_file(&fixture.llm).unwrap());
        let package = GgufrsFile::open(output).unwrap();
        let packaged: Arc<dyn TensorSource> =
            Arc::new(package.load_component(ComponentRole::Llm).unwrap());
        let raw = TensorCatalog::from_sources(vec![(ComponentId::Llm, raw)]).unwrap();
        let packaged = TensorCatalog::from_sources(vec![(ComponentId::Llm, packaged)]).unwrap();
        let project = |catalog: &TensorCatalog| {
            catalog
                .entries()
                .iter()
                .map(|entry| {
                    (
                        entry.component,
                        entry.name.clone(),
                        entry.shape.clone(),
                        entry.ggml_type,
                        entry.byte_len,
                        entry.layer,
                        entry.row_count,
                        entry.row_bytes,
                    )
                })
                .collect::<Vec<_>>()
        };

        assert_eq!(project(&raw), project(&packaged));
    }

    #[test]
    fn catalog_keeps_source_alive_and_reports_q8_rows() {
        let fixture = test_gguf_pair();
        let path = fixture.dir.join("q8-rows.gguf");
        write_test_gguf(
            &path,
            &[],
            &[SourceTensor {
                name: "blk.0.weight",
                ggml_type: GGMLType::Q8_0,
                dims: vec![64, 4],
                bytes: vec![0; 272],
            }],
        );
        let source: Arc<dyn TensorSource> = Arc::new(GGUFLoader::from_file(path).unwrap());
        let source_lifetime = Arc::downgrade(&source);
        let catalog = TensorCatalog::from_sources(vec![(ComponentId::Llm, source)]).unwrap();

        let id = catalog.find(ComponentId::Llm, "blk.0.weight").unwrap();
        let entry = catalog.entry(id).unwrap();
        assert_eq!((entry.row_count, entry.row_bytes), (4, 68));
        assert_eq!(catalog.bytes(id).unwrap().len(), 272);
        assert!(source_lifetime.upgrade().is_some());
    }
}
