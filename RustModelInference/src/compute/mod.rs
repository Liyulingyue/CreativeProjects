pub mod cpu;
pub mod device;
pub mod gpu;
pub mod layer;
pub mod layer_engine;
pub mod npu;

pub use cpu::CpuDevice;
pub use device::{ComputeError, ComputeDevice, DeviceConfig, DeviceKind, DeviceRatio, OpType, Result, Scheduler, WorkSpec};
pub use gpu::GpuDevice;
pub use layer::{LayerSpec, LayerOp, LayerScheduleConfig, LayerDeviceConfig, LayerError, LayerResult};
pub use layer_engine::{LayerEngine, LayerMode, GpuWeightCache};
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
            eprintln!("Found {} GPU(s)", gpu_count);
            for id in 0..gpu_count {
                match GpuDevice::new(id as u8) {
                    Ok(gpu) => {
                        if gpu.is_available() && !gpu.is_software_renderer() {
                            eprintln!("  GPU {}: {} (available)", id, gpu.name());
                            scheduler.add_device(gpu);
                        } else if gpu.is_software_renderer() {
                            eprintln!("  GPU {}: {} (software renderer, skipped)", id, gpu.name());
                        } else {
                            eprintln!("  GPU {}: {} (not available)", id, gpu.name());
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
        // Check if this device kind already has a config
        let existing_idx = sched.config().iter().position(|c| c.kind == kind);
        if let Some(idx) = existing_idx {
            // Update existing
            sched.config_mut()[idx].ratio = DeviceRatio::new(ratio);
        } else {
            // Add new
            sched.config_mut().push(DeviceConfig::new(kind, ratio));
        }
    }
}

pub fn enable_gpu(id: u8, ratio: u8) {
    set_device_ratio(DeviceKind::Gpu(id), ratio);
}

pub fn get_device_ratio(kind: DeviceKind) -> u8 {
    if let Ok(sched) = scheduler().lock() {
        for c in sched.config().iter() {
            if c.kind == kind {
                return c.ratio.ratio();
            }
        }
    }
    0
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
    if gpu_configured() {
        let spec = WorkSpec::new_matmul_q8(
            weight.to_vec(),
            input.to_vec(),
            scales.to_vec(),
            n_in,
            n_out,
        );
        if let Ok(sched) = scheduler().lock() {
            if let Ok(result) = sched.execute_parallel(spec) {
                return result;
            }
        }
    }
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
