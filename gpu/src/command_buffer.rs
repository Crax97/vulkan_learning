use core::panic;
use std::{ffi::CString, ops::Deref};

use ash::{extensions::ext::DebugUtils, prelude::VkResult, RawPtr, vk::{
    self, CommandBufferAllocateInfo, CommandBufferBeginInfo, CommandBufferLevel,
    CommandBufferUsageFlags, DebugUtilsLabelEXT, DependencyFlags, IndexType, Offset2D,
    PipelineBindPoint, PipelineStageFlags, Rect2D, ShaderStageFlags,
    StructureType, SubmitInfo, Viewport,
    ClearDepthStencilValue
}};
use ash::vk::{ImageLayout, RenderingAttachmentInfoKHR, RenderingFlags, RenderingInfoKHR, ResolveModeFlags};

use crate::{GPUFence, GPUSemaphore, GpuImage, ToVk, GpuImageView};

use super::{
    Gpu, GpuBuffer, GpuDescriptorSet, Pipeline, QueueType,
};

#[derive(Default)]
pub struct CommandBufferSubmitInfo<'a> {
    pub wait_semaphores: &'a [&'a GPUSemaphore],
    pub wait_stages: &'a [PipelineStageFlags],
    pub signal_semaphores: &'a [&'a GPUSemaphore],
    pub fence: Option<&'a GPUFence>,
}

pub struct CommandBuffer<'g> {
    gpu: &'g Gpu,
    inner_command_buffer: vk::CommandBuffer,
    has_recorded_anything: bool,
    has_been_submitted: bool,
    target_queue: vk::Queue,
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
pub struct MemoryBarrier {
    pub src_access_mask: vk::AccessFlags,
    pub dst_access_mask: vk::AccessFlags,
}

impl ToVk for MemoryBarrier {
    type Inner = vk::MemoryBarrier;

    fn to_vk(&self) -> Self::Inner {
        Self::Inner {
            s_type: StructureType::MEMORY_BARRIER,
            p_next: std::ptr::null(),
            src_access_mask: self.src_access_mask,
            dst_access_mask: self.dst_access_mask,
        }
    }
}

pub struct BufferMemoryBarrier<'a> {
    pub src_access_mask: vk::AccessFlags,
    pub dst_access_mask: vk::AccessFlags,
    pub src_queue_family_index: u32,
    pub dst_queue_family_index: u32,
    pub buffer: &'a GpuBuffer,
    pub offset: vk::DeviceSize,
    pub size: vk::DeviceSize,
}

impl<'a> ToVk for BufferMemoryBarrier<'a> {
    type Inner = vk::BufferMemoryBarrier;

    fn to_vk(&self) -> Self::Inner {
        Self::Inner {
            s_type: StructureType::BUFFER_MEMORY_BARRIER,
            p_next: std::ptr::null(),
            src_access_mask: self.src_access_mask,
            dst_access_mask: self.dst_access_mask,
            src_queue_family_index: self.src_queue_family_index,
            dst_queue_family_index: self.dst_queue_family_index,
            buffer: self.buffer.inner,
            offset: self.offset,
            size: self.size,
        }
    }
}

pub struct ImageMemoryBarrier<'a> {
    pub src_access_mask: vk::AccessFlags,
    pub dst_access_mask: vk::AccessFlags,
    pub old_layout: vk::ImageLayout,
    pub new_layout: vk::ImageLayout,
    pub src_queue_family_index: u32,
    pub dst_queue_family_index: u32,
    pub image: &'a GpuImage,
    pub subresource_range: vk::ImageSubresourceRange,
}

impl<'a> ToVk for ImageMemoryBarrier<'a> {
    type Inner = vk::ImageMemoryBarrier;

    fn to_vk(&self) -> Self::Inner {
        Self::Inner {
            s_type: StructureType::IMAGE_MEMORY_BARRIER,
            p_next: std::ptr::null(),
            src_access_mask: self.src_access_mask,
            dst_access_mask: self.dst_access_mask,
            src_queue_family_index: self.src_queue_family_index,
            dst_queue_family_index: self.dst_queue_family_index,
            old_layout: self.old_layout,
            new_layout: self.new_layout,
            image: self.image.inner,
            subresource_range: self.subresource_range,
        }
    }
}

#[derive(Default)]
pub struct PipelineBarrierInfo<'a> {
    pub src_stage_mask: PipelineStageFlags,
    pub dst_stage_mask: PipelineStageFlags,
    pub dependency_flags: DependencyFlags,
    pub memory_barriers: &'a [MemoryBarrier],
    pub buffer_memory_barriers: &'a [BufferMemoryBarrier<'a>],
    pub image_memory_barriers: &'a [ImageMemoryBarrier<'a>],
}

impl<'g> CommandBuffer<'g> {
    pub fn new(gpu: &'g Gpu, target_queue: QueueType) -> VkResult<Self> {
        let device = gpu.vk_logical_device();
        let inner_command_buffer = unsafe {
            device.allocate_command_buffers(&CommandBufferAllocateInfo {
                s_type: StructureType::COMMAND_BUFFER_ALLOCATE_INFO,
                p_next: std::ptr::null(),
                command_pool: target_queue.get_vk_command_pool(gpu),
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
            has_been_submitted: false,
            target_queue: target_queue.get_vk_queue(gpu),
        })
    }
    pub fn begin_render_pass<'p>(
        &'p mut self,
        info: &BeginRenderPassInfo<'p>,
    ) -> RenderPassCommand<'p, 'g> {
        RenderPassCommand::<'p, 'g>::new(self, info)
    }

    pub fn pipeline_barrier(&mut self, barrier_info: &PipelineBarrierInfo) {
        self.has_recorded_anything = true;
        let device = self.gpu.vk_logical_device();
        let memory_barriers: Vec<_> = barrier_info
            .memory_barriers
            .iter()
            .map(|b| b.to_vk())
            .collect();
        let buffer_memory_barriers: Vec<_> = barrier_info
            .buffer_memory_barriers
            .iter()
            .map(|b| b.to_vk())
            .collect();
        let image_memory_barriers: Vec<_> = barrier_info
            .image_memory_barriers
            .iter()
            .map(|b| b.to_vk())
            .collect();
        unsafe {
            device.cmd_pipeline_barrier(
                self.inner_command_buffer,
                barrier_info.src_stage_mask,
                barrier_info.dst_stage_mask,
                barrier_info.dependency_flags,
                &memory_barriers,
                &buffer_memory_barriers,
                &image_memory_barriers,
            )
        };
    }

    pub fn bind_descriptor_sets(
        &self,
        bind_point: PipelineBindPoint,
        material: &Pipeline,
        first_index: u32,
        descriptor_sets: &[&GpuDescriptorSet],
    ) {
        let descriptor_sets: Vec<_> = descriptor_sets
            .iter()
            .map(|d| d.allocation.descriptor_set)
            .collect();
        unsafe {
            self.gpu.vk_logical_device().cmd_bind_descriptor_sets(
                self.inner_command_buffer,
                bind_point,
                material.pipeline_layout,
                first_index,
                &descriptor_sets,
                &[],
            );
        }
    }

    pub fn submit(mut self, submit_info: &CommandBufferSubmitInfo) -> VkResult<()> {
        self.has_been_submitted = true;
        if !self.has_recorded_anything {
            return Ok(());
        }

        let device = self.gpu.vk_logical_device();
        unsafe {
            device
                .end_command_buffer(self.inner())
                .expect("Failed to end inner command buffer");
            let target_queue = self.target_queue;

            let wait_semaphores: Vec<_> = submit_info
                .wait_semaphores
                .iter()
                .map(|s| s.inner)
                .collect();

            let signal_semaphores: Vec<_> = submit_info
                .signal_semaphores
                .iter()
                .map(|s| s.inner)
                .collect();

            device.queue_submit(
                target_queue,
                &[SubmitInfo {
                    s_type: StructureType::SUBMIT_INFO,
                    p_next: std::ptr::null(),
                    wait_semaphore_count: wait_semaphores.len() as _,
                    p_wait_semaphores: wait_semaphores.as_ptr(),
                    p_wait_dst_stage_mask: submit_info.wait_stages.as_ptr(),
                    command_buffer_count: 1,
                    p_command_buffers: [self.inner_command_buffer].as_ptr(),
                    signal_semaphore_count: signal_semaphores.len() as _,
                    p_signal_semaphores: signal_semaphores.as_ptr(),
                }],
                if let Some(fence) = &submit_info.fence {
                    fence.inner
                } else {
                    vk::Fence::null()
                },
            )
        }
    }

    pub fn inner(&self) -> vk::CommandBuffer {
        self.inner_command_buffer
    }
}

// Debug utilities

pub struct ScopedDebugLabelInner {
    debug_utils: DebugUtils,
    command_buffer: vk::CommandBuffer,
}

impl ScopedDebugLabelInner {
    fn new(
        label: &str,
        color: [f32; 4],
        debug_utils: DebugUtils,
        command_buffer: vk::CommandBuffer,
    ) -> Self {
        unsafe {
            let c_label = CString::new(label).unwrap();
            debug_utils.cmd_begin_debug_utils_label(
                command_buffer,
                &DebugUtilsLabelEXT {
                    s_type: StructureType::DEBUG_UTILS_LABEL_EXT,
                    p_next: std::ptr::null(),
                    p_label_name: c_label.as_ptr(),
                    color,
                },
            );
        }
        Self {
            debug_utils,
            command_buffer,
        }
    }
    fn end(&self) {
        unsafe {
            self.debug_utils
                .cmd_end_debug_utils_label(self.command_buffer);
        }
    }
}

pub struct ScopedDebugLabel {
    inner: Option<ScopedDebugLabelInner>,
}

impl ScopedDebugLabel {
    pub fn end(mut self) {
        if let Some(label) = self.inner.take() {
            label.end();
        }
    }
}

impl Drop for ScopedDebugLabel {
    fn drop(&mut self) {
        if let Some(label) = self.inner.take() {
            label.end();
        }
    }
}

impl<'g> CommandBuffer<'g> {
    pub fn begin_debug_region(&self, label: &str, color: [f32; 4]) -> ScopedDebugLabel {
        ScopedDebugLabel {
            inner: self.gpu.state.debug_utilities.as_ref().map(|debug_utils| {
                ScopedDebugLabelInner::new(label, color, debug_utils.clone(), self.inner())
            }),
        }
    }

    pub fn insert_debug_label(&self, label: &str, color: [f32; 4]) {
        if let Some(debug_utils) = &self.gpu.state.debug_utilities {
            unsafe {
                let c_label = CString::new(label).unwrap();
                debug_utils.cmd_insert_debug_utils_label(
                    self.inner(),
                    &DebugUtilsLabelEXT {
                        s_type: StructureType::DEBUG_UTILS_LABEL_EXT,
                        p_next: std::ptr::null(),
                        p_label_name: c_label.as_ptr(),
                        color,
                    },
                );
            }
        }
    }
}

impl<'g> Drop for CommandBuffer<'g> {
    fn drop(&mut self) {
        if !self.has_been_submitted {
            panic!("CommandBuffer::submit hasn't been called!");
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ColorLoadOp {
    DontCare,
    Load,
    Clear([f32; 4]),
}

#[derive(Clone, Copy, Debug)]
pub enum DepthLoadOp {
    DontCare,
    Load,
    Clear(f32),
}
#[derive(Clone, Copy, Debug)]
pub enum StencilLoadOp {
    DontCare,
    Load,
    Clear(u8),
}



impl ToVk for ColorLoadOp{
    type Inner = vk::AttachmentLoadOp;

    fn to_vk(&self) -> Self::Inner {
        use ColorLoadOp::{DontCare, Load, Clear};
        match self {
            DontCare => Self::Inner::DONT_CARE,
            Load => Self::Inner::LOAD,
            Clear(_) => Self::Inner::CLEAR,
        }
    }
}


impl ToVk for DepthLoadOp {
    type Inner = vk::AttachmentLoadOp;

    fn to_vk(&self) -> Self::Inner {
        use DepthLoadOp::{DontCare, Load, Clear};
        match self {
            DontCare => Self::Inner::DONT_CARE,
            Load => Self::Inner::LOAD,
            Clear(_) => Self::Inner::CLEAR,
        }
    }
}


impl ToVk for StencilLoadOp {
    type Inner = vk::AttachmentLoadOp;

    fn to_vk(&self) -> Self::Inner {
        use StencilLoadOp::{DontCare, Load, Clear};
        match self {
            DontCare => Self::Inner::DONT_CARE,
            Load => Self::Inner::LOAD,
            Clear(_) => Self::Inner::CLEAR,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum AttachmentStoreOp {
    DontCare,
    Store,
}

impl ToVk for AttachmentStoreOp {
    type Inner = vk::AttachmentStoreOp;

    fn to_vk(&self) -> Self::Inner {
        match self {
            AttachmentStoreOp::DontCare => Self::Inner::DONT_CARE,
            AttachmentStoreOp::Store => Self::Inner::STORE
        }
    }
}

#[derive(Clone, Copy)]
pub struct ColorAttachment<'a> {
    pub image_view: &'a GpuImageView,
    pub load_op: ColorLoadOp,
    pub store_op: AttachmentStoreOp,
    pub initial_layout: ImageLayout,
}

#[derive(Clone, Copy)]
pub struct DepthAttachment<'a> {
    pub image_view: &'a GpuImageView,
    pub load_op: DepthLoadOp,
    pub store_op: AttachmentStoreOp,
    pub initial_layout: ImageLayout,
}

#[derive(Clone, Copy)]
pub struct StencilAttachment<'a> {
    pub image_view: &'a GpuImageView,
    pub load_op: StencilLoadOp,
    pub store_op: AttachmentStoreOp,
    pub initial_layout: ImageLayout,
}

#[derive(Clone, Copy)]
pub struct BeginRenderPassInfo<'a> {
    pub color_attachments: &'a [ColorAttachment<'a>],
    pub depth_attachment: Option<DepthAttachment<'a>>,
    pub stencil_attachment: Option<StencilAttachment<'a>>,
    pub render_area: Rect2D,
}

impl<'c, 'g> RenderPassCommand<'c, 'g> {
    fn new(command_buffer: &'c mut CommandBuffer<'g>, info: &BeginRenderPassInfo<'c>) -> Self {
        let color_attachments: Vec<_> = info.color_attachments.iter().map(|attch| {
           RenderingAttachmentInfoKHR {
               s_type: StructureType::RENDERING_ATTACHMENT_INFO,
               p_next: std::ptr::null(),
               image_view: attch.image_view.inner,
               image_layout: attch.initial_layout,
               resolve_mode: ResolveModeFlags::NONE,
               resolve_image_view: vk::ImageView::null(),
               resolve_image_layout: ImageLayout::UNDEFINED,
               load_op: attch.load_op.to_vk(),
               store_op: attch.store_op.to_vk(),
               clear_value: match attch.load_op {
                   ColorLoadOp::Clear(color) => {ash::vk::ClearValue {
                       color: ash::vk::ClearColorValue { float32: color}
                   }}
                   _ => ash::vk::ClearValue::default()
               }
           } 
        }).collect();

        let depth_attachment = info.depth_attachment.map(|attch| {
            RenderingAttachmentInfoKHR {
                s_type: StructureType::RENDERING_ATTACHMENT_INFO,
                p_next: std::ptr::null(),
                image_view: attch.image_view.inner,
                image_layout: attch.initial_layout,
                resolve_mode: ResolveModeFlags::NONE,
                resolve_image_view: vk::ImageView::null(),
                resolve_image_layout: ImageLayout::UNDEFINED,
                load_op: attch.load_op.to_vk(),
                store_op: attch.store_op.to_vk(),
                clear_value: match attch.load_op {
                    DepthLoadOp::Clear(d) => {ash::vk::ClearValue {
                        depth_stencil: ClearDepthStencilValue {
                            depth: d,
                            stencil: 255,
                        }}}
                    _ => ash::vk::ClearValue::default()
                }
            }
        });

        let stencil_attachment = info.stencil_attachment.map(|attch| {
            RenderingAttachmentInfoKHR {
                s_type: StructureType::RENDERING_ATTACHMENT_INFO,
                p_next: std::ptr::null(),
                image_view: attch.image_view.inner,
                image_layout: attch.initial_layout,
                resolve_mode: ResolveModeFlags::NONE,
                resolve_image_view: vk::ImageView::null(),
                resolve_image_layout: ImageLayout::UNDEFINED,
                load_op: attch.load_op.to_vk(),
                store_op: attch.store_op.to_vk(),
                clear_value: match attch.load_op {
                    StencilLoadOp::Clear(s) => {ash::vk::ClearValue {
                        depth_stencil: ClearDepthStencilValue {
                            depth: 0.0,
                            stencil: s as _,
                        }}}
                    _ => ash::vk::ClearValue::default()
                }
            }
        });
        
        let create_info = RenderingInfoKHR {
            s_type: StructureType::RENDERING_INFO_KHR,
            p_next: std::ptr::null(),
            flags: RenderingFlags::empty(),
            layer_count: 1,
            view_mask: 0,
            render_area: info.render_area,
            color_attachment_count: color_attachments.len() as _,
            p_color_attachments: color_attachments.as_ptr(),
            p_depth_attachment: depth_attachment.as_ref().as_raw_ptr(),
            p_stencil_attachment: stencil_attachment.as_ref().as_raw_ptr(),
            
        };
        unsafe {
            command_buffer.gpu.state.dynamic_rendering.cmd_begin_rendering(
                command_buffer.inner_command_buffer,
                &create_info,
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

    pub fn bind_pipeline(&mut self, material: &Pipeline) {
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
    pub fn draw(
        &mut self,
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    ) {
        self.prepare_draw();
        self.has_draw_command = true;
        self.command_buffer.has_recorded_anything = true;
        let device = self.command_buffer.gpu.vk_logical_device();
        unsafe {
            device.cmd_draw(
                self.command_buffer.inner(),
                vertex_count,
                instance_count,
                first_vertex,
                first_instance,
            );
        }
    }

    fn prepare_draw(&self) {
        let device = self.command_buffer.gpu.vk_logical_device();

        // Negate height because of Khronos brain farts
        let height = self.render_area.extent.height as f32;
        let viewport = match self.viewport_area {
            Some(viewport) => viewport,
            None => Viewport {
                x: 0 as f32,
                y: 0.0,
                width: self.render_area.extent.width as f32,
                height,
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

    pub fn bind_index_buffer(
        &self,
        buffer: &GpuBuffer,
        offset: vk::DeviceSize,
        index_type: IndexType,
    ) {
        let device = self.command_buffer.gpu.vk_logical_device();
        let index_buffer = buffer.inner;
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
        buffers: &[&GpuBuffer],
        offsets: &[vk::DeviceSize],
    ) {
        assert!(buffers.len() == offsets.len());
        let device = self.command_buffer.gpu.vk_logical_device();
        let vertex_buffers: Vec<_> = buffers.iter().map(|b| b.inner).collect();
        unsafe {
            device.cmd_bind_vertex_buffers(
                self.command_buffer.inner_command_buffer,
                first_binding,
                &vertex_buffers,
                offsets,
            );
        }
    }

    pub fn push_constant<T: Copy + Sized>(&self, pipeline: &Pipeline, data: &T, offset: u32) {
        let device = self.command_buffer.gpu.vk_logical_device();
        unsafe {
            let ptr: *const u8 = data as *const T as *const u8;
            let slice = std::slice::from_raw_parts(ptr, std::mem::size_of::<T>());
            device.cmd_push_constants(
                self.command_buffer.inner_command_buffer,
                pipeline.pipeline_layout,
                ShaderStageFlags::ALL,
                offset,
                slice,
            );
        }
    }
}

impl<'c, 'g> AsRef<CommandBuffer<'g>> for RenderPassCommand<'c, 'g> {
    fn as_ref(&self) -> &CommandBuffer<'g> {
        self.command_buffer
    }
}

impl<'c, 'g> Deref for RenderPassCommand<'c, 'g> {
    type Target = CommandBuffer<'g>;

    fn deref(&self) -> &Self::Target {
        self.command_buffer
    }
}

impl<'c, 'g> Drop for RenderPassCommand<'c, 'g> {
    fn drop(&mut self) {
        unsafe { self.command_buffer.gpu.state.dynamic_rendering.cmd_end_rendering(self.command_buffer.inner_command_buffer) };
    }
}
