pub mod cpu;
pub mod device;
#[cfg(all(target_os = "macos", feature = "metal"))]
pub mod metal;
pub mod program;
pub mod session;
#[cfg(feature = "vulkan")]
pub mod vulkan;

use std::collections::BTreeSet;
use std::sync::Arc;

pub use cpu::{CpuProvider, CpuSession};
pub use device::{
    BackendError, BackendKind, DeviceCapabilities, DeviceDescriptor, DeviceDiscovery,
    DeviceProvider, DeviceRegistry, DeviceSession, FenceId, LayerFamily, LifecycleProbe, ProgramId,
    RunParams, SessionStats, SlotId,
};
#[cfg(all(target_os = "macos", feature = "metal"))]
pub use metal::MetalProvider;
pub use program::*;
pub use session::{CompiledModel, ExecutionRun};
#[cfg(feature = "vulkan")]
pub use vulkan::VulkanProvider;

pub fn register_requested_providers(
    registry: &mut DeviceRegistry,
    requested: &BTreeSet<BackendKind>,
    cpu_threads: usize,
) -> Result<(), BackendError> {
    for backend in requested {
        match backend {
            BackendKind::Cpu => {
                registry.register_provider(Arc::new(CpuProvider::new(cpu_threads)))?;
            }
            BackendKind::Vulkan => {
                #[cfg(feature = "vulkan")]
                registry.register_provider(Arc::new(VulkanProvider::new()?))?;
                #[cfg(not(feature = "vulkan"))]
                return Err(BackendError::BackendUnavailable {
                    backend: BackendKind::Vulkan,
                });
            }
            BackendKind::Metal => {
                #[cfg(all(target_os = "macos", feature = "metal"))]
                registry.register_provider(Arc::new(MetalProvider::new()?))?;
                #[cfg(not(all(target_os = "macos", feature = "metal")))]
                return Err(BackendError::BackendUnavailable {
                    backend: BackendKind::Metal,
                });
            }
            BackendKind::Npu => {
                return Err(BackendError::BackendUnavailable {
                    backend: BackendKind::Npu,
                });
            }
        }
    }
    Ok(())
}

#[doc(hidden)]
pub fn execute_q8_cpu_compat(
    weight: &[u8],
    input: &[u8],
    scales: &[f32],
    output: &mut [f32],
    n_in: usize,
    n_out: usize,
) {
    crate::ops::matmul_q8_0_quantized(weight, input, scales, output, n_in, n_out);
}
