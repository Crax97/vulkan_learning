use ash::vk::{self, BufferUsageFlags, ImageLayout};
use gpu::{
    BufferCreateInfo, BufferRange, DescriptorInfo, DescriptorSetInfo, DescriptorType, Gpu,
    GpuBuffer, GpuDescriptorSet, MemoryDomain,
};
use resource_map::{Resource, ResourceHandle, ResourceMap};
use std::collections::HashMap;

use crate::texture::Texture;

use super::master_material::MasterMaterial;

#[derive(Clone)]
pub struct MaterialInstanceDescription<'a> {
    pub name: &'a str,
    pub texture_inputs: HashMap<String, ResourceHandle<Texture>>,
}

pub struct MaterialInstance {
    pub(crate) name: String,
    pub(crate) owner: ResourceHandle<MasterMaterial>,
    pub(crate) parameter_buffer: Option<GpuBuffer>,
    pub(crate) user_descriptor_set: GpuDescriptorSet,
    #[allow(dead_code)]
    pub(crate) current_inputs: HashMap<String, ResourceHandle<Texture>>,
    pub(crate) parameter_block_size: usize,
}

impl Resource for MaterialInstance {
    fn get_description(&self) -> &str {
        "Material Instance"
    }
}

impl MaterialInstance {
    pub fn create_instance(
        gpu: &Gpu,
        owner: ResourceHandle<MasterMaterial>,
        resource_map: &ResourceMap,
        description: &MaterialInstanceDescription,
    ) -> anyhow::Result<MaterialInstance> {
        let master_owner = resource_map.get(&owner);

        let parameter_buffer = if !master_owner.material_parameters.is_empty() {
            Some(gpu.create_buffer(
                &BufferCreateInfo {
                    label: Some(&format!("{} - Parameter buffer", description.name)),
                    size: master_owner.parameter_block_size,
                    usage: BufferUsageFlags::UNIFORM_BUFFER | BufferUsageFlags::TRANSFER_DST,
                },
                MemoryDomain::DeviceLocal,
            )?)
        } else {
            None
        };
        let user_descriptor_set = Self::create_user_descriptor_set(
            gpu,
            resource_map,
            master_owner,
            description,
            &parameter_buffer,
        )?;
        Ok(MaterialInstance {
            name: description.name.to_owned(),
            owner,
            parameter_buffer,
            user_descriptor_set,
            current_inputs: description.texture_inputs.clone(),
            parameter_block_size: master_owner.parameter_block_size,
        })
    }

    pub fn write_parameters<T: Sized + Copy>(&self, gpu: &Gpu, block: T) -> anyhow::Result<()> {
        assert!(
            std::mem::size_of::<T>() <= self.parameter_block_size
                && self.parameter_buffer.is_some()
        );
        gpu.write_buffer_data(self.parameter_buffer.as_ref().unwrap(), &[block])?;
        Ok(())
    }

    fn create_user_descriptor_set(
        gpu: &Gpu,
        resource_map: &ResourceMap,
        master: &MasterMaterial,
        description: &MaterialInstanceDescription<'_>,
        param_buffer: &Option<GpuBuffer>,
    ) -> anyhow::Result<GpuDescriptorSet> {
        let mut descriptors: Vec<_> = master
            .texture_inputs
            .iter()
            .enumerate()
            .map(|(i, tex)| {
                let tex = resource_map.get(&description.texture_inputs[&tex.name]);
                DescriptorInfo {
                    binding: i as _,
                    element_type: DescriptorType::CombinedImageSampler(gpu::SamplerState {
                        sampler: &resource_map.get(&tex.sampler).0,
                        image_view: &resource_map.get(&tex.image_view).view,
                        image_layout: ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    }),
                    binding_stage: gpu::ShaderStage::VertexFragment,
                }
            })
            .collect();

        if let Some(buffer) = &param_buffer {
            descriptors.push(DescriptorInfo {
                binding: descriptors.len() as _,
                binding_stage: gpu::ShaderStage::VertexFragment,
                element_type: DescriptorType::UniformBuffer(BufferRange {
                    handle: buffer,
                    offset: 0,
                    size: vk::WHOLE_SIZE,
                }),
            });
        }

        let descriptor = gpu.create_descriptor_set(&DescriptorSetInfo {
            descriptors: &descriptors,
        })?;
        Ok(descriptor)
    }
}
