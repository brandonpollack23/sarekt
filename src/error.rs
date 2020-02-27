use crate::error::SarektError::CStrError;
use ash;
use std::{error::Error, ffi::NulError, fmt};

pub type SarektResult<T> = Result<T, SarektError>;

// TODO add more details to these errors
#[derive(Debug)]
pub enum SarektError {
  Unknown,
  InstanceError(ash::InstanceError),
  CStrError(NulError),
}

impl From<ash::InstanceError> for SarektError {
  fn from(instance_error: ash::InstanceError) -> Self {
    SarektError::InstanceError(instance_error)
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
      SarektError::InstanceError(ash_error) => {
        write!(f, "The vulkan wrapper ash produced an error: {}", ash_error)
      }
      SarektError::CStrError(e) => write!(f, "{}", e),
    }
  }
}

impl Error for SarektError {}

