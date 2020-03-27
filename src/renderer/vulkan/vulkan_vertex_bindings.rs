use crate::{
  error::SarektResult,
  renderer::vertex_bindings::{
    BindTextureInfo, BindUniformInfo, DefaultForwardShaderLayout, DefaultForwardShaderVertex,
    DescriptorLayoutInfo, VertexBindings,
  },
};
use ash::vk;

// TODO SHADERS use reflection to generate these at compile time (generically?).

unsafe impl VertexBindings for DefaultForwardShaderVertex {
  type BVA = vk::VertexInputAttributeDescription;
  type BVB = vk::VertexInputBindingDescription;

  fn get_binding_description() -> Self::BVB {
    vk::VertexInputBindingDescription::builder()
      .binding(0)
      .stride(std::mem::size_of::<Self>() as u32)
      .input_rate(vk::VertexInputRate::VERTEX)
      .build()
  }

  fn get_attribute_descriptions() -> Vec<Self::BVA> {
    // Position
    let position_attr = vk::VertexInputAttributeDescription::builder()
      .binding(0) // Which binding in the shader.
      .location(0) // The layout location in the shader.
      .format(vk::Format::R32G32B32_SFLOAT) // RGB is unintuitive but the point is its three floats.
      .offset(offset_of!(DefaultForwardShaderVertex, position) as u32)
      .build();
    let color_attr = vk::VertexInputAttributeDescription::builder()
      .binding(0) // Which binding in the shader.
      .location(1) // The layout location in the shader.
      .format(vk::Format::R32G32B32_SFLOAT) // RGB is unintuitive but the point is its two floats.
      .offset(offset_of!(DefaultForwardShaderVertex, color) as u32)
      .build();
    let texture_attr = vk::VertexInputAttributeDescription::builder()
      .binding(0)
      .location(2)
      .format(vk::Format::R32G32_SFLOAT)
      .offset(offset_of!(DefaultForwardShaderVertex, texture_coordinates) as u32)
      .build();

    vec![position_attr, color_attr, texture_attr]
  }
}

// TODO SHADERS use reflection to generate descriptor set layouts.

unsafe impl DescriptorLayoutInfo for DefaultForwardShaderLayout {
  type BackendDescriptorSetLayoutBindings = [vk::DescriptorSetLayoutBinding; 2];

  fn get_descriptor_set_layout_bindings() -> Self::BackendDescriptorSetLayoutBindings {
    // Create the bindings for each part of the uniform.
    [
      vk::DescriptorSetLayoutBinding::builder()
      .binding(0)
      .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
      .descriptor_count(1) // If this uniform contained an array (like of lights, or transforms for each bone for animation) this is how many.
      // TODO PERFORMANCE measure forwarding enables from vertex shader as flat vs making accessible from fragment shader.
      .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT) // used in the vertex and fragment shader.
      // .immutable_samplers() no samplers since there's no textures. 
      .build(),
      vk::DescriptorSetLayoutBinding::builder()
        .binding(1)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT)
        .build(),
    ]
  }

  fn get_bind_uniform_info() -> SarektResult<BindUniformInfo> {
    Ok(BindUniformInfo {
      bindings: vec![0],
      offset: 0u64,
      range: std::mem::size_of::<DefaultForwardShaderLayout>() as u64,
    })
  }

  fn get_bind_texture_info() -> SarektResult<BindTextureInfo> {
    Ok(BindTextureInfo { bindings: vec![1] })
  }
}
