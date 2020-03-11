use crate::renderer::{
  vulkan::{vulkan_shader_functions::VulkanShaderFunctions, VulkanShaderHandle},
  ShaderHandle,
};
use ash::vk;

/// Just a pipeline bundle to help return when creating the base pipeline.
pub struct BasePipelineBundle {
  pub pipeline: vk::Pipeline,
  pub pipeline_layout: vk::PipelineLayout,
  pub pipeline_create_info: vk::GraphicsPipelineCreateInfo,
  pub descriptor_set_layouts: Option<Vec<vk::DescriptorSetLayout>>,
  pub vertex_shader_handle: Option<VulkanShaderHandle>,
  pub fragment_shader_handle: Option<VulkanShaderHandle>,
}
impl BasePipelineBundle {
  pub fn new(
    pipeline: vk::Pipeline, pipeline_layout: vk::PipelineLayout,
    pipeline_create_info: vk::GraphicsPipelineCreateInfo,
    descriptor_set_layouts: Vec<vk::DescriptorSetLayout>,
    vertex_shader_handle: ShaderHandle<VulkanShaderFunctions>,
    fragment_shader_handle: ShaderHandle<VulkanShaderFunctions>,
  ) -> Self {
    Self {
      pipeline,
      pipeline_layout,
      pipeline_create_info,
      descriptor_set_layouts: Some(descriptor_set_layouts),
      vertex_shader_handle: Some(vertex_shader_handle),
      fragment_shader_handle: Some(fragment_shader_handle),
    }
  }
}
