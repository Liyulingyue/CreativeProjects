pub mod cpu;
pub mod device;
#[cfg(all(target_os = "macos", feature = "metal"))]
pub mod metal;
pub mod program;
pub mod session;
#[cfg(feature = "vulkan")]
pub mod vulkan;

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
