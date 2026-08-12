use thiserror::Error;
use std::sync::Arc;

#[derive(Error, Debug)]
pub enum ComputeError {
    #[error("Device {0} does not support this operation")]
    UnsupportedOp(String),

    #[error("Device not available: {0}")]
    DeviceNotAvailable(String),

    #[error("Memory error: {0}")]
    MemoryError(String),

    #[error("Hybrid execution failed: {0}")]
    HybridError(String),
}

pub type Result<T> = std::result::Result<T, ComputeError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    Cpu,
    Gpu,
    Npu,
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
    pub output: Arc<Vec<f32>>,
    pub n_in: usize,
    pub n_out: usize,
}

impl WorkSpec {
    pub fn new_matmul_q8(
        weight: Vec<u8>,
        input: Vec<u8>,
        scales: Vec<f32>,
        output: Vec<f32>,
        n_in: usize,
        n_out: usize,
    ) -> Self {
        Self {
            op: OpType::MatMulQ8,
            weight: Arc::new(weight),
            input: Arc::new(input),
            scales: Arc::new(scales),
            output: Arc::new(output),
            n_in,
            n_out,
        }
    }

    pub fn split_at(&self, split_idx: usize) -> (WorkSpec, WorkSpec) {
        let row_bytes = self.n_in * 16 / 8;
        let blocks_per_row = (self.n_in + 31) / 32;

        let cpu_input = self.input[..split_idx * row_bytes].to_vec();
        let cpu_scales = self.scales[..blocks_per_row * split_idx].to_vec();

        let gpu_input = self.input[split_idx * row_bytes..].to_vec();
        let gpu_scales = self.scales[blocks_per_row * split_idx..].to_vec();

        let gpu_n_out = self.n_out - split_idx;

        (
            WorkSpec {
                op: self.op,
                weight: Arc::clone(&self.weight),
                input: Arc::new(cpu_input),
                scales: Arc::new(cpu_scales),
                output: Arc::new(vec![0.0; split_idx]),
                n_in: self.n_in,
                n_out: split_idx,
            },
            WorkSpec {
                op: self.op,
                weight: Arc::clone(&self.weight),
                input: Arc::new(gpu_input),
                scales: Arc::new(gpu_scales),
                output: Arc::new(vec![0.0; gpu_n_out]),
                n_in: self.n_in,
                n_out: gpu_n_out,
            },
        )
    }

    pub fn merge_results(&mut self, cpu_result: Vec<f32>, gpu_result: Vec<f32>) {
        let mut combined = Vec::with_capacity(self.n_out);
        combined.extend(cpu_result);
        combined.extend(gpu_result);
        self.output = Arc::new(combined);
    }
}

pub trait ComputeDevice: Send + Sync {
    fn device_type(&self) -> DeviceType;
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
    fn supports(&self, op: OpType) -> bool;

    fn execute_matmul_q8(&self, spec: &WorkSpec) -> Result<Vec<f32>>;
    fn sync(&self);
}

#[derive(Clone, Copy)]
pub struct GpuRatio(u8);

impl GpuRatio {
    pub fn new(ratio: u8) -> Self {
        Self(if ratio > 100 { 100 } else { ratio })
    }

    pub fn cpu(&self) -> u8 {
        100 - self.0
    }

    pub fn gpu(&self) -> u8 {
        self.0
    }

    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }

    pub fn is_full(&self) -> bool {
        self.0 == 100
    }
}

impl Default for GpuRatio {
    fn default() -> Self {
        Self(0)
    }
}

impl From<u8> for GpuRatio {
    fn from(v: u8) -> Self {
        Self::new(v)
    }
}

pub struct HybridExecutor {
    cpu: Arc<dyn ComputeDevice>,
    gpu: Option<Arc<dyn ComputeDevice>>,
    gpu_ratio: GpuRatio,
}

impl HybridExecutor {
    pub fn new() -> Self {
        let cpu = CpuDevice::new();

        #[cfg(feature = "gpu")]
        let gpu = GpuDevice::new().ok().map(Arc::new);
        #[cfg(not(feature = "gpu"))]
        let gpu = None;

        Self {
            cpu: Arc::new(cpu),
            gpu,
            gpu_ratio: GpuRatio::default(),
        }
    }

    pub fn with_gpu_ratio(mut self, ratio: u8) -> Self {
        self.gpu_ratio = GpuRatio::new(ratio);
        self
    }

    pub fn set_gpu_ratio(&mut self, ratio: u8) {
        self.gpu_ratio = GpuRatio::new(ratio);
    }

    pub fn gpu_ratio(&self) -> u8 {
        self.gpu_ratio.gpu()
    }

    pub fn is_gpu_available(&self) -> bool {
        self.gpu.as_ref().map(|g| g.is_available()).unwrap_or(false)
    }

    pub fn execute(&self, spec: WorkSpec) -> Result<Vec<f32>> {
        if spec.op.is_heavy() && self.gpu_ratio.gpu() > 0 && self.is_gpu_available() {
            let split_idx = (spec.n_out * self.gpu_ratio.gpu() as usize) / 100;
            if split_idx > 0 && split_idx < spec.n_out {
                let (cpu_spec, gpu_spec) = spec.split_at(split_idx);

                let cpu_result = self.cpu.execute_matmul_q8(&cpu_spec)?;
                let gpu_result = self.gpu.as_ref().unwrap().execute_matmul_q8(&gpu_spec)?;

                let mut combined = Vec::with_capacity(spec.n_out);
                combined.extend(cpu_result);
                combined.extend(gpu_result);
                return Ok(combined);
            }
        }

        self.cpu.execute_matmul_q8(&spec)
    }

    pub fn execute_parallel(&self, spec: WorkSpec) -> Result<Vec<f32>> {
        if spec.op.is_heavy() && self.gpu_ratio.gpu() > 0 && self.is_gpu_available() {
            let split_idx = (spec.n_out * self.gpu_ratio.gpu() as usize) / 100;
            if split_idx > 0 && split_idx < spec.n_out {
                let (cpu_spec, gpu_spec) = spec.split_at(split_idx);

                let cpu_handle = {
                    let cpu = Arc::clone(&self.cpu);
                    let cpu_spec = cpu_spec.clone();
                    std::thread::spawn(move || cpu.execute_matmul_q8(&cpu_spec))
                };

                let gpu_result = self.gpu.as_ref().unwrap().execute_matmul_q8(&gpu_spec)?;
                let cpu_result = cpu_handle.join()
                    .map_err(|_| ComputeError::HybridError("CPU thread panicked".to_string()))??;

                let mut combined = Vec::with_capacity(spec.n_out);
                combined.extend(cpu_result);
                combined.extend(gpu_result);
                return Ok(combined);
            }
        }

        self.cpu.execute_matmul_q8(&spec)
    }

    pub fn execute_cpu_only(&self, spec: WorkSpec) -> Result<Vec<f32>> {
        self.cpu.execute_matmul_q8(&spec)
    }

    pub fn execute_gpu_only(&self, spec: WorkSpec) -> Result<Vec<f32>> {
        if let Some(ref gpu) = self.gpu {
            if gpu.is_available() {
                return gpu.execute_matmul_q8(&spec);
            }
        }
        self.cpu.execute_matmul_q8(&spec)
    }
}

impl Default for HybridExecutor {
    fn default() -> Self {
        Self::new()
    }
}

pub use crate::compute::cpu::CpuDevice;
pub use crate::compute::gpu::GpuDevice;
