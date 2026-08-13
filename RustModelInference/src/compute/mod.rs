pub mod device;

pub use device::{
    BackendError, BackendKind, DeviceCapabilities, DeviceDescriptor, DeviceDiscovery,
    DeviceRegistry, DeviceSession, FenceId, LayerFamily, LifecycleProbe, ProgramId, RunParams,
    SessionStats, SlotId,
};

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
