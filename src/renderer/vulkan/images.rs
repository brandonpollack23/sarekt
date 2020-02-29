use ash::{version::DeviceV1_0, vk, Device};
use log::info;
use std::sync::Arc;

pub struct ImageAndView {
  pub image: vk::Image,
  pub view: vk::ImageView,
  logical_device: Arc<Device>,
}
impl ImageAndView {
  /// Creates an image and imageview pairing, with a Drop implementation.
  /// Unsafe because logical_device must outlive it.
  pub unsafe fn new(logical_device: Arc<Device>, image: vk::Image, view: vk::ImageView) -> Self {
    Self {
      image,
      view,
      logical_device,
    }
  }
}
impl Drop for ImageAndView {
  fn drop(&mut self) {
    unsafe {
      info!("Destrying render target view...");
      self.logical_device.destroy_image_view(self.view, None);
    }
  }
}
