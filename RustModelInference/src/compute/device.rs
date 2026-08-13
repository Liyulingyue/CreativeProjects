use super::program::DevicePlan;
use crate::{ComponentId, DeviceId, GGMLType, PlacementMode, TensorCatalog, TensorId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BackendKind {
    Cpu,
    Vulkan,
    Metal,
    Npu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LayerFamily {
    Qwen3,
    Qwen35Dense,
    Qwen35Recurrent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCapabilities {
    pub components: BTreeSet<ComponentId>,
    pub modes: BTreeSet<PlacementMode>,
    pub layer_families: BTreeSet<LayerFamily>,
    pub tensor_types: BTreeSet<GGMLType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceDescriptor {
    pub id: DeviceId,
    pub backend: BackendKind,
    pub physical_key: String,
    pub name: String,
    pub usable_bytes: u64,
    pub max_allocation_bytes: u64,
    pub buffer_alignment: u64,
    pub unified_memory: bool,
    pub capabilities: DeviceCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SlotId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProgramId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FenceId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionStats {
    pub resident_bytes: u64,
    pub resident_allocations: u64,
    pub resident_frees: u64,
    pub weight_uploads: u64,
    pub weight_upload_bytes: u64,
    pub activation_h2d_bytes: u64,
    pub activation_d2h_bytes: u64,
    pub submissions: u64,
    pub host_waits: u64,
}

#[derive(Default)]
struct LifecycleCounters {
    resident_bytes: AtomicU64,
    resident_allocations: AtomicU64,
    resident_frees: AtomicU64,
    weight_uploads: AtomicU64,
    weight_upload_bytes: AtomicU64,
    activation_h2d_bytes: AtomicU64,
    activation_d2h_bytes: AtomicU64,
    submissions: AtomicU64,
    host_waits: AtomicU64,
}

#[derive(Clone, Default)]
pub struct LifecycleProbe(Arc<LifecycleCounters>);

impl LifecycleProbe {
    pub fn snapshot(&self) -> SessionStats {
        SessionStats {
            resident_bytes: self.0.resident_bytes.load(Ordering::Relaxed),
            resident_allocations: self.0.resident_allocations.load(Ordering::Relaxed),
            resident_frees: self.0.resident_frees.load(Ordering::Relaxed),
            weight_uploads: self.0.weight_uploads.load(Ordering::Relaxed),
            weight_upload_bytes: self.0.weight_upload_bytes.load(Ordering::Relaxed),
            activation_h2d_bytes: self.0.activation_h2d_bytes.load(Ordering::Relaxed),
            activation_d2h_bytes: self.0.activation_d2h_bytes.load(Ordering::Relaxed),
            submissions: self.0.submissions.load(Ordering::Relaxed),
            host_waits: self.0.host_waits.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("backend is unavailable: {backend:?}")]
    BackendUnavailable { backend: BackendKind },
    #[error("device is unavailable: {device:?}")]
    DeviceUnavailable { device: DeviceId },
    #[error("duplicate backend registration: {backend:?}")]
    DuplicateBackend { backend: BackendKind },
    #[error("duplicate device id: {device:?}")]
    DuplicateDeviceId { device: DeviceId },
    #[error("descriptor {id:?} reports {actual:?}, expected {expected:?}")]
    DescriptorBackendMismatch {
        id: DeviceId,
        expected: BackendKind,
        actual: BackendKind,
    },
    #[error("unsupported {operation} for {device:?}")]
    Unsupported {
        device: DeviceId,
        operation: &'static str,
    },
    #[error("allocation failed on {device:?}: {message}")]
    Allocation { device: DeviceId, message: String },
    #[error("weight upload failed on {device:?}: {message}")]
    Upload { device: DeviceId, message: String },
    #[error("pipeline creation failed on {device:?}: {message}")]
    Pipeline { device: DeviceId, message: String },
    #[error("submission failed on {device:?}: {message}")]
    Submission { device: DeviceId, message: String },
    #[error("program is missing for tensor {tensor:?}")]
    ProgramMissing { tensor: TensorId },
    #[error("invalid compiled handle")]
    InvalidHandle,
    #[error("inference state is poisoned")]
    PoisonedRun,
}

pub struct RunParams<'a> {
    pub token_count: u32,
    pub position_start: u32,
    pub mrope_positions: &'a [[u32; 4]],
    pub token_ids: &'a [u32],
}

pub trait DeviceSession: Send {
    fn descriptor(&self) -> &DeviceDescriptor;
    fn write_f32(&mut self, slot: SlotId, values: &[f32]) -> Result<(), BackendError>;
    fn submit(
        &mut self,
        program: ProgramId,
        params: &RunParams<'_>,
    ) -> Result<FenceId, BackendError>;
    fn wait(&mut self, fence: FenceId) -> Result<(), BackendError>;
    fn read_f32(&mut self, slot: SlotId, values: &mut [f32]) -> Result<(), BackendError>;
    fn reset_state(&mut self) -> Result<(), BackendError>;
    fn stats(&self) -> SessionStats;
    fn lifecycle_probe(&self) -> LifecycleProbe;
}

pub trait DeviceDiscovery: Send + Sync {
    fn backend(&self) -> BackendKind;
    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, BackendError>;
}

pub trait DeviceProvider: DeviceDiscovery {
    fn open(
        &self,
        descriptor: &DeviceDescriptor,
        plan: &DevicePlan,
        catalog: Arc<TensorCatalog>,
    ) -> Result<Box<dyn DeviceSession>, BackendError>;
}

struct ProviderDiscovery(Arc<dyn DeviceProvider>);

impl DeviceDiscovery for ProviderDiscovery {
    fn backend(&self) -> BackendKind {
        self.0.backend()
    }

    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, BackendError> {
        self.0.enumerate()
    }
}

pub struct DeviceRegistry {
    discoveries: BTreeMap<BackendKind, Arc<dyn DeviceDiscovery>>,
    providers: BTreeMap<BackendKind, Arc<dyn DeviceProvider>>,
    descriptors: BTreeMap<DeviceId, DeviceDescriptor>,
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self {
            discoveries: BTreeMap::new(),
            providers: BTreeMap::new(),
            descriptors: BTreeMap::new(),
        }
    }

    pub fn register_provider(
        &mut self,
        provider: Arc<dyn DeviceProvider>,
    ) -> Result<(), BackendError> {
        let backend = provider.backend();
        if self.discoveries.contains_key(&backend) || self.providers.contains_key(&backend) {
            return Err(BackendError::DuplicateBackend { backend });
        }
        self.discoveries
            .insert(backend, Arc::new(ProviderDiscovery(provider.clone())));
        self.providers.insert(backend, provider);
        Ok(())
    }

    pub fn provider(&self, backend: BackendKind) -> Result<Arc<dyn DeviceProvider>, BackendError> {
        self.providers
            .get(&backend)
            .cloned()
            .ok_or(BackendError::BackendUnavailable { backend })
    }

    pub fn register_discovery(
        &mut self,
        discovery: Arc<dyn DeviceDiscovery>,
    ) -> Result<(), BackendError> {
        let backend = discovery.backend();
        if self.discoveries.contains_key(&backend) {
            return Err(BackendError::DuplicateBackend { backend });
        }
        self.discoveries.insert(backend, discovery);
        Ok(())
    }

    pub fn discover(&mut self, requested: &BTreeSet<BackendKind>) -> Result<(), BackendError> {
        for backend in requested {
            let discovery = self
                .discoveries
                .get(backend)
                .ok_or(BackendError::BackendUnavailable { backend: *backend })?;
            for descriptor in discovery.enumerate()? {
                if descriptor.backend != *backend {
                    return Err(BackendError::DescriptorBackendMismatch {
                        id: descriptor.id,
                        expected: *backend,
                        actual: descriptor.backend,
                    });
                }
                let id = descriptor.id.clone();
                if self.descriptors.contains_key(&id) {
                    return Err(BackendError::DuplicateDeviceId { device: id });
                }
                self.descriptors.insert(id, descriptor);
            }
        }
        Ok(())
    }

    pub fn get(&self, id: &DeviceId) -> Option<&DeviceDescriptor> {
        self.descriptors.get(id)
    }

    pub fn require(&self, id: &DeviceId) -> Result<&DeviceDescriptor, BackendError> {
        self.get(id)
            .ok_or_else(|| BackendError::DeviceUnavailable { device: id.clone() })
    }
}

impl Default for DeviceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestDiscovery {
        backend: BackendKind,
        descriptor: DeviceDescriptor,
        calls: Arc<AtomicUsize>,
    }

    impl TestDiscovery {
        fn new(backend: BackendKind, id: &str, calls: Arc<AtomicUsize>) -> Self {
            Self {
                backend,
                descriptor: DeviceDescriptor {
                    id: DeviceId::parse(id).unwrap(),
                    backend,
                    physical_key: id.into(),
                    name: id.into(),
                    usable_bytes: 1024,
                    max_allocation_bytes: 1024,
                    buffer_alignment: 1,
                    unified_memory: backend == BackendKind::Cpu,
                    capabilities: DeviceCapabilities {
                        components: BTreeSet::from([ComponentId::Llm]),
                        modes: BTreeSet::from([PlacementMode::Row]),
                        layer_families: BTreeSet::new(),
                        tensor_types: BTreeSet::from([GGMLType::Q8_0]),
                    },
                },
                calls,
            }
        }
    }

    impl DeviceDiscovery for TestDiscovery {
        fn backend(&self) -> BackendKind {
            self.backend
        }

        fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, BackendError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![self.descriptor.clone()])
        }
    }

    #[test]
    fn discovers_only_requested_backends_and_rejects_duplicate_ids() {
        let cpu_calls = Arc::new(AtomicUsize::new(0));
        let vulkan_calls = Arc::new(AtomicUsize::new(0));
        let mut registry = DeviceRegistry::new();
        registry
            .register_discovery(Arc::new(TestDiscovery::new(
                BackendKind::Cpu,
                "cpu0",
                cpu_calls.clone(),
            )))
            .unwrap();
        registry
            .register_discovery(Arc::new(TestDiscovery::new(
                BackendKind::Vulkan,
                "vulkan0",
                vulkan_calls.clone(),
            )))
            .unwrap();

        registry
            .discover(&BTreeSet::from([BackendKind::Cpu]))
            .unwrap();

        assert_eq!(cpu_calls.load(Ordering::SeqCst), 1);
        assert_eq!(vulkan_calls.load(Ordering::SeqCst), 0);
        assert!(registry.get(&DeviceId::parse("cpu0").unwrap()).is_some());
    }

    #[test]
    fn npu_id_has_a_contract_but_no_implicit_provider() {
        let registry = DeviceRegistry::new();
        let error = registry
            .require(&DeviceId::parse("npu0").unwrap())
            .unwrap_err();
        assert!(matches!(error, BackendError::DeviceUnavailable { .. }));
    }

    #[test]
    fn rejects_duplicate_backends_and_device_ids() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut duplicate_backend = DeviceRegistry::new();
        duplicate_backend
            .register_discovery(Arc::new(TestDiscovery::new(
                BackendKind::Cpu,
                "cpu0",
                calls.clone(),
            )))
            .unwrap();
        assert!(matches!(
            duplicate_backend.register_discovery(Arc::new(TestDiscovery::new(
                BackendKind::Cpu,
                "cpu1",
                calls.clone(),
            ))),
            Err(BackendError::DuplicateBackend {
                backend: BackendKind::Cpu
            })
        ));

        let mut duplicate_id = DeviceRegistry::new();
        duplicate_id
            .register_discovery(Arc::new(TestDiscovery::new(
                BackendKind::Cpu,
                "cpu0",
                calls.clone(),
            )))
            .unwrap();
        duplicate_id
            .register_discovery(Arc::new(TestDiscovery::new(
                BackendKind::Vulkan,
                "cpu0",
                calls,
            )))
            .unwrap();
        assert!(matches!(
            duplicate_id.discover(&BTreeSet::from([BackendKind::Cpu, BackendKind::Vulkan])),
            Err(BackendError::DuplicateDeviceId { .. })
        ));
    }
}
