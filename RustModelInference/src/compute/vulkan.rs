use super::{ComputeDevice, Op, OpParams, ComputeError, Result, OpType};
use ash::vk;
use std::ffi::CStr;
use std::os::raw::c_char;

pub struct VulkanDevice {
    _entry: ash::Entry,
    _instance: ash::Instance,
    _physical_device: ash::vk::PhysicalDevice,
    device: ash::Device,
    queue: ash::vk::Queue,
    queue_family: u32,
    pipeline: ash::vk::Pipeline,
    pipeline_layout: ash::vk::PipelineLayout,
    descriptor_set_layout: ash::vk::DescriptorSetLayout,
    descriptor_pool: ash::vk::DescriptorPool,
    descriptor_sets: Vec<ash::vk::DescriptorSet>,
    available: bool,
}

impl VulkanDevice {
    pub fn new() -> Result<Self> {
        let entry = match unsafe { ash::Entry::load() } {
            Ok(e) => e,
            Err(e) => return Err(ComputeError::VulkanError(format!("Failed to load Vulkan: {:?}", e))),
        };

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
            .map_err(|e| ComputeError::VulkanError(format!("Failed to create instance: {:?}", e)))?;

        let physical_devices = unsafe { instance.enumerate_physical_devices() }
            .map_err(|e| ComputeError::VulkanError(format!("Failed to enumerate devices: {:?}", e)))?;

        let physical_device = physical_devices.into_iter()
            .next()
            .ok_or_else(|| ComputeError::VulkanError("No Vulkan device found".to_string()))?;

        let queue_family = unsafe {
            let props = instance.get_physical_device_queue_family_properties(physical_device);
            props.iter()
                .position(|q| q.queue_flags.contains(vk::QueueFlags::COMPUTE))
                .ok_or_else(|| ComputeError::VulkanError("No compute queue".to_string()))? as u32
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
            .map_err(|e| ComputeError::VulkanError(format!("Failed to create device: {:?}", e)))?;

        let queue = unsafe { device.get_device_queue(queue_family, 0) };

        let shader_code = include_bytes!("kernels/matmul_q8_0.spv");

        let module = unsafe { device.create_shader_module(&vk::ShaderModuleCreateInfo {
            code_size: shader_code.len(),
            p_code: shader_code.as_ptr() as *const u32,
            ..Default::default()
        }, None) }.map_err(|e| ComputeError::VulkanError(format!("Failed to create shader: {:?}", e)))?;

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

        let descriptor_set_layout = unsafe { device.create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo {
            binding_count: 4,
            p_bindings: bindings.as_ptr(),
            ..Default::default()
        }, None) }.map_err(|e| ComputeError::VulkanError(format!("Failed to create DSL: {:?}", e)))?;

        let pipeline_layout = unsafe { device.create_pipeline_layout(&vk::PipelineLayoutCreateInfo {
            set_layout_count: 1,
            p_set_layouts: &descriptor_set_layout,
            push_constant_range_count: 1,
            p_push_constant_ranges: push_constant_ranges.as_ptr(),
            ..Default::default()
        }, None) }.map_err(|e| ComputeError::VulkanError(format!("Failed to create PL: {:?}", e)))?;

        let pipeline_info = vk::ComputePipelineCreateInfo {
            stage,
            layout: pipeline_layout,
            ..Default::default()
        };

        let pipelines = unsafe { device.create_compute_pipelines(
            vk::PipelineCache::null(),
            &[pipeline_info],
            None
        ) }.map_err(|(pipelines, e)| ComputeError::VulkanError(format!("Pipeline create error: {:?}", e)))?;
        let pipeline = pipelines[0];

        unsafe { device.destroy_shader_module(module, None) };

        let descriptor_pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 4,
        }];
        let descriptor_pool = unsafe { device.create_descriptor_pool(&vk::DescriptorPoolCreateInfo {
            max_sets: 1,
            pool_size_count: 1,
            p_pool_sizes: descriptor_pool_sizes.as_ptr(),
            ..Default::default()
        }, None) }.map_err(|e| ComputeError::VulkanError(format!("Failed to create DP: {:?}", e)))?;

        let descriptor_sets = unsafe { device.allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo {
            descriptor_pool,
            p_set_layouts: &descriptor_set_layout,
            descriptor_set_count: 1,
            ..Default::default()
        }) }.map_err(|e| ComputeError::VulkanError(format!("Failed to alloc DS: {:?}", e)))?;

        Ok(Self {
            _entry: entry,
            _instance: instance,
            _physical_device: physical_device,
            device,
            queue,
            queue_family,
            pipeline,
            pipeline_layout,
            descriptor_set_layout,
            descriptor_pool,
            descriptor_sets,
            available: true,
        })
    }

    fn dispatch_matmul_gpu(&mut self, weight: &[u8], input: &[u8], scales: &[f32],
                          output: &mut [f32], n_in: usize, n_out: usize) -> Result<()> {
        let block_size = 32;
        let n_blocks = (n_out + block_size - 1) / block_size;

        let weight_packed: Vec<u32> = weight.chunks(4)
            .map(|c| u32::from_le_bytes([c[0], c.get(1).copied().unwrap_or(0), c.get(2).copied().unwrap_or(0), c.get(3).copied().unwrap_or(0)]))
            .collect();
        let weight_size = weight_packed.len() * 4;
        let input_packed: Vec<u32> = input.chunks(4)
            .map(|c| u32::from_le_bytes([c[0], c.get(1).copied().unwrap_or(0), c.get(2).copied().unwrap_or(0), c.get(3).copied().unwrap_or(0)]))
            .collect();
        let input_size = input_packed.len() * 4;
        let scales_size = scales.len() * 4;
        let output_size = output.len() * 4;

        let (weight_buf, weight_mem) = self.create_buffer(weight_size as vk::DeviceSize)?;
        let (input_buf, input_mem) = self.create_buffer(input_size as vk::DeviceSize)?;
        let (scales_buf, scales_mem) = self.create_buffer(scales_size as vk::DeviceSize)?;
        let (output_buf, output_mem) = self.create_buffer(output_size as vk::DeviceSize)?;

        self.copy_to_device(weight_mem, unsafe { std::slice::from_raw_parts(weight_packed.as_ptr() as *const u8, weight_size) })?;
        self.copy_to_device(input_mem, unsafe { std::slice::from_raw_parts(input_packed.as_ptr() as *const u8, input_size) })?;
        self.copy_to_device(scales_mem, unsafe { std::slice::from_raw_parts(scales.as_ptr() as *const u8, scales_size) })?;

        let buffer_infos = [
            vk::DescriptorBufferInfo { buffer: weight_buf, offset: 0, range: weight_size as vk::DeviceSize },
            vk::DescriptorBufferInfo { buffer: input_buf, offset: 0, range: input_size as vk::DeviceSize },
            vk::DescriptorBufferInfo { buffer: scales_buf, offset: 0, range: scales_size as vk::DeviceSize },
            vk::DescriptorBufferInfo { buffer: output_buf, offset: 0, range: output_size as vk::DeviceSize },
        ];

        let writes = [
            vk::WriteDescriptorSet {
                dst_set: self.descriptor_sets[0],
                dst_binding: 0,
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                p_buffer_info: &buffer_infos[0],
                ..Default::default()
            },
            vk::WriteDescriptorSet {
                dst_set: self.descriptor_sets[0],
                dst_binding: 1,
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                p_buffer_info: &buffer_infos[1],
                ..Default::default()
            },
            vk::WriteDescriptorSet {
                dst_set: self.descriptor_sets[0],
                dst_binding: 2,
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                p_buffer_info: &buffer_infos[2],
                ..Default::default()
            },
            vk::WriteDescriptorSet {
                dst_set: self.descriptor_sets[0],
                dst_binding: 3,
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                p_buffer_info: &buffer_infos[3],
                ..Default::default()
            },
        ];

        unsafe { self.device.update_descriptor_sets(&writes, &[]) };

        let command_pool = unsafe { self.device.create_command_pool(&vk::CommandPoolCreateInfo {
            queue_family_index: self.queue_family,
            ..Default::default()
        }, None) }.map_err(|e| ComputeError::VulkanError(format!("Command pool: {:?}", e)))?;

        let command_buffer = unsafe { self.device.allocate_command_buffers(&vk::CommandBufferAllocateInfo {
            command_pool,
            level: vk::CommandBufferLevel::PRIMARY,
            command_buffer_count: 1,
            ..Default::default()
        }) }.map_err(|e| ComputeError::VulkanError(format!("Command buffer: {:?}", e)))?[0];

        unsafe { self.device.begin_command_buffer(command_buffer, &vk::CommandBufferBeginInfo {
            flags: vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT,
            ..Default::default()
        }) }.map_err(|e| ComputeError::VulkanError(format!("Begin command: {:?}", e)))?;

        unsafe { self.device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::COMPUTE, self.pipeline) };
        unsafe { self.device.cmd_bind_descriptor_sets(command_buffer, vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout, 0, &[self.descriptor_sets[0]], &[]) };

        let push_constants_u32 = [n_in as u32, n_out as u32, n_blocks as u32];
        let push_constants: &[u8] = unsafe { std::slice::from_raw_parts(push_constants_u32.as_ptr() as *const u8, 12) };
        unsafe { self.device.cmd_push_constants(command_buffer, self.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE, 0, push_constants) };

        unsafe { self.device.cmd_dispatch(command_buffer, n_blocks as u32, 1, 1) };

        unsafe { self.device.end_command_buffer(command_buffer) }
            .map_err(|e| ComputeError::VulkanError(format!("End command: {:?}", e)))?;

        let submit_info = vk::SubmitInfo {
            command_buffer_count: 1,
            p_command_buffers: &command_buffer,
            ..Default::default()
        };

        unsafe { self.device.queue_submit(self.queue, &[submit_info], vk::Fence::null()) }
            .map_err(|e| ComputeError::VulkanError(format!("Queue submit: {:?}", e)))?;

        unsafe { self.device.queue_wait_idle(self.queue) }
            .map_err(|e| ComputeError::VulkanError(format!("Queue wait: {:?}", e)))?;

        let mut output_data = vec![0u8; output_size];
        self.copy_from_device(output_mem, &mut output_data)?;
        output.copy_from_slice(unsafe { std::slice::from_raw_parts(output_data.as_ptr() as *const f32, output.len()) });

        unsafe {
            self.device.destroy_command_pool(command_pool, None);
            self.device.destroy_buffer(weight_buf, None);
            self.device.destroy_buffer(input_buf, None);
            self.device.destroy_buffer(scales_buf, None);
            self.device.destroy_buffer(output_buf, None);
            self.device.free_memory(weight_mem, None);
            self.device.free_memory(input_mem, None);
            self.device.free_memory(scales_mem, None);
            self.device.free_memory(output_mem, None);
        }

        Ok(())
    }

    fn create_buffer(&self, size: vk::DeviceSize) -> Result<(vk::Buffer, vk::DeviceMemory)> {
        let buffer = unsafe { self.device.create_buffer(&vk::BufferCreateInfo {
            size,
            usage: vk::BufferUsageFlags::STORAGE_BUFFER,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            ..Default::default()
        }, None) }.map_err(|e| ComputeError::VulkanError(format!("Buffer create: {:?}", e)))?;

        let mem_reqs = unsafe { self.device.get_buffer_memory_requirements(buffer) };
        let mem_props = unsafe { self._instance.get_physical_device_memory_properties(self._physical_device) };

        let memory_type_index = (0..mem_props.memory_type_count as usize)
            .position(|i| {
                let ty = mem_props.memory_types[i as usize];
                (mem_reqs.memory_type_bits & (1 << i)) != 0 && ty.property_flags.contains(vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT)
            })
            .ok_or_else(|| ComputeError::MemoryError("No suitable memory type".to_string()))? as u32;

        let mem = unsafe { self.device.allocate_memory(&vk::MemoryAllocateInfo {
            allocation_size: mem_reqs.size,
            memory_type_index,
            ..Default::default()
        }, None) }.map_err(|e| ComputeError::VulkanError(format!("Alloc memory: {:?}", e)))?;

        unsafe { self.device.bind_buffer_memory(buffer, mem, 0) }
            .map_err(|e| ComputeError::VulkanError(format!("Bind memory: {:?}", e)))?;

        Ok((buffer, mem))
    }

    fn copy_to_device(&self, memory: vk::DeviceMemory, data: &[u8]) -> Result<()> {
        let ptr = unsafe { self.device.map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty()) }
            .map_err(|e| ComputeError::VulkanError(format!("Map memory: {:?}", e)))?;

        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len()) };
        unsafe { self.device.unmap_memory(memory) };
        Ok(())
    }

    fn copy_from_device(&self, memory: vk::DeviceMemory, data: &mut [u8]) -> Result<()> {
        let ptr = unsafe { self.device.map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty()) }
            .map_err(|e| ComputeError::VulkanError(format!("Map memory: {:?}", e)))?;

        unsafe { std::ptr::copy_nonoverlapping(ptr as *const u8, data.as_mut_ptr(), data.len()) };
        unsafe { self.device.unmap_memory(memory) };
        Ok(())
    }
}

impl ComputeDevice for VulkanDevice {
    fn name(&self) -> &str {
        "Vulkan-GPU"
    }

    fn supports(&self, op_type: OpType) -> bool {
        self.available && matches!(op_type, OpType::MatMulQ8)
    }

    fn dispatch(&mut self, op: &mut Op) -> Result<()> {
        if !self.available {
            return Err(ComputeError::VulkanError("GPU not available".to_string()));
        }
        match &mut op.params {
            OpParams::MatMul { weight, input, scales, output, n_in, n_out } => {
                self.dispatch_matmul_gpu(weight, input, scales, output, *n_in, *n_out)
            }
            _ => Err(ComputeError::UnsupportedOp("Vulkan".to_string()))
        }
    }

    fn sync(&mut self) -> Result<()> {
        if self.available {
            unsafe { self.device.device_wait_idle() }
                .map_err(|e| ComputeError::VulkanError(format!("Sync error: {:?}", e)))?;
        }
        Ok(())
    }
}

impl Drop for VulkanDevice {
    fn drop(&mut self) {
        if self.available {
            unsafe {
                self.device.destroy_pipeline(self.pipeline, None);
                self.device.destroy_pipeline_layout(self.pipeline_layout, None);
                self.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
                self.device.destroy_descriptor_pool(self.descriptor_pool, None);
                self.device.destroy_device(None);
                self._instance.destroy_instance(None);
            }
        }
    }
}
