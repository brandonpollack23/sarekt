use crate::error::SarektError::CStrError;
use ash::InstanceError;
use std::{error::Error, ffi::NulError, fmt};

pub type SarektResult<T> = Result<T, SarektError>;

#[derive(Debug)]
pub enum SarektError {
  Unknown,
  CouldNotCreateInstance(&'static str, InstanceError),
  CStrError(NulError),
}

impl fmt::Display for SarektError {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match self {
      SarektError::Unknown => write!(f, "Unknown Error"),
      SarektError::CouldNotCreateInstance(s, ie) => write!(f, "{} caused by {:?}", s, ie),
      SarektError::CStrError(e) => write!(f, "{}", e),
    }
  }
}

impl Error for SarektError {}

impl From<NulError> for SarektError {
  fn from(e: NulError) -> SarektError {
    CStrError(e)
  }
}
