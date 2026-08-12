use thiserror::Error;
use std::sync::Arc;

#[derive(Error, Debug)]
pub enum ComputeError {
    #[error("Device {0} does not support this operation")]
    UnsupportedOp(String),

    #[error("Device {0} not available")]
    DeviceNotAvailable(String),

    #[error("Memory error: {0}")]
    MemoryError(String),

    #[error("Execution failed: {0}")]
    ExecutionError(String),
}

pub type Result<T> = std::result::Result<T, ComputeError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceKind {
    Cpu(u8),   // Cpu(0) = 第一个CPU, Cpu(1) = 第二个CPU (NUMA节点等)
    Gpu(u8),   // Gpu(0) = 第一个GPU, Gpu(1) = 第二个GPU
    Npu(u8),   // Npu(0) = 第一个NPU, Npu(1) = 第二个NPU
}

impl DeviceKind {
    pub fn is_accelerator(&self) -> bool {
        !matches!(self, DeviceKind::Cpu(_))
    }

    pub fn id(&self) -> u8 {
        match self {
            DeviceKind::Cpu(id) | DeviceKind::Gpu(id) | DeviceKind::Npu(id) => *id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpType {
    MatMulF32,
    MatMulF16,
    MatMulQ8,
    MatMulQ4,
    RmsNorm,
    Silu,
    Softmax,
    RoPE,
    Attention,
}

impl OpType {
    pub fn is_heavy(&self) -> bool {
        matches!(self, OpType::MatMulF32 | OpType::MatMulF16 | OpType::MatMulQ8 | OpType::MatMulQ4)
    }
}

#[derive(Debug, Clone)]
pub struct WorkSpec {
    pub op: OpType,
    pub weight: Arc<Vec<u8>>,
    pub input: Arc<Vec<u8>>,
    pub scales: Arc<Vec<f32>>,
    pub n_in: usize,
    pub n_out: usize,
}

impl WorkSpec {
    pub fn new_matmul_q8(
        weight: Vec<u8>,
        input: Vec<u8>,
        scales: Vec<f32>,
        n_in: usize,
        n_out: usize,
    ) -> Self {
        Self {
            op: OpType::MatMulQ8,
            weight: Arc::new(weight),
            input: Arc::new(input),
            scales: Arc::new(scales),
            n_in,
            n_out,
        }
    }

    pub fn split_at(&self, split_idx: usize) -> (WorkSpec, WorkSpec) {
        let row_bytes = self.n_in * 16 / 8;
        let blocks_per_row = (self.n_in + 31) / 32;

        let (cpu_input, gpu_input) = self.input.split_at(split_idx * row_bytes);
        let (cpu_scales, gpu_scales) = self.scales.split_at(blocks_per_row * split_idx);

        (
            WorkSpec {
                op: self.op,
                weight: Arc::clone(&self.weight),
                input: Arc::new(cpu_input.to_vec()),
                scales: Arc::new(cpu_scales.to_vec()),
                n_in: self.n_in,
                n_out: split_idx,
            },
            WorkSpec {
                op: self.op,
                weight: Arc::clone(&self.weight),
                input: Arc::new(gpu_input.to_vec()),
                scales: Arc::new(gpu_scales.to_vec()),
                n_in: self.n_in,
                n_out: self.n_out - split_idx,
            },
        )
    }

    pub fn split_for(&self, parts: usize) -> Vec<WorkSpec> {
        if parts <= 1 {
            return vec![self.clone()];
        }

        let chunk_size = (self.n_out + parts - 1) / parts;
        let mut specs = Vec::with_capacity(parts);

        for i in 0..parts {
            let start = i * chunk_size;
            let end = (start + chunk_size).min(self.n_out);
            if start >= self.n_out {
                break;
            }

            let row_bytes = self.n_in * 16 / 8;
            let blocks_per_row = (self.n_in + 31) / 32;

            let input_start = start * row_bytes;
            let input_end = end * row_bytes;
            let scales_start = blocks_per_row * start;
            let scales_end = blocks_per_row * end;

            specs.push(WorkSpec {
                op: self.op,
                weight: Arc::clone(&self.weight),
                input: Arc::new(self.input[input_start..input_end].to_vec()),
                scales: Arc::new(self.scales[scales_start..scales_end].to_vec()),
                n_in: self.n_in,
                n_out: end - start,
            });
        }

        specs
    }
}

pub trait ComputeDevice: Send + Sync {
    fn kind(&self) -> DeviceKind;
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
    fn supports(&self, op: OpType) -> bool;

    fn execute_matmul_q8(&self, spec: &WorkSpec) -> Result<Vec<f32>>;
    fn sync(&self);
}

#[derive(Clone, Copy)]
pub struct DeviceRatio(u8);

impl DeviceRatio {
    pub fn new(ratio: u8) -> Self {
        Self(ratio.min(100))
    }

    pub fn ratio(&self) -> u8 {
        self.0
    }

    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }
}

impl Default for DeviceRatio {
    fn default() -> Self {
        Self(0)
    }
}

impl From<u8> for DeviceRatio {
    fn from(v: u8) -> Self {
        Self::new(v)
    }
}

#[derive(Clone)]
pub struct DeviceConfig {
    pub kind: DeviceKind,
    pub ratio: DeviceRatio,
}

impl DeviceConfig {
    pub fn new(kind: DeviceKind, ratio: u8) -> Self {
        Self {
            kind,
            ratio: DeviceRatio::new(ratio),
        }
    }

    pub fn cpu(id: u8, ratio: u8) -> Self {
        Self::new(DeviceKind::Cpu(id), ratio)
    }

    pub fn gpu(id: u8, ratio: u8) -> Self {
        Self::new(DeviceKind::Gpu(id), ratio)
    }

    pub fn npu(id: u8, ratio: u8) -> Self {
        Self::new(DeviceKind::Npu(id), ratio)
    }
}

pub struct Scheduler {
    devices: Vec<Arc<dyn ComputeDevice>>,
    config: Vec<DeviceConfig>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
            config: Vec::new(),
        }
    }

    pub fn add_device<D: ComputeDevice + 'static>(&mut self, device: D) -> &mut Self {
        self.devices.push(Arc::new(device));
        self
    }

    pub fn with_device<D: ComputeDevice + 'static>(mut self, device: D) -> Self {
        self.add_device(device);
        self
    }

    pub fn set_config(&mut self, config: Vec<DeviceConfig>) -> &mut Self {
        self.config = config;
        self
    }

    pub fn with_config(mut self, config: Vec<DeviceConfig>) -> Self {
        self.set_config(config);
        self
    }

    pub fn execute(&self, spec: WorkSpec) -> Result<Vec<f32>> {
        let active_devices = self.active_devices();
        if active_devices.is_empty() {
            return Err(ComputeError::DeviceNotAvailable("No devices available".to_string()));
        }

        if active_devices.len() == 1 {
            return active_devices[0].execute_matmul_q8(&spec);
        }

        let parts = active_devices.len();
        let chunks = spec.split_for(parts);

        let mut results = Vec::with_capacity(parts);
        for (device, chunk) in active_devices.iter().zip(chunks.iter()) {
            results.push(device.execute_matmul_q8(chunk)?);
        }

        let mut combined = Vec::with_capacity(spec.n_out);
        for r in results {
            combined.extend(r);
        }
        Ok(combined)
    }

    pub fn execute_parallel(&self, spec: WorkSpec) -> Result<Vec<f32>> {
        let active_devices = self.active_devices();
        if active_devices.is_empty() {
            return Err(ComputeError::DeviceNotAvailable("No devices available".to_string()));
        }

        if active_devices.len() == 1 {
            return active_devices[0].execute_matmul_q8(&spec);
        }

        let parts = active_devices.len();
        let chunks = spec.split_for(parts);

        let handles: Vec<_> = active_devices
            .iter()
            .zip(chunks.iter())
            .map(|(device, chunk)| {
                let device = Arc::clone(device);
                let chunk = chunk.clone();
                std::thread::spawn(move || device.execute_matmul_q8(&chunk))
            })
            .collect();

        let mut results = Vec::with_capacity(parts);
        for h in handles {
            results.push(h.join().map_err(|_| ComputeError::ExecutionError("Thread panicked".to_string()))??);
        }

        let mut combined = Vec::with_capacity(spec.n_out);
        for r in results {
            combined.extend(r);
        }
        Ok(combined)
    }

    fn active_devices(&self) -> Vec<Arc<dyn ComputeDevice>> {
        if self.config.is_empty() {
            return self.devices.iter().filter(|d| d.is_available()).cloned().collect();
        }

        self.devices
            .iter()
            .filter(|d| d.is_available())
            .filter(|d| {
                self.config.iter().any(|c| c.kind == d.kind() && !c.ratio.is_zero())
            })
            .cloned()
            .collect()
    }

    pub fn available_devices(&self) -> Vec<DeviceKind> {
        self.devices.iter().filter(|d| d.is_available()).map(|d| d.kind()).collect()
    }

    pub fn gpu_configured(&self) -> bool {
        if self.config.is_empty() {
            return false;
        }
        self.config.iter().any(|c| matches!(c.kind, DeviceKind::Gpu(_)) && c.ratio.ratio() > 0)
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

pub use crate::compute::cpu::CpuDevice;
pub use crate::compute::gpu::GpuDevice;
