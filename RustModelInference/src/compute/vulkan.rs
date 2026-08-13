use super::device::{
    BackendError, BackendKind, DeviceCapabilities, DeviceDescriptor, DeviceDiscovery,
    DeviceProvider, DeviceSession, FenceId, LifecycleProbe, ProgramId, RunParams, SessionStats,
    SlotId,
};
use super::program::{
    DevicePlan, ProgramKind, ProgramPlan, ResidentTensorPlan, SlotPlan, SlotStorage,
};
use crate::{ComponentId, DeviceId, GGMLType, PlacementMode, TensorCatalog};
use ash::{vk, Entry, Instance};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CStr, CString};
use std::io::Cursor;
use std::sync::Arc;

const LOCAL_SIZE: u32 = 64;
const Q8_BLOCK_ELEMENTS: u64 = 32;
const Q8_BLOCK_BYTES: u64 = 34;

pub struct VulkanProvider {
    context: Arc<VulkanContext>,
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
        Ok(Self {
            context: Arc::new(VulkanContext {
                _entry: entry,
                instance,
            }),
        })
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
                            modes: BTreeSet::from([PlacementMode::Row]),
                            layer_families: BTreeSet::new(),
                            tensor_types: BTreeSet::from([GGMLType::Q8_0]),
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
            .adapters()?
            .into_iter()
            .map(|adapter| adapter.descriptor)
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
        let adapter = self
            .adapters()?
            .into_iter()
            .find(|adapter| adapter.descriptor.id == descriptor.id)
            .ok_or_else(|| BackendError::DeviceUnavailable {
                device: descriptor.id.clone(),
            })?;
        VulkanSession::open(Arc::clone(&self.context), adapter, plan, catalog)
            .map(|session| Box::new(session) as Box<dyn DeviceSession>)
    }
}

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

struct ProgramResource {
    plan: ProgramPlan,
    command: vk::CommandBuffer,
    fence: vk::Fence,
    dispatches: Vec<Dispatch>,
    n_in: u32,
    output_stride: u32,
    mode: u32,
}

struct Pending {
    id: FenceId,
    program: ProgramId,
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
    command_pool: vk::CommandPool,
    slots: BTreeMap<SlotId, SlotPlan>,
    programs: BTreeMap<ProgramId, ProgramResource>,
    next_fence: u64,
    pending: Option<Pending>,
    stats: SessionStats,
}

impl VulkanSession {
    fn open(
        context: Arc<VulkanContext>,
        adapter: AdapterInfo,
        plan: &DevicePlan,
        catalog: Arc<TensorCatalog>,
    ) -> Result<Self, BackendError> {
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
        let queue = unsafe { device.get_device_queue(adapter.queue_family, 0) };

        let resident_size = align_up(plan.memory.resident_bytes.max(4), 4);
        let arena_size = plan
            .slots
            .iter()
            .try_fold(0_u64, |end, slot| {
                slot.arena_offset
                    .checked_add(slot.byte_len)
                    .map(|slot_end| end.max(slot_end))
            })
            .ok_or(BackendError::InvalidHandle)?
            .max(4);
        let staging_size = resident_size.max(arena_size).max(4);
        if resident_size > adapter.descriptor.max_allocation_bytes
            || arena_size > adapter.descriptor.max_allocation_bytes
            || staging_size > adapter.descriptor.max_allocation_bytes
        {
            unsafe { device.destroy_device(None) };
            return Err(allocation(
                &adapter.descriptor,
                "buffer exceeds adapter allocation limit",
            ));
        }

        let resident = create_buffer(
            &device,
            &adapter,
            resident_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        let arena = match create_buffer(
            &device,
            &adapter,
            arena_size,
            vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        ) {
            Ok(value) => value,
            Err(error) => {
                unsafe {
                    destroy_buffer(&device, &resident);
                    device.destroy_device(None);
                }
                return Err(error);
            }
        };
        let staging = match create_buffer(
            &device,
            &adapter,
            staging_size,
            vk::BufferUsageFlags::TRANSFER_SRC | vk::BufferUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::HOST_VISIBLE,
        ) {
            Ok(value) => value,
            Err(error) => {
                unsafe {
                    destroy_buffer(&device, &arena);
                    destroy_buffer(&device, &resident);
                    device.destroy_device(None);
                }
                return Err(error);
            }
        };
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

        let command_pool = unsafe {
            device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(adapter.queue_family)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )
        }
        .map_err(|error| allocation(&adapter.descriptor, format!("command pool: {error:?}")))?;

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
        let push_range = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .offset(0)
            .size(32)];
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
        let main = CStr::from_bytes_with_nul(b"main\0").unwrap();
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(shader)
            .name(main);
        let pipeline_info = [vk::ComputePipelineCreateInfo::default()
            .stage(stage)
            .layout(pipeline_layout)];
        let pipeline = unsafe {
            device.create_compute_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
        }
        .map_err(|(_, error)| {
            pipeline_error(&adapter.descriptor, format!("compute pipeline: {error:?}"))
        })?[0];
        unsafe { device.destroy_shader_module(shader, None) };

        let slots = validate_slots(plan, adapter.max_storage_buffer_range)?;
        let specs = build_program_specs(plan, &catalog, &adapter)?;
        let descriptor_count = specs.iter().map(|spec| spec.chunks.len()).sum::<usize>();
        if specs.is_empty() || descriptor_count == 0 {
            return Err(BackendError::InvalidHandle);
        }
        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count((descriptor_count * 3) as u32)];
        let descriptor_pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(descriptor_count as u32)
                    .pool_sizes(&pool_sizes),
                None,
            )
        }
        .map_err(|error| {
            pipeline_error(&adapter.descriptor, format!("descriptor pool: {error:?}"))
        })?;
        let layouts = vec![descriptor_set_layout; descriptor_count];
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
                    .command_buffer_count(specs.len() as u32),
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
                let infos = [
                    vk::DescriptorBufferInfo::default()
                        .buffer(resident.buffer)
                        .offset(chunk.descriptor_offset)
                        .range(chunk.descriptor_range),
                    vk::DescriptorBufferInfo::default()
                        .buffer(arena.buffer)
                        .offset(input.arena_offset)
                        .range(input.byte_len),
                    vk::DescriptorBufferInfo::default()
                        .buffer(arena.buffer)
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
                unsafe { device.update_descriptor_sets(&writes, &[]) };
                dispatches.push(Dispatch {
                    set,
                    local_rows: chunk.local_rows,
                    global_row_start: chunk.global_row_start,
                    output_row_start: chunk.output_row_start,
                    weight_byte_bias: chunk.weight_byte_bias,
                });
            }
            let fence = unsafe {
                device.create_fence(
                    &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                    None,
                )
            }
            .map_err(|error| pipeline_error(&adapter.descriptor, format!("fence: {error:?}")))?;
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
                    },
                )
                .is_some()
            {
                return Err(BackendError::InvalidHandle);
            }
        }

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
            command_pool,
            slots,
            programs,
            next_fence: 1,
            pending: None,
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
        if self.pending.is_some() {
            Err(BackendError::Submission {
                device: self.descriptor.id.clone(),
                message: "Vulkan work is pending".into(),
            })
        } else {
            Ok(())
        }
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
        self.require_idle()?;
        let resource = self
            .programs
            .get(&program)
            .ok_or(BackendError::InvalidHandle)?;
        let batch_capacity = match &resource.plan.kind {
            ProgramKind::Q8Rows { batch_capacity, .. } => *batch_capacity,
            ProgramKind::EmbeddingRows { .. } => {
                (self.slots[&resource.plan.output].byte_len / 4 / u64::from(resource.n_in)) as u32
            }
            _ => return Err(BackendError::InvalidHandle),
        };
        if params.token_count == 0 || params.token_count > batch_capacity {
            return Err(BackendError::InvalidHandle);
        }
        let input = &self.slots[&resource.plan.input];
        let output = &self.slots[&resource.plan.output];
        let input_bytes = if resource.mode == 0 {
            u64::from(params.token_count) * u64::from(resource.n_in) * 4
        } else {
            if params.token_ids.len() != params.token_count as usize {
                return Err(BackendError::InvalidHandle);
            }
            let bytes = params.token_count as usize * size_of::<u32>();
            unsafe {
                std::ptr::copy_nonoverlapping(
                    params.token_ids.as_ptr() as *const u8,
                    (self.staging_ptr + input.arena_offset as usize) as *mut u8,
                    bytes,
                )
            };
            self.flush_staging(input.arena_offset, bytes as u64)?;
            self.stats.activation_h2d_bytes += bytes as u64;
            bytes as u64
        };
        let output_bytes = u64::from(params.token_count) * u64::from(resource.output_stride) * 4;
        if input_bytes > input.byte_len || output_bytes > output.byte_len {
            return Err(BackendError::InvalidHandle);
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
                self.device.reset_fences(&[fence])?;
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
                let commands = [command];
                let submit = [vk::SubmitInfo::default().command_buffers(&commands)];
                self.device.queue_submit(self.queue, &submit, fence)?;
                Ok(())
            }
        })();
        record_result.map_err(|error| {
            submission(&self.descriptor, format!("record or submit: {error:?}"))
        })?;
        let id = FenceId(self.next_fence);
        self.next_fence += 1;
        self.pending = Some(Pending { id, program });
        self.stats.submissions += 1;
        Ok(id)
    }

    fn wait(&mut self, fence: FenceId) -> Result<(), BackendError> {
        let pending = self
            .pending
            .as_ref()
            .filter(|pending| pending.id == fence)
            .ok_or(BackendError::InvalidHandle)?;
        let vk_fence = self.programs[&pending.program].fence;
        unsafe { self.device.wait_for_fences(&[vk_fence], true, u64::MAX) }
            .map_err(|error| submission(&self.descriptor, format!("wait fence: {error:?}")))?;
        self.pending = None;
        self.stats.host_waits += 1;
        Ok(())
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
        if let Some(pending) = &self.pending {
            let fence = self.programs[&pending.program].fence;
            let _ = unsafe { self.device.wait_for_fences(&[fence], true, u64::MAX) };
        }
        unsafe {
            self.device.unmap_memory(self.staging.memory);
            for program in self.programs.values() {
                self.device.destroy_fence(program.fence, None);
            }
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_pipeline(self.pipeline, None);
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
}

fn build_program_specs(
    plan: &DevicePlan,
    catalog: &TensorCatalog,
    adapter: &AdapterInfo,
) -> Result<Vec<ProgramSpec>, BackendError> {
    plan.programs
        .iter()
        .map(|program| {
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
            {
                return Err(BackendError::InvalidHandle);
            }
            let resident = plan
                .tensors
                .iter()
                .find(|resident| resident.tensor == tensor && resident.rows == rows)
                .ok_or(BackendError::InvalidHandle)?;
            let row_bytes = u64::from(n_in) / Q8_BLOCK_ELEMENTS * Q8_BLOCK_BYTES;
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
            })
        })
        .collect()
}

fn row_chunks(
    resident: &ResidentTensorPlan,
    row_bytes: u64,
    max_range: u64,
    alignment: u64,
) -> Result<Vec<ChunkSpec>, BackendError> {
    let total_rows = resident.rows.end - resident.rows.start;
    let mut chunks = Vec::new();
    let mut local_start = 0_u32;
    while local_start < total_rows {
        let byte_start = resident.arena_offset + u64::from(local_start) * row_bytes;
        let descriptor_offset = align_down(byte_start, alignment);
        let bias = byte_start - descriptor_offset;
        let available = max_range
            .checked_sub(bias)
            .ok_or(BackendError::InvalidHandle)?;
        let mut rows = (available / row_bytes).min(u64::from(total_rows - local_start));
        while rows != 0 && align_up(bias + rows * row_bytes, 4) > max_range {
            rows -= 1;
        }
        if rows == 0 || bias > u64::from(u32::MAX) {
            return Err(BackendError::InvalidHandle);
        }
        chunks.push(ChunkSpec {
            descriptor_offset,
            descriptor_range: align_up(bias + rows * row_bytes, 4),
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
    let mut slots = BTreeMap::new();
    for (index, slot) in plan.slots.iter().enumerate() {
        if slot.id.0 as usize != index
            || slot.byte_len == 0
            || slot.byte_len > max_storage_buffer_range
            || slot.arena_offset % plan.descriptor.buffer_alignment != 0
        {
            return Err(BackendError::InvalidHandle);
        }
        slots.insert(slot.id, slot.clone());
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
        let source = catalog
            .bytes(tensor.tensor)
            .map_err(|error| BackendError::Upload {
                device: descriptor.id.clone(),
                message: error.to_string(),
            })?;
        let bytes = source
            .get(tensor.source_bytes.start as usize..tensor.source_bytes.end as usize)
            .ok_or(BackendError::InvalidHandle)?;
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
    if let Err(error) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
        unsafe {
            device.free_memory(memory, None);
            device.destroy_buffer(buffer, None);
        }
        return Err(allocation(
            &adapter.descriptor,
            format!("bind buffer: {error:?}"),
        ));
    }
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
