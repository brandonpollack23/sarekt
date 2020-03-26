/// The trait used for loading images into Sarekt.  An implementation is
/// provided for the rust [image](https://crates.io/crates/image) crate.  Feel free to create one in your own project for other crates (by wrapping in a newtype to avoid the orphan problem).
pub trait ImageData {
  /// Returns r8g8b8a8 32 bit (4 byte) color array of pixels.
  fn into_rbga_32(self) -> Vec<u8>;

  /// Returns (width, height) of the image.
  fn dimensions(&self) -> (u32, u32);
}

impl ImageData for image::DynamicImage {
  fn into_rbga_32(self) -> Vec<u8> {
    self.into_rgba().into_vec()
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
  fn into_rbga_32(self) -> Vec<u8> {
    self.inner.to_vec()
  }

  fn dimensions(&self) -> (u32, u32) {
    (1, 1)
  }
}
