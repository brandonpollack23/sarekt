use crate::error::SarektError::CStrError;
use ash::InstanceError;
use std::{error::Error, ffi::NulError, fmt};

pub type SarektResult<T> = Result<T, SarektError>;

// TODO add more details to these errors
#[derive(Debug)]
pub enum SarektError {
  Unknown,
  CouldNotCreateInstance(&'static str, InstanceError),
  CStrError(NulError),
  CouldNotQueryDeviceSurfaceSupport,
  CouldNotEnumerateExtensionsForWindowSystem,
  CouldNotSelectPhysicalDevice,
  CouldNotCreateLogicalDevice,
  CouldNotCreateSurface,
}

impl fmt::Display for SarektError {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match self {
      SarektError::Unknown => write!(f, "Unknown Error"),
      SarektError::CouldNotCreateInstance(s, ie) => write!(f, "{} caused by {:?}", s, ie),
      SarektError::CStrError(e) => write!(f, "{}", e),
      SarektError::CouldNotQueryDeviceSurfaceSupport => {
        write!(f, "Could Not Query Device Surface Support")
      }
      SarektError::CouldNotEnumerateExtensionsForWindowSystem => {
        write!(f, "Could Not Select Physical Device")
      }
      SarektError::CouldNotSelectPhysicalDevice => write!(f, "Could Not Select Physical Device"),
      SarektError::CouldNotCreateLogicalDevice => write!(f, "Could Not Create Logical Device"),
      SarektError::CouldNotCreateSurface => write!(f, "Could Not Create Surface"),
    }
  }
}

impl Error for SarektError {}

impl From<NulError> for SarektError {
  fn from(e: NulError) -> SarektError {
    CStrError(e)
  }
}
