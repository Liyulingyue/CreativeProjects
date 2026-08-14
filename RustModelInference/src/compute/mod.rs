pub mod cpu;
pub mod device;
pub mod gpu;
pub mod npu;

pub use cpu::CpuDevice;
pub use device::{ComputeError, ComputeDevice, DeviceConfig, DeviceKind, DeviceRatio, OpType, Result, Scheduler, WorkSpec};
pub use gpu::GpuDevice;
pub use npu::NpuDevice;

use std::sync::{Mutex, OnceLock};

static GLOBAL_SCHEDULER: OnceLock<Mutex<Scheduler>> = OnceLock::new();

pub fn init() -> &'static Mutex<Scheduler> {
    GLOBAL_SCHEDULER.get_or_init(|| {
        let mut scheduler = Scheduler::new();

        let cpu_count = num_physical_cpus();
        eprintln!("Found {} physical CPU socket(s)", cpu_count);
        for id in 0..cpu_count {
            scheduler.add_device(CpuDevice::new(id));
            eprintln!("  CPU {}: CPU-AVX2", id);
        }

        #[cfg(feature = "gpu")]
        {
            let gpu_count = GpuDevice::gpu_count();
            eprintln!("Found {} GPU(s) - DISABLED (kernel bug)", gpu_count);
            for id in 0..gpu_count {
                match GpuDevice::new(id as u8) {
                    Ok(gpu) => {
                        if gpu.is_available() && !gpu.is_software_renderer() {
                            eprintln!("  GPU {}: {} (available but disabled)", id, gpu.name());
                        } else {
                            eprintln!("  GPU {}: {} (skipped)", id, gpu.name());
                        }
                    }
                    Err(e) => {
                        eprintln!("  GPU {}: init failed: {:?}", id, e);
                    }
                }
            }
        }

        Mutex::new(scheduler)
    })
}

fn num_physical_cpus() -> u8 {
    1
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

pub fn enable_gpu(id: u8, ratio: u8) {
    set_device_ratio(DeviceKind::Gpu(id), ratio);
}

pub fn gpu_configured() -> bool {
    if let Ok(sched) = scheduler().lock() {
        sched.gpu_configured()
    } else {
        false
    }
}

pub fn matmul_q8(
    weight: &[u8],
    input: &[u8],
    scales: &[f32],
    n_in: usize,
    n_out: usize,
) -> Vec<f32> {
    // GPU path disabled - Intel Graphics is too slow for compute
    // GPU produces correct output but ~4x slower than CPU
    let mut output = vec![0.0f32; n_out];
    crate::ops::matmul_q8_0_quantized(weight, input, scales, &mut output, n_in, n_out);
    output
}

pub fn matmul_q8_inplace(
    weight: &[u8],
    input: &[u8],
    scales: &[f32],
    output: &mut [f32],
    n_in: usize,
    n_out: usize,
) {
    let result = matmul_q8(weight, input, scales, n_in, n_out);
    output.copy_from_slice(&result);
}
