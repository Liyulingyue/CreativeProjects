use std::collections::{BTreeMap, BTreeSet};

use crate::compute::BackendKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComponentId {
    Llm,
    Vision,
}

impl ComponentId {
    fn parse(value: &str) -> Result<Self, PlacementError> {
        match value {
            "llm" => Ok(Self::Llm),
            "vision" => Ok(Self::Vision),
            _ => Err(PlacementError::UnknownComponent(value.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlacementMode {
    Layer,
    Row,
}

impl PlacementMode {
    fn parse(value: &str) -> Result<Self, PlacementError> {
        match value {
            "layer" => Ok(Self::Layer),
            "row" => Ok(Self::Row),
            _ => Err(PlacementError::UnknownMode(value.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn parse(value: &str) -> Result<Self, PlacementError> {
        let suffix = ["cpu", "vulkan", "metal", "npu"]
            .into_iter()
            .find_map(|prefix| value.strip_prefix(prefix))
            .ok_or_else(|| PlacementError::InvalidDevice(value.to_owned()))?;
        if suffix.is_empty()
            || !suffix.bytes().all(|byte| byte.is_ascii_digit())
            || suffix.parse::<u32>().is_err()
        {
            return Err(PlacementError::InvalidDevice(value.to_owned()));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
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

pub fn parse_placement(value: &str) -> Result<PlacementRule, PlacementError> {
    let syntax = || PlacementError::Syntax(value.to_owned());
    let (left, targets) = value.split_once('=').ok_or_else(syntax)?;
    if left.contains('=') || targets.is_empty() || targets.contains('=') {
        return Err(syntax());
    }
    let (component, mode) = left.split_once(':').ok_or_else(syntax)?;
    if component.is_empty() || mode.is_empty() || mode.contains(':') {
        return Err(syntax());
    }
    let component = ComponentId::parse(component)?;
    let mode = PlacementMode::parse(mode)?;
    let mut seen = BTreeSet::new();
    let mut parsed = Vec::new();
    for (ordinal, target) in targets.split(',').enumerate() {
        let (device, weight) = target.split_once('@').ok_or_else(syntax)?;
        if device.is_empty() || weight.is_empty() || weight.contains('@') {
            return Err(syntax());
        }
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
    Ok(PlacementRule {
        component,
        mode,
        targets,
    })
}

pub fn parse_placements(
    values: &[String],
) -> Result<BTreeMap<ComponentId, PlacementRule>, PlacementError> {
    let mut rules = BTreeMap::new();
    for value in values {
        let rule = parse_placement(value)?;
        let component = rule.component;
        if rules.insert(component, rule).is_some() {
            return Err(PlacementError::DuplicateComponent(component));
        }
    }
    Ok(rules)
}

pub fn parse_requested_placements(
    values: &[String],
) -> Result<(BTreeMap<ComponentId, PlacementRule>, BTreeSet<BackendKind>), PlacementError> {
    let defaults = ["llm:row=cpu0@1".to_owned()];
    let rules = parse_placements(if values.is_empty() { &defaults } else { values })?;
    let backends = rules
        .values()
        .flat_map(|rule| &rule.targets)
        .map(|target| {
            if target.device.as_str().starts_with("cpu") {
                BackendKind::Cpu
            } else if target.device.as_str().starts_with("vulkan") {
                BackendKind::Vulkan
            } else if target.device.as_str().starts_with("metal") {
                BackendKind::Metal
            } else {
                BackendKind::Npu
            }
        })
        .collect();
    Ok((rules, backends))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn requested_placements_default_to_cpu_and_collect_backends() {
        assert_eq!(
            parse_requested_placements(&[]).unwrap().1,
            BTreeSet::from([BackendKind::Cpu])
        );
        assert_eq!(
            parse_requested_placements(&[
                "llm:row=metal0@1".to_owned(),
                "vision:row=vulkan0@1".to_owned(),
            ])
            .unwrap()
            .1,
            BTreeSet::from([BackendKind::Metal, BackendKind::Vulkan])
        );
    }
}
