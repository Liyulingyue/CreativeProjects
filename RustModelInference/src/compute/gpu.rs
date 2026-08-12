use super::device::{ComputeDevice, ComputeError, DeviceType, OpType, Result, WorkSpec};

#[cfg(feature = "gpu")]
use ash::vk;

pub struct GpuDevice {
    name: String,
    available: bool,
    #[cfg(feature = "gpu")]
    entry: Option<ash::Entry>,
    #[cfg(feature = "gpu")]
    instance: Option<ash::Instance>,
    #[cfg(feature = "gpu")]
    physical_device: Option<ash::vk::PhysicalDevice>,
    #[cfg(feature = "gpu")]
    device: Option<ash::Device>,
    #[cfg(feature = "gpu")]
    queue: Option<ash::vk::Queue>,
    #[cfg(feature = "gpu")]
    queue_family: Option<u32>,
    #[cfg(feature = "gpu")]
    pipeline: Option<ash::vk::Pipeline>,
    #[cfg(feature = "gpu")]
    pipeline_layout: Option<ash::vk::PipelineLayout>,
    #[cfg(feature = "gpu")]
    descriptor_set_layout: Option<ash::vk::DescriptorSetLayout>,
    #[cfg(feature = "gpu")]
    descriptor_pool: Option<ash::vk::DescriptorPool>,
    #[cfg(feature = "gpu")]
    descriptor_sets: Vec<ash::vk::DescriptorSet>,
}

impl GpuDevice {
    pub fn new() -> Result<Self> {
        #[cfg(feature = "gpu")]
        {
            match Self::try_init_vulkan() {
                Ok((entry, instance, physical_device, device, queue, queue_family, pipeline, pipeline_layout, dsl, dp, ds)) => {
                    Ok(Self {
                        name: "GPU-Vulkan".to_string(),
                        available: true,
                        entry: Some(entry),
                        instance: Some(instance),
                        physical_device: Some(physical_device),
                        device: Some(device),
                        queue: Some(queue),
                        queue_family: Some(queue_family),
                        pipeline: Some(pipeline),
                        pipeline_layout: Some(pipeline_layout),
                        descriptor_set_layout: Some(dsl),
                        descriptor_pool: Some(dp),
                        descriptor_sets: ds,
                    })
                }
                Err(e) => {
                    eprintln!("GPU init failed: {:?}", e);
                    Ok(Self::dummy())
                }
            }
        }
        #[cfg(not(feature = "gpu"))]
        {
            Ok(Self::dummy())
        }
    }

    fn dummy() -> Self {
        Self {
            name: "GPU-None".to_string(),
            available: false,
            #[cfg(feature = "gpu")]
            entry: None,
            #[cfg(feature = "gpu")]
            instance: None,
            #[cfg(feature = "gpu")]
            physical_device: None,
            #[cfg(feature = "gpu")]
            device: None,
            #[cfg(feature = "gpu")]
            queue: None,
            #[cfg(feature = "gpu")]
            queue_family: None,
            #[cfg(feature = "gpu")]
            pipeline: None,
            #[cfg(feature = "gpu")]
            pipeline_layout: None,
            #[cfg(feature = "gpu")]
            descriptor_set_layout: None,
            #[cfg(feature = "gpu")]
            descriptor_pool: None,
            #[cfg(feature = "gpu")]
            descriptor_sets: Vec::new(),
        }
    }

    #[cfg(feature = "gpu")]
    fn try_init_vulkan() -> Result<(
        ash::Entry,
        ash::Instance,
        ash::vk::PhysicalDevice,
        ash::Device,
        ash::vk::Queue,
        u32,
        ash::vk::Pipeline,
        ash::vk::PipelineLayout,
        ash::vk::DescriptorSetLayout,
        ash::vk::DescriptorPool,
        Vec<ash::vk::DescriptorSet>,
    )> {
        let entry = unsafe { ash::Entry::load() }
            .map_err(|e| ComputeError::DeviceNotAvailable(format!("Vulkan load failed: {:?}", e)))?;

        let app_info = vk::ApplicationInfo {
            api_version: vk::API_VERSION_1_0,
            ..Default::default()
        };

        let create_info = vk::InstanceCreateInfo {
            p_application_info: &app_info,
            enabled_layer_count: 0,
            pp_enabled_layer_names: std::ptr::null(),
            enabled_extension_count: 0,
            pp_enabled_extension_names: std::ptr::null(),
            ..Default::default()
        };

        let instance = unsafe { entry.create_instance(&create_info, None) }
            .map_err(|e| ComputeError::DeviceNotAvailable(format!("Instance create: {:?}", e)))?;

        let physical_devices = unsafe { instance.enumerate_physical_devices() }
            .map_err(|e| ComputeError::DeviceNotAvailable(format!("Enumerate devices: {:?}", e)))?;

        let physical_device = physical_devices
            .into_iter()
            .next()
            .ok_or_else(|| ComputeError::DeviceNotAvailable("No Vulkan device found".to_string()))?;

        let queue_family = unsafe {
            let props = instance.get_physical_device_queue_family_properties(physical_device);
            props
                .iter()
                .position(|q| q.queue_flags.contains(vk::QueueFlags::COMPUTE))
                .ok_or_else(|| ComputeError::DeviceNotAvailable("No compute queue".to_string()))? as u32
        };

        let priorities = [1.0f32];
        let queue_info = vk::DeviceQueueCreateInfo {
            queue_family_index: queue_family,
            queue_count: 1,
            p_queue_priorities: priorities.as_ptr(),
            ..Default::default()
        };

        let device_create_info = vk::DeviceCreateInfo {
            queue_create_info_count: 1,
            p_queue_create_infos: &queue_info,
            enabled_extension_count: 0,
            pp_enabled_extension_names: std::ptr::null(),
            p_enabled_features: &Default::default(),
            ..Default::default()
        };

        let device = unsafe { instance.create_device(physical_device, &device_create_info, None) }
            .map_err(|e| ComputeError::DeviceNotAvailable(format!("Device create: {:?}", e)))?;

        let queue = unsafe { device.get_device_queue(queue_family, 0) };

        let shader_code = include_bytes!("kernels/matmul_q8_0.spv");
        let module = unsafe {
            device
                .create_shader_module(
                    &vk::ShaderModuleCreateInfo {
                        code_size: shader_code.len(),
                        p_code: shader_code.as_ptr() as *const u32,
                        ..Default::default()
                    },
                    None,
                )
                .map_err(|e| ComputeError::DeviceNotAvailable(format!("Shader create: {:?}", e)))?
        };

        let entry_name = CStr::from_bytes_with_nul(b"main\0").unwrap();
        let stage = vk::PipelineShaderStageCreateInfo {
            stage: vk::ShaderStageFlags::COMPUTE,
            module,
            p_name: entry_name.as_ptr(),
            ..Default::default()
        };

        let push_constant_ranges = [vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::COMPUTE,
            offset: 0,
            size: 12,
        }];

        let bindings = [
            vk::DescriptorSetLayoutBinding {
                binding: 0,
                descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 1,
                stage_flags: vk::ShaderStageFlags::COMPUTE,
                ..Default::default()
            },
            vk::DescriptorSetLayoutBinding {
                binding: 1,
                descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 1,
                stage_flags: vk::ShaderStageFlags::COMPUTE,
                ..Default::default()
            },
            vk::DescriptorSetLayoutBinding {
                binding: 2,
                descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 1,
                stage_flags: vk::ShaderStageFlags::COMPUTE,
                ..Default::default()
            },
            vk::DescriptorSetLayoutBinding {
                binding: 3,
                descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 1,
                stage_flags: vk::ShaderStageFlags::COMPUTE,
                ..Default::default()
            },
        ];

        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo {
                        binding_count: 4,
                        p_bindings: bindings.as_ptr(),
                        ..Default::default()
                    },
                    None,
                )
                .map_err(|e| ComputeError::DeviceNotAvailable(format!("DSL create: {:?}", e)))?
        };

        let pipeline_layout = unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo {
                        set_layout_count: 1,
                        p_set_layouts: &descriptor_set_layout,
                        push_constant_range_count: 1,
                        p_push_constant_ranges: push_constant_ranges.as_ptr(),
                        ..Default::default()
                    },
                    None,
                )
                .map_err(|e| ComputeError::DeviceNotAvailable(format!("PL create: {:?}", e)))?
        };

        let pipeline_info = vk::ComputePipelineCreateInfo {
            stage,
            layout: pipeline_layout,
            ..Default::default()
        };

        let pipelines = unsafe {
            device.create_compute_pipelines(
                vk::PipelineCache::null(),
                &[pipeline_info],
                None,
            )
        }
        .map_err(|(pipelines, e)| ComputeError::DeviceNotAvailable(format!("Pipeline create: {:?}", e)))?;

        let pipeline = pipelines[0];

        unsafe { device.destroy_shader_module(module, None) };

        let descriptor_pool = unsafe {
            device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo {
                        max_sets: 1,
                        pool_size_count: 1,
                        p_pool_sizes: &vk::DescriptorPoolSize {
                            ty: vk::DescriptorType::STORAGE_BUFFER,
                            descriptor_count: 4,
                        },
                        ..Default::default()
                    },
                    None,
                )
                .map_err(|e| ComputeError::DeviceNotAvailable(format!("DP create: {:?}", e)))?
        };

        let descriptor_sets = unsafe {
            device
                .allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo {
                    descriptor_pool,
                    p_set_layouts: &descriptor_set_layout,
                    descriptor_set_count: 1,
                    ..Default::default()
                })
                .map_err(|e| ComputeError::DeviceNotAvailable(format!("DS alloc: {:?}", e)))?
        };

        Ok((
            entry,
            instance,
            physical_device,
            device,
            queue,
            queue_family,
            pipeline,
            pipeline_layout,
            descriptor_set_layout,
            descriptor_pool,
            descriptor_sets,
        ))
    }
}

impl ComputeDevice for GpuDevice {
    fn device_type(&self) -> DeviceType {
        DeviceType::Gpu
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_available(&self) -> bool {
        self.available
    }

    fn supports(&self, op: OpType) -> bool {
        self.available && matches!(op, OpType::MatMulQ8 | OpType::MatMulF32 | OpType::MatMulF16)
    }

    #[cfg(feature = "gpu")]
    fn execute_matmul_q8(&self, spec: &WorkSpec) -> Result<Vec<f32>> {
        let device = self.device.as_ref().ok_or_else(|| {
            ComputeError::DeviceNotAvailable("GPU device not initialized".to_string())
        })?;

        let n_in = spec.n_in;
        let n_out = spec.n_out;

        let weight_packed: Vec<u32> = spec
            .weight
            .chunks(4)
            .map(|c| {
                u32::from_le_bytes([
                    c[0],
                    c.get(1).copied().unwrap_or(0),
                    c.get(2).copied().unwrap_or(0),
                    c.get(3).copied().unwrap_or(0),
                ])
            })
            .collect();
        let weight_size = weight_packed.len() * 4;
        let input_size = spec.input.len();
        let scales_size = spec.scales.len() * 4;
        let output_size = n_out * 4;

        let (weight_buf, weight_mem) = self.create_buffer(device, weight_size as u64)?;
        let (input_buf, input_mem) = self.create_buffer(device, input_size as u64)?;
        let (scales_buf, scales_mem) = self.create_buffer(device, scales_size as u64)?;
        let (output_buf, output_mem) = self.create_buffer(device, output_size as u64)?;

        self.copy_to_device(
            device,
            weight_mem,
            unsafe { std::slice::from_raw_parts(weight_packed.as_ptr() as *const u8, weight_size) },
        )?;
        self.copy_to_device(device, input_mem, &spec.input)?;
        self.copy_to_device(
            device,
            scales_mem,
            unsafe { std::slice::from_raw_parts(spec.scales.as_ptr() as *const u8, scales_size) },
        )?;

        let buffer_infos = [
            vk::DescriptorBufferInfo {
                buffer: weight_buf,
                offset: 0,
                range: weight_size as u64,
            },
            vk::DescriptorBufferInfo {
                buffer: input_buf,
                offset: 0,
                range: input_size as u64,
            },
            vk::DescriptorBufferInfo {
                buffer: scales_buf,
                offset: 0,
                range: scales_size as u64,
            },
            vk::DescriptorBufferInfo {
                buffer: output_buf,
                offset: 0,
                range: output_size as u64,
            },
        ];

        let ds = &self.descriptor_sets[0];
        let writes = [
            vk::WriteDescriptorSet {
                dst_set: *ds,
                dst_binding: 0,
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                p_buffer_info: &buffer_infos[0],
                ..Default::default()
            },
            vk::WriteDescriptorSet {
                dst_set: *ds,
                dst_binding: 1,
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                p_buffer_info: &buffer_infos[1],
                ..Default::default()
            },
            vk::WriteDescriptorSet {
                dst_set: *ds,
                dst_binding: 2,
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                p_buffer_info: &buffer_infos[2],
                ..Default::default()
            },
            vk::WriteDescriptorSet {
                dst_set: *ds,
                dst_binding: 3,
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                p_buffer_info: &buffer_infos[3],
                ..Default::default()
            },
        ];

        unsafe { device.update_descriptor_sets(&writes, &[]) };

        let command_pool = unsafe {
            device
                .create_command_pool(
                    &vk::CommandPoolCreateInfo {
                        queue_family_index: self.queue_family.unwrap(),
                        ..Default::default()
                    },
                    None,
                )
                .map_err(|e| ComputeError::DeviceNotAvailable(format!("Command pool: {:?}", e)))?
        };

        let command_buffer = unsafe {
            device
                .allocate_command_buffers(&vk::CommandBufferAllocateInfo {
                    command_pool,
                    level: vk::CommandBufferLevel::PRIMARY,
                    command_buffer_count: 1,
                    ..Default::default()
                })
                .map_err(|e| ComputeError::DeviceNotAvailable(format!("CB alloc: {:?}", e)))?[0]
        };

        unsafe {
            device
                .begin_command_buffer(
                    command_buffer,
                    &vk::CommandBufferBeginInfo {
                        flags: vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT,
                        ..Default::default()
                    },
                )
                .map_err(|e| ComputeError::DeviceNotAvailable(format!("CB begin: {:?}", e)))?
        };

        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::COMPUTE, self.pipeline.unwrap())
        };
        unsafe {
            device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline_layout.unwrap(),
                0,
                &[*ds],
                &[],
            )
        };

        let block_size = 32;
        let n_blocks = (n_out + block_size - 1) / block_size;
        let push_constants_u32 = [n_in as u32, n_out as u32, n_blocks as u32];
        let push_constants =
            unsafe { std::slice::from_raw_parts(push_constants_u32.as_ptr() as *const u8, 12) };
        unsafe {
            device.cmd_push_constants(
                command_buffer,
                self.pipeline_layout.unwrap(),
                vk::ShaderStageFlags::COMPUTE,
                0,
                push_constants,
            )
        };

        unsafe { device.cmd_dispatch(command_buffer, n_blocks as u32, 1, 1) };

        unsafe { device.end_command_buffer(command_buffer) }
            .map_err(|e| ComputeError::DeviceNotAvailable(format!("CB end: {:?}", e)))?;

        let submit_info = vk::SubmitInfo {
            command_buffer_count: 1,
            p_command_buffers: &command_buffer,
            ..Default::default()
        };

        unsafe { device.queue_submit(self.queue.unwrap(), &[submit_info], vk::Fence::null()) }
            .map_err(|e| ComputeError::DeviceNotAvailable(format!("Queue submit: {:?}", e)))?;

        unsafe { device.queue_wait_idle(self.queue.unwrap()) }
            .map_err(|e| ComputeError::DeviceNotAvailable(format!("Queue wait: {:?}", e)))?;

        let mut output_data = vec![0u8; output_size];
        self.copy_from_device(device, output_mem, &mut output_data)?;

        let result: Vec<f32> = output_data
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();

        unsafe {
            device.destroy_command_pool(command_pool, None);
            device.destroy_buffer(weight_buf, None);
            device.destroy_buffer(input_buf, None);
            device.destroy_buffer(scales_buf, None);
            device.destroy_buffer(output_buf, None);
            device.free_memory(weight_mem, None);
            device.free_memory(input_mem, None);
            device.free_memory(scales_mem, None);
            device.free_memory(output_mem, None);
        }

        Ok(result)
    }

    #[cfg(not(feature = "gpu"))]
    fn execute_matmul_q8(&self, _spec: &WorkSpec) -> Result<Vec<f32>> {
        Err(ComputeError::DeviceNotAvailable(
            "GPU not available".to_string(),
        ))
    }

    fn sync(&self) {
        #[cfg(feature = "gpu")]
        if let Some(ref device) = self.device {
            let _ = device.device_wait_idle();
        }
    }
}

#[cfg(feature = "gpu")]
impl GpuDevice {
    fn create_buffer(&self, device: &ash::Device, size: u64) -> Result<(vk::Buffer, vk::DeviceMemory)> {
        let buffer = unsafe {
            device
                .create_buffer(
                    &vk::BufferCreateInfo {
                        size,
                        usage: vk::BufferUsageFlags::STORAGE_BUFFER,
                        sharing_mode: vk::SharingMode::EXCLUSIVE,
                        ..Default::default()
                    },
                    None,
                )
                .map_err(|e| ComputeError::DeviceNotAvailable(format!("Buffer create: {:?}", e)))?
        };

        let mem_reqs = unsafe { device.get_buffer_memory_requirements(buffer) };
        let instance = self.entry.as_ref().unwrap();
        let pd = self.physical_device.unwrap();
        let mem_props = unsafe { instance.get_physical_device_memory_properties(pd) };

        let memory_type_index = (0..mem_props.memory_type_count as usize)
            .position(|i| {
                let ty = mem_props.memory_types[i];
                mem_reqs.memory_type_bits & (1 << i) != 0
                    && ty.property_flags
                        .contains(vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT)
            })
            .ok_or_else(|| ComputeError::DeviceNotAvailable("No suitable memory type".to_string()))?
            as u32;

        let mem = unsafe {
            device
                .allocate_memory(
                    &vk::MemoryAllocateInfo {
                        allocation_size: mem_reqs.size,
                        memory_type_index,
                        ..Default::default()
                    },
                    None,
                )
                .map_err(|e| ComputeError::DeviceNotAvailable(format!("Alloc memory: {:?}", e)))?
        };

        unsafe { device.bind_buffer_memory(buffer, mem, 0) }
            .map_err(|e| ComputeError::DeviceNotAvailable(format!("Bind memory: {:?}", e)))?;

        Ok((buffer, mem))
    }

    fn copy_to_device(&self, device: &ash::Device, memory: vk::DeviceMemory, data: &[u8]) -> Result<()> {
        let ptr = unsafe {
            device
                .map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())
                .map_err(|e| ComputeError::DeviceNotAvailable(format!("Map memory: {:?}", e)))?
        };

        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len()) };
        unsafe { device.unmap_memory(memory) };
        Ok(())
    }

    fn copy_from_device(&self, device: &ash::Device, memory: vk::DeviceMemory, data: &mut [u8]) -> Result<()> {
        let ptr = unsafe {
            device
                .map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())
                .map_err(|e| ComputeError::DeviceNotAvailable(format!("Map memory: {:?}", e)))?
        };

        unsafe { std::ptr::copy_nonoverlapping(ptr as *const u8, data.as_mut_ptr(), data.len()) };
        unsafe { device.unmap_memory(memory) };
        Ok(())
    }
}

impl Drop for GpuDevice {
    fn drop(&mut self) {
        #[cfg(feature = "gpu")]
        if self.available {
            if let (Some(device), Some(pipeline), Some(pl), Some(dsl), Some(dp)) = (
                &self.device,
                &self.pipeline,
                &self.pipeline_layout,
                &self.descriptor_set_layout,
                &self.descriptor_pool,
            ) {
                unsafe {
                    device.destroy_pipeline(*pipeline, None);
                    device.destroy_pipeline_layout(*pl, None);
                    device.destroy_descriptor_set_layout(*dsl, None);
                    device.destroy_descriptor_pool(*dp, None);
                    device.destroy_device(None);
                }
            }
            if let Some(instance) = &self.instance {
                unsafe { instance.destroy_instance(None) };
            }
        }
    }
}
