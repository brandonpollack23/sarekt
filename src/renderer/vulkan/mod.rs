use crate::{
  error::{SarektError, SarektResult},
  image_data::ImageDataFormat,
  renderer::{
    config::NumSamples, vulkan::vulkan_shader_functions::VulkanShaderFunctions, ShaderHandle,
  },
};
use ash::vk;
use std::convert::TryFrom;

pub mod images;
pub mod queues;
pub mod vulkan_buffer_image_functions;
pub mod vulkan_renderer;
pub mod vulkan_shader_functions;
pub mod vulkan_vertex_bindings;

pub type VulkanShaderHandle = ShaderHandle<VulkanShaderFunctions>;

impl From<NumSamples> for vk::SampleCountFlags {
  fn from(num_samples: NumSamples) -> vk::SampleCountFlags {
    match num_samples {
      NumSamples::One => vk::SampleCountFlags::TYPE_1,
      NumSamples::Two => vk::SampleCountFlags::TYPE_2,
      NumSamples::Four => vk::SampleCountFlags::TYPE_4,
      NumSamples::Eight => vk::SampleCountFlags::TYPE_8,
    }
  }
}

impl From<ImageDataFormat> for vk::Format {
  fn from(image_data_format: ImageDataFormat) -> vk::Format {
    match image_data_format {
      ImageDataFormat::R8G8B8Srgb => vk::Format::R8G8B8_SRGB,
      ImageDataFormat::B8G8R8Srgb => vk::Format::B8G8R8_SRGB,
      ImageDataFormat::B8G8R8A8Srgb => vk::Format::B8G8R8A8_SRGB,
      ImageDataFormat::R8G8B8A8Srgb => vk::Format::R8G8B8A8_SRGB,

      ImageDataFormat::R8G8B8Unorm => vk::Format::R8G8B8_UNORM,
      ImageDataFormat::B8G8R8Unorm => vk::Format::B8G8R8_UNORM,
      ImageDataFormat::B8G8R8A8Unorm => vk::Format::B8G8R8A8_UNORM,
      ImageDataFormat::R8G8B8A8Unorm => vk::Format::R8G8B8A8_UNORM,

      ImageDataFormat::RGB16Unorm => vk::Format::R5G6B5_UNORM_PACK16,
      ImageDataFormat::RGBA16Unorm => vk::Format::R5G5B5A1_UNORM_PACK16,

      ImageDataFormat::D32Float => vk::Format::D32_SFLOAT,
      ImageDataFormat::D32FloatS8 => vk::Format::D32_SFLOAT_S8_UINT,
      ImageDataFormat::D24NormS8 => vk::Format::D24_UNORM_S8_UINT,
    }
  }
}

impl TryFrom<vk::Format> for ImageDataFormat {
  type Error = SarektError;

  fn try_from(format: vk::Format) -> SarektResult<ImageDataFormat> {
    match format {
      vk::Format::R8G8B8_SRGB => Ok(ImageDataFormat::R8G8B8Srgb),
      vk::Format::B8G8R8_SRGB => Ok(ImageDataFormat::B8G8R8Srgb),
      vk::Format::B8G8R8A8_SRGB => Ok(ImageDataFormat::B8G8R8A8Srgb),
      vk::Format::R8G8B8A8_SRGB => Ok(ImageDataFormat::R8G8B8A8Srgb),

      vk::Format::R8G8B8_UNORM => Ok(ImageDataFormat::R8G8B8Unorm),
      vk::Format::B8G8R8_UNORM => Ok(ImageDataFormat::B8G8R8Unorm),
      vk::Format::B8G8R8A8_UNORM => Ok(ImageDataFormat::B8G8R8A8Unorm),
      vk::Format::R8G8B8A8_UNORM => Ok(ImageDataFormat::R8G8B8A8Unorm),

      vk::Format::R5G6B5_UNORM_PACK16 => Ok(ImageDataFormat::RGB16Unorm),
      vk::Format::R5G5B5A1_UNORM_PACK16 => Ok(ImageDataFormat::RGBA16Unorm),

      vk::Format::D32_SFLOAT => Ok(ImageDataFormat::D32Float),
      vk::Format::D32_SFLOAT_S8_UINT => Ok(ImageDataFormat::D32FloatS8),
      vk::Format::D24_UNORM_S8_UINT => Ok(ImageDataFormat::D24NormS8),

      _ => Err(SarektError::UnsupportedImageFormat),
    }
  }
}
