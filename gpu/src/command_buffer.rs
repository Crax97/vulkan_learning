use ash::{
    prelude::VkResult,
    vk::{
        self, ClearValue, CommandBufferAllocateInfo, CommandBufferBeginInfo, CommandBufferLevel,
        CommandBufferUsageFlags, IndexType, Offset2D, PipelineBindPoint, PipelineStageFlags,
        Rect2D, RenderPassBeginInfo, StructureType, SubmitInfo, SubpassContents, Viewport,
    },
};

use crate::{GPUFence, GPUSemaphore};

use super::{
    material::RenderPass, Gpu, GpuBuffer, GpuDescriptorSet, GpuFramebuffer, Material, QueueType,
    ResourceHandle,
};

#[derive(Default)]
pub struct CommandBufferSubmitInfo {
    pub wait_semaphores: Vec<ResourceHandle<GPUSemaphore>>,
    pub wait_stages: Vec<PipelineStageFlags>,
    pub signal_semaphores: Vec<ResourceHandle<GPUSemaphore>>,
    pub fence: Option<ResourceHandle<GPUFence>>,
    pub target_queue: QueueType,
}

pub struct CommandBuffer<'g> {
    gpu: &'g Gpu,
    inner_command_buffer: vk::CommandBuffer,
    has_recorded_anything: bool,
    info: CommandBufferSubmitInfo,
}

pub struct RenderPassCommand<'c, 'g>
where
    'g: 'c,
{
    command_buffer: &'c mut CommandBuffer<'g>,
    viewport_area: Option<Viewport>,
    scissor_area: Option<Rect2D>,
    has_draw_command: bool,
    render_area: Rect2D,
}
impl<'g> CommandBuffer<'g> {
    pub fn new(gpu: &'g Gpu, info: CommandBufferSubmitInfo) -> VkResult<Self> {
        let device = gpu.vk_logical_device();
        let inner_command_buffer = unsafe {
            device.allocate_command_buffers(&CommandBufferAllocateInfo {
                s_type: StructureType::COMMAND_BUFFER_ALLOCATE_INFO,
                p_next: std::ptr::null(),
                command_pool: info.target_queue.get_vk_queue(gpu),
                level: CommandBufferLevel::PRIMARY,
                command_buffer_count: 1,
            })
        }?[0];

        unsafe {
            device.begin_command_buffer(
                inner_command_buffer,
                &CommandBufferBeginInfo {
                    s_type: StructureType::COMMAND_BUFFER_BEGIN_INFO,
                    p_next: std::ptr::null(),
                    flags: CommandBufferUsageFlags::ONE_TIME_SUBMIT,
                    p_inheritance_info: std::ptr::null(),
                },
            )
        }?;

        Ok(Self {
            gpu,
            inner_command_buffer,
            has_recorded_anything: false,
            info,
        })
    }
    pub fn begin_render_pass<'p>(
        &'p mut self,
        info: &BeginRenderPassInfo<'p>,
    ) -> RenderPassCommand<'p, 'g> {
        RenderPassCommand::<'p, 'g>::new(self, &info)
    }

    pub fn inner(&self) -> vk::CommandBuffer {
        self.inner_command_buffer.clone()
    }
}

#[derive(Clone, Copy)]
pub struct BeginRenderPassInfo<'a> {
    pub framebuffer: &'a GpuFramebuffer,
    pub render_pass: &'a RenderPass,
    pub clear_color_values: &'a [ClearValue],
    pub render_area: Rect2D,
}

impl<'c, 'g> RenderPassCommand<'c, 'g> {
    fn new(command_buffer: &'c mut CommandBuffer<'g>, info: &BeginRenderPassInfo<'c>) -> Self {
        let device = command_buffer.gpu.vk_logical_device();
        let create_info = RenderPassBeginInfo {
            s_type: StructureType::RENDER_PASS_BEGIN_INFO,
            p_next: std::ptr::null(),
            render_pass: info.render_pass.inner,
            framebuffer: info.framebuffer.inner,
            render_area: info.render_area,
            clear_value_count: info.clear_color_values.len() as _,
            p_clear_values: info.clear_color_values.as_ptr(),
        };
        unsafe {
            device.cmd_begin_render_pass(
                command_buffer.inner_command_buffer,
                &create_info,
                SubpassContents::INLINE,
            );
        };

        Self {
            command_buffer,
            has_draw_command: false,
            viewport_area: None,
            scissor_area: None,
            render_area: info.render_area,
        }
    }

    pub fn bind_material(&mut self, material: &Material) {
        let device = self.command_buffer.gpu.vk_logical_device();
        unsafe {
            device.cmd_bind_pipeline(
                self.command_buffer.inner_command_buffer,
                PipelineBindPoint::GRAPHICS,
                material.pipeline,
            )
        }
    }

    pub fn draw_indexed(
        &mut self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) {
        self.prepare_draw();
        self.has_draw_command = true;
        self.command_buffer.has_recorded_anything = true;
        let device = self.command_buffer.gpu.vk_logical_device();
        unsafe {
            device.cmd_draw_indexed(
                self.command_buffer.inner(),
                index_count,
                instance_count,
                first_index,
                vertex_offset,
                first_instance,
            );
        }
    }

    fn prepare_draw(&self) {
        let device = self.command_buffer.gpu.vk_logical_device();

        let viewport = match self.viewport_area {
            Some(viewport) => viewport,
            None => Viewport {
                x: 0 as f32,
                y: 0 as f32,
                width: self.render_area.extent.width as f32,
                height: self.render_area.extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            },
        };
        let scissor = match self.scissor_area {
            Some(scissor) => scissor,
            None => Rect2D {
                offset: Offset2D { x: 0, y: 0 },
                extent: self.render_area.extent,
            },
        };
        unsafe {
            device.cmd_set_viewport(self.command_buffer.inner(), 0, &[viewport]);
            device.cmd_set_scissor(self.command_buffer.inner(), 0, &[scissor]);
        }
    }

    pub fn bind_descriptor_sets(
        &self,
        bind_point: PipelineBindPoint,
        material: &Material,
        first_index: u32,
        descriptor_sets: &[&ResourceHandle<GpuDescriptorSet>],
    ) {
        let descriptor_sets: Vec<_> = descriptor_sets
            .iter()
            .map(|d| {
                self.command_buffer
                    .gpu
                    .resource_map
                    .get(d)
                    .unwrap()
                    .allocation
                    .descriptor_set
            })
            .collect();
        unsafe {
            self.command_buffer
                .gpu
                .vk_logical_device()
                .cmd_bind_descriptor_sets(
                    self.command_buffer.inner_command_buffer,
                    bind_point,
                    material.pipeline_layout,
                    first_index,
                    &descriptor_sets,
                    &[],
                );
        }
    }
    pub fn bind_index_buffer(
        &self,
        buffer: &ResourceHandle<GpuBuffer>,
        offset: vk::DeviceSize,
        index_type: IndexType,
    ) {
        let device = self.command_buffer.gpu.vk_logical_device();
        let index_buffer = self
            .command_buffer
            .gpu
            .resource_map
            .get(buffer)
            .unwrap()
            .inner;
        unsafe {
            device.cmd_bind_index_buffer(
                self.command_buffer.inner_command_buffer,
                index_buffer,
                offset,
                index_type,
            );
        }
    }
    pub fn bind_vertex_buffer(
        &self,
        first_binding: u32,
        buffers: &[&ResourceHandle<GpuBuffer>],
        offsets: &[vk::DeviceSize],
    ) {
        assert!(buffers.len() == offsets.len());
        let device = self.command_buffer.gpu.vk_logical_device();
        let vertex_buffers: Vec<_> = buffers
            .iter()
            .map(|b| self.command_buffer.gpu.resource_map.get(b).unwrap().inner)
            .collect();
        unsafe {
            device.cmd_bind_vertex_buffers(
                self.command_buffer.inner_command_buffer,
                first_binding,
                &vertex_buffers,
                offsets,
            );
        }
    }
}

impl<'c, 'g> Drop for RenderPassCommand<'c, 'g> {
    fn drop(&mut self) {
        let device = &self.command_buffer.gpu.vk_logical_device();
        unsafe { device.cmd_end_render_pass(self.command_buffer.inner_command_buffer) };
    }
}

impl<'g> Drop for CommandBuffer<'g> {
    fn drop(&mut self) {
        if !self.has_recorded_anything {
            return;
        }

        let device = self.gpu.vk_logical_device();
        unsafe {
            device
                .end_command_buffer(self.inner())
                .expect("Failed to end inner command buffer");
            let target_queue = match self.info.target_queue {
                QueueType::Graphics => self.gpu.state.graphics_queue.clone(),
                QueueType::AsyncCompute => self.gpu.state.async_compute_queue.clone(),
                QueueType::Transfer => self.gpu.state.transfer_queue.clone(),
            };

            let wait_semaphores: Vec<_> = self
                .info
                .wait_semaphores
                .iter()
                .map(|s| self.gpu.resource_map.get(s).unwrap().inner)
                .collect();

            let signal_semaphores: Vec<_> = self
                .info
                .signal_semaphores
                .iter()
                .map(|s| self.gpu.resource_map.get(s).unwrap().inner)
                .collect();

            device
                .queue_submit(
                    target_queue,
                    &[SubmitInfo {
                        s_type: StructureType::SUBMIT_INFO,
                        p_next: std::ptr::null(),
                        wait_semaphore_count: wait_semaphores.len() as _,
                        p_wait_semaphores: wait_semaphores.as_ptr(),
                        p_wait_dst_stage_mask: self.info.wait_stages.as_ptr(),
                        command_buffer_count: 1,
                        p_command_buffers: [self.inner_command_buffer].as_ptr(),
                        signal_semaphore_count: signal_semaphores.len() as _,
                        p_signal_semaphores: signal_semaphores.as_ptr(),
                    }],
                    if let Some(fence) = &self.info.fence {
                        self.gpu.resource_map.get(fence).unwrap().inner
                    } else {
                        vk::Fence::null()
                    },
                )
                .unwrap();
        }
    }
}