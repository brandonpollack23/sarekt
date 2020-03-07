use crate::renderer::vertex_bindings::{DefaultForwardShaderVertex, VertexBindings};
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
