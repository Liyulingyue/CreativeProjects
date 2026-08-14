use super::device::{ComputeDevice, DeviceKind, OpType, Result, WorkSpec};

pub struct NpuDevice {
    id: u8,
    name: String,
    available: bool,
}

impl NpuDevice {
    pub fn new(id: u8) -> Self {
        let name = format!("NPU-{}", id);
        Self {
            id,
            name,
            available: false,
        }
    }

    pub fn id(&self) -> u8 {
        self.id
    }

    pub fn npu_count() -> u8 {
        0
    }
}

impl ComputeDevice for NpuDevice {
    fn kind(&self) -> DeviceKind {
        DeviceKind::Npu(self.id)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_available(&self) -> bool {
        self.available
    }

    fn supports(&self, op: OpType) -> bool {
        matches!(
            op,
            OpType::MatMulF32 | OpType::MatMulF16 | OpType::MatMulQ8 | OpType::MatMulQ4
        )
    }

    fn execute_matmul_q8(&self, spec: &WorkSpec) -> Result<Vec<f32>> {
        Err(crate::compute::ComputeError::DeviceNotAvailable(format!(
            "NPU {} not yet implemented",
            self.id
        )).into())
    }

    fn sync(&self) {}
}

impl Default for NpuDevice {
    fn default() -> Self {
        Self::new(0)
    }
}
