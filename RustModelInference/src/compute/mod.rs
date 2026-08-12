pub mod cpu;
pub mod device;
pub mod gpu;

pub use cpu::CpuDevice;
pub use device::{ComputeError, DeviceType, GpuRatio, HybridExecutor, OpType, Result, WorkSpec};
pub use gpu::GpuDevice;

use std::sync::{Mutex, OnceLock};

static GLOBAL_EXECUTOR: OnceLock<Mutex<HybridExecutor>> = OnceLock::new();

pub fn init() -> &'static Mutex<HybridExecutor> {
    GLOBAL_EXECUTOR.get_or_init(|| Mutex::new(HybridExecutor::new()))
}

pub fn executor() -> &'static Mutex<HybridExecutor> {
    init()
}

pub fn set_gpu_ratio(ratio: u8) {
    if let Ok(mut exec) = init().lock() {
        exec.set_gpu_ratio(ratio);
    }
}

pub fn gpu_ratio() -> u8 {
    init().lock().map(|e| e.gpu_ratio()).unwrap_or(0)
}
