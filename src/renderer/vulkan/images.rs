use ash::{version::DeviceV1_0, vk, Device};
use log::info;
use std::sync::Arc;

pub struct ImageAndView {
  pub image: vk::Image,
  pub view: vk::ImageView,
}
impl ImageAndView {
  /// Creates an image and imageview pairing, with a Drop implementation.
  /// Unsafe because logical_device must outlive it.
  pub unsafe fn new(image: vk::Image, view: vk::ImageView) -> Self {
    Self { image, view }
  }
}
