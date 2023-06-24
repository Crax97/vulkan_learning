use std::collections::HashMap;

use ash::{
    prelude::VkResult,
    vk::{self, ImageLayout},
};
use gpu::{
    BufferRange, DescriptorInfo, DescriptorSetInfo, Gpu, GpuBuffer, GpuDescriptorSet, Pipeline,
    SamplerState,
};
use resource_map::{Resource, ResourceHandle, ResourceMap};

use crate::texture::Texture;

#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug)]
pub enum MaterialDomain {
    Surface,
    DepthOnly,
}

#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug)]
pub enum PipelineTarget {
    ColorAndDepth,
    DepthOnly,
}

pub struct Material {
    pub pipelines: HashMap<PipelineTarget, Pipeline>,
    pub uniform_buffers: Vec<GpuBuffer>,
    pub textures: Vec<ResourceHandle<Texture>>,
    pub resources_descriptor_set: GpuDescriptorSet,
}

impl Material {
    pub fn new(
        gpu: &Gpu,
        resource_map: &ResourceMap,
        pipelines: HashMap<PipelineTarget, Pipeline>,
        uniform_buffers: Vec<GpuBuffer>,
        textures: Vec<ResourceHandle<Texture>>,
    ) -> VkResult<Self> {
        let mut uniform_descriptors = vec![];
        let mut bind_index = 0;
        for buffer in uniform_buffers.iter() {
            uniform_descriptors.push(DescriptorInfo {
                binding: bind_index,
                element_type: gpu::DescriptorType::UniformBuffer(BufferRange {
                    handle: &buffer,
                    offset: 0,
                    size: vk::WHOLE_SIZE,
                }),
                binding_stage: gpu::ShaderStage::VertexFragment,
            });
            bind_index += 1;
        }
        for texture in textures.iter() {
            let texture = resource_map.get(texture);
            uniform_descriptors.push(DescriptorInfo {
                binding: bind_index,
                element_type: gpu::DescriptorType::CombinedImageSampler(SamplerState {
                    sampler: &resource_map.get(&texture.sampler).0,
                    image_view: &resource_map.get(&texture.image_view).view,
                    image_layout: ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                }),
                binding_stage: gpu::ShaderStage::VertexFragment,
            });
            bind_index += 1;
        }
        let resources_descriptor_set = gpu.create_descriptor_set(&DescriptorSetInfo {
            descriptors: &uniform_descriptors,
        })?;

        Ok(Self {
            pipelines,
            uniform_buffers,
            textures,
            resources_descriptor_set,
        })
    }
}

impl Resource for Material {
    fn get_description(&self) -> &str {
        "Material"
    }
}
