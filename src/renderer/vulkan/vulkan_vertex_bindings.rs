use crate::renderer::vertex_bindings::{
  DefaultForwardShaderUniforms, DefaultForwardShaderVertex, DescriptorLayoutInfo, VertexBindings,
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
      .format(vk::Format::R32G32_SFLOAT) // RG is unintuitive but the point is its two floats.
      .offset(offset_of!(DefaultForwardShaderVertex, position) as u32)
      .build();
    let color_attr = vk::VertexInputAttributeDescription::builder()
      .binding(0) // Which binding in the shader.
      .location(1) // The layout location in the shader.
      .format(vk::Format::R32G32B32_SFLOAT) // RGB is unintuitive but the point is its two floats.
      .offset(offset_of!(DefaultForwardShaderVertex, color) as u32)
      .build();

    vec![position_attr, color_attr]
  }
}

// TODO SHADERS use reflection to generate descriptor set layouts.

unsafe impl DescriptorLayoutInfo for DefaultForwardShaderUniforms {
  type BackendDescriptorSetLayoutBindings = vk::DescriptorSetLayoutBinding;

  fn get_descriptor_set_layout_bindings() -> Self::BackendDescriptorSetLayoutBindings {
    // Create the bindings for each part of the uniform.
    let layout_binding = vk::DescriptorSetLayoutBinding::builder()
      .binding(0)
      .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
      .descriptor_count(1) // If this uniform contained an array (like of lights, or transforms for each bone for animation) this is how many.
      .stage_flags(vk::ShaderStageFlags::VERTEX) // used in the vertex shader.
      // .immutable_samplers() no samplers since there's no textures. 
      .build();

    layout_binding
  }
}
