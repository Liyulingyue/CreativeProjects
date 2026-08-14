use super::device::{ComputeDevice, DeviceKind, OpType, Result, WorkSpec};

pub struct CpuDevice {
    id: u8,
    name: String,
}

impl CpuDevice {
    pub fn new(id: u8) -> Self {
        let has_avx2 = crate::ops::has_avx2_fma();
        let has_neon = cfg!(target_arch = "aarch64");

        let name = if has_avx2 {
            format!("CPU-AVX2-{}", id)
        } else if has_neon {
            format!("CPU-NEON-{}", id)
        } else {
            format!("CPU-Scalar-{}", id)
        };

        Self { id, name }
    }

    pub fn id(&self) -> u8 {
        self.id
    }
}

impl ComputeDevice for CpuDevice {
    fn kind(&self) -> DeviceKind {
        DeviceKind::Cpu(self.id)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_available(&self) -> bool {
        true
    }

    fn supports(&self, op: OpType) -> bool {
        matches!(
            op,
            OpType::MatMulF32 | OpType::MatMulF16 | OpType::MatMulQ8 | OpType::MatMulQ4
        )
    }

    fn execute_matmul_q8(&self, spec: &WorkSpec) -> Result<Vec<f32>> {
        let n_in = spec.n_in;
        let n_out = spec.n_out;

        let mut output = vec![0.0f32; n_out];
        crate::ops::matmul_q8_0_quantized(
            &spec.weight,
            &spec.input,
            &spec.scales,
            &mut output,
            n_in,
            n_out,
        );
        Ok(output)
    }

    fn sync(&self) {}
}

impl Default for CpuDevice {
    fn default() -> Self {
        Self::new(0)
    }
}
