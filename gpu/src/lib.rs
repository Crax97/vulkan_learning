mod allocator;
mod command_buffer;
mod descriptor_set;
mod gpu;
mod pipeline;
mod swapchain;
mod types;

pub use crate::gpu::*;
pub use allocator::*;
use ash::vk::ImageLayout;
pub use command_buffer::*;
pub use pipeline::*;
use std::fmt::{Debug, Formatter};
use std::hash::Hash;
pub use swapchain::Swapchain;
pub use types::*;

#[derive(Default)]
pub enum QueueType {
    #[default]
    Graphics,
    AsyncCompute,
    Transfer,
}
impl QueueType {
    fn get_vk_command_pool(&self, gpu: &Gpu) -> ash::vk::CommandPool {
        let thread_local_state = &gpu.thread_local_states[gpu.swapchain.current_frame.get()];
        match self {
            QueueType::Graphics => thread_local_state.graphics_command_pool,
            QueueType::AsyncCompute => thread_local_state.compute_command_pool,
            QueueType::Transfer => thread_local_state.transfer_command_pool,
        }
    }
    fn get_vk_queue(&self, gpu: &Gpu) -> ash::vk::Queue {
        match self {
            QueueType::Graphics => gpu.state.graphics_queue,
            QueueType::AsyncCompute => gpu.state.async_compute_queue,
            QueueType::Transfer => gpu.state.transfer_queue,
        }
    }
}

#[derive(Clone, Hash)]
pub struct BufferRange<'a> {
    pub handle: &'a GpuBuffer,
    pub offset: u64,
    pub size: u64,
}

#[derive(Clone, Hash)]
pub struct SamplerState<'a> {
    pub sampler: &'a GpuSampler,
    pub image_view: &'a GpuImageView,
    pub image_layout: ImageLayout,
}

#[derive(Clone)]
pub enum DescriptorType<'a> {
    UniformBuffer(BufferRange<'a>),
    StorageBuffer(BufferRange<'a>),
    Sampler(SamplerState<'a>),
    CombinedImageSampler(SamplerState<'a>),
}

impl<'a> std::hash::Hash for DescriptorType<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
    }
}

impl<'a> Debug for DescriptorType<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", core::mem::discriminant(self)))
    }
}

#[derive(Clone, Copy, Hash, Debug, PartialEq, Ord, PartialOrd, Eq)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    VertexFragment,
    Compute,
    All,
}

#[derive(Clone, Hash, Debug)]
pub struct DescriptorInfo<'a> {
    pub binding: u32,
    pub element_type: DescriptorType<'a>,
    pub binding_stage: ShaderStage,
}

#[derive(Clone, Hash)]
pub struct DescriptorSetInfo<'a> {
    pub descriptors: &'a [DescriptorInfo<'a>],
}
