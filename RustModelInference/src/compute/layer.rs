use super::device::{ComputeDevice, DeviceKind, OpType, WorkSpec};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct LayerSpec {
    pub layer_id: usize,
    pub ops: Vec<LayerOp>,
    pub device_affinity: Option<DeviceKind>,
}

#[derive(Debug, Clone)]
pub enum LayerOp {
    MatMul {
        weight: Vec<u8>,
        scales: Vec<f32>,
        n_in: usize,
        n_out: usize,
    },
    RmsNorm {
        weight: Vec<f32>,
        eps: f32,
    },
    Silu,
    Softmax,
    RoPE {
        positions: Vec<usize>,
    },
}

impl LayerSpec {
    pub fn new_matmul_layer(layer_id: usize, weight: Vec<u8>, scales: Vec<f32>, n_in: usize, n_out: usize) -> Self {
        Self {
            layer_id,
            ops: vec![LayerOp::MatMul {
                weight,
                scales,
                n_in,
                n_out,
            }],
            device_affinity: None,
        }
    }

    pub fn with_affinity(mut self, device: DeviceKind) -> Self {
        self.device_affinity = Some(device);
        self
    }
}

pub trait LayerExecute {
    fn execute_layer(&self, layer: &LayerSpec, input: &[f32]) -> Result<Vec<f32>, LayerError>;
}

#[derive(Debug, Clone)]
pub struct LayerScheduleConfig {
    pub layer_configs: Vec<LayerDeviceConfig>,
}

#[derive(Debug, Clone)]
pub struct LayerDeviceConfig {
    pub layer_id: usize,
    pub device: DeviceKind,
    pub ratio: u8,
}

impl LayerScheduleConfig {
    pub fn new() -> Self {
        Self {
            layer_configs: Vec::new(),
        }
    }

    pub fn add_layer(mut self, layer_id: usize, device: DeviceKind, ratio: u8) -> Self {
        self.layer_configs.push(LayerDeviceConfig {
            layer_id,
            device,
            ratio,
        });
        self
    }

    pub fn get_device_for_layer(&self, layer_id: usize) -> Option<DeviceKind> {
        self.layer_configs
            .iter()
            .find(|c| c.layer_id == layer_id)
            .map(|c| c.device)
    }

    pub fn get_ratio_for_layer(&self, layer_id: usize) -> u8 {
        self.layer_configs
            .iter()
            .find(|c| c.layer_id == layer_id)
            .map(|c| c.ratio)
            .unwrap_or(100)
    }
}

impl Default for LayerScheduleConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum LayerError {
    DeviceNotAvailable(String),
    UnsupportedOp(String),
    ExecutionFailed(String),
    WeightPreallocationFailed(String),
}

impl std::fmt::Display for LayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayerError::DeviceNotAvailable(d) => write!(f, "Device not available: {}", d),
            LayerError::UnsupportedOp(op) => write!(f, "Unsupported operation: {}", op),
            LayerError::ExecutionFailed(msg) => write!(f, "Execution failed: {}", msg),
            LayerError::WeightPreallocationFailed(msg) => write!(f, "Weight preallocation failed: {}", msg),
        }
    }
}

impl std::error::Error for LayerError {}

pub type LayerResult<T> = std::result::Result<T, LayerError>;
