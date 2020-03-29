use crate::{
  error::{SarektError, SarektResult},
  image_data::ImageDataFormat::*,
};
use safe_transmute::to_bytes::transmute_to_bytes_vec;

/// The trait used for loading images into Sarekt.  An implementation is
/// provided for the rust [image](https://crates.io/crates/image) crate.  Feel free to create one in your own project for other crates (by wrapping in a newtype to avoid the orphan problem).
pub trait ImageData {
  /// Returns r8g8b8a8 32 bit (4 byte) color array of pixels.
  fn into_bytes(self) -> Vec<u8>;

  /// Returns (width, height) of the image.
  fn dimensions(&self) -> (u32, u32);

  /// Underlying image format.
  fn format(&self) -> SarektResult<ImageDataFormat>;
}

pub enum ImageDataFormat {
  R8G8B8,
  B8G8R8,
  B8G8R8A8,
  R8G8B8A8,
  RGB16,
  RGBA16,
  // Depth Buffer Formats
  D32Float,
  D32FloatS8,
  D24NormS8,
}

impl ImageData for image::DynamicImage {
  fn into_bytes(self) -> Vec<u8> {
    match self {
      image::DynamicImage::ImageBgr8(img) => img.into_raw(),
      image::DynamicImage::ImageLuma8(img) => img.into_raw(),
      image::DynamicImage::ImageLumaA8(img) => img.into_raw(),
      image::DynamicImage::ImageRgb8(img) => img.into_raw(),
      image::DynamicImage::ImageRgba8(img) => img.into_raw(),
      image::DynamicImage::ImageBgra8(img) => img.into_raw(),
      image::DynamicImage::ImageLuma16(img) => transmute_to_bytes_vec(img.into_raw()).unwrap(),
      image::DynamicImage::ImageLumaA16(img) => transmute_to_bytes_vec(img.into_raw()).unwrap(),
      image::DynamicImage::ImageRgb16(img) => transmute_to_bytes_vec(img.into_raw()).unwrap(),
      image::DynamicImage::ImageRgba16(img) => transmute_to_bytes_vec(img.into_raw()).unwrap(),
    }
  }

  fn dimensions(&self) -> (u32, u32) {
    match self {
      image::DynamicImage::ImageBgr8(img) => img.dimensions(),
      image::DynamicImage::ImageLuma8(img) => img.dimensions(),
      image::DynamicImage::ImageLumaA8(img) => img.dimensions(),
      image::DynamicImage::ImageRgb8(img) => img.dimensions(),
      image::DynamicImage::ImageRgba8(img) => img.dimensions(),
      image::DynamicImage::ImageBgra8(img) => img.dimensions(),
      image::DynamicImage::ImageLuma16(img) => img.dimensions(),
      image::DynamicImage::ImageLumaA16(img) => img.dimensions(),
      image::DynamicImage::ImageRgb16(img) => img.dimensions(),
      image::DynamicImage::ImageRgba16(img) => img.dimensions(),
    }
  }

  fn format(&self) -> SarektResult<ImageDataFormat> {
    match self {
      image::DynamicImage::ImageBgr8(img) => Ok(B8G8R8A8),
      image::DynamicImage::ImageLuma8(_) => Err(SarektError::UnsupportedImageFormat),
      image::DynamicImage::ImageLumaA8(_) => Err(SarektError::UnsupportedImageFormat),
      image::DynamicImage::ImageRgb8(img) => Ok(R8G8B8),
      image::DynamicImage::ImageRgba8(img) => Ok(R8G8B8A8),
      image::DynamicImage::ImageBgra8(img) => Ok(B8G8R8A8),
      image::DynamicImage::ImageLuma16(_) => Err(SarektError::UnsupportedImageFormat),
      image::DynamicImage::ImageLumaA16(_) => Err(SarektError::UnsupportedImageFormat),
      image::DynamicImage::ImageRgb16(img) => Ok(RGB16),
      image::DynamicImage::ImageRgba16(img) => Ok(RGBA16),
    }
  }
}

/// Image data that represents a single color.
pub struct Monocolor {
  inner: [u8; 4],
}
impl Monocolor {
  pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
    Monocolor {
      inner: [r, g, b, a],
    }
  }

  pub fn clear() -> Self {
    Self::new(0, 0, 0, 0)
  }
}
impl ImageData for Monocolor {
  fn into_bytes(self) -> Vec<u8> {
    self.inner.to_vec()
  }

  fn dimensions(&self) -> (u32, u32) {
    (1, 1)
  }

  fn format(&self) -> SarektResult<ImageDataFormat> {
    Ok(R8G8B8A8)
  }
}
