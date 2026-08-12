pub mod cpu;
pub mod device;
pub mod gpu;

pub use cpu::CpuDevice;
pub use device::{ComputeError, ComputeDevice, DeviceConfig, DeviceKind, DeviceRatio, OpType, Result, Scheduler, WorkSpec};
pub use gpu::GpuDevice;

use std::sync::{Mutex, OnceLock};

static GLOBAL_SCHEDULER: OnceLock<Mutex<Scheduler>> = OnceLock::new();

pub fn init() -> &'static Mutex<Scheduler> {
    GLOBAL_SCHEDULER.get_or_init(|| {
        let mut scheduler = Scheduler::new();
        scheduler.add_device(CpuDevice::new());

        #[cfg(feature = "gpu")]
        if let Ok(gpu) = GpuDevice::new() {
            if gpu.is_available() {
                scheduler.add_device(gpu);
            }
        }

        Mutex::new(scheduler)
    })
}

pub fn scheduler() -> &'static Mutex<Scheduler> {
    init()
}

pub fn set_device_ratio(kind: DeviceKind, ratio: u8) {
    if let Ok(mut sched) = init().lock() {
        let config = DeviceConfig::new(kind, ratio);
        sched.set_config(vec![config]);
    }
}

pub fn enable_gpu(ratio: u8) {
    set_device_ratio(DeviceKind::Gpu(0), ratio);
}
