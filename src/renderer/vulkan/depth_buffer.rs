use crate::{
  error::{SarektError, SarektResult},
  renderer::{
    buffers_and_images::BufferImageStore, vulkan::vulkan_buffer_image_functions::ImageAndMemory,
    VulkanBufferFunctions,
  },
};
use ash::{version::InstanceV1_0, vk, Device, Instance};
use std::sync::{Arc, RwLock};

struct DepthResources {
  pub depth_image: ImageAndMemory,
}
impl DepthResources {
  fn new(
    instance: &Instance, physical_device: vk::PhysicalDevice,
    buffer_image_store: &Arc<RwLock<BufferImageStore<VulkanBufferFunctions>>>,
  ) -> SarektResult<DepthResources> {
    let format = Self::find_depth_format(instance, physical_device)?;
    // let depth_buffer = BufferImageStore::
    // TODO NOW CONTINUE WITH CREATE NONINIT IMAGE FUNCTION

    Err(SarektError::UnsupportedImageFormat)
  }

  fn find_supported_format(
    instance: &Instance, physical_device: vk::PhysicalDevice, format_candidates: &[vk::Format],
    tiling: vk::ImageTiling, features: vk::FormatFeatureFlags,
  ) -> SarektResult<vk::Format> {
    for &format in format_candidates.iter() {
      let props =
        unsafe { instance.get_physical_device_format_properties(physical_device, format) };

      if tiling == vk::ImageTiling::LINEAR && (props.linear_tiling_features & features) == features
      {
        // linear tiling requested and supported by this format.
        return Ok(format);
      } else if tiling == vk::ImageTiling::OPTIMAL
        && (props.optimal_tiling_features & features) == features
      {
        return Ok(format);
      }
    }

    Err(SarektError::NoSuitableDepthBufferFormat)
  }

  fn find_depth_format(
    instance: &Instance, physical_device: vk::PhysicalDevice,
  ) -> SarektResult<vk::Format> {
    let format_candidates = [
      vk::Format::D32_SFLOAT,
      vk::Format::D32_SFLOAT_S8_UINT,
      vk::Format::D24_UNORM_S8_UINT,
    ];
    let tiling = vk::ImageTiling::OPTIMAL;
    let features = vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT;

    Self::find_supported_format(
      instance,
      physical_device,
      &format_candidates,
      tiling,
      features,
    )
  }

  fn has_stencil_component(format: vk::Format) -> bool {
    format == vk::Format::D32_SFLOAT_S8_UINT || format == vk::Format::D24_UNORM_S8_UINT
  }
}
