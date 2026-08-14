use super::{ComputeDevice, Op, OpType, Result, CpuDevice, VulkanDevice};

pub struct Scheduler {
    cpu: CpuDevice,
    gpu: Option<VulkanDevice>,
}

impl Scheduler {
    pub fn new() -> Self {
        let gpu = VulkanDevice::new().ok();
        Self {
            cpu: CpuDevice::new(),
            gpu,
        }
    }
    
    pub fn dispatch(&mut self, op: &mut Op) -> Result<()> {
        // Choose device based on op type and availability
        let use_gpu = self.gpu.as_ref().map(|g| g.supports(op.op_type)).unwrap_or(false);

        if use_gpu {
            self.gpu.as_mut().unwrap().dispatch(op)
        } else {
            self.cpu.dispatch(op)
        }
    }
    
    pub fn sync(&mut self) -> Result<()> {
        self.cpu.sync()?;
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.sync()?;
        }
        Ok(())
    }
    
    pub fn has_gpu(&self) -> bool {
        self.gpu.is_some()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}
