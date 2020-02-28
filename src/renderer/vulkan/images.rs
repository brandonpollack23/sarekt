use ash::{version::DeviceV1_0, vk, Device};
use log::info;

pub struct ImageAndView {
  pub image: vk::Image,
  pub view: vk::ImageView,
  device_handle: vk::Device,
  destroy_image_view_fn: vk::PFN_vkDestroyImageView,
}
impl ImageAndView {
  /// Creates an image and imageview pairing, with a Drop implementation.
  /// Unsafe because logical_device must outlive it.
  pub unsafe fn new(logical_device: &Device, image: vk::Image, view: vk::ImageView) -> Self {
    Self {
      image,
      view,
      device_handle: logical_device.handle(),
      destroy_image_view_fn: logical_device.fp_v1_0().destroy_image_view,
    }
  }
}
impl Drop for ImageAndView {
  fn drop(&mut self) {
    unsafe {
      info!("Destrying render target view...");
      let destroy_image_view_fn = &self.destroy_image_view_fn;
      destroy_image_view_fn(self.device_handle, self.view, std::ptr::null());
    }
  }
}
