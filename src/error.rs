use crate::error::SarektError::CStrError;

use std::{error::Error, ffi::NulError, fmt};

pub type SarektResult<T> = Result<T, SarektError>;

#[derive(Debug)]
pub enum SarektError {
  Unknown,
  VulkanError(ash::vk::Result),
  InstanceError(ash::InstanceError),
  UnknownShader,
  CouldNotSelectPhysicalDevice,
  CStrError(NulError),
}

impl From<ash::vk::Result> for SarektError {
  fn from(e: ash::vk::Result) -> Self {
    SarektError::VulkanError(e)
  }
}
impl From<ash::InstanceError> for SarektError {
  fn from(e: ash::InstanceError) -> Self {
    SarektError::InstanceError(e)
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
      SarektError::VulkanError(r) => write!(f, "Vulkan Error: {}", r),
      SarektError::InstanceError(e) => write!(f, "The vulkan wrapper ash produced an error: {}", e),
      SarektError::UnknownShader => write!(f, "Tried to act on unknown shader"),
      SarektError::CouldNotSelectPhysicalDevice => {
        write!(f, "Sarekt could not find a suitable physical device")
      }
      SarektError::CStrError(e) => write!(f, "{}", e),
    }
  }
}

impl Error for SarektError {}
