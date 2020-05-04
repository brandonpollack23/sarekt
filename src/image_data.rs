use crate::{
  error::{SarektError, SarektResult},
  image_data::ImageDataFormat::*,
};
use safe_transmute::to_bytes::transmute_to_bytes_vec;

/// The trait used for loading images into Sarekt.  An implementation is
/// provided for the rust [image](https://crates.io/crates/image) crate.  Feel free to create one in your own project for other crates (by wrapping in a newtype to avoid the orphan problem).
pub trait ImageData {
  /// Returns byte color array of pixels.
  fn into_bytes(self) -> Vec<u8>;

  /// Converts to rgba8, a format that must be supported by at least the Vulkan
  /// backend
  fn into_rgba8(self) -> Self;

  /// Returns (width, height) of the image.
  fn dimensions(&self) -> (u32, u32);

  /// Underlying image format.
  fn format(&self) -> SarektResult<ImageDataFormat>;
}

#[derive(Copy, Clone, Debug)]
pub enum ImageDataFormat {
  R8G8B8Srgb,
  B8G8R8Srgb,
  B8G8R8A8Srgb,
  R8G8B8A8Srgb,

  R8G8B8Unorm,
  B8G8R8Unorm,
  B8G8R8A8Unorm,
  R8G8B8A8Unorm,
  RGB16Unorm,
  RGBA16Unorm,
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

  fn into_rgba8(self) -> Self {
    image::DynamicImage::ImageRgba8(self.into_rgba())
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
      image::DynamicImage::ImageBgr8(_) => Ok(B8G8R8A8Srgb),
      image::DynamicImage::ImageLuma8(_) => Err(SarektError::UnsupportedImageFormat),
      image::DynamicImage::ImageLumaA8(_) => Err(SarektError::UnsupportedImageFormat),
      image::DynamicImage::ImageRgb8(_) => Ok(R8G8B8Srgb),
      image::DynamicImage::ImageRgba8(_) => Ok(R8G8B8A8Srgb),
      image::DynamicImage::ImageBgra8(_) => Ok(B8G8R8A8Srgb),
      image::DynamicImage::ImageLuma16(_) => Err(SarektError::UnsupportedImageFormat),
      image::DynamicImage::ImageLumaA16(_) => Err(SarektError::UnsupportedImageFormat),
      image::DynamicImage::ImageRgb16(_) => Ok(RGB16Unorm),
      image::DynamicImage::ImageRgba16(_) => Ok(RGBA16Unorm),
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
    Self::new(1, 1, 1, 0)
  }
}
impl ImageData for Monocolor {
  fn into_bytes(self) -> Vec<u8> {
    self.inner.to_vec()
  }

  fn into_rgba8(self) -> Self {
    self
  }

  fn dimensions(&self) -> (u32, u32) {
    (1, 1)
  }

  fn format(&self) -> SarektResult<ImageDataFormat> {
    Ok(R8G8B8A8Srgb)
  }
}
