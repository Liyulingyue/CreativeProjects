use crate::ggufrs::{GgufrsError, GgufrsFile, MappedSegment, SegmentKind, TensorRecord};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalDevice {
    pub id: String,
    pub capacity: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementPolicy {
    LayerSplit,
    TensorSplit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementSlice {
    Whole,
    Rows { start: u64, end: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub component_id: u32,
    pub segment_id: u32,
    pub tensor_name: String,
    pub slice: PlacementSlice,
    pub segment_byte_range: Range<u64>,
    pub device_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadPlan {
    pub component_ids: Vec<u32>,
    pub devices: Vec<LogicalDevice>,
    pub primary_device: String,
    pub policy: PlacementPolicy,
    pub placements: Vec<Placement>,
}

fn invalid_plan(context: impl Into<String>) -> GgufrsError {
    GgufrsError::InvalidPlan {
        context: context.into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RowLayout {
    row_count: u64,
    row_bytes: u64,
}

fn row_layout(record: &TensorRecord) -> Result<RowLayout, GgufrsError> {
    let row_elements = *record
        .info
        .dims
        .first()
        .ok_or_else(|| invalid_plan(format!("{} has rank zero", record.info.name)))?;
    let row_count = record.info.dims[1..]
        .iter()
        .try_fold(1u64, |count, dimension| count.checked_mul(*dimension))
        .ok_or_else(|| invalid_plan(format!("{} row count overflow", record.info.name)))?;
    let (block_elements, block_bytes) = record.info.ggml_type.type_traits();
    let block_elements = block_elements as u64;
    if row_elements == 0 || row_elements % block_elements != 0 {
        return Err(GgufrsError::UnsplittableTensor {
            component_id: record.component_id,
            tensor: record.info.name.clone(),
            row_bytes: 0,
            remaining: Vec::new(),
            reason: "row element count is not divisible by quantization block size".into(),
        });
    }
    let row_bytes = row_elements
        .checked_div(block_elements)
        .and_then(|blocks| blocks.checked_mul(block_bytes as u64))
        .ok_or_else(|| invalid_plan(format!("{} row size overflow", record.info.name)))?;
    if row_count.checked_mul(row_bytes) != Some(record.byte_len) {
        return Err(invalid_plan(format!(
            "{} row layout does not match byte_len {}",
            record.info.name, record.byte_len
        )));
    }
    Ok(RowLayout {
        row_count,
        row_bytes,
    })
}

fn device_index(devices: &[LogicalDevice], id: &str) -> Result<usize, GgufrsError> {
    devices
        .iter()
        .position(|device| device.id == id)
        .ok_or_else(|| invalid_plan(format!("unknown logical device {id:?}")))
}

fn checked_tensor_range(record: &TensorRecord) -> Result<Range<u64>, GgufrsError> {
    let end = record
        .segment_offset
        .checked_add(record.byte_len)
        .ok_or_else(|| {
            invalid_plan(format!(
                "component {} segment {} tensor {} byte range overflow",
                record.component_id, record.segment_id, record.info.name
            ))
        })?;
    Ok(record.segment_offset..end)
}

fn validate_devices(devices: &[LogicalDevice], primary_device: &str) -> Result<usize, GgufrsError> {
    if devices.is_empty() {
        return Err(invalid_plan(
            "load plan requires at least one logical device",
        ));
    }
    let mut ids = BTreeSet::new();
    for device in devices {
        if device.id.is_empty() {
            return Err(invalid_plan("logical device id must not be empty"));
        }
        if !ids.insert(device.id.as_str()) {
            return Err(invalid_plan(format!(
                "duplicate logical device id {:?}",
                device.id
            )));
        }
    }
    device_index(devices, primary_device)
}

fn selected_components(
    package: &GgufrsFile,
    component_ids: &[u32],
) -> Result<Vec<u32>, GgufrsError> {
    let requested = component_ids.iter().copied().collect::<BTreeSet<_>>();
    if requested.len() != component_ids.len() {
        return Err(invalid_plan("selected component ids must be unique"));
    }
    let selected = package
        .components()
        .iter()
        .filter(|component| requested.contains(&component.id))
        .map(|component| component.id)
        .collect::<Vec<_>>();
    if selected.len() != requested.len() {
        let unknown = requested
            .iter()
            .find(|id| !selected.contains(id))
            .copied()
            .unwrap();
        return Err(invalid_plan(format!("unknown component id {unknown}")));
    }
    Ok(selected)
}

pub fn build_load_plan(
    package: &GgufrsFile,
    component_ids: &[u32],
    devices: &[LogicalDevice],
    primary_device: &str,
    policy: PlacementPolicy,
) -> Result<LoadPlan, GgufrsError> {
    let primary = validate_devices(devices, primary_device)?;
    let component_ids = selected_components(package, component_ids)?;
    let selected = component_ids.iter().copied().collect::<BTreeSet<_>>();
    let mut remaining = devices
        .iter()
        .map(|device| device.capacity)
        .collect::<Vec<_>>();
    let mut placements = Vec::new();

    for record in package.tensors().iter().filter(|record| {
        selected.contains(&record.component_id)
            && package
                .segment(record.segment_id)
                .is_some_and(|segment| segment.kind != SegmentKind::Layer)
    }) {
        if record.byte_len > remaining[primary] {
            return Err(GgufrsError::CapacityExceeded {
                device_id: devices[primary].id.clone(),
                required: record.byte_len,
                available: remaining[primary],
                context: format!(
                    "component {} segment {} tensor {}",
                    record.component_id, record.segment_id, record.info.name
                ),
            });
        }
        placements.push(Placement {
            component_id: record.component_id,
            segment_id: record.segment_id,
            tensor_name: record.info.name.clone(),
            slice: PlacementSlice::Whole,
            segment_byte_range: checked_tensor_range(record)?,
            device_id: devices[primary].id.clone(),
        });
        remaining[primary] = remaining[primary]
            .checked_sub(record.byte_len)
            .ok_or_else(|| invalid_plan("primary device capacity underflow"))?;
    }

    let mut layer_segments = component_ids
        .iter()
        .flat_map(|component_id| package.segments_for_component(*component_id))
        .filter(|segment| segment.kind == SegmentKind::Layer)
        .collect::<Vec<_>>();
    layer_segments.sort_by_key(|segment| (segment.component_id, segment.layer, segment.id));

    let mut current_device = 0usize;
    match policy {
        PlacementPolicy::LayerSplit => {
            for segment in layer_segments {
                let records = package.tensors_for_segment(segment.id).collect::<Vec<_>>();
                let required = records.iter().try_fold(0u64, |sum, record| {
                    sum.checked_add(record.byte_len)
                        .ok_or_else(|| invalid_plan("layer payload overflow"))
                })?;
                while current_device < devices.len() && required > remaining[current_device] {
                    current_device += 1;
                }
                if current_device == devices.len() {
                    return Err(GgufrsError::CapacityExceeded {
                        device_id: devices.last().unwrap().id.clone(),
                        required,
                        available: remaining.last().copied().unwrap_or(0),
                        context: format!(
                            "component {} segment {}",
                            segment.component_id, segment.id
                        ),
                    });
                }
                for record in records {
                    placements.push(Placement {
                        component_id: record.component_id,
                        segment_id: record.segment_id,
                        tensor_name: record.info.name.clone(),
                        slice: PlacementSlice::Whole,
                        segment_byte_range: checked_tensor_range(record)?,
                        device_id: devices[current_device].id.clone(),
                    });
                }
                remaining[current_device] = remaining[current_device]
                    .checked_sub(required)
                    .ok_or_else(|| invalid_plan("logical device capacity underflow"))?;
            }
        }
        PlacementPolicy::TensorSplit => {
            for segment in layer_segments {
                for record in package.tensors_for_segment(segment.id) {
                    let layout = row_layout(record)?;
                    let mut row_start = 0u64;
                    while row_start < layout.row_count {
                        while current_device < devices.len()
                            && remaining[current_device] < layout.row_bytes
                        {
                            current_device += 1;
                        }
                        if current_device == devices.len() {
                            return Err(GgufrsError::UnsplittableTensor {
                                component_id: record.component_id,
                                tensor: record.info.name.clone(),
                                row_bytes: layout.row_bytes,
                                remaining: devices
                                    .iter()
                                    .zip(&remaining)
                                    .map(|(device, bytes)| (device.id.clone(), *bytes))
                                    .collect(),
                                reason: "no remaining device can hold one complete row".into(),
                            });
                        }
                        let rows_here = (remaining[current_device] / layout.row_bytes)
                            .min(layout.row_count - row_start);
                        let row_end = row_start
                            .checked_add(rows_here)
                            .ok_or_else(|| invalid_plan("tensor row range overflow"))?;
                        let start_delta =
                            row_start.checked_mul(layout.row_bytes).ok_or_else(|| {
                                invalid_plan(format!(
                                    "{} row byte offset overflow",
                                    record.info.name
                                ))
                            })?;
                        let end_delta = row_end.checked_mul(layout.row_bytes).ok_or_else(|| {
                            invalid_plan(format!("{} row byte end overflow", record.info.name))
                        })?;
                        let start =
                            record
                                .segment_offset
                                .checked_add(start_delta)
                                .ok_or_else(|| {
                                    invalid_plan(format!(
                                        "{} segment byte start overflow",
                                        record.info.name
                                    ))
                                })?;
                        let end =
                            record
                                .segment_offset
                                .checked_add(end_delta)
                                .ok_or_else(|| {
                                    invalid_plan(format!(
                                        "{} segment byte end overflow",
                                        record.info.name
                                    ))
                                })?;
                        placements.push(Placement {
                            component_id: record.component_id,
                            segment_id: record.segment_id,
                            tensor_name: record.info.name.clone(),
                            slice: if row_start == 0 && row_end == layout.row_count {
                                PlacementSlice::Whole
                            } else {
                                PlacementSlice::Rows {
                                    start: row_start,
                                    end: row_end,
                                }
                            },
                            segment_byte_range: start..end,
                            device_id: devices[current_device].id.clone(),
                        });
                        let used = rows_here.checked_mul(layout.row_bytes).ok_or_else(|| {
                            invalid_plan(format!(
                                "{} placement byte size overflow",
                                record.info.name
                            ))
                        })?;
                        remaining[current_device] = remaining[current_device]
                            .checked_sub(used)
                            .ok_or_else(|| invalid_plan("logical device capacity underflow"))?;
                        row_start = row_end;
                    }
                }
            }
        }
    }
    let plan = LoadPlan {
        component_ids,
        devices: devices.to_vec(),
        primary_device: primary_device.into(),
        policy,
        placements,
    };
    plan.validate(package)?;
    Ok(plan)
}

impl LoadPlan {
    pub fn validate(&self, package: &GgufrsFile) -> Result<(), GgufrsError> {
        validate_devices(&self.devices, &self.primary_device)?;
        if selected_components(package, &self.component_ids)? != self.component_ids {
            return Err(invalid_plan("component ids are not in package order"));
        }
        let selected = self.component_ids.iter().copied().collect::<BTreeSet<_>>();
        for placement in &self.placements {
            if !selected.contains(&placement.component_id) {
                return Err(invalid_plan(format!(
                    "placement references unselected component {}",
                    placement.component_id
                )));
            }
            device_index(&self.devices, &placement.device_id)?;
            let record = package
                .tensors()
                .iter()
                .find(|record| {
                    record.component_id == placement.component_id
                        && record.segment_id == placement.segment_id
                        && record.info.name == placement.tensor_name
                })
                .ok_or_else(|| {
                    invalid_plan(format!(
                        "placement references unknown component {} segment {} tensor {}",
                        placement.component_id, placement.segment_id, placement.tensor_name
                    ))
                })?;
            let tensor_range = checked_tensor_range(record)?;
            if placement.segment_byte_range.start > placement.segment_byte_range.end
                || placement.segment_byte_range.start < tensor_range.start
                || placement.segment_byte_range.end > tensor_range.end
            {
                return Err(invalid_plan(format!(
                    "component {} segment {} tensor {} placement range {:?} is outside {:?}",
                    placement.component_id,
                    placement.segment_id,
                    placement.tensor_name,
                    placement.segment_byte_range,
                    tensor_range
                )));
            }
        }

        let mut layer_devices = BTreeMap::<u32, String>::new();
        for record in package
            .tensors()
            .iter()
            .filter(|record| selected.contains(&record.component_id))
        {
            let segment = package.segment(record.segment_id).ok_or_else(|| {
                invalid_plan(format!(
                    "component {} tensor {} references unknown segment {}",
                    record.component_id, record.info.name, record.segment_id
                ))
            })?;
            let layout = row_layout(record)?;
            let tensor_range = checked_tensor_range(record)?;
            let mut row_ranges = Vec::<Range<u64>>::new();
            for placement in self.placements.iter().filter(|placement| {
                placement.component_id == record.component_id
                    && placement.segment_id == record.segment_id
                    && placement.tensor_name == record.info.name
            }) {
                if segment.kind != SegmentKind::Layer && placement.device_id != self.primary_device
                {
                    return Err(invalid_plan(format!(
                        "component {} segment {} tensor {} must be on primary device {:?}",
                        record.component_id,
                        record.segment_id,
                        record.info.name,
                        self.primary_device
                    )));
                }
                if self.policy == PlacementPolicy::LayerSplit
                    && placement.slice != PlacementSlice::Whole
                {
                    return Err(invalid_plan(format!(
                        "LayerSplit placement for component {} segment {} tensor {} is not whole",
                        record.component_id, record.segment_id, record.info.name
                    )));
                }
                if self.policy == PlacementPolicy::LayerSplit && segment.kind == SegmentKind::Layer
                {
                    if let Some(device_id) = layer_devices.get(&segment.id) {
                        if device_id != &placement.device_id {
                            return Err(invalid_plan(format!(
                                "component {} layer segment {} spans devices {:?} and {:?}",
                                segment.component_id, segment.id, device_id, placement.device_id
                            )));
                        }
                    } else {
                        layer_devices.insert(segment.id, placement.device_id.clone());
                    }
                }

                let rows = match placement.slice {
                    PlacementSlice::Whole => {
                        if placement.segment_byte_range != tensor_range {
                            return Err(invalid_plan(format!(
                                "whole placement for component {} segment {} tensor {} has byte range {:?}, expected {:?}",
                                record.component_id,
                                record.segment_id,
                                record.info.name,
                                placement.segment_byte_range,
                                tensor_range
                            )));
                        }
                        0..layout.row_count
                    }
                    PlacementSlice::Rows { start, end } => {
                        if start >= end || end > layout.row_count {
                            return Err(invalid_plan(format!(
                                "component {} segment {} tensor {} has invalid row range {start}..{end} of {}",
                                record.component_id,
                                record.segment_id,
                                record.info.name,
                                layout.row_count
                            )));
                        }
                        let expected_start = start
                            .checked_mul(layout.row_bytes)
                            .and_then(|offset| record.segment_offset.checked_add(offset))
                            .ok_or_else(|| {
                                invalid_plan(format!(
                                    "{} row placement start overflow",
                                    record.info.name
                                ))
                            })?;
                        let expected_end = end
                            .checked_mul(layout.row_bytes)
                            .and_then(|offset| record.segment_offset.checked_add(offset))
                            .ok_or_else(|| {
                                invalid_plan(format!(
                                    "{} row placement end overflow",
                                    record.info.name
                                ))
                            })?;
                        if placement.segment_byte_range != (expected_start..expected_end) {
                            return Err(invalid_plan(format!(
                                "component {} segment {} tensor {} row range {start}..{end} has byte range {:?}, expected {:?}",
                                record.component_id,
                                record.segment_id,
                                record.info.name,
                                placement.segment_byte_range,
                                expected_start..expected_end
                            )));
                        }
                        start..end
                    }
                };
                row_ranges.push(rows);
            }
            row_ranges.sort_unstable_by_key(|range| range.start);
            let mut covered = 0u64;
            for range in row_ranges {
                if range.start != covered {
                    return Err(invalid_plan(format!(
                        "component {} segment {} tensor {} row coverage expected {covered}, got {}",
                        record.component_id, record.segment_id, record.info.name, range.start
                    )));
                }
                covered = range.end;
            }
            if covered != layout.row_count {
                return Err(invalid_plan(format!(
                    "component {} segment {} tensor {} row coverage ends at {covered}, expected {}",
                    record.component_id, record.segment_id, record.info.name, layout.row_count
                )));
            }
        }

        if self.policy == PlacementPolicy::LayerSplit {
            let mut previous_device = None;
            for component_id in &self.component_ids {
                for segment in package
                    .segments_for_component(*component_id)
                    .filter(|segment| segment.kind == SegmentKind::Layer)
                {
                    let device_id = layer_devices.get(&segment.id).ok_or_else(|| {
                        invalid_plan(format!(
                            "component {} layer segment {} has no assigned device",
                            segment.component_id, segment.id
                        ))
                    })?;
                    let index = device_index(&self.devices, device_id)?;
                    if previous_device.is_some_and(|previous| index < previous) {
                        return Err(invalid_plan(format!(
                            "component {} layer segment {} backtracks to device {:?}",
                            segment.component_id, segment.id, device_id
                        )));
                    }
                    previous_device = Some(index);
                }
            }
        }
        for device in &self.devices {
            let used = self.used_bytes(&device.id)?;
            if used > device.capacity {
                return Err(GgufrsError::CapacityExceeded {
                    device_id: device.id.clone(),
                    required: used,
                    available: device.capacity,
                    context: "load plan placements".into(),
                });
            }
        }
        Ok(())
    }

    pub fn used_bytes(&self, device_id: &str) -> Result<u64, GgufrsError> {
        self.placements
            .iter()
            .filter(|placement| placement.device_id == device_id)
            .try_fold(0u64, |used, placement| {
                let bytes = placement
                    .segment_byte_range
                    .end
                    .checked_sub(placement.segment_byte_range.start)
                    .ok_or_else(|| {
                        invalid_plan(format!(
                            "component {} segment {} tensor {} has reversed byte range {:?}",
                            placement.component_id,
                            placement.segment_id,
                            placement.tensor_name,
                            placement.segment_byte_range,
                        ))
                    })?;
                used.checked_add(bytes).ok_or_else(|| {
                    invalid_plan(format!("payload total overflow for device {device_id:?}"))
                })
            })
    }
}

pub struct LogicalCpuPlacement {
    pub placement: Placement,
    mapping: Arc<MappedSegment>,
}

impl LogicalCpuPlacement {
    pub fn bytes(&self) -> Option<&[u8]> {
        (self.mapping.segment_id == self.placement.segment_id).then_some(())?;
        let start = usize::try_from(self.placement.segment_byte_range.start).ok()?;
        let end = usize::try_from(self.placement.segment_byte_range.end).ok()?;
        self.mapping.bytes.get(start..end)
    }
}

pub struct LogicalCpuDeviceLoad {
    pub id: String,
    pub placements: Vec<LogicalCpuPlacement>,
}

pub struct LogicalCpuLoad {
    pub devices: Vec<LogicalCpuDeviceLoad>,
}

impl LogicalCpuLoad {
    pub fn release_device(&mut self, device_id: &str) -> bool {
        let Some(device) = self
            .devices
            .iter_mut()
            .find(|device| device.id == device_id)
        else {
            return false;
        };
        let released = !device.placements.is_empty();
        device.placements.clear();
        released
    }

    pub fn release_component(&mut self, component_id: u32) -> usize {
        let before = self
            .devices
            .iter()
            .map(|device| device.placements.len())
            .sum::<usize>();
        for device in &mut self.devices {
            device
                .placements
                .retain(|loaded| loaded.placement.component_id != component_id);
        }
        before
            - self
                .devices
                .iter()
                .map(|device| device.placements.len())
                .sum::<usize>()
    }
}

pub fn load_logical_cpu(
    package: &GgufrsFile,
    plan: &LoadPlan,
) -> Result<LogicalCpuLoad, GgufrsError> {
    plan.validate(package)?;
    let mut mappings = BTreeMap::<u32, Arc<MappedSegment>>::new();
    for placement in &plan.placements {
        if !mappings.contains_key(&placement.segment_id) {
            mappings.insert(
                placement.segment_id,
                package.map_segment_shared(placement.segment_id)?,
            );
        }
    }
    let mut devices = plan
        .devices
        .iter()
        .map(|device| LogicalCpuDeviceLoad {
            id: device.id.clone(),
            placements: Vec::new(),
        })
        .collect::<Vec<_>>();
    for placement in &plan.placements {
        let target = devices
            .iter_mut()
            .find(|device| device.id == placement.device_id)
            .ok_or_else(|| {
                invalid_plan(format!(
                    "placement for component {} tensor {} references unknown device {:?}",
                    placement.component_id, placement.tensor_name, placement.device_id
                ))
            })?;
        let mapping = mappings.get(&placement.segment_id).ok_or_else(|| {
            invalid_plan(format!(
                "placement for component {} tensor {} has no mapping for segment {}",
                placement.component_id, placement.tensor_name, placement.segment_id
            ))
        })?;
        target.placements.push(LogicalCpuPlacement {
            placement: placement.clone(),
            mapping: Arc::clone(mapping),
        });
    }
    Ok(LogicalCpuLoad { devices })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ggufrs::{GgufrsFile, SegmentKind};
    use crate::model::{GGMLType, TensorInfo};

    struct ExportedFixture {
        package: GgufrsFile,
        #[allow(dead_code)]
        inputs: crate::ggufrs::test_support::TestInputs,
    }

    fn exported_test_package() -> ExportedFixture {
        let inputs = crate::ggufrs::test_support::test_gguf_pair();
        let output = inputs.dir.join("load-plan.ggufrs");
        crate::ggufrs::export_ggufrs(
            &output,
            &inputs.llm,
            Some(&inputs.mmproj),
            crate::ggufrs::ExportOptions::default(),
        )
        .unwrap();
        let package = GgufrsFile::open(output).unwrap();
        ExportedFixture { package, inputs }
    }

    #[test]
    fn layer_split_keeps_layers_contiguous_and_shared_on_primary() {
        let fixture = exported_test_package();
        let package = &fixture.package;
        let llm = package
            .component_id(crate::ggufrs::ComponentRole::Llm)
            .unwrap();
        let mmproj = package
            .component_id(crate::ggufrs::ComponentRole::Mmproj)
            .unwrap();
        let devices = vec![
            LogicalDevice {
                id: "cpu0".into(),
                capacity: 354,
            },
            LogicalDevice {
                id: "cpu1".into(),
                capacity: 34,
            },
        ];
        let plan = build_load_plan(
            package,
            &[llm, mmproj],
            &devices,
            "cpu0",
            PlacementPolicy::LayerSplit,
        )
        .unwrap();
        plan.validate(package).unwrap();
        assert_eq!(
            plan,
            build_load_plan(
                package,
                &[llm, mmproj],
                &devices,
                "cpu0",
                PlacementPolicy::LayerSplit,
            )
            .unwrap()
        );

        for placement in &plan.placements {
            let segment = package.segment(placement.segment_id).unwrap();
            if segment.kind != SegmentKind::Layer || placement.component_id == mmproj {
                assert_eq!(placement.device_id, "cpu0");
            }
        }
        let layer_devices: Vec<&str> = package
            .segments_for_component(llm)
            .filter(|segment| segment.kind == SegmentKind::Layer)
            .map(|segment| {
                plan.placements
                    .iter()
                    .find(|placement| placement.segment_id == segment.id)
                    .unwrap()
                    .device_id
                    .as_str()
            })
            .collect();
        assert_eq!(layer_devices, vec!["cpu0", "cpu1"]);
        assert_eq!(plan.used_bytes("cpu0").unwrap(), 354);
        assert_eq!(plan.used_bytes("cpu1").unwrap(), 34);
    }

    #[test]
    fn tensor_split_uses_only_complete_quantized_rows() {
        let fixture = crate::ggufrs::test_support::test_q8_row_package(4, 32);
        let devices = vec![
            LogicalDevice {
                id: "cpu0".into(),
                capacity: 196,
            },
            LogicalDevice {
                id: "cpu1".into(),
                capacity: 68,
            },
        ];
        let plan = build_load_plan(
            &fixture.package,
            &[fixture.llm_component],
            &devices,
            "cpu0",
            PlacementPolicy::TensorSplit,
        )
        .unwrap();
        plan.validate(&fixture.package).unwrap();
        let rows: Vec<&Placement> = plan
            .placements
            .iter()
            .filter(|placement| placement.tensor_name == "blk.0.weight")
            .collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].slice, PlacementSlice::Rows { start: 0, end: 2 });
        assert_eq!(rows[1].slice, PlacementSlice::Rows { start: 2, end: 4 });
        assert!(rows.iter().all(|placement| {
            (placement.segment_byte_range.end - placement.segment_byte_range.start) % 34 == 0
        }));
    }

    #[test]
    fn row_layout_rejects_incomplete_quantization_blocks() {
        let record = TensorRecord {
            component_id: 0,
            segment_id: 0,
            info: TensorInfo {
                name: "bad".into(),
                dims: vec![31, 2],
                ggml_type: GGMLType::Q8_0,
                offset: 0,
            },
            segment_offset: 0,
            byte_len: 68,
        };
        assert!(matches!(
            row_layout(&record),
            Err(GgufrsError::UnsplittableTensor { .. })
        ));
    }

    #[test]
    fn tensor_split_keeps_rank_one_layer_tensors_whole() {
        let fixture = exported_test_package();
        let package = &fixture.package;
        let llm = package
            .component_id(crate::ggufrs::ComponentRole::Llm)
            .unwrap();
        let devices = vec![
            LogicalDevice {
                id: "cpu0".into(),
                capacity: 128,
            },
            LogicalDevice {
                id: "cpu1".into(),
                capacity: 68,
            },
        ];
        let plan = build_load_plan(
            package,
            &[llm],
            &devices,
            "cpu0",
            PlacementPolicy::TensorSplit,
        )
        .unwrap();
        let layers: Vec<&Placement> = plan
            .placements
            .iter()
            .filter(|placement| placement.tensor_name.starts_with("blk."))
            .collect();
        assert_eq!(layers.len(), 2);
        assert!(layers.iter().all(|placement| {
            placement.device_id == "cpu1" && placement.slice == PlacementSlice::Whole
        }));
    }

    #[test]
    fn planning_reports_capacity_and_row_failures() {
        let fixture = exported_test_package();
        let package = &fixture.package;
        let llm = package
            .component_id(crate::ggufrs::ComponentRole::Llm)
            .unwrap();
        let too_small = vec![LogicalDevice {
            id: "cpu0".into(),
            capacity: 127,
        }];
        assert!(matches!(
            build_load_plan(
                package,
                &[llm],
                &too_small,
                "cpu0",
                PlacementPolicy::LayerSplit,
            ),
            Err(GgufrsError::CapacityExceeded { .. })
        ));

        let fixture = crate::ggufrs::test_support::test_q8_row_package(2, 64);
        let devices = vec![
            LogicalDevice {
                id: "cpu0".into(),
                capacity: 128,
            },
            LogicalDevice {
                id: "cpu1".into(),
                capacity: 34,
            },
        ];
        assert!(matches!(
            build_load_plan(
                &fixture.package,
                &[fixture.llm_component],
                &devices,
                "cpu0",
                PlacementPolicy::TensorSplit,
            ),
            Err(GgufrsError::UnsplittableTensor { row_bytes: 68, .. })
        ));
    }

    #[test]
    fn planning_rejects_ambiguous_devices_and_components() {
        let fixture = exported_test_package();
        let package = &fixture.package;
        let llm = package
            .component_id(crate::ggufrs::ComponentRole::Llm)
            .unwrap();
        let duplicate = vec![
            LogicalDevice {
                id: "cpu".into(),
                capacity: 1024,
            },
            LogicalDevice {
                id: "cpu".into(),
                capacity: 1024,
            },
        ];
        assert!(build_load_plan(
            package,
            &[llm],
            &duplicate,
            "cpu",
            PlacementPolicy::LayerSplit,
        )
        .is_err());
        let valid = vec![LogicalDevice {
            id: "cpu".into(),
            capacity: 1024,
        }];
        assert!(build_load_plan(
            package,
            &[llm],
            &valid,
            "missing",
            PlacementPolicy::LayerSplit,
        )
        .is_err());
        assert!(build_load_plan(
            package,
            &[llm, llm],
            &valid,
            "cpu",
            PlacementPolicy::LayerSplit,
        )
        .is_err());
        assert!(build_load_plan(
            package,
            &[u32::MAX],
            &valid,
            "cpu",
            PlacementPolicy::LayerSplit,
        )
        .is_err());
    }

    #[test]
    fn validation_rejects_missing_overlapping_or_misaligned_rows() {
        let fixture = crate::ggufrs::test_support::test_q8_row_package(4, 32);
        let devices = vec![
            LogicalDevice {
                id: "cpu0".into(),
                capacity: 196,
            },
            LogicalDevice {
                id: "cpu1".into(),
                capacity: 68,
            },
        ];
        let plan = build_load_plan(
            &fixture.package,
            &[fixture.llm_component],
            &devices,
            "cpu0",
            PlacementPolicy::TensorSplit,
        )
        .unwrap();

        let mut missing = plan.clone();
        missing.placements.pop();
        assert!(missing.validate(&fixture.package).is_err());

        let mut overlap = plan.clone();
        let second = overlap
            .placements
            .iter_mut()
            .find(|placement| {
                placement.tensor_name == "blk.0.weight"
                    && placement.slice == PlacementSlice::Rows { start: 2, end: 4 }
            })
            .unwrap();
        second.slice = PlacementSlice::Rows { start: 1, end: 4 };
        assert!(overlap.validate(&fixture.package).is_err());

        let mut misaligned = plan;
        let row = misaligned
            .placements
            .iter_mut()
            .find(|placement| placement.tensor_name == "blk.0.weight")
            .unwrap();
        row.segment_byte_range.end -= 1;
        assert!(misaligned.validate(&fixture.package).is_err());
    }

    #[test]
    fn validation_rejects_forged_tensor_ownership_device_and_capacity() {
        let fixture = exported_test_package();
        let package = &fixture.package;
        let llm = package
            .component_id(crate::ggufrs::ComponentRole::Llm)
            .unwrap();
        let mmproj = package
            .component_id(crate::ggufrs::ComponentRole::Mmproj)
            .unwrap();
        let devices = vec![
            LogicalDevice {
                id: "cpu0".into(),
                capacity: 1024,
            },
            LogicalDevice {
                id: "cpu1".into(),
                capacity: 1024,
            },
        ];
        let plan = build_load_plan(
            package,
            &[llm, mmproj],
            &devices,
            "cpu0",
            PlacementPolicy::LayerSplit,
        )
        .unwrap();

        let mut wrong_owner = plan.clone();
        wrong_owner
            .placements
            .iter_mut()
            .find(|placement| placement.component_id == mmproj)
            .unwrap()
            .device_id = "cpu1".into();
        assert!(wrong_owner.validate(package).is_err());

        let mut forged = plan.clone();
        forged.devices[0].capacity = u64::MAX;
        let mut extra = forged.placements[0].clone();
        extra.tensor_name = "not-a-package-tensor".into();
        forged.placements.push(extra);
        assert!(forged.validate(package).is_err());

        let mut unknown_device = plan.clone();
        unknown_device.placements[0].device_id = "missing".into();
        assert!(unknown_device.validate(package).is_err());

        let mut over_capacity = plan;
        over_capacity.devices[0].capacity = 1;
        assert!(matches!(
            over_capacity.validate(package),
            Err(GgufrsError::CapacityExceeded { .. })
        ));
    }

    #[test]
    fn layer_split_validation_requires_whole_single_device_layers() {
        let fixture = crate::ggufrs::test_support::test_q8_row_package(4, 32);
        let devices = vec![
            LogicalDevice {
                id: "cpu0".into(),
                capacity: 196,
            },
            LogicalDevice {
                id: "cpu1".into(),
                capacity: 68,
            },
        ];
        let mut plan = build_load_plan(
            &fixture.package,
            &[fixture.llm_component],
            &devices,
            "cpu0",
            PlacementPolicy::TensorSplit,
        )
        .unwrap();
        plan.policy = PlacementPolicy::LayerSplit;
        assert!(plan.validate(&fixture.package).is_err());
    }

    #[test]
    fn layer_split_validation_rejects_device_order_backtracking() {
        let fixture = exported_test_package();
        let package = &fixture.package;
        let llm = package
            .component_id(crate::ggufrs::ComponentRole::Llm)
            .unwrap();
        let devices = vec![
            LogicalDevice {
                id: "cpu0".into(),
                capacity: 1024,
            },
            LogicalDevice {
                id: "cpu1".into(),
                capacity: 1024,
            },
        ];
        let mut plan = build_load_plan(
            package,
            &[llm],
            &devices,
            "cpu0",
            PlacementPolicy::LayerSplit,
        )
        .unwrap();
        plan.placements
            .iter_mut()
            .find(|placement| placement.tensor_name == "blk.0.weight")
            .unwrap()
            .device_id = "cpu1".into();
        assert!(plan.validate(package).is_err());
    }

    #[test]
    fn logical_cpu_shares_split_segment_mapping_until_each_device_releases() {
        let fixture = crate::ggufrs::test_support::test_q8_row_package(4, 32);
        let devices = vec![
            LogicalDevice {
                id: "cpu0".into(),
                capacity: 196,
            },
            LogicalDevice {
                id: "cpu1".into(),
                capacity: 68,
            },
        ];
        let plan = build_load_plan(
            &fixture.package,
            &[fixture.llm_component],
            &devices,
            "cpu0",
            PlacementPolicy::TensorSplit,
        )
        .unwrap();
        let mut load = load_logical_cpu(&fixture.package, &plan).unwrap();
        let weak = {
            let first = load.devices[0]
                .placements
                .iter()
                .find(|loaded| loaded.placement.tensor_name == "blk.0.weight")
                .unwrap();
            assert_eq!(first.bytes().unwrap(), &[0x22; 68]);
            assert!(load.devices[1]
                .placements
                .iter()
                .any(|loaded| Arc::ptr_eq(&first.mapping, &loaded.mapping)));
            Arc::downgrade(&first.mapping)
        };
        assert!(!load.release_device("missing"));
        assert!(load.release_device("cpu0"));
        assert!(weak.upgrade().is_some());
        assert!(!load.release_device("cpu0"));
        assert!(load.release_device("cpu1"));
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn logical_cpu_releases_one_component_without_invalidating_another() {
        let fixture = exported_test_package();
        let package = &fixture.package;
        let llm = package
            .component_id(crate::ggufrs::ComponentRole::Llm)
            .unwrap();
        let mmproj = package
            .component_id(crate::ggufrs::ComponentRole::Mmproj)
            .unwrap();
        let devices = vec![LogicalDevice {
            id: "cpu0".into(),
            capacity: 388,
        }];
        let plan = build_load_plan(
            package,
            &[llm, mmproj],
            &devices,
            "cpu0",
            PlacementPolicy::LayerSplit,
        )
        .unwrap();
        let mut load = load_logical_cpu(package, &plan).unwrap();
        assert_eq!(
            load.devices[0]
                .placements
                .iter()
                .find(|loaded| loaded.placement.tensor_name == "mm.0.weight")
                .unwrap()
                .bytes()
                .unwrap(),
            fixture.inputs.mmproj_weight
        );

        assert_eq!(load.release_component(mmproj), 3);
        assert!(load.devices[0]
            .placements
            .iter()
            .all(|loaded| loaded.placement.component_id == llm));
        assert_eq!(
            load.devices[0]
                .placements
                .iter()
                .find(|loaded| loaded.placement.tensor_name == "token_embd.weight")
                .unwrap()
                .bytes()
                .unwrap(),
            fixture.inputs.llm_shared
        );
        assert_eq!(load.release_component(mmproj), 0);
    }
}
