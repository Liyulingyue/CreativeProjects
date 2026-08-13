use super::device::{
    BackendError, BackendKind, DeviceCapabilities, DeviceDescriptor, DeviceDiscovery,
    DeviceProvider, DeviceSession, FenceId, LifecycleProbe, ProgramId, RunParams, SessionStats,
    SlotId,
};
use super::program::{
    DevicePlan, LayerOp, ProgramKind, ProgramPlan, ResidentTensorPlan, SlotKind, SlotPlan,
    SlotStorage,
};
use crate::{ComponentId, DeviceId, GGMLType, LayerFamily, PlacementMode, TensorCatalog};
use ash::{vk, Entry, Instance};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ffi::{CStr, CString};
use std::io::Cursor;
use std::sync::Arc;

const LOCAL_SIZE: u32 = 64;
const Q8_BLOCK_ELEMENTS: u64 = 32;
const Q8_BLOCK_BYTES: u64 = 34;

pub struct VulkanProvider {
    context: Arc<VulkanContext>,
    adapters: Vec<AdapterInfo>,
}

struct VulkanContext {
    _entry: Entry,
    instance: Instance,
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        unsafe { self.instance.destroy_instance(None) };
    }
}

#[derive(Clone)]
struct AdapterInfo {
    descriptor: DeviceDescriptor,
    physical: vk::PhysicalDevice,
    queue_family: u32,
    memory: vk::PhysicalDeviceMemoryProperties,
    non_coherent_atom_size: u64,
    max_storage_buffer_range: u64,
    portability_subset: bool,
}

struct HandleOwner<T, F: FnMut(T)> {
    handles: Vec<T>,
    release: F,
}

impl<T, F: FnMut(T)> HandleOwner<T, F> {
    fn new(release: F) -> Self {
        Self {
            handles: Vec::new(),
            release,
        }
    }

    fn push(&mut self, handle: T) {
        self.handles.push(handle);
    }

    fn disarm(mut self) -> Vec<T> {
        std::mem::take(&mut self.handles)
    }

    fn release_last(&mut self) {
        if let Some(handle) = self.handles.pop() {
            (self.release)(handle);
        }
    }
}

fn take_created_handle<T, U, F, G>(
    owner: &mut HandleOwner<U, F>,
    result: Result<Vec<T>, (Vec<T>, vk::Result)>,
    wrap: G,
) -> Result<T, vk::Result>
where
    F: FnMut(U),
    G: Fn(T) -> U,
{
    match result {
        Ok(mut handles) if handles.len() == 1 => Ok(handles.pop().unwrap()),
        Ok(handles) => {
            for handle in handles {
                owner.push(wrap(handle));
            }
            Err(vk::Result::ERROR_UNKNOWN)
        }
        Err((handles, error)) => {
            for handle in handles {
                owner.push(wrap(handle));
            }
            Err(error)
        }
    }
}

struct DeviceOwner(Option<ash::Device>);

impl DeviceOwner {
    fn device(&self) -> &ash::Device {
        self.0.as_ref().expect("device owner is armed")
    }

    fn disarm(mut self) -> ash::Device {
        self.0.take().expect("device owner is armed")
    }
}

impl Drop for DeviceOwner {
    fn drop(&mut self) {
        if let Some(device) = self.0.take() {
            unsafe { device.destroy_device(None) };
        }
    }
}

enum OpenHandle {
    Buffer(BufferAllocation),
    Mapped(vk::DeviceMemory),
    CommandPool(vk::CommandPool),
    DescriptorSetLayout(vk::DescriptorSetLayout),
    PipelineLayout(vk::PipelineLayout),
    Shader(vk::ShaderModule),
    Pipeline(vk::Pipeline),
    DescriptorPool(vk::DescriptorPool),
    Fence(vk::Fence),
}

unsafe fn release_open_handle(device: &ash::Device, handle: OpenHandle) {
    match handle {
        OpenHandle::Buffer(buffer) => destroy_buffer(device, &buffer),
        OpenHandle::Mapped(memory) => device.unmap_memory(memory),
        OpenHandle::CommandPool(pool) => device.destroy_command_pool(pool, None),
        OpenHandle::DescriptorSetLayout(layout) => {
            device.destroy_descriptor_set_layout(layout, None)
        }
        OpenHandle::PipelineLayout(layout) => device.destroy_pipeline_layout(layout, None),
        OpenHandle::Shader(shader) => device.destroy_shader_module(shader, None),
        OpenHandle::Pipeline(pipeline) => device.destroy_pipeline(pipeline, None),
        OpenHandle::DescriptorPool(pool) => device.destroy_descriptor_pool(pool, None),
        OpenHandle::Fence(fence) => device.destroy_fence(fence, None),
    }
}

impl<T, F: FnMut(T)> Drop for HandleOwner<T, F> {
    fn drop(&mut self) {
        while let Some(handle) = self.handles.pop() {
            (self.release)(handle);
        }
    }
}

impl VulkanProvider {
    pub fn new() -> Result<Self, BackendError> {
        let entry = load_entry()?;
        let app_name = CString::new("rust-model-inference").unwrap();
        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .api_version(vk::API_VERSION_1_1);

        let mut extension_names = Vec::new();
        let mut flags = vk::InstanceCreateFlags::empty();
        #[cfg(target_os = "macos")]
        {
            let extensions = unsafe { entry.enumerate_instance_extension_properties(None) }
                .map_err(|error| {
                    unavailable(format!("enumerate instance extensions: {error:?}"))
                })?;
            if extensions.iter().any(|extension| {
                extension_name(&extension.extension_name) == ash::khr::portability_enumeration::NAME
            }) {
                extension_names.push(ash::khr::portability_enumeration::NAME.as_ptr());
                flags |= vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR;
            }
        }
        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names)
            .flags(flags);
        let instance = unsafe { entry.create_instance(&create_info, None) }
            .map_err(|error| unavailable(format!("create Vulkan 1.1 instance: {error:?}")))?;
        let context = Arc::new(VulkanContext {
            _entry: entry,
            instance,
        });
        let mut provider = Self {
            context,
            adapters: Vec::new(),
        };
        provider.adapters = provider.adapters()?;
        Ok(provider)
    }

    fn adapters(&self) -> Result<Vec<AdapterInfo>, BackendError> {
        let physical_devices = unsafe { self.context.instance.enumerate_physical_devices() }
            .map_err(|error| unavailable(format!("enumerate physical devices: {error:?}")))?;
        let mut adapters = Vec::new();
        for physical in physical_devices {
            let base = unsafe {
                self.context
                    .instance
                    .get_physical_device_properties(physical)
            };
            if vk::api_version_major(base.api_version) < 1
                || (vk::api_version_major(base.api_version) == 1
                    && vk::api_version_minor(base.api_version) < 1)
            {
                continue;
            }
            let device_extensions = unsafe {
                self.context
                    .instance
                    .enumerate_device_extension_properties(physical)
            }
            .map_err(|error| unavailable(format!("enumerate device extensions: {error:?}")))?;
            let has_extension = |name: &CStr| {
                device_extensions
                    .iter()
                    .any(|extension| extension_name(&extension.extension_name) == name)
            };

            let mut id = vk::PhysicalDeviceIDProperties::default();
            let mut maintenance = vk::PhysicalDeviceMaintenance3Properties::default();
            let mut properties = vk::PhysicalDeviceProperties2::default()
                .push_next(&mut id)
                .push_next(&mut maintenance);
            unsafe {
                self.context
                    .instance
                    .get_physical_device_properties2(physical, &mut properties)
            };

            let memory = unsafe {
                self.context
                    .instance
                    .get_physical_device_memory_properties(physical)
            };
            let mut usable_bytes = device_local_heap_bytes(&memory);
            if has_extension(ash::ext::memory_budget::NAME) {
                let mut budget = vk::PhysicalDeviceMemoryBudgetPropertiesEXT::default();
                let mut memory2 =
                    vk::PhysicalDeviceMemoryProperties2::default().push_next(&mut budget);
                unsafe {
                    self.context
                        .instance
                        .get_physical_device_memory_properties2(physical, &mut memory2)
                };
                let available = device_local_heap_budget(&memory, &budget);
                if available != 0 {
                    usable_bytes = available;
                }
            }
            let heap_limit = device_local_heap_bytes(&memory);
            let max_allocation = if maintenance.max_memory_allocation_size == 0 {
                heap_limit
            } else {
                maintenance.max_memory_allocation_size.min(heap_limit)
            };
            let uuid = if id.device_uuid.iter().any(|byte| *byte != 0) {
                hex_uuid(&id.device_uuid)
            } else {
                format!("{:04x}:{:04x}", base.vendor_id, base.device_id)
            };
            let name = unsafe { CStr::from_ptr(base.device_name.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            let queue_families = unsafe {
                self.context
                    .instance
                    .get_physical_device_queue_family_properties(physical)
            };
            for (queue_family, queue) in queue_families.iter().enumerate() {
                if queue.queue_count == 0 || !queue.queue_flags.contains(vk::QueueFlags::COMPUTE) {
                    continue;
                }
                let index = adapters.len();
                adapters.push(AdapterInfo {
                    descriptor: DeviceDescriptor {
                        id: DeviceId::parse(&format!("vulkan{index}"))
                            .expect("enumerated Vulkan id is valid"),
                        backend: BackendKind::Vulkan,
                        physical_key: format!("vulkan:{uuid}"),
                        name: format!("{name} (compute queue {queue_family})"),
                        usable_bytes,
                        max_allocation_bytes: max_allocation,
                        buffer_alignment: base.limits.min_storage_buffer_offset_alignment.max(4),
                        unified_memory: is_unified_memory(&memory),
                        capabilities: DeviceCapabilities {
                            components: BTreeSet::from([ComponentId::Llm]),
                            modes: BTreeSet::from([PlacementMode::Layer, PlacementMode::Row]),
                            layer_families: BTreeSet::from([LayerFamily::Qwen3]),
                            tensor_types: BTreeSet::from([
                                GGMLType::F32,
                                GGMLType::F16,
                                GGMLType::Q8_0,
                            ]),
                        },
                    },
                    physical,
                    queue_family: queue_family as u32,
                    memory,
                    non_coherent_atom_size: base.limits.non_coherent_atom_size.max(1),
                    max_storage_buffer_range: u64::from(base.limits.max_storage_buffer_range),
                    portability_subset: has_extension(ash::khr::portability_subset::NAME),
                });
            }
        }
        Ok(adapters)
    }
}

impl DeviceDiscovery for VulkanProvider {
    fn backend(&self) -> BackendKind {
        BackendKind::Vulkan
    }

    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, BackendError> {
        Ok(self
            .adapters
            .iter()
            .map(|adapter| adapter.descriptor.clone())
            .collect())
    }
}

impl DeviceProvider for VulkanProvider {
    fn open(
        &self,
        descriptor: &DeviceDescriptor,
        plan: &DevicePlan,
        catalog: Arc<TensorCatalog>,
    ) -> Result<Box<dyn DeviceSession>, BackendError> {
        if descriptor.backend != BackendKind::Vulkan || descriptor != &plan.descriptor {
            return Err(BackendError::InvalidHandle);
        }
        let expected = self
            .adapters
            .iter()
            .find(|adapter| adapter.descriptor.id == descriptor.id)
            .ok_or_else(|| BackendError::DeviceUnavailable {
                device: descriptor.id.clone(),
            })?;
        let adapter = select_adapter(self.adapters()?, descriptor, expected)?;
        VulkanSession::open(Arc::clone(&self.context), adapter, plan, catalog)
            .map(|session| Box::new(session) as Box<dyn DeviceSession>)
    }
}

fn select_adapter(
    adapters: Vec<AdapterInfo>,
    descriptor: &DeviceDescriptor,
    expected: &AdapterInfo,
) -> Result<AdapterInfo, BackendError> {
    match adapters
        .into_iter()
        .find(|adapter| adapter.descriptor.id == descriptor.id)
    {
        Some(adapter)
            if immutable_adapter_matches(&adapter, expected)
                && immutable_descriptor_matches(descriptor, &expected.descriptor) =>
        {
            Ok(adapter)
        }
        Some(_) => Err(BackendError::InvalidHandle),
        None => Err(BackendError::DeviceUnavailable {
            device: descriptor.id.clone(),
        }),
    }
}

fn immutable_adapter_matches(current: &AdapterInfo, expected: &AdapterInfo) -> bool {
    immutable_descriptor_matches(&current.descriptor, &expected.descriptor)
        && current.queue_family == expected.queue_family
        && current.non_coherent_atom_size == expected.non_coherent_atom_size
        && current.max_storage_buffer_range == expected.max_storage_buffer_range
        && current.portability_subset == expected.portability_subset
}

fn immutable_descriptor_matches(current: &DeviceDescriptor, expected: &DeviceDescriptor) -> bool {
    current.id == expected.id
        && current.backend == expected.backend
        && current.physical_key == expected.physical_key
        && current.name == expected.name
        && current.max_allocation_bytes == expected.max_allocation_bytes
        && current.buffer_alignment == expected.buffer_alignment
        && current.unified_memory == expected.unified_memory
        && current.capabilities == expected.capabilities
}

#[derive(Clone, Copy)]
struct BufferAllocation {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    allocation_size: u64,
    coherent: bool,
}

#[derive(Clone, Copy)]
struct Dispatch {
    set: vk::DescriptorSet,
    local_rows: u32,
    global_row_start: u32,
    output_row_start: u32,
    weight_byte_bias: u32,
}

unsafe fn bind_q8_dispatch(
    device: &ash::Device,
    set: vk::DescriptorSet,
    resident: vk::Buffer,
    arena: vk::Buffer,
    input: &SlotPlan,
    output: &SlotPlan,
    chunk: ChunkSpec,
) -> Dispatch {
    let infos = [
        vk::DescriptorBufferInfo::default()
            .buffer(resident)
            .offset(chunk.descriptor_offset)
            .range(chunk.descriptor_range),
        vk::DescriptorBufferInfo::default()
            .buffer(arena)
            .offset(input.arena_offset)
            .range(input.byte_len),
        vk::DescriptorBufferInfo::default()
            .buffer(arena)
            .offset(output.arena_offset)
            .range(output.byte_len),
    ];
    let writes = [0, 1, 2].map(|binding| {
        vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(binding)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(std::slice::from_ref(&infos[binding as usize]))
    });
    device.update_descriptor_sets(&writes, &[]);
    Dispatch {
        set,
        local_rows: chunk.local_rows,
        global_row_start: chunk.global_row_start,
        output_row_start: chunk.output_row_start,
        weight_byte_bias: chunk.weight_byte_bias,
    }
}

struct ProgramResource {
    plan: ProgramPlan,
    command: vk::CommandBuffer,
    fence: vk::Fence,
    dispatches: Vec<Dispatch>,
    n_in: u32,
    output_stride: u32,
    mode: u32,
    layer_set: Option<vk::DescriptorSet>,
    layer_ops: Vec<BoundLayerOp>,
}

#[derive(Clone, Copy)]
struct ResidentRange {
    offset: u64,
}

enum LayerOpSpec {
    RmsNorm {
        input: SlotId,
        weight: ResidentRange,
        output: SlotId,
        elements: u32,
        groups: u32,
        epsilon_bits: u32,
        weight_f16: bool,
    },
    Q8Matmul {
        input: SlotId,
        chunks: Vec<ChunkSpec>,
        output: SlotId,
        n_in: u32,
        rows: u32,
    },
    Rope {
        q: SlotId,
        k: SlotId,
        q_width: u32,
        k_width: u32,
        key_head_dim: u32,
        freq_base_bits: u32,
    },
    KvAppend {
        k: SlotId,
        v: SlotId,
        key_state: SlotId,
        value_state: SlotId,
        key_width: u32,
        value_width: u32,
    },
    Attention {
        q: SlotId,
        output: SlotId,
        head_count: u32,
        kv_head_count: u32,
        key_state: SlotId,
        value_state: SlotId,
        key_head_dim: u32,
        value_head_dim: u32,
        context_capacity: u32,
    },
    SiluMul {
        gate: SlotId,
        up: SlotId,
        elements: u32,
    },
    Add {
        left: SlotId,
        right: SlotId,
        output: SlotId,
        elements: u32,
    },
}

enum BoundLayerOp {
    RmsNorm {
        input: SlotId,
        weight: ResidentRange,
        output: SlotId,
        elements: u32,
        groups: u32,
        epsilon_bits: u32,
        weight_f16: bool,
    },
    Q8Matmul {
        input: SlotId,
        dispatches: Vec<Dispatch>,
        output: SlotId,
        n_in: u32,
        rows: u32,
    },
    Rope {
        q: SlotId,
        k: SlotId,
        q_width: u32,
        k_width: u32,
        key_head_dim: u32,
        freq_base_bits: u32,
    },
    KvAppend {
        k: SlotId,
        v: SlotId,
        key_state: SlotId,
        value_state: SlotId,
        key_width: u32,
        value_width: u32,
    },
    Attention {
        q: SlotId,
        output: SlotId,
        head_count: u32,
        kv_head_count: u32,
        key_state: SlotId,
        value_state: SlotId,
        key_head_dim: u32,
        value_head_dim: u32,
        context_capacity: u32,
    },
    SiluMul {
        gate: SlotId,
        up: SlotId,
        elements: u32,
    },
    Add {
        left: SlotId,
        right: SlotId,
        output: SlotId,
        elements: u32,
    },
}

struct Pending {
    id: FenceId,
    program: ProgramId,
}

#[derive(Default)]
struct SubmissionTracker {
    pending: VecDeque<Pending>,
    poisoned: bool,
}

impl SubmissionTracker {
    fn require_idle(&self) -> Result<(), &'static str> {
        if self.poisoned {
            Err("Vulkan session is poisoned")
        } else if !self.pending.is_empty() {
            Err("Vulkan work is pending")
        } else {
            Ok(())
        }
    }

    fn finish_submit(
        &mut self,
        result: Result<(), vk::Result>,
        pending: Pending,
    ) -> Result<(), vk::Result> {
        match result {
            Ok(()) => {
                self.pending.push_back(pending);
                Ok(())
            }
            Err(error) => {
                self.poisoned = true;
                self.pending.clear();
                Err(error)
            }
        }
    }
}

fn drain_pending(
    tracker: &mut SubmissionTracker,
    target: FenceId,
    mut finish: impl FnMut(Pending) -> Result<(), BackendError>,
) -> Result<(), BackendError> {
    if !tracker.pending.iter().any(|pending| pending.id == target) {
        return Err(BackendError::InvalidHandle);
    }
    loop {
        let pending = tracker
            .pending
            .pop_front()
            .expect("validated pending fence");
        let id = pending.id;
        if let Err(error) = finish(pending) {
            tracker.poisoned = true;
            tracker.pending.clear();
            return Err(error);
        }
        if id == target {
            return Ok(());
        }
    }
}

struct VulkanSession {
    _context: Arc<VulkanContext>,
    descriptor: DeviceDescriptor,
    device: ash::Device,
    queue: vk::Queue,
    resident: BufferAllocation,
    arena: BufferAllocation,
    staging: BufferAllocation,
    staging_ptr: usize,
    staging_size: u64,
    non_coherent_atom_size: u64,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    layer_pipeline: vk::Pipeline,
    command_pool: vk::CommandPool,
    slots: BTreeMap<SlotId, SlotPlan>,
    programs: BTreeMap<ProgramId, ProgramResource>,
    next_fence: u64,
    submission: SubmissionTracker,
    stats: SessionStats,
}

impl VulkanSession {
    fn open(
        context: Arc<VulkanContext>,
        adapter: AdapterInfo,
        plan: &DevicePlan,
        catalog: Arc<TensorCatalog>,
    ) -> Result<Self, BackendError> {
        let validated = validate_plan(plan, &catalog, &adapter)?;
        let priorities = [1.0];
        let queue_info = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(adapter.queue_family)
            .queue_priorities(&priorities)];
        let mut extensions = Vec::new();
        if adapter.portability_subset {
            extensions.push(ash::khr::portability_subset::NAME.as_ptr());
        }
        let device_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_info)
            .enabled_extension_names(&extensions);
        let device = unsafe {
            context
                .instance
                .create_device(adapter.physical, &device_info, None)
        }
        .map_err(|error| allocation(&adapter.descriptor, format!("create device: {error:?}")))?;
        let device_owner = DeviceOwner(Some(device));
        let device = device_owner.device();
        let queue = unsafe { device.get_device_queue(adapter.queue_family, 0) };
        let mut handles = HandleOwner::new(|handle| unsafe { release_open_handle(device, handle) });

        let resident_size = validated.resident_size;
        let arena_size = validated.arena_size;
        let staging_size = validated.staging_size;

        let resident = create_buffer(
            &device,
            &adapter,
            resident_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        handles.push(OpenHandle::Buffer(resident));
        let arena = create_buffer(
            &device,
            &adapter,
            arena_size,
            vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        handles.push(OpenHandle::Buffer(arena));
        let staging = create_buffer(
            &device,
            &adapter,
            staging_size,
            vk::BufferUsageFlags::TRANSFER_SRC | vk::BufferUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::HOST_VISIBLE,
        )?;
        handles.push(OpenHandle::Buffer(staging));
        let staging_ptr = unsafe {
            device.map_memory(
                staging.memory,
                0,
                staging.allocation_size,
                vk::MemoryMapFlags::empty(),
            )
        }
        .map_err(|error| allocation(&adapter.descriptor, format!("map staging: {error:?}")))?
            as usize;
        handles.push(OpenHandle::Mapped(staging.memory));

        let command_pool = unsafe {
            device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(adapter.queue_family)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )
        }
        .map_err(|error| allocation(&adapter.descriptor, format!("command pool: {error:?}")))?;
        handles.push(OpenHandle::CommandPool(command_pool));

        upload_resident(
            &device,
            queue,
            command_pool,
            &resident,
            &staging,
            staging_ptr,
            adapter.non_coherent_atom_size,
            &adapter.descriptor,
            plan,
            &catalog,
        )?;

        let bindings = [0, 1, 2].map(|binding| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(binding)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
        });
        let descriptor_set_layout = unsafe {
            device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )
        }
        .map_err(|error| {
            pipeline_error(&adapter.descriptor, format!("descriptor layout: {error:?}"))
        })?;
        handles.push(OpenHandle::DescriptorSetLayout(descriptor_set_layout));
        let push_range = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .offset(0)
            .size(64)];
        let set_layouts = [descriptor_set_layout];
        let pipeline_layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&set_layouts)
                    .push_constant_ranges(&push_range),
                None,
            )
        }
        .map_err(|error| {
            pipeline_error(&adapter.descriptor, format!("pipeline layout: {error:?}"))
        })?;
        handles.push(OpenHandle::PipelineLayout(pipeline_layout));
        let shader_bytes = include_bytes!("vulkan/shaders/q8_0_rows.spv");
        let shader_code = ash::util::read_spv(&mut Cursor::new(shader_bytes.as_slice()))
            .map_err(|error| pipeline_error(&adapter.descriptor, error.to_string()))?;
        let shader = unsafe {
            device.create_shader_module(
                &vk::ShaderModuleCreateInfo::default().code(&shader_code),
                None,
            )
        }
        .map_err(|error| {
            pipeline_error(&adapter.descriptor, format!("shader module: {error:?}"))
        })?;
        handles.push(OpenHandle::Shader(shader));
        let main = CStr::from_bytes_with_nul(b"main\0").unwrap();
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(shader)
            .name(main);
        let pipeline_info = [vk::ComputePipelineCreateInfo::default()
            .stage(stage)
            .layout(pipeline_layout)];
        let pipeline = take_created_handle(
            &mut handles,
            unsafe {
                device.create_compute_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
            },
            OpenHandle::Pipeline,
        )
        .map_err(|error| {
            pipeline_error(&adapter.descriptor, format!("compute pipeline: {error:?}"))
        })?;
        handles.release_last();
        handles.push(OpenHandle::Pipeline(pipeline));
        let layer_bytes = include_bytes!("vulkan/shaders/layer_ops.spv");
        let layer_code = ash::util::read_spv(&mut Cursor::new(layer_bytes.as_slice()))
            .map_err(|error| pipeline_error(&adapter.descriptor, error.to_string()))?;
        let layer_shader = unsafe {
            device.create_shader_module(
                &vk::ShaderModuleCreateInfo::default().code(&layer_code),
                None,
            )
        }
        .map_err(|error| {
            pipeline_error(
                &adapter.descriptor,
                format!("layer shader module: {error:?}"),
            )
        })?;
        handles.push(OpenHandle::Shader(layer_shader));
        let layer_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(layer_shader)
            .name(main);
        let layer_info = [vk::ComputePipelineCreateInfo::default()
            .stage(layer_stage)
            .layout(pipeline_layout)];
        let layer_pipeline = take_created_handle(
            &mut handles,
            unsafe {
                device.create_compute_pipelines(vk::PipelineCache::null(), &layer_info, None)
            },
            OpenHandle::Pipeline,
        )
        .map_err(|error| {
            pipeline_error(
                &adapter.descriptor,
                format!("layer compute pipeline: {error:?}"),
            )
        })?;
        handles.release_last();
        handles.push(OpenHandle::Pipeline(layer_pipeline));

        let slots = validated.slots;
        let specs = validated.specs;
        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(validated.storage_descriptor_count)];
        let descriptor_pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(validated.descriptor_set_count)
                    .pool_sizes(&pool_sizes),
                None,
            )
        }
        .map_err(|error| {
            pipeline_error(&adapter.descriptor, format!("descriptor pool: {error:?}"))
        })?;
        handles.push(OpenHandle::DescriptorPool(descriptor_pool));
        let layouts = vec![descriptor_set_layout; validated.descriptor_set_count as usize];
        let sets = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(descriptor_pool)
                    .set_layouts(&layouts),
            )
        }
        .map_err(|error| {
            pipeline_error(&adapter.descriptor, format!("descriptor sets: {error:?}"))
        })?;
        let commands = unsafe {
            device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(validated.command_buffer_count),
            )
        }
        .map_err(|error| {
            pipeline_error(&adapter.descriptor, format!("command buffers: {error:?}"))
        })?;

        let mut set_index = 0;
        let mut programs = BTreeMap::new();
        for (program_index, spec) in specs.into_iter().enumerate() {
            let input = &slots[&spec.plan.input];
            let output = &slots[&spec.plan.output];
            let mut dispatches = Vec::with_capacity(spec.chunks.len());
            for chunk in spec.chunks {
                let set = sets[set_index];
                set_index += 1;
                dispatches.push(unsafe {
                    bind_q8_dispatch(
                        &device,
                        set,
                        resident.buffer,
                        arena.buffer,
                        input,
                        output,
                        chunk,
                    )
                });
            }
            let layer_set = if spec.layer_ops.is_empty() {
                None
            } else {
                let set = sets[set_index];
                set_index += 1;
                let infos = [
                    vk::DescriptorBufferInfo::default()
                        .buffer(resident.buffer)
                        .range(resident_size),
                    vk::DescriptorBufferInfo::default()
                        .buffer(arena.buffer)
                        .range(arena_size),
                    vk::DescriptorBufferInfo::default()
                        .buffer(arena.buffer)
                        .range(arena_size),
                ];
                let writes = [0, 1, 2].map(|binding| {
                    vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(binding)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(std::slice::from_ref(&infos[binding as usize]))
                });
                unsafe { device.update_descriptor_sets(&writes, &[]) };
                Some(set)
            };
            let mut layer_ops = Vec::with_capacity(spec.layer_ops.len());
            for op in spec.layer_ops {
                layer_ops.push(match op {
                    LayerOpSpec::Q8Matmul {
                        input,
                        chunks,
                        output,
                        n_in,
                        rows,
                    } => {
                        let mut q8_dispatches = Vec::with_capacity(chunks.len());
                        for chunk in chunks {
                            let set = sets[set_index];
                            set_index += 1;
                            q8_dispatches.push(unsafe {
                                bind_q8_dispatch(
                                    &device,
                                    set,
                                    resident.buffer,
                                    arena.buffer,
                                    &slots[&input],
                                    &slots[&output],
                                    chunk,
                                )
                            });
                        }
                        BoundLayerOp::Q8Matmul {
                            input,
                            dispatches: q8_dispatches,
                            output,
                            n_in,
                            rows,
                        }
                    }
                    LayerOpSpec::RmsNorm {
                        input,
                        weight,
                        output,
                        elements,
                        groups,
                        epsilon_bits,
                        weight_f16,
                    } => BoundLayerOp::RmsNorm {
                        input,
                        weight,
                        output,
                        elements,
                        groups,
                        epsilon_bits,
                        weight_f16,
                    },
                    LayerOpSpec::Rope {
                        q,
                        k,
                        q_width,
                        k_width,
                        key_head_dim,
                        freq_base_bits,
                    } => BoundLayerOp::Rope {
                        q,
                        k,
                        q_width,
                        k_width,
                        key_head_dim,
                        freq_base_bits,
                    },
                    LayerOpSpec::KvAppend {
                        k,
                        v,
                        key_state,
                        value_state,
                        key_width,
                        value_width,
                    } => BoundLayerOp::KvAppend {
                        k,
                        v,
                        key_state,
                        value_state,
                        key_width,
                        value_width,
                    },
                    LayerOpSpec::Attention {
                        q,
                        output,
                        head_count,
                        kv_head_count,
                        key_state,
                        value_state,
                        key_head_dim,
                        value_head_dim,
                        context_capacity,
                    } => BoundLayerOp::Attention {
                        q,
                        output,
                        head_count,
                        kv_head_count,
                        key_state,
                        value_state,
                        key_head_dim,
                        value_head_dim,
                        context_capacity,
                    },
                    LayerOpSpec::SiluMul { gate, up, elements } => {
                        BoundLayerOp::SiluMul { gate, up, elements }
                    }
                    LayerOpSpec::Add {
                        left,
                        right,
                        output,
                        elements,
                    } => BoundLayerOp::Add {
                        left,
                        right,
                        output,
                        elements,
                    },
                });
            }
            let fence = unsafe {
                device.create_fence(
                    &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                    None,
                )
            }
            .map_err(|error| pipeline_error(&adapter.descriptor, format!("fence: {error:?}")))?;
            handles.push(OpenHandle::Fence(fence));
            let id = spec.plan.id;
            if programs
                .insert(
                    id,
                    ProgramResource {
                        plan: spec.plan,
                        command: commands[program_index],
                        fence,
                        dispatches,
                        n_in: spec.n_in,
                        output_stride: spec.output_stride,
                        mode: spec.mode,
                        layer_set,
                        layer_ops,
                    },
                )
                .is_some()
            {
                return Err(BackendError::InvalidHandle);
            }
        }

        handles.disarm();
        let device = device_owner.disarm();
        Ok(Self {
            _context: context,
            descriptor: adapter.descriptor,
            device,
            queue,
            resident,
            arena,
            staging,
            staging_ptr,
            staging_size,
            non_coherent_atom_size: adapter.non_coherent_atom_size,
            descriptor_set_layout,
            descriptor_pool,
            pipeline_layout,
            pipeline,
            layer_pipeline,
            command_pool,
            slots,
            programs,
            next_fence: 1,
            submission: SubmissionTracker::default(),
            stats: SessionStats {
                resident_bytes: plan.memory.resident_bytes,
                resident_allocations: 1,
                weight_uploads: plan.tensors.len() as u64,
                weight_upload_bytes: plan
                    .tensors
                    .iter()
                    .map(|tensor| tensor.source_bytes.end - tensor.source_bytes.start)
                    .sum(),
                ..SessionStats::default()
            },
        })
    }

    fn require_idle(&self) -> Result<(), BackendError> {
        self.submission
            .require_idle()
            .map_err(|message| BackendError::Submission {
                device: self.descriptor.id.clone(),
                message: message.into(),
            })
    }

    fn require_submit(&self, program: ProgramId) -> Result<(), BackendError> {
        if self.submission.poisoned {
            Err(submission(&self.descriptor, "Vulkan session is poisoned"))
        } else if self
            .submission
            .pending
            .iter()
            .any(|pending| pending.program == program)
        {
            Err(submission(
                &self.descriptor,
                "Vulkan program is already pending",
            ))
        } else {
            Ok(())
        }
    }

    unsafe fn compute_barrier(&self, command: vk::CommandBuffer) {
        let barrier = vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE);
        self.device.cmd_pipeline_barrier(
            command,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[barrier],
            &[],
            &[],
        );
    }

    unsafe fn record_q8(
        &self,
        command: vk::CommandBuffer,
        dispatches: &[Dispatch],
        batch: u32,
        n_in: u32,
        output_stride: u32,
        mode: u32,
    ) -> Result<(), BackendError> {
        self.device
            .cmd_bind_pipeline(command, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        for (index, dispatch) in dispatches.iter().enumerate() {
            if index != 0 {
                self.compute_barrier(command);
            }
            self.device.cmd_bind_descriptor_sets(
                command,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline_layout,
                0,
                &[dispatch.set],
                &[],
            );
            let push = [
                batch,
                n_in,
                dispatch.local_rows,
                dispatch.global_row_start,
                output_stride,
                mode,
                dispatch.weight_byte_bias,
                dispatch.output_row_start,
            ];
            self.device.cmd_push_constants(
                command,
                self.pipeline_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                std::slice::from_raw_parts(push.as_ptr().cast(), 32),
            );
            let work = if mode == 0 {
                batch.checked_mul(dispatch.local_rows)
            } else {
                batch.checked_mul(n_in)
            }
            .ok_or(BackendError::InvalidHandle)?;
            self.device
                .cmd_dispatch(command, work.div_ceil(LOCAL_SIZE), 1, 1);
        }
        Ok(())
    }

    unsafe fn record_layer_op(
        &self,
        command: vk::CommandBuffer,
        set: vk::DescriptorSet,
        op: &BoundLayerOp,
        params: &RunParams<'_>,
    ) -> Result<(), BackendError> {
        let batch = params.token_count;
        let word = |slot: SlotId| {
            u32::try_from(self.slots[&slot].arena_offset / 4)
                .map_err(|_| BackendError::InvalidHandle)
        };
        let fits_f32 = |slot: SlotId, width: u32| {
            u64::from(batch)
                .checked_mul(u64::from(width))
                .and_then(|values| values.checked_mul(4))
                .is_some_and(|bytes| bytes <= self.slots[&slot].byte_len)
        };
        if let BoundLayerOp::Q8Matmul {
            input,
            dispatches,
            output,
            n_in,
            rows,
        } = op
        {
            if !fits_f32(*input, *n_in) || !fits_f32(*output, *rows) {
                return Err(BackendError::InvalidHandle);
            }
            return self.record_q8(command, dispatches, batch, *n_in, *rows, 0);
        }
        let (push, work) = match op {
            BoundLayerOp::RmsNorm {
                input,
                weight,
                output,
                elements,
                groups,
                epsilon_bits,
                weight_f16,
            } => {
                let width = elements
                    .checked_mul(*groups)
                    .ok_or(BackendError::InvalidHandle)?;
                if !fits_f32(*input, width) || !fits_f32(*output, width) {
                    return Err(BackendError::InvalidHandle);
                }
                (
                    [
                        0,
                        batch,
                        word(*input)?,
                        u32::try_from(weight.offset).map_err(|_| BackendError::InvalidHandle)?,
                        word(*output)?,
                        *elements,
                        *groups,
                        *epsilon_bits,
                        u32::from(*weight_f16),
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    ],
                    batch.checked_mul(*groups),
                )
            }
            BoundLayerOp::Rope {
                q,
                k,
                q_width,
                k_width,
                key_head_dim,
                freq_base_bits,
            } => {
                if !fits_f32(*q, *q_width) || !fits_f32(*k, *k_width) {
                    return Err(BackendError::InvalidHandle);
                }
                (
                    [
                        1,
                        batch,
                        word(*q)?,
                        word(*k)?,
                        *q_width,
                        *k_width,
                        *key_head_dim,
                        params.position_start,
                        *freq_base_bits,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    ],
                    batch.checked_mul((q_width + k_width) / 2),
                )
            }
            BoundLayerOp::KvAppend {
                k,
                v,
                key_state,
                value_state,
                key_width,
                value_width,
            } => {
                let end = params
                    .position_start
                    .checked_add(batch)
                    .ok_or(BackendError::InvalidHandle)?;
                let state_fits = |slot: SlotId, width: u32| {
                    u64::from(end)
                        .checked_mul(u64::from(width))
                        .and_then(|v| v.checked_mul(2))
                        .is_some_and(|bytes| bytes <= self.slots[&slot].byte_len)
                };
                if !fits_f32(*k, *key_width)
                    || !fits_f32(*v, *value_width)
                    || !state_fits(*key_state, *key_width)
                    || !state_fits(*value_state, *value_width)
                {
                    return Err(BackendError::InvalidHandle);
                }
                (
                    [
                        2,
                        batch,
                        word(*k)?,
                        word(*v)?,
                        word(*key_state)?,
                        word(*value_state)?,
                        *key_width,
                        *value_width,
                        params.position_start,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    ],
                    Some(batch),
                )
            }
            BoundLayerOp::Attention {
                q,
                output,
                head_count,
                kv_head_count,
                key_state,
                value_state,
                key_head_dim,
                value_head_dim,
                context_capacity,
            } => {
                if params
                    .position_start
                    .checked_add(batch)
                    .is_none_or(|end| end > *context_capacity)
                {
                    return Err(BackendError::InvalidHandle);
                }
                let q_width = head_count
                    .checked_mul(*key_head_dim)
                    .ok_or(BackendError::InvalidHandle)?;
                let output_width = head_count
                    .checked_mul(*value_head_dim)
                    .ok_or(BackendError::InvalidHandle)?;
                if !fits_f32(*q, q_width) || !fits_f32(*output, output_width) {
                    return Err(BackendError::InvalidHandle);
                }
                (
                    [
                        3,
                        batch,
                        word(*q)?,
                        word(*key_state)?,
                        word(*value_state)?,
                        word(*output)?,
                        *head_count,
                        *kv_head_count,
                        *key_head_dim,
                        *value_head_dim,
                        params.position_start,
                        0,
                        0,
                        0,
                        0,
                        0,
                    ],
                    batch
                        .checked_mul(*head_count)
                        .and_then(|v| v.checked_mul(*value_head_dim)),
                )
            }
            BoundLayerOp::SiluMul { gate, up, elements } => {
                if !fits_f32(*gate, *elements) || !fits_f32(*up, *elements) {
                    return Err(BackendError::InvalidHandle);
                }
                (
                    [
                        4,
                        batch,
                        word(*gate)?,
                        word(*up)?,
                        *elements,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    ],
                    batch.checked_mul(*elements),
                )
            }
            BoundLayerOp::Add {
                left,
                right,
                output,
                elements,
            } => {
                if !fits_f32(*left, *elements)
                    || !fits_f32(*right, *elements)
                    || !fits_f32(*output, *elements)
                {
                    return Err(BackendError::InvalidHandle);
                }
                (
                    [
                        5,
                        batch,
                        word(*left)?,
                        word(*right)?,
                        word(*output)?,
                        *elements,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    ],
                    batch.checked_mul(*elements),
                )
            }
            BoundLayerOp::Q8Matmul { .. } => unreachable!(),
        };
        let work = work.ok_or(BackendError::InvalidHandle)?;
        self.device
            .cmd_bind_pipeline(command, vk::PipelineBindPoint::COMPUTE, self.layer_pipeline);
        self.device.cmd_bind_descriptor_sets(
            command,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[set],
            &[],
        );
        self.device.cmd_push_constants(
            command,
            self.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            std::slice::from_raw_parts(push.as_ptr().cast(), 64),
        );
        self.device
            .cmd_dispatch(command, work.div_ceil(LOCAL_SIZE), 1, 1);
        Ok(())
    }

    fn flush_staging(&self, offset: u64, size: u64) -> Result<(), BackendError> {
        flush_memory(
            &self.device,
            &self.staging,
            self.non_coherent_atom_size,
            offset,
            size,
            &self.descriptor,
        )
    }

    fn invalidate_staging(&self, offset: u64, size: u64) -> Result<(), BackendError> {
        if self.staging.coherent || size == 0 {
            return Ok(());
        }
        let range = mapped_range(
            self.staging.memory,
            self.staging.allocation_size,
            self.non_coherent_atom_size,
            offset,
            size,
        );
        unsafe { self.device.invalidate_mapped_memory_ranges(&[range]) }.map_err(|error| {
            BackendError::Submission {
                device: self.descriptor.id.clone(),
                message: format!("invalidate staging: {error:?}"),
            }
        })
    }
}

impl DeviceSession for VulkanSession {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.descriptor
    }

    fn write_f32(&mut self, slot: SlotId, values: &[f32]) -> Result<(), BackendError> {
        self.require_idle()?;
        let slot = self
            .slots
            .get(&slot)
            .filter(|slot| slot.storage == SlotStorage::F32)
            .ok_or(BackendError::InvalidHandle)?;
        let byte_len = values
            .len()
            .checked_mul(size_of::<f32>())
            .ok_or(BackendError::InvalidHandle)? as u64;
        if byte_len > slot.byte_len {
            return Err(BackendError::InvalidHandle);
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                values.as_ptr() as *const u8,
                (self.staging_ptr + slot.arena_offset as usize) as *mut u8,
                byte_len as usize,
            )
        };
        self.flush_staging(slot.arena_offset, byte_len)?;
        self.stats.activation_h2d_bytes += byte_len;
        Ok(())
    }

    fn submit(
        &mut self,
        program: ProgramId,
        params: &RunParams<'_>,
    ) -> Result<FenceId, BackendError> {
        self.require_submit(program)?;
        let resource = self
            .programs
            .get(&program)
            .ok_or(BackendError::InvalidHandle)?;
        if matches!(
            resource.plan.kind,
            ProgramKind::LayerSegment { .. } | ProgramKind::FinalNormQ8Logits { .. }
        ) {
            if params.token_count == 0 {
                return Err(BackendError::InvalidHandle);
            }
            let fence = resource.fence;
            let command = resource.command;
            if !unsafe { self.device.get_fence_status(fence) }
                .map_err(|error| submission(&self.descriptor, format!("fence status: {error:?}")))?
            {
                return Err(submission(
                    &self.descriptor,
                    "program fence is not complete",
                ));
            }
            let record_result = (|| -> Result<(), BackendError> {
                unsafe {
                    self.device
                        .reset_command_buffer(command, vk::CommandBufferResetFlags::empty())
                        .map_err(|error| {
                            submission(&self.descriptor, format!("reset command: {error:?}"))
                        })?;
                    self.device
                        .begin_command_buffer(
                            command,
                            &vk::CommandBufferBeginInfo::default()
                                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                        )
                        .map_err(|error| {
                            submission(&self.descriptor, format!("begin command: {error:?}"))
                        })?;
                    let set = resource.layer_set.ok_or(BackendError::InvalidHandle)?;
                    for (index, op) in resource.layer_ops.iter().enumerate() {
                        if index != 0 {
                            self.compute_barrier(command);
                        }
                        self.record_layer_op(command, set, op, params)?;
                    }
                    if matches!(resource.plan.kind, ProgramKind::FinalNormQ8Logits { .. }) {
                        let output = &self.slots[&resource.plan.output];
                        let barrier = vk::BufferMemoryBarrier::default()
                            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                            .buffer(self.arena.buffer)
                            .offset(output.arena_offset)
                            .size(output.byte_len);
                        self.device.cmd_pipeline_barrier(
                            command,
                            vk::PipelineStageFlags::COMPUTE_SHADER,
                            vk::PipelineStageFlags::TRANSFER,
                            vk::DependencyFlags::empty(),
                            &[],
                            &[barrier],
                            &[],
                        );
                        self.device.cmd_copy_buffer(
                            command,
                            self.arena.buffer,
                            self.staging.buffer,
                            &[vk::BufferCopy::default()
                                .src_offset(output.arena_offset)
                                .dst_offset(output.arena_offset)
                                .size(output.byte_len)],
                        );
                    }
                    self.device.end_command_buffer(command).map_err(|error| {
                        submission(&self.descriptor, format!("end command: {error:?}"))
                    })?;
                    self.device.reset_fences(&[fence]).map_err(|error| {
                        submission(&self.descriptor, format!("reset fence: {error:?}"))
                    })?;
                    self.device
                        .queue_submit(
                            self.queue,
                            &[vk::SubmitInfo::default().command_buffers(&[command])],
                            fence,
                        )
                        .map_err(|error| {
                            submission(&self.descriptor, format!("queue submit: {error:?}"))
                        })?;
                }
                Ok(())
            })();
            let id = FenceId(self.next_fence);
            self.submission
                .finish_submit(
                    record_result.map_err(|_| vk::Result::ERROR_UNKNOWN),
                    Pending { id, program },
                )
                .map_err(|error| {
                    submission(
                        &self.descriptor,
                        format!("record or submit failed; session poisoned: {error:?}"),
                    )
                })?;
            self.next_fence += 1;
            self.stats.submissions += 1;
            return Ok(id);
        }
        let input = &self.slots[&resource.plan.input];
        let output = &self.slots[&resource.plan.output];
        let (input_bytes, output_bytes) = match &resource.plan.kind {
            ProgramKind::Q8Rows { batch_capacity, .. } => {
                if params.token_count == 0 || params.token_count > *batch_capacity {
                    return Err(BackendError::InvalidHandle);
                }
                let input_bytes = u64::from(params.token_count)
                    .checked_mul(u64::from(resource.n_in))
                    .and_then(|values| values.checked_mul(size_of::<f32>() as u64))
                    .ok_or(BackendError::InvalidHandle)?;
                let output_bytes = u64::from(params.token_count)
                    .checked_mul(u64::from(resource.output_stride))
                    .and_then(|values| values.checked_mul(size_of::<f32>() as u64))
                    .ok_or(BackendError::InvalidHandle)?;
                (input_bytes, output_bytes)
            }
            ProgramKind::EmbeddingRows { row_count, .. } => {
                if params.token_count == 0
                    || params.token_ids.len() != params.token_count as usize
                    || params.token_ids.iter().any(|token| token >= row_count)
                {
                    return Err(BackendError::InvalidHandle);
                }
                let input_bytes = u64::from(params.token_count)
                    .checked_mul(size_of::<u32>() as u64)
                    .ok_or(BackendError::InvalidHandle)?;
                let output_bytes = u64::from(params.token_count)
                    .checked_mul(u64::from(resource.output_stride))
                    .and_then(|values| values.checked_mul(size_of::<f32>() as u64))
                    .ok_or(BackendError::InvalidHandle)?;
                (input_bytes, output_bytes)
            }
            _ => return Err(BackendError::InvalidHandle),
        };
        if input_bytes > input.byte_len || output_bytes > output.byte_len {
            return Err(BackendError::InvalidHandle);
        }
        if resource.mode == 1 {
            let bytes = usize::try_from(input_bytes).map_err(|_| BackendError::InvalidHandle)?;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    params.token_ids.as_ptr() as *const u8,
                    (self.staging_ptr + input.arena_offset as usize) as *mut u8,
                    bytes,
                )
            };
            self.flush_staging(input.arena_offset, bytes as u64)?;
            self.stats.activation_h2d_bytes += bytes as u64;
        }
        let fence = resource.fence;
        let command = resource.command;
        let dispatches = resource.dispatches.clone();
        let n_in = resource.n_in;
        let output_stride = resource.output_stride;
        let mode = resource.mode;
        let input_offset = input.arena_offset;
        let output_offset = output.arena_offset;

        if !unsafe { self.device.get_fence_status(fence) }
            .map_err(|error| submission(&self.descriptor, format!("fence status: {error:?}")))?
        {
            return Err(submission(
                &self.descriptor,
                "program fence is not complete",
            ));
        }
        let record_result = (|| -> Result<(), vk::Result> {
            unsafe {
                self.device
                    .reset_command_buffer(command, vk::CommandBufferResetFlags::empty())?;
                self.device.begin_command_buffer(
                    command,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )?;
                self.device.cmd_copy_buffer(
                    command,
                    self.staging.buffer,
                    self.arena.buffer,
                    &[vk::BufferCopy::default()
                        .src_offset(input_offset)
                        .dst_offset(input_offset)
                        .size(input_bytes)],
                );
                let input_barrier = vk::BufferMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .buffer(self.arena.buffer)
                    .offset(input_offset)
                    .size(input_bytes);
                self.device.cmd_pipeline_barrier(
                    command,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[input_barrier],
                    &[],
                );
                self.device.cmd_bind_pipeline(
                    command,
                    vk::PipelineBindPoint::COMPUTE,
                    self.pipeline,
                );
                for (index, dispatch) in dispatches.iter().enumerate() {
                    if index != 0 {
                        let compute_barrier = vk::MemoryBarrier::default()
                            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                            .dst_access_mask(
                                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
                            );
                        self.device.cmd_pipeline_barrier(
                            command,
                            vk::PipelineStageFlags::COMPUTE_SHADER,
                            vk::PipelineStageFlags::COMPUTE_SHADER,
                            vk::DependencyFlags::empty(),
                            &[compute_barrier],
                            &[],
                            &[],
                        );
                    }
                    self.device.cmd_bind_descriptor_sets(
                        command,
                        vk::PipelineBindPoint::COMPUTE,
                        self.pipeline_layout,
                        0,
                        &[dispatch.set],
                        &[],
                    );
                    let push = [
                        params.token_count,
                        n_in,
                        dispatch.local_rows,
                        dispatch.global_row_start,
                        output_stride,
                        mode,
                        dispatch.weight_byte_bias,
                        dispatch.output_row_start,
                    ];
                    let bytes = std::slice::from_raw_parts(push.as_ptr() as *const u8, 32);
                    self.device.cmd_push_constants(
                        command,
                        self.pipeline_layout,
                        vk::ShaderStageFlags::COMPUTE,
                        0,
                        bytes,
                    );
                    let work = if mode == 0 {
                        params.token_count * dispatch.local_rows
                    } else {
                        params.token_count * n_in
                    };
                    self.device
                        .cmd_dispatch(command, work.div_ceil(LOCAL_SIZE), 1, 1);
                }
                let output_barrier = vk::BufferMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .buffer(self.arena.buffer)
                    .offset(output_offset)
                    .size(output_bytes);
                self.device.cmd_pipeline_barrier(
                    command,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[output_barrier],
                    &[],
                );
                self.device.cmd_copy_buffer(
                    command,
                    self.arena.buffer,
                    self.staging.buffer,
                    &[vk::BufferCopy::default()
                        .src_offset(output_offset)
                        .dst_offset(output_offset)
                        .size(output_bytes)],
                );
                self.device.end_command_buffer(command)?;
                self.device.reset_fences(&[fence])?;
                let commands = [command];
                let submit = [vk::SubmitInfo::default().command_buffers(&commands)];
                self.device.queue_submit(self.queue, &submit, fence)
            }
        })();
        let id = FenceId(self.next_fence);
        if let Err(error) = self
            .submission
            .finish_submit(record_result, Pending { id, program })
        {
            return Err(submission(
                &self.descriptor,
                format!("record or submit failed; session poisoned: {error:?}"),
            ));
        }
        self.next_fence += 1;
        self.stats.submissions += 1;
        Ok(id)
    }

    fn wait(&mut self, fence: FenceId) -> Result<(), BackendError> {
        self.stats.host_waits += 1;
        drain_pending(&mut self.submission, fence, |pending| {
            let vk_fence = self.programs[&pending.program].fence;
            unsafe { self.device.wait_for_fences(&[vk_fence], true, u64::MAX) }
                .map_err(|error| submission(&self.descriptor, format!("wait fence: {error:?}")))
        })
    }

    fn read_f32(&mut self, slot: SlotId, values: &mut [f32]) -> Result<(), BackendError> {
        self.require_idle()?;
        let slot = self
            .slots
            .get(&slot)
            .filter(|slot| slot.storage == SlotStorage::F32)
            .ok_or(BackendError::InvalidHandle)?;
        let byte_len = values
            .len()
            .checked_mul(size_of::<f32>())
            .ok_or(BackendError::InvalidHandle)? as u64;
        if byte_len > slot.byte_len {
            return Err(BackendError::InvalidHandle);
        }
        self.invalidate_staging(slot.arena_offset, byte_len)?;
        unsafe {
            std::ptr::copy_nonoverlapping(
                (self.staging_ptr + slot.arena_offset as usize) as *const u8,
                values.as_mut_ptr() as *mut u8,
                byte_len as usize,
            )
        };
        self.stats.activation_d2h_bytes += byte_len;
        Ok(())
    }

    fn reset_state(&mut self) -> Result<(), BackendError> {
        self.require_idle()?;
        unsafe {
            std::ptr::write_bytes(self.staging_ptr as *mut u8, 0, self.staging_size as usize)
        };
        self.flush_staging(0, self.staging_size)
    }

    fn stats(&self) -> SessionStats {
        self.stats.clone()
    }

    fn lifecycle_probe(&self) -> LifecycleProbe {
        LifecycleProbe::default()
    }
}

impl Drop for VulkanSession {
    fn drop(&mut self) {
        while let Some(pending) = self.submission.pending.pop_front() {
            let fence = self.programs[&pending.program].fence;
            let _ = unsafe { self.device.wait_for_fences(&[fence], true, u64::MAX) };
        }
        if self.submission.poisoned {
            let _ = unsafe { self.device.device_wait_idle() };
        }
        unsafe {
            self.device.unmap_memory(self.staging.memory);
            for program in self.programs.values() {
                self.device.destroy_fence(program.fence, None);
            }
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline(self.layer_pipeline, None);
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            destroy_buffer(&self.device, &self.staging);
            destroy_buffer(&self.device, &self.arena);
            destroy_buffer(&self.device, &self.resident);
            self.device.destroy_device(None);
        }
        self.stats.resident_frees = self.stats.resident_allocations;
    }
}

struct ChunkSpec {
    descriptor_offset: u64,
    descriptor_range: u64,
    local_rows: u32,
    global_row_start: u32,
    output_row_start: u32,
    weight_byte_bias: u32,
}

struct ProgramSpec {
    plan: ProgramPlan,
    chunks: Vec<ChunkSpec>,
    n_in: u32,
    output_stride: u32,
    mode: u32,
    layer_ops: Vec<LayerOpSpec>,
}

struct ValidatedPlan {
    slots: BTreeMap<SlotId, SlotPlan>,
    specs: Vec<ProgramSpec>,
    resident_size: u64,
    arena_size: u64,
    staging_size: u64,
    descriptor_set_count: u32,
    storage_descriptor_count: u32,
    command_buffer_count: u32,
}

fn descriptor_counts(program_count: usize, set_count: usize) -> Option<(u32, u32, u32)> {
    let command_buffer_count = u32::try_from(program_count).ok()?;
    let descriptor_set_count = u32::try_from(set_count).ok()?;
    let storage_descriptor_count = descriptor_set_count.checked_mul(3)?;
    Some((
        descriptor_set_count,
        storage_descriptor_count,
        command_buffer_count,
    ))
}

fn validate_plan(
    plan: &DevicePlan,
    catalog: &TensorCatalog,
    adapter: &AdapterInfo,
) -> Result<ValidatedPlan, BackendError> {
    if plan.descriptor.backend != adapter.descriptor.backend
        || plan.descriptor.physical_key != adapter.descriptor.physical_key
        || plan.descriptor.name != adapter.descriptor.name
        || plan.descriptor.buffer_alignment == 0
        || plan.descriptor.buffer_alignment != adapter.descriptor.buffer_alignment
        || plan.memory.resident_bytes > plan.descriptor.max_allocation_bytes
    {
        return Err(BackendError::InvalidHandle);
    }
    let slots = validate_slots(plan, adapter.max_storage_buffer_range)?;
    let arena_size = slots
        .values()
        .try_fold(0_u64, |end, slot| {
            slot.arena_offset
                .checked_add(slot.byte_len)
                .map(|slot_end| end.max(slot_end))
        })
        .ok_or(BackendError::InvalidHandle)?;
    for resident in &plan.tensors {
        let entry = catalog
            .entry(resident.tensor)
            .ok_or(BackendError::InvalidHandle)?;
        let expected_start = entry
            .segment_byte_range
            .start
            .checked_add(
                u64::from(resident.rows.start)
                    .checked_mul(entry.row_bytes)
                    .ok_or(BackendError::InvalidHandle)?,
            )
            .ok_or(BackendError::InvalidHandle)?;
        let expected_end = entry
            .segment_byte_range
            .start
            .checked_add(
                u64::from(resident.rows.end)
                    .checked_mul(entry.row_bytes)
                    .ok_or(BackendError::InvalidHandle)?,
            )
            .ok_or(BackendError::InvalidHandle)?;
        let resident_len = expected_end
            .checked_sub(expected_start)
            .ok_or(BackendError::InvalidHandle)?;
        let arena_end = resident
            .arena_offset
            .checked_add(resident_len)
            .ok_or(BackendError::InvalidHandle)?;
        if resident.rows.start >= resident.rows.end
            || u64::from(resident.rows.end) > entry.row_count
            || resident.source_bytes != (expected_start..expected_end)
            || arena_end > plan.memory.resident_bytes
            || resident.arena_offset % plan.descriptor.buffer_alignment != 0
        {
            return Err(BackendError::InvalidHandle);
        }
    }
    let mut resident_ranges = plan
        .tensors
        .iter()
        .map(|resident| {
            let len = resident
                .source_bytes
                .end
                .checked_sub(resident.source_bytes.start)
                .ok_or(BackendError::InvalidHandle)?;
            Ok(resident.arena_offset
                ..resident
                    .arena_offset
                    .checked_add(len)
                    .ok_or(BackendError::InvalidHandle)?)
        })
        .collect::<Result<Vec<_>, BackendError>>()?;
    resident_ranges.sort_by_key(|range| range.start);
    if resident_ranges
        .windows(2)
        .any(|pair| pair[0].end > pair[1].start)
    {
        return Err(BackendError::InvalidHandle);
    }
    let resident_size = align_up_checked(plan.memory.resident_bytes.max(4), 4)
        .ok_or(BackendError::InvalidHandle)?;
    let arena_size = arena_size.max(4);
    let staging_size = resident_size.max(arena_size);
    let required_bytes = resident_size
        .checked_add(arena_size)
        .and_then(|bytes| bytes.checked_add(staging_size))
        .ok_or(BackendError::InvalidHandle)?;
    if [resident_size, arena_size, staging_size]
        .into_iter()
        .any(|size| {
            size > plan.descriptor.max_allocation_bytes
                || size > adapter.descriptor.max_allocation_bytes
        })
        || required_bytes > plan.descriptor.usable_bytes
        || required_bytes > adapter.descriptor.usable_bytes
        || plan.memory.required_bytes > adapter.descriptor.usable_bytes
    {
        return Err(BackendError::InvalidHandle);
    }
    let specs = build_program_specs(plan, catalog, adapter)?;
    let has_fixed = specs.iter().any(|spec| !spec.layer_ops.is_empty());
    if has_fixed
        && (resident_size > adapter.max_storage_buffer_range
            || arena_size > adapter.max_storage_buffer_range)
    {
        return Err(BackendError::InvalidHandle);
    }
    let descriptor_count = specs
        .iter()
        .try_fold(0_usize, |count, spec| {
            let layer_count = usize::from(!spec.layer_ops.is_empty());
            let q8_count = spec.layer_ops.iter().try_fold(0_usize, |count, op| {
                count.checked_add(match op {
                    LayerOpSpec::Q8Matmul { chunks, .. } => chunks.len(),
                    _ => 0,
                })
            })?;
            count
                .checked_add(spec.chunks.len())?
                .checked_add(layer_count)?
                .checked_add(q8_count)
        })
        .ok_or(BackendError::InvalidHandle)?;
    let (descriptor_set_count, storage_descriptor_count, command_buffer_count) =
        descriptor_counts(specs.len(), descriptor_count).ok_or(BackendError::InvalidHandle)?;
    let mut program_ids = BTreeSet::new();
    if specs.is_empty()
        || descriptor_set_count == 0
        || specs.iter().any(|spec| {
            let Some(input) = slots.get(&spec.plan.input) else {
                return true;
            };
            let Some(output) = slots.get(&spec.plan.output) else {
                return true;
            };
            let program_bytes_valid = match &spec.plan.kind {
                ProgramKind::Q8Rows {
                    rows,
                    batch_capacity,
                    ..
                } => {
                    *batch_capacity != 0
                        && matches!(input.kind, SlotKind::Activation | SlotKind::Scratch)
                        && input.storage == SlotStorage::F32
                        && output.kind == SlotKind::Result
                        && output.storage == SlotStorage::F32
                        && u64::from(*batch_capacity)
                            .checked_mul(u64::from(spec.n_in))
                            .and_then(|values| values.checked_mul(4))
                            .is_some_and(|bytes| bytes <= input.byte_len)
                        && u64::from(*batch_capacity)
                            .checked_mul(rows.len() as u64)
                            .and_then(|values| values.checked_mul(4))
                            .is_some_and(|bytes| bytes <= output.byte_len)
                }
                ProgramKind::EmbeddingRows { row_count, .. } => {
                    let output_row_bytes = u64::from(spec.n_in).checked_mul(4);
                    let input_capacity = input.byte_len / size_of::<u32>() as u64;
                    *row_count != 0
                        && input.kind == SlotKind::Scratch
                        && input.storage == SlotStorage::I8
                        && input.byte_len >= 4
                        && output.storage == SlotStorage::F32
                        && matches!(output.kind, SlotKind::Activation | SlotKind::Result)
                        && output_row_bytes.is_some_and(|bytes| {
                            bytes != 0
                                && output.byte_len % bytes == 0
                                && output.byte_len / bytes <= input_capacity
                                && output.byte_len / bytes <= u64::from(u32::MAX)
                        })
                }
                ProgramKind::LayerSegment { .. } | ProgramKind::FinalNormQ8Logits { .. } => true,
            };
            let fixed = matches!(
                spec.plan.kind,
                ProgramKind::LayerSegment { .. } | ProgramKind::FinalNormQ8Logits { .. }
            );
            !program_ids.insert(spec.plan.id)
                || (spec.plan.input == spec.plan.output
                    && !matches!(spec.plan.kind, ProgramKind::LayerSegment { .. }))
                || !program_bytes_valid
                || (spec.chunks.is_empty() && !fixed)
                || spec.chunks.iter().any(|chunk| {
                    chunk
                        .descriptor_offset
                        .checked_add(chunk.descriptor_range)
                        .is_none_or(|end| end > plan.memory.resident_bytes)
                })
        })
    {
        return Err(BackendError::InvalidHandle);
    }
    Ok(ValidatedPlan {
        slots,
        specs,
        resident_size,
        arena_size,
        staging_size,
        descriptor_set_count,
        storage_descriptor_count,
        command_buffer_count,
    })
}

fn build_program_specs(
    plan: &DevicePlan,
    catalog: &TensorCatalog,
    adapter: &AdapterInfo,
) -> Result<Vec<ProgramSpec>, BackendError> {
    plan.programs
        .iter()
        .map(|program| {
            if matches!(
                program.kind,
                ProgramKind::LayerSegment { .. } | ProgramKind::FinalNormQ8Logits { .. }
            ) {
                if matches!(&program.kind, ProgramKind::LayerSegment { families, .. }
                    if families.iter().any(|family| *family != LayerFamily::Qwen3))
                {
                    return Err(BackendError::InvalidHandle);
                }
                if program.layer_ops.is_empty() {
                    return Err(BackendError::InvalidHandle);
                }
                return Ok(ProgramSpec {
                    plan: program.clone(),
                    chunks: Vec::new(),
                    n_in: 0,
                    output_stride: 0,
                    mode: 2,
                    layer_ops: bind_layer_ops(program, plan, catalog, adapter)?,
                });
            }
            let (tensor, rows, n_in, output_stride, mode) = match &program.kind {
                ProgramKind::Q8Rows { tensor, rows, .. } => {
                    (*tensor, rows.clone(), 0, rows.len() as u32, 0)
                }
                ProgramKind::EmbeddingRows { tensor, row_count } => {
                    (*tensor, 0..*row_count, 0, 0, 1)
                }
                _ => {
                    return Err(BackendError::Unsupported {
                        device: adapter.descriptor.id.clone(),
                        operation: "Vulkan program kind",
                    })
                }
            };
            let entry = catalog.entry(tensor).ok_or(BackendError::InvalidHandle)?;
            let n_in = if n_in == 0 {
                u32::try_from(entry.shape[0]).map_err(|_| BackendError::InvalidHandle)?
            } else {
                n_in
            };
            if entry.ggml_type != GGMLType::Q8_0
                || n_in == 0
                || n_in % Q8_BLOCK_ELEMENTS as u32 != 0
                || (mode == 1 && entry.row_count != u64::from(rows.end))
            {
                return Err(BackendError::InvalidHandle);
            }
            let resident = plan
                .tensors
                .iter()
                .find(|resident| resident.tensor == tensor && resident.rows == rows)
                .ok_or(BackendError::InvalidHandle)?;
            let row_bytes = u64::from(n_in) / Q8_BLOCK_ELEMENTS * Q8_BLOCK_BYTES;
            let source_len = resident
                .source_bytes
                .end
                .checked_sub(resident.source_bytes.start)
                .ok_or(BackendError::InvalidHandle)?;
            let expected_len = u64::from(
                rows.end
                    .checked_sub(rows.start)
                    .ok_or(BackendError::InvalidHandle)?,
            )
            .checked_mul(row_bytes)
            .ok_or(BackendError::InvalidHandle)?;
            let resident_end = resident
                .arena_offset
                .checked_add(expected_len)
                .ok_or(BackendError::InvalidHandle)?;
            if source_len != expected_len || resident_end > plan.memory.resident_bytes {
                return Err(BackendError::InvalidHandle);
            }
            let chunks = row_chunks(
                resident,
                row_bytes,
                adapter.max_storage_buffer_range,
                adapter.descriptor.buffer_alignment,
            )?;
            Ok(ProgramSpec {
                plan: program.clone(),
                chunks,
                n_in,
                output_stride: if mode == 0 { output_stride } else { n_in },
                mode,
                layer_ops: Vec::new(),
            })
        })
        .collect()
}

fn bind_layer_ops(
    program: &ProgramPlan,
    plan: &DevicePlan,
    catalog: &TensorCatalog,
    adapter: &AdapterInfo,
) -> Result<Vec<LayerOpSpec>, BackendError> {
    let slot = |id| {
        plan.slots
            .iter()
            .find(|slot| slot.id == id)
            .ok_or(BackendError::InvalidHandle)
    };
    let f32_slot = |id| slot(id).is_ok_and(|slot| slot.storage == SlotStorage::F32);
    let resident = |tensor| {
        let entry = catalog.entry(tensor).ok_or(BackendError::InvalidHandle)?;
        let plan = plan
            .tensors
            .iter()
            .find(|plan| {
                plan.tensor == tensor
                    && plan.rows.start == 0
                    && u64::from(plan.rows.end) == entry.row_count
            })
            .ok_or(BackendError::InvalidHandle)?;
        Ok::<_, BackendError>((
            entry,
            plan,
            ResidentRange {
                offset: plan.arena_offset,
            },
        ))
    };
    let mut widths = BTreeMap::new();
    let mut bound = Vec::with_capacity(program.layer_ops.len());
    for op in &program.layer_ops {
        match *op {
            LayerOp::RmsNorm {
                input,
                weight,
                output,
                epsilon_bits,
            } => {
                let (entry, _, weight_range) = resident(weight)?;
                let elements =
                    u32::try_from(entry.shape[0]).map_err(|_| BackendError::InvalidHandle)?;
                let groups = u32::try_from(slot(input)?.byte_len / 4 / u64::from(elements))
                    .map_err(|_| BackendError::InvalidHandle)?;
                if !matches!(entry.ggml_type, GGMLType::F32 | GGMLType::F16)
                    || groups == 0
                    || !f32_slot(input)
                    || !f32_slot(output)
                {
                    return Err(BackendError::InvalidHandle);
                }
                widths.insert(input, elements);
                widths.insert(output, elements);
                bound.push(LayerOpSpec::RmsNorm {
                    input,
                    weight: weight_range,
                    output,
                    elements,
                    groups,
                    epsilon_bits,
                    weight_f16: entry.ggml_type == GGMLType::F16,
                });
            }
            LayerOp::Q8Matmul {
                input,
                weight,
                output,
            } => {
                let (entry, resident_plan, _) = resident(weight)?;
                let n_in =
                    u32::try_from(entry.shape[0]).map_err(|_| BackendError::InvalidHandle)?;
                let rows =
                    u32::try_from(entry.row_count).map_err(|_| BackendError::InvalidHandle)?;
                if entry.ggml_type != GGMLType::Q8_0
                    || n_in == 0
                    || n_in % Q8_BLOCK_ELEMENTS as u32 != 0
                    || !f32_slot(input)
                    || !f32_slot(output)
                    || widths.get(&input).is_some_and(|width| *width != n_in)
                {
                    return Err(BackendError::InvalidHandle);
                }
                let row_bytes = u64::from(n_in) / Q8_BLOCK_ELEMENTS * Q8_BLOCK_BYTES;
                widths.insert(input, n_in);
                widths.insert(output, rows);
                bound.push(LayerOpSpec::Q8Matmul {
                    input,
                    chunks: row_chunks(
                        resident_plan,
                        row_bytes,
                        adapter.max_storage_buffer_range,
                        adapter.descriptor.buffer_alignment,
                    )?,
                    output,
                    n_in,
                    rows,
                });
            }
            LayerOp::Rope {
                q,
                k,
                key_head_dim,
                rope_dims,
                freq_base_bits,
            } if key_head_dim == rope_dims && f32_slot(q) && f32_slot(k) => {
                bound.push(LayerOpSpec::Rope {
                    q,
                    k,
                    q_width: *widths.get(&q).ok_or(BackendError::InvalidHandle)?,
                    k_width: *widths.get(&k).ok_or(BackendError::InvalidHandle)?,
                    key_head_dim,
                    freq_base_bits,
                });
            }
            LayerOp::KvAppend {
                k,
                v,
                key_state,
                value_state,
                ..
            } => {
                let Some((key_width, value_width)) =
                    program.layer_ops.iter().find_map(|op| match op {
                        LayerOp::Attention {
                            kv_head_count,
                            key_state: keys,
                            value_state: values,
                            key_head_dim,
                            value_head_dim,
                            ..
                        } if *keys == key_state && *values == value_state => Some((
                            kv_head_count.checked_mul(*key_head_dim)?,
                            kv_head_count.checked_mul(*value_head_dim)?,
                        )),
                        _ => None,
                    })
                else {
                    return Err(BackendError::InvalidHandle);
                };
                if !f32_slot(k)
                    || !f32_slot(v)
                    || slot(key_state)?.storage != SlotStorage::F16
                    || slot(value_state)?.storage != SlotStorage::F16
                {
                    return Err(BackendError::InvalidHandle);
                }
                bound.push(LayerOpSpec::KvAppend {
                    k,
                    v,
                    key_state,
                    value_state,
                    key_width,
                    value_width,
                });
            }
            LayerOp::Attention {
                q,
                output,
                head_count,
                kv_head_count,
                key_state,
                value_state,
                key_head_dim,
                value_head_dim,
                context_capacity,
                ..
            } if kv_head_count != 0 && f32_slot(q) && f32_slot(output) => {
                widths.insert(
                    output,
                    head_count
                        .checked_mul(value_head_dim)
                        .ok_or(BackendError::InvalidHandle)?,
                );
                bound.push(LayerOpSpec::Attention {
                    q,
                    output,
                    head_count,
                    kv_head_count,
                    key_state,
                    value_state,
                    key_head_dim,
                    value_head_dim,
                    context_capacity,
                });
            }
            LayerOp::SiluMul { gate, up } if f32_slot(gate) && f32_slot(up) => {
                let elements = *widths.get(&up).ok_or(BackendError::InvalidHandle)?;
                if widths.get(&gate) != Some(&elements) {
                    return Err(BackendError::InvalidHandle);
                }
                bound.push(LayerOpSpec::SiluMul { gate, up, elements });
            }
            LayerOp::Add {
                left,
                right,
                output,
            } if f32_slot(left) && f32_slot(right) && f32_slot(output) => {
                let elements = *widths
                    .get(&left)
                    .or_else(|| widths.get(&right))
                    .ok_or(BackendError::InvalidHandle)?;
                widths.insert(output, elements);
                bound.push(LayerOpSpec::Add {
                    left,
                    right,
                    output,
                    elements,
                });
            }
            _ => return Err(BackendError::InvalidHandle),
        }
    }
    Ok(bound)
}

fn row_chunks(
    resident: &ResidentTensorPlan,
    row_bytes: u64,
    max_range: u64,
    alignment: u64,
) -> Result<Vec<ChunkSpec>, BackendError> {
    let total_rows = resident
        .rows
        .end
        .checked_sub(resident.rows.start)
        .ok_or(BackendError::InvalidHandle)?;
    let mut chunks = Vec::new();
    let mut local_start = 0_u32;
    while local_start < total_rows {
        let byte_start = resident
            .arena_offset
            .checked_add(
                u64::from(local_start)
                    .checked_mul(row_bytes)
                    .ok_or(BackendError::InvalidHandle)?,
            )
            .ok_or(BackendError::InvalidHandle)?;
        let descriptor_offset = align_down(byte_start, alignment);
        let bias = byte_start - descriptor_offset;
        let available = max_range
            .checked_sub(bias)
            .ok_or(BackendError::InvalidHandle)?;
        let mut rows = (available / row_bytes).min(u64::from(total_rows - local_start));
        while rows != 0
            && align_up(
                bias.checked_add(
                    rows.checked_mul(row_bytes)
                        .ok_or(BackendError::InvalidHandle)?,
                )
                .ok_or(BackendError::InvalidHandle)?,
                4,
            ) > max_range
        {
            rows -= 1;
        }
        if rows == 0 || bias > u64::from(u32::MAX) {
            return Err(BackendError::InvalidHandle);
        }
        chunks.push(ChunkSpec {
            descriptor_offset,
            descriptor_range: align_up(
                bias.checked_add(
                    rows.checked_mul(row_bytes)
                        .ok_or(BackendError::InvalidHandle)?,
                )
                .ok_or(BackendError::InvalidHandle)?,
                4,
            ),
            local_rows: rows as u32,
            global_row_start: resident.rows.start + local_start,
            output_row_start: local_start,
            weight_byte_bias: bias as u32,
        });
        local_start += rows as u32;
    }
    Ok(chunks)
}

fn validate_slots(
    plan: &DevicePlan,
    max_storage_buffer_range: u64,
) -> Result<BTreeMap<SlotId, SlotPlan>, BackendError> {
    if plan.descriptor.buffer_alignment == 0 {
        return Err(BackendError::InvalidHandle);
    }
    let mut slots = BTreeMap::new();
    let mut ranges = Vec::with_capacity(plan.slots.len());
    for (index, slot) in plan.slots.iter().enumerate() {
        if slot.id.0 as usize != index
            || slot.byte_len == 0
            || slot.byte_len > max_storage_buffer_range
            || slot.alignment != plan.descriptor.buffer_alignment
            || slot.arena_offset % plan.descriptor.buffer_alignment != 0
        {
            return Err(BackendError::InvalidHandle);
        }
        ranges.push(
            slot.arena_offset
                ..slot
                    .arena_offset
                    .checked_add(slot.byte_len)
                    .ok_or(BackendError::InvalidHandle)?,
        );
        slots.insert(slot.id, slot.clone());
    }
    ranges.sort_by_key(|range| range.start);
    if ranges.windows(2).any(|pair| pair[0].end > pair[1].start) {
        return Err(BackendError::InvalidHandle);
    }
    Ok(slots)
}

fn upload_resident(
    device: &ash::Device,
    queue: vk::Queue,
    command_pool: vk::CommandPool,
    resident: &BufferAllocation,
    staging: &BufferAllocation,
    staging_ptr: usize,
    atom_size: u64,
    descriptor: &DeviceDescriptor,
    plan: &DevicePlan,
    catalog: &TensorCatalog,
) -> Result<(), BackendError> {
    let mut copies = Vec::with_capacity(plan.tensors.len());
    for tensor in &plan.tensors {
        let bytes = resident_source_bytes(catalog, tensor)?;
        let end = tensor
            .arena_offset
            .checked_add(bytes.len() as u64)
            .ok_or(BackendError::InvalidHandle)?;
        if end > plan.memory.resident_bytes || end > staging.allocation_size {
            return Err(BackendError::InvalidHandle);
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                (staging_ptr + tensor.arena_offset as usize) as *mut u8,
                bytes.len(),
            )
        };
        copies.push(
            vk::BufferCopy::default()
                .src_offset(tensor.arena_offset)
                .dst_offset(tensor.arena_offset)
                .size(bytes.len() as u64),
        );
    }
    flush_memory(
        device,
        staging,
        atom_size,
        0,
        plan.memory.resident_bytes,
        descriptor,
    )?;
    let command = unsafe {
        device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )
    }
    .map_err(|error| BackendError::Upload {
        device: descriptor.id.clone(),
        message: format!("allocate upload command: {error:?}"),
    })?[0];
    let fence =
        unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None) }.map_err(|error| {
            BackendError::Upload {
                device: descriptor.id.clone(),
                message: format!("create upload fence: {error:?}"),
            }
        })?;
    let result = (|| -> Result<(), vk::Result> {
        unsafe {
            device.begin_command_buffer(
                command,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
            if !copies.is_empty() {
                device.cmd_copy_buffer(command, staging.buffer, resident.buffer, &copies);
                let barrier = vk::BufferMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .buffer(resident.buffer)
                    .offset(0)
                    .size(plan.memory.resident_bytes);
                device.cmd_pipeline_barrier(
                    command,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[barrier],
                    &[],
                );
            }
            device.end_command_buffer(command)?;
            let commands = [command];
            device.queue_submit(
                queue,
                &[vk::SubmitInfo::default().command_buffers(&commands)],
                fence,
            )?;
            device.wait_for_fences(&[fence], true, u64::MAX)?;
            Ok(())
        }
    })();
    unsafe {
        device.destroy_fence(fence, None);
        device.free_command_buffers(command_pool, &[command]);
    }
    result.map_err(|error| BackendError::Upload {
        device: descriptor.id.clone(),
        message: format!("upload resident weights: {error:?}"),
    })
}

fn resident_source_bytes<'a>(
    catalog: &'a TensorCatalog,
    resident: &ResidentTensorPlan,
) -> Result<&'a [u8], BackendError> {
    let entry = catalog
        .entry(resident.tensor)
        .ok_or(BackendError::InvalidHandle)?;
    let source = catalog
        .bytes(resident.tensor)
        .map_err(|_| BackendError::InvalidHandle)?;
    let start = resident
        .source_bytes
        .start
        .checked_sub(entry.segment_byte_range.start)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(BackendError::InvalidHandle)?;
    let end = resident
        .source_bytes
        .end
        .checked_sub(entry.segment_byte_range.start)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(BackendError::InvalidHandle)?;
    source.get(start..end).ok_or(BackendError::InvalidHandle)
}

fn create_buffer(
    device: &ash::Device,
    adapter: &AdapterInfo,
    size: u64,
    usage: vk::BufferUsageFlags,
    required: vk::MemoryPropertyFlags,
) -> Result<BufferAllocation, BackendError> {
    let buffer = unsafe {
        device.create_buffer(
            &vk::BufferCreateInfo::default()
                .size(size)
                .usage(usage)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            None,
        )
    }
    .map_err(|error| allocation(&adapter.descriptor, format!("create buffer: {error:?}")))?;
    let mut buffer_owner =
        HandleOwner::new(|buffer| unsafe { device.destroy_buffer(buffer, None) });
    buffer_owner.push(buffer);
    let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
    let memory_type = (0..adapter.memory.memory_type_count)
        .find(|index| {
            requirements.memory_type_bits & (1 << index) != 0
                && adapter.memory.memory_types[*index as usize]
                    .property_flags
                    .contains(required)
        })
        .ok_or_else(|| allocation(&adapter.descriptor, "no compatible memory type"))?;
    let flags = adapter.memory.memory_types[memory_type as usize].property_flags;
    let memory = unsafe {
        device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(memory_type),
            None,
        )
    }
    .map_err(|error| allocation(&adapter.descriptor, format!("allocate buffer: {error:?}")))?;
    let mut memory_owner = HandleOwner::new(|memory| unsafe { device.free_memory(memory, None) });
    memory_owner.push(memory);
    if let Err(error) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
        return Err(allocation(
            &adapter.descriptor,
            format!("bind buffer: {error:?}"),
        ));
    }
    memory_owner.disarm();
    buffer_owner.disarm();
    Ok(BufferAllocation {
        buffer,
        memory,
        allocation_size: requirements.size,
        coherent: flags.contains(vk::MemoryPropertyFlags::HOST_COHERENT),
    })
}

unsafe fn destroy_buffer(device: &ash::Device, allocation: &BufferAllocation) {
    device.destroy_buffer(allocation.buffer, None);
    device.free_memory(allocation.memory, None);
}

fn flush_memory(
    device: &ash::Device,
    allocation: &BufferAllocation,
    atom_size: u64,
    offset: u64,
    size: u64,
    descriptor: &DeviceDescriptor,
) -> Result<(), BackendError> {
    if allocation.coherent || size == 0 {
        return Ok(());
    }
    let range = mapped_range(
        allocation.memory,
        allocation.allocation_size,
        atom_size,
        offset,
        size,
    );
    unsafe { device.flush_mapped_memory_ranges(&[range]) }.map_err(|error| BackendError::Upload {
        device: descriptor.id.clone(),
        message: format!("flush staging: {error:?}"),
    })
}

fn mapped_range(
    memory: vk::DeviceMemory,
    allocation_size: u64,
    atom_size: u64,
    offset: u64,
    size: u64,
) -> vk::MappedMemoryRange<'static> {
    let start = align_down(offset, atom_size);
    let end = align_up(offset.saturating_add(size), atom_size).min(allocation_size);
    vk::MappedMemoryRange::default()
        .memory(memory)
        .offset(start)
        .size(end - start)
}

fn extension_name(name: &[std::ffi::c_char]) -> &CStr {
    unsafe { CStr::from_ptr(name.as_ptr()) }
}

fn load_entry() -> Result<Entry, BackendError> {
    let default = unsafe { Entry::load() };
    #[cfg(target_os = "macos")]
    if default.is_err() {
        return unsafe { Entry::load_from("/opt/homebrew/lib/libvulkan.dylib") }
            .map_err(|error| unavailable(error.to_string()));
    }
    default.map_err(|error| unavailable(error.to_string()))
}

fn device_local_heap_bytes(memory: &vk::PhysicalDeviceMemoryProperties) -> u64 {
    memory.memory_heaps[..memory.memory_heap_count as usize]
        .iter()
        .filter(|heap| heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
        .map(|heap| heap.size)
        .max()
        .unwrap_or(0)
}

fn device_local_heap_budget(
    memory: &vk::PhysicalDeviceMemoryProperties,
    budget: &vk::PhysicalDeviceMemoryBudgetPropertiesEXT<'_>,
) -> u64 {
    memory.memory_heaps[..memory.memory_heap_count as usize]
        .iter()
        .enumerate()
        .filter(|(_, heap)| heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
        .map(|(index, _)| budget.heap_budget[index].saturating_sub(budget.heap_usage[index]))
        .max()
        .unwrap_or(0)
}

fn is_unified_memory(memory: &vk::PhysicalDeviceMemoryProperties) -> bool {
    memory.memory_types[..memory.memory_type_count as usize]
        .iter()
        .any(|memory_type| {
            memory_type.property_flags.contains(
                vk::MemoryPropertyFlags::DEVICE_LOCAL | vk::MemoryPropertyFlags::HOST_VISIBLE,
            )
        })
}

fn hex_uuid(uuid: &[u8; vk::UUID_SIZE]) -> String {
    uuid.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn align_down(value: u64, alignment: u64) -> u64 {
    value / alignment.max(1) * alignment.max(1)
}

fn align_up(value: u64, alignment: u64) -> u64 {
    let alignment = alignment.max(1);
    value.saturating_add(alignment - 1) / alignment * alignment
}

fn align_up_checked(value: u64, alignment: u64) -> Option<u64> {
    let alignment = alignment.max(1);
    value
        .checked_add(alignment - 1)
        .map(|value| value / alignment * alignment)
}

fn unavailable(message: impl Into<String>) -> BackendError {
    BackendError::Allocation {
        device: DeviceId::parse("vulkan0").expect("vulkan0 is valid"),
        message: message.into(),
    }
}

fn allocation(descriptor: &DeviceDescriptor, message: impl Into<String>) -> BackendError {
    BackendError::Allocation {
        device: descriptor.id.clone(),
        message: message.into(),
    }
}

fn pipeline_error(descriptor: &DeviceDescriptor, message: impl Into<String>) -> BackendError {
    BackendError::Pipeline {
        device: descriptor.id.clone(),
        message: message.into(),
    }
}

fn submission(descriptor: &DeviceDescriptor, message: impl Into<String>) -> BackendError {
    BackendError::Submission {
        device: descriptor.id.clone(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MetaValue, SourceFormat, SourceTensorRecord, TensorInfo, TensorSource};

    struct TestSource {
        info: TensorInfo,
        bytes: Vec<u8>,
        segment_start: u64,
    }

    impl TensorSource for TestSource {
        fn metadata(&self, _key: &str) -> Option<&MetaValue> {
            None
        }

        fn tensor_info(&self, name: &str) -> Option<&TensorInfo> {
            (name == self.info.name).then_some(&self.info)
        }

        fn tensor_slice(&self, name: &str) -> Option<&[u8]> {
            (name == self.info.name).then_some(self.bytes.as_slice())
        }

        fn source_format(&self) -> SourceFormat {
            SourceFormat::Gguf
        }

        fn tensor_records(&self) -> Vec<SourceTensorRecord> {
            vec![SourceTensorRecord {
                info: self.info.clone(),
                segment_id: 0,
                segment_byte_range: self.segment_start
                    ..self.segment_start + self.bytes.len() as u64,
                layer: None,
            }]
        }
    }

    fn test_catalog() -> TensorCatalog {
        let bytes = vec![0; 129 * 68];
        TensorCatalog::from_sources(vec![(
            ComponentId::Llm,
            Arc::new(TestSource {
                info: TensorInfo {
                    name: "weight".into(),
                    dims: vec![64, 129],
                    ggml_type: GGMLType::Q8_0,
                    offset: 0,
                },
                bytes,
                segment_start: 0,
            }),
        )])
        .unwrap()
    }

    fn test_adapter() -> AdapterInfo {
        AdapterInfo {
            descriptor: DeviceDescriptor {
                id: DeviceId::parse("vulkan0").unwrap(),
                backend: BackendKind::Vulkan,
                physical_key: "vulkan:00112233445566778899aabbccddeeff".into(),
                name: "test adapter (compute queue 3)".into(),
                usable_bytes: 1 << 20,
                max_allocation_bytes: 1 << 20,
                buffer_alignment: 16,
                unified_memory: false,
                capabilities: DeviceCapabilities {
                    components: BTreeSet::from([ComponentId::Llm]),
                    modes: BTreeSet::from([PlacementMode::Row]),
                    layer_families: BTreeSet::new(),
                    tensor_types: BTreeSet::from([GGMLType::Q8_0]),
                },
            },
            physical: vk::PhysicalDevice::null(),
            queue_family: 3,
            memory: vk::PhysicalDeviceMemoryProperties::default(),
            non_coherent_atom_size: 16,
            max_storage_buffer_range: 1 << 20,
            portability_subset: false,
        }
    }

    fn test_plan(descriptor: DeviceDescriptor) -> DevicePlan {
        DevicePlan {
            descriptor,
            tensors: vec![ResidentTensorPlan {
                tensor: crate::TensorId(0),
                rows: 17..113,
                source_bytes: 17 * 68..113 * 68,
                arena_offset: 0,
            }],
            slots: vec![
                SlotPlan {
                    id: SlotId(0),
                    kind: super::super::SlotKind::Activation,
                    storage: SlotStorage::F32,
                    byte_len: 2 * 64 * 4,
                    alignment: 16,
                    arena_offset: 0,
                },
                SlotPlan {
                    id: SlotId(1),
                    kind: super::super::SlotKind::Result,
                    storage: SlotStorage::F32,
                    byte_len: 2 * 96 * 4,
                    alignment: 16,
                    arena_offset: 2 * 64 * 4,
                },
            ],
            programs: vec![ProgramPlan {
                id: ProgramId(0),
                kind: ProgramKind::Q8Rows {
                    tensor: crate::TensorId(0),
                    rows: 17..113,
                    batch_capacity: 2,
                },
                input: SlotId(0),
                output: SlotId(1),
                layer_ops: Vec::new(),
            }],
            memory: super::super::MemoryPlan {
                resident_bytes: 96 * 68,
                scratch_bytes: 2 * 64 * 4 + 2 * 96 * 4,
                staging_bytes: 96 * 68,
                required_bytes: 96 * 68 * 2 + 2 * 64 * 4 + 2 * 96 * 4,
                largest_allocation_bytes: 96 * 68,
                ..super::super::MemoryPlan::default()
            },
        }
    }

    #[test]
    fn forged_resident_source_and_arena_ranges_are_rejected() {
        let catalog = test_catalog();
        let adapter = test_adapter();
        let mut short_source = test_plan(adapter.descriptor.clone());
        short_source.tensors[0].source_bytes = 17 * 68..18 * 68;
        assert!(build_program_specs(&short_source, &catalog, &adapter).is_err());

        let mut outside_resident = test_plan(adapter.descriptor.clone());
        outside_resident.tensors[0].arena_offset = outside_resident.memory.resident_bytes - 34;
        assert!(build_program_specs(&outside_resident, &catalog, &adapter).is_err());
    }

    #[test]
    fn resident_source_range_is_resolved_relative_to_tensor_slice() {
        let bytes = (0..129 * 68).map(|value| value as u8).collect::<Vec<_>>();
        let catalog = TensorCatalog::from_sources(vec![(
            ComponentId::Llm,
            Arc::new(TestSource {
                info: TensorInfo {
                    name: "weight".into(),
                    dims: vec![64, 129],
                    ggml_type: GGMLType::Q8_0,
                    offset: 0,
                },
                bytes: bytes.clone(),
                segment_start: 4096,
            }),
        )])
        .unwrap();
        let resident = ResidentTensorPlan {
            tensor: crate::TensorId(0),
            rows: 17..18,
            source_bytes: 4096 + 17 * 68..4096 + 18 * 68,
            arena_offset: 0,
        };

        assert_eq!(
            resident_source_bytes(&catalog, &resident).unwrap(),
            &bytes[17 * 68..18 * 68]
        );
    }

    #[test]
    fn forged_slots_program_handles_and_memory_totals_are_rejected() {
        let catalog = test_catalog();
        let adapter = test_adapter();

        let mut missing_output = test_plan(adapter.descriptor.clone());
        missing_output.programs[0].output = SlotId(99);
        assert!(validate_plan(&missing_output, &catalog, &adapter).is_err());

        let mut slot_overflow = test_plan(adapter.descriptor.clone());
        slot_overflow.slots[1].arena_offset = u64::MAX - 3;
        assert!(validate_plan(&slot_overflow, &catalog, &adapter).is_err());

        let mut short_resident = test_plan(adapter.descriptor.clone());
        short_resident.memory.resident_bytes = 68;
        assert!(validate_plan(&short_resident, &catalog, &adapter).is_err());
    }

    #[test]
    fn overlapping_slot_arena_ranges_are_rejected() {
        let catalog = test_catalog();
        let adapter = test_adapter();
        let mut plan = test_plan(adapter.descriptor.clone());
        plan.slots[1].arena_offset = plan.slots[0].byte_len - 16;

        assert!(validate_plan(&plan, &catalog, &adapter).is_err());
    }

    #[test]
    fn q8_public_input_requires_writable_f32_activation_storage() {
        let catalog = test_catalog();
        let adapter = test_adapter();

        let mut result_input = test_plan(adapter.descriptor.clone());
        result_input.slots[0].kind = SlotKind::Result;
        assert!(validate_plan(&result_input, &catalog, &adapter).is_err());

        let mut i8_input = test_plan(adapter.descriptor.clone());
        i8_input.slots[0].kind = SlotKind::Scratch;
        i8_input.slots[0].storage = SlotStorage::I8;
        assert!(validate_plan(&i8_input, &catalog, &adapter).is_err());
    }

    #[test]
    fn embedding_input_capacity_must_cover_output_rows_before_open() {
        let catalog = test_catalog();
        let adapter = test_adapter();
        let mut plan = test_plan(adapter.descriptor.clone());
        plan.tensors[0].rows = 0..129;
        plan.tensors[0].source_bytes = 0..129 * 68;
        plan.slots[0].kind = SlotKind::Scratch;
        plan.slots[0].storage = SlotStorage::I8;
        plan.slots[0].byte_len = size_of::<u32>() as u64;
        plan.slots[1].kind = SlotKind::Activation;
        plan.slots[1].byte_len = 2 * 64 * size_of::<f32>() as u64;
        plan.slots[1].arena_offset = 16;
        plan.programs[0].kind = ProgramKind::EmbeddingRows {
            tensor: crate::TensorId(0),
            row_count: 129,
        };
        plan.memory.resident_bytes = 129 * 68;

        assert!(validate_plan(&plan, &catalog, &adapter).is_err());
    }

    #[test]
    fn slot_alignment_must_match_the_planner_contract() {
        let catalog = test_catalog();
        let adapter = test_adapter();
        let mut plan = test_plan(adapter.descriptor.clone());
        plan.slots[0].alignment = 1;

        assert!(validate_plan(&plan, &catalog, &adapter).is_err());
    }

    #[test]
    fn forged_plan_limits_cannot_exceed_the_reopened_adapter() {
        let catalog = test_catalog();

        let mut allocation_adapter = test_adapter();
        let mut allocation_plan = test_plan(allocation_adapter.descriptor.clone());
        allocation_plan.descriptor.max_allocation_bytes = 1 << 30;
        allocation_adapter.descriptor.max_allocation_bytes = 1024;
        assert!(validate_plan(&allocation_plan, &catalog, &allocation_adapter).is_err());

        let mut capacity_adapter = test_adapter();
        let mut capacity_plan = test_plan(capacity_adapter.descriptor.clone());
        capacity_plan.descriptor.usable_bytes = 1 << 30;
        capacity_adapter.descriptor.usable_bytes = 1024;
        assert!(validate_plan(&capacity_plan, &catalog, &capacity_adapter).is_err());

        let mut alignment_adapter = test_adapter();
        let alignment_plan = test_plan(alignment_adapter.descriptor.clone());
        alignment_adapter.descriptor.buffer_alignment = 1024;
        assert!(validate_plan(&alignment_plan, &catalog, &alignment_adapter).is_err());
    }

    #[test]
    fn partial_created_handles_are_owned_before_the_error_is_returned() {
        use std::sync::Mutex;

        let released = Arc::new(Mutex::new(Vec::new()));
        {
            let output = Arc::clone(&released);
            let mut owner = HandleOwner::new(move |handle| output.lock().unwrap().push(handle));
            assert_eq!(
                take_created_handle(
                    &mut owner,
                    Err((vec![1, 2], vk::Result::ERROR_OUT_OF_DEVICE_MEMORY)),
                    |handle| handle,
                ),
                Err(vk::Result::ERROR_OUT_OF_DEVICE_MEMORY)
            );
        }
        assert_eq!(*released.lock().unwrap(), vec![2, 1]);
    }

    #[test]
    fn descriptor_counts_reject_every_u32_truncation_or_overflow() {
        assert_eq!(descriptor_counts(2, 3), Some((3, 9, 2)));
        assert_eq!(descriptor_counts(1, u32::MAX as usize / 3 + 1), None);
        assert_eq!(descriptor_counts(u32::MAX as usize + 1, 1), None);
    }

    #[test]
    fn open_handle_owner_releases_every_failure_prefix_in_reverse_order() {
        use std::sync::Mutex;

        for fail_after in 1..=8 {
            let released = Arc::new(Mutex::new(Vec::new()));
            {
                let output = Arc::clone(&released);
                let mut owner = HandleOwner::new(move |handle| output.lock().unwrap().push(handle));
                for handle in 0..fail_after {
                    owner.push(handle);
                }
                // Inject failure by leaving scope at every possible prefix.
            }
            assert_eq!(
                *released.lock().unwrap(),
                (0..fail_after).rev().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn open_handle_owner_transfers_without_cleanup() {
        use std::sync::Mutex;

        let released = Arc::new(Mutex::new(Vec::new()));
        let output = Arc::clone(&released);
        let mut owner = HandleOwner::new(move |handle| output.lock().unwrap().push(handle));
        owner.push(1);
        owner.push(2);

        assert_eq!(owner.disarm(), vec![1, 2]);
        assert!(released.lock().unwrap().is_empty());
    }

    #[test]
    fn failed_queue_submit_poison_is_stable_and_never_looks_idle() {
        let mut tracker = SubmissionTracker::default();
        assert_eq!(
            tracker.finish_submit(
                Err(vk::Result::ERROR_DEVICE_LOST),
                Pending {
                    id: FenceId(1),
                    program: ProgramId(0),
                },
            ),
            Err(vk::Result::ERROR_DEVICE_LOST)
        );

        assert_eq!(tracker.require_idle(), Err("Vulkan session is poisoned"));
        assert_eq!(tracker.require_idle(), Err("Vulkan session is poisoned"));
    }

    #[test]
    fn predecessor_wait_failure_clears_pending_and_poisons_future_work() {
        let mut tracker = SubmissionTracker {
            pending: VecDeque::from([
                Pending {
                    id: FenceId(1),
                    program: ProgramId(0),
                },
                Pending {
                    id: FenceId(2),
                    program: ProgramId(1),
                },
            ]),
            poisoned: false,
        };
        assert!(matches!(
            drain_pending(&mut tracker, FenceId(2), |pending| {
                if pending.id == FenceId(1) {
                    Err(BackendError::InvalidHandle)
                } else {
                    Ok(())
                }
            }),
            Err(BackendError::InvalidHandle)
        ));
        assert!(tracker.pending.is_empty());
        assert_eq!(tracker.require_idle(), Err("Vulkan session is poisoned"));
    }

    #[test]
    fn final_fence_drains_predecessors_with_one_host_wait_boundary() {
        let provider = VulkanProvider::new().unwrap();
        let descriptor = provider.enumerate().unwrap().remove(0);
        let catalog = Arc::new(test_catalog());
        let mut plan = test_plan(descriptor.clone());
        let output_offset = align_up(plan.slots[0].byte_len, descriptor.buffer_alignment);
        plan.slots[0].alignment = descriptor.buffer_alignment;
        plan.slots[1].alignment = descriptor.buffer_alignment;
        plan.slots[1].arena_offset = output_offset;
        for id in [ProgramId(1), ProgramId(2)] {
            let mut program = plan.programs[0].clone();
            program.id = id;
            plan.programs.push(program);
        }
        let mut session = provider.open(&descriptor, &plan, catalog).unwrap();
        session.write_f32(SlotId(0), &[1.0; 128]).unwrap();
        let params = RunParams {
            token_count: 2,
            position_start: 0,
            mrope_positions: &[],
            token_ids: &[],
        };
        session.submit(ProgramId(0), &params).unwrap();
        session.submit(ProgramId(1), &params).unwrap();
        let final_fence = session.submit(ProgramId(2), &params).unwrap();
        assert!(matches!(
            session.submit(ProgramId(2), &params),
            Err(BackendError::Submission { .. })
        ));
        session.wait(final_fence).unwrap();
        assert_eq!(session.stats().host_waits, 1);
    }

    #[test]
    fn reopened_adapter_must_match_discovered_physical_identity_and_queue() {
        let expected = test_adapter();
        let mut changed_physical = test_adapter();
        changed_physical.descriptor.physical_key = "vulkan:ffffffffffffffffffffffffffffffff".into();
        assert!(select_adapter(vec![changed_physical], &expected.descriptor, &expected).is_err());

        let mut changed_queue = test_adapter();
        changed_queue.descriptor.name = "test adapter (compute queue 4)".into();
        changed_queue.queue_family = 4;
        assert!(select_adapter(vec![changed_queue], &expected.descriptor, &expected).is_err());

        assert_eq!(
            select_adapter(vec![test_adapter()], &expected.descriptor, &expected)
                .unwrap()
                .queue_family,
            3
        );
    }

    #[test]
    fn reopened_adapter_must_match_discovered_immutable_limits() {
        let expected = test_adapter();

        let mut forged_allocation = expected.descriptor.clone();
        forged_allocation.max_allocation_bytes += 1;
        assert!(select_adapter(vec![test_adapter()], &forged_allocation, &expected).is_err());

        let mut forged_alignment = expected.descriptor.clone();
        forged_alignment.buffer_alignment *= 2;
        assert!(select_adapter(vec![test_adapter()], &forged_alignment, &expected).is_err());

        let mut changed_storage_range = test_adapter();
        changed_storage_range.max_storage_buffer_range /= 2;
        assert!(
            select_adapter(vec![changed_storage_range], &expected.descriptor, &expected).is_err()
        );

        let mut changed_atom_size = test_adapter();
        changed_atom_size.non_coherent_atom_size *= 2;
        assert!(select_adapter(vec![changed_atom_size], &expected.descriptor, &expected).is_err());
    }

    #[test]
    fn reopened_adapter_allows_dynamic_budget_drift() {
        let expected = test_adapter();
        let mut planned = expected.descriptor.clone();
        planned.usable_bytes *= 2;
        let mut current = test_adapter();
        current.descriptor.usable_bytes /= 2;

        assert_eq!(
            select_adapter(vec![current], &planned, &expected)
                .unwrap()
                .descriptor
                .usable_bytes,
            expected.descriptor.usable_bytes / 2
        );
    }

    #[test]
    fn reopened_adapter_uses_current_budget_for_plan_capacity() {
        let catalog = test_catalog();
        let expected = test_adapter();
        let mut current = test_adapter();
        current.descriptor.usable_bytes = 1 << 18;
        let mut plan = test_plan(expected.descriptor.clone());
        plan.memory.required_bytes = 1 << 19;

        let current = select_adapter(vec![current], &expected.descriptor, &expected).unwrap();
        assert!(validate_plan(&plan, &catalog, &current).is_err());
    }

    #[test]
    fn resident_chunks_keep_complete_rows_within_storage_range() {
        let resident = ResidentTensorPlan {
            tensor: crate::TensorId(0),
            rows: 17..27,
            source_bytes: 17 * 68..27 * 68,
            arena_offset: 0,
        };

        let chunks = row_chunks(&resident, 68, 300, 256).unwrap();

        assert_eq!(
            chunks
                .iter()
                .map(|chunk| (
                    chunk.global_row_start,
                    chunk.local_rows,
                    chunk.output_row_start
                ))
                .collect::<Vec<_>>(),
            vec![(17, 4, 0), (21, 4, 4), (25, 2, 8)]
        );
        assert!(chunks
            .iter()
            .all(|chunk| { chunk.descriptor_offset % 256 == 0 && chunk.descriptor_range <= 300 }));
    }
}
