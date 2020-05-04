use crate::{
  error::{SarektError, SarektResult},
  image_data::ImageDataFormat,
  renderer::{
    buffers_and_images::{BufferImageHandle, BufferImageStore},
    config::NumSamples,
    vulkan::vulkan_buffer_image_functions::ImageAndMemory,
    VulkanBufferImageFunctions,
  },
};
use ash::{version::InstanceV1_0, vk, Instance};
use std::{
  convert::TryInto,
  sync::{Arc, RwLock},
};

/// All resources relating to the Depth buffer (z buffer).
/// This includes the image handle, the image that references, and the format.
pub struct DepthAttachment {
  pub depth_buffer_image_handle: BufferImageHandle<VulkanBufferImageFunctions>,
  pub image_and_memory: ImageAndMemory,
  pub format: vk::Format,
}
impl DepthAttachment {
  pub fn new(
    instance: &Instance, physical_device: vk::PhysicalDevice,
    buffer_image_store: &Arc<RwLock<BufferImageStore<VulkanBufferImageFunctions>>>,
    extent: (u32, u32), num_msaa_samples: NumSamples,
  ) -> SarektResult<DepthAttachment> {
    let format = Self::find_depth_format(instance, physical_device)?;
    let (depth_buffer_image_handle, buffer_or_image) =
      BufferImageStore::create_uninitialized_image_msaa(
        buffer_image_store,
        extent,
        format.try_into()?,
        num_msaa_samples,
      )?;

    let image_and_memory = buffer_or_image.handle.image().unwrap();

    Ok(DepthAttachment {
      depth_buffer_image_handle,
      image_and_memory,
      format,
    })
  }

  fn find_supported_format(
    instance: &Instance, physical_device: vk::PhysicalDevice, format_candidates: &[vk::Format],
    tiling: vk::ImageTiling, features: vk::FormatFeatureFlags,
  ) -> SarektResult<vk::Format> {
    for &format in format_candidates.iter() {
      let props =
        unsafe { instance.get_physical_device_format_properties(physical_device, format) };

      if tiling == vk::ImageTiling::LINEAR && (props.linear_tiling_features & features) == features
        || (tiling == vk::ImageTiling::OPTIMAL
          && (props.optimal_tiling_features & features) == features)
      {
        // linear tiling requested and supported by this format.
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

/// Used for resolving MSAA, see https://www.khronos.org/registry/vulkan/specs/1.2-khr-extensions/html/chap7.html#VkSubpassDescription
pub struct ResolveAttachment {
  pub resolve_image_handle: BufferImageHandle<VulkanBufferImageFunctions>,
  pub resolve_image: ImageAndMemory,
  pub format: vk::Format,
}
impl ResolveAttachment {
  pub fn new(
    buffer_image_store: &Arc<RwLock<BufferImageStore<VulkanBufferImageFunctions>>>,
    dimensions: (u32, u32), format: ImageDataFormat, num_msaa_samples: NumSamples,
  ) -> SarektResult<ResolveAttachment> {
    let (resolve_image_handle, resolve_image) = BufferImageStore::create_uninitialized_image_msaa(
      buffer_image_store,
      dimensions,
      format,
      num_msaa_samples,
    )?;

    Ok(ResolveAttachment {
      resolve_image_handle,
      resolve_image: resolve_image.handle.image()?,
      format: format
        .try_into()
        .expect("Format not supported by sarekt for msaa color buffer"),
    })
  }
}
