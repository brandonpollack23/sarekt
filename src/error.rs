use crate::error::SarektError::CStrError;

use ash::vk;
use std::{error::Error, ffi::NulError, fmt};

pub type SarektResult<T> = Result<T, SarektError>;

#[derive(Debug)]
pub enum SarektError {
  Unknown,
  CouldNotSelectPhysicalDevice,
  SuboptimalSwapchain,
  SwapchainOutOfDate,
  CStrError(NulError),
  VulkanError(vk::Result),
  InstanceError(ash::InstanceError),
  UnknownShader,
  IncompatibleShaderCode,
  IncorrectLoaderFunction,
  NoUniformBuffer,
  IncorrectBufferType,
  UnknownBuffer,
  NoSuitableMemoryHeap,
  VulkanMemoryAllocatorError(vk_mem::error::Error),
}

impl From<vk::Result> for SarektError {
  fn from(e: vk::Result) -> Self {
    match e {
      vk::Result::SUBOPTIMAL_KHR => SarektError::SuboptimalSwapchain,
      vk::Result::ERROR_OUT_OF_DATE_KHR => SarektError::SwapchainOutOfDate,
      e => SarektError::VulkanError(e),
    }
  }
}
impl From<ash::InstanceError> for SarektError {
  fn from(e: ash::InstanceError) -> Self {
    SarektError::InstanceError(e)
  }
}
impl From<vk_mem::error::Error> for SarektError {
  fn from(e: vk_mem::Error) -> Self {
    SarektError::VulkanMemoryAllocatorError(e)
  }
}
impl From<NulError> for SarektError {
  fn from(e: NulError) -> SarektError {
    CStrError(e)
  }
}

impl fmt::Display for SarektError {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match self {
      SarektError::Unknown => write!(f, "Unknown Error"),
      SarektError::SwapchainOutOfDate => write!(
        f,
        "Swapchain is out of date, try using recreate_swapchain method"
      ),
      SarektError::SuboptimalSwapchain => write!(
        f,
        "Swapchain suboptimal, try using recreate_swapchain method"
      ),
      SarektError::VulkanError(r) => write!(f, "Vulkan Error: {}", r),
      SarektError::InstanceError(e) => write!(f, "The vulkan wrapper ash produced an error: {}", e),
      SarektError::UnknownShader => write!(f, "Tried to act on unknown shader"),
      SarektError::UnknownBuffer => write!(f, "Tried to act on unknown buffer"),
      SarektError::IncorrectLoaderFunction => write!(
        f,
        "Attempted to load a special buffer type with the generic load_buffer function.  Did you \
         mean to use load_uniform buffer?"
      ),
      SarektError::NoUniformBuffer => {
        write!(f, "There is no uniform buffer to access in this object")
      }
      SarektError::IncorrectBufferType => write!(
        f,
        "Tried to load a buffer type that didn't match with function call.  Perhaps you've \
         tricked Sarekt into storing a Vertex buffer where it should have been a Uniform buffer?"
      ),
      SarektError::NoSuitableMemoryHeap => write!(
        f,
        "Could not find memory heap that was suitable for the device allocation."
      ),
      SarektError::VulkanMemoryAllocatorError(e) => {
        write!(f, "Vulkan memory allocator error: {}", e)
      }
      SarektError::IncompatibleShaderCode => {
        write!(f, "Tried to load an incompatible shader type into backend")
      }
      SarektError::CouldNotSelectPhysicalDevice => {
        write!(f, "Sarekt could not find a suitable physical device")
      }
      SarektError::CStrError(e) => write!(f, "{}", e),
    }
  }
}

impl Error for SarektError {}
