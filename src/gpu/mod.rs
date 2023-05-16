mod allocator;
mod command_buffer;
mod descriptor_set;
mod gpu;
mod material;
mod resource;
mod swapchain;
mod types;

pub use allocator::*;
use ash::vk::ImageLayout;
pub use command_buffer::*;
pub use gpu::*;
pub use material::*;
pub use resource::*;
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
    fn get_vk_queue(&self, gpu: &Gpu) -> ash::vk::CommandPool {
        match self {
            QueueType::Graphics => gpu.thread_local_state.graphics_command_pool,
            QueueType::AsyncCompute => gpu.thread_local_state.compute_command_pool,
            QueueType::Transfer => gpu.thread_local_state.transfer_command_pool,
        }
    }
    fn get_vk_queue_index(&self, gpu: &Gpu) -> u32 {
        match self {
            QueueType::Graphics => gpu.state.queue_families.graphics_family.index,
            QueueType::AsyncCompute => gpu.state.queue_families.async_compute_family.index,
            QueueType::Transfer => gpu.state.queue_families.transfer_family.index,
        }
    }
}

pub struct BufferRange {
    pub handle: ResourceHandle<GpuBuffer>,
    pub offset: u64,
    pub size: u64,
}

pub struct SamplerState {
    pub sampler: ResourceHandle<GpuSampler>,
    pub image_view: ResourceHandle<GpuImage>,
    pub image_layout: ImageLayout,
}

pub enum DescriptorType {
    UniformBuffer(BufferRange),
    StorageBuffer(BufferRange),
    Sampler(SamplerState),
    CombinedImageSampler(SamplerState),
}

#[derive(Clone, Copy, Debug)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute,
}

pub struct DescriptorInfo {
    pub binding: u32,
    pub element_type: DescriptorType,
    pub binding_stage: ShaderStage,
}

pub struct DescriptorSetInfo<'a> {
    pub descriptors: &'a [DescriptorInfo],
}
