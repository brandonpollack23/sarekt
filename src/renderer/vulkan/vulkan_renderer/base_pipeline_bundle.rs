use crate::{
  error::SarektResult,
  image_data::ImageDataFormat,
  renderer::{
    buffers_and_images::{BufferImageHandle, BufferImageStore},
    config::NumSamples,
    vulkan::{
      vulkan_buffer_image_functions::ImageAndMemory, vulkan_renderer::depth_buffer::DepthResources,
      vulkan_shader_functions::VulkanShaderFunctions, VulkanShaderHandle,
    },
    ShaderHandle, VulkanBufferImageFunctions,
  },
};
use ash::vk;
use std::{
  convert::TryInto,
  sync::{Arc, RwLock},
};

/// Just a pipeline bundle to help return when creating the base pipeline.
pub struct BasePipelineBundle {
  pub pipeline: vk::Pipeline,
  pub pipeline_layout: vk::PipelineLayout,
  pub pipeline_create_info: vk::GraphicsPipelineCreateInfo,
  pub resolve_attachment: ResolveAttachment,
  pub depth_resources: Option<DepthResources>,
  pub descriptor_set_layouts: Option<Vec<vk::DescriptorSetLayout>>,
  pub vertex_shader_handle: Option<VulkanShaderHandle>,
  pub fragment_shader_handle: Option<VulkanShaderHandle>,
}
impl BasePipelineBundle {
  pub fn new(
    pipeline: vk::Pipeline, pipeline_layout: vk::PipelineLayout,
    pipeline_create_info: vk::GraphicsPipelineCreateInfo,
    descriptor_set_layouts: Vec<vk::DescriptorSetLayout>, resolve_attachment: ResolveAttachment,
    depth_resources: DepthResources, vertex_shader_handle: ShaderHandle<VulkanShaderFunctions>,
    fragment_shader_handle: ShaderHandle<VulkanShaderFunctions>,
  ) -> Self {
    Self {
      pipeline,
      pipeline_layout,
      pipeline_create_info,
      resolve_attachment,
      depth_resources: Some(depth_resources),
      descriptor_set_layouts: Some(descriptor_set_layouts),
      vertex_shader_handle: Some(vertex_shader_handle),
      fragment_shader_handle: Some(fragment_shader_handle),
    }
  }
}

// TODO NOW put this in depth_buffer and rename it render_resources and make it
// similar to depth. Maybe dont make this if MSAA is 1, Waste
// of resources maybe.
pub struct ResolveAttachment {
  pub msaa_color_image_handle: BufferImageHandle<VulkanBufferImageFunctions>,
  pub msaa_color_image: ImageAndMemory,
  pub format: vk::Format,
}
impl ResolveAttachment {
  pub fn new(
    buffer_image_store: &Arc<RwLock<BufferImageStore<VulkanBufferImageFunctions>>>,
    dimensions: (u32, u32), format: ImageDataFormat, num_msaa_samples: NumSamples,
  ) -> SarektResult<ResolveAttachment> {
    let (msaa_color_image_handle, msaa_color_image) =
      BufferImageStore::create_uninitialized_image_msaa(
        buffer_image_store,
        dimensions,
        format,
        num_msaa_samples,
      )?;

    Ok(ResolveAttachment {
      msaa_color_image_handle,
      msaa_color_image: msaa_color_image.handle.image()?,
      format: format
        .try_into()
        .expect("Format not supported by sarekt for msaa color buffer"),
    })
  }
}
