use ash::vk;

#[derive(Debug)]
pub struct ImageAndView {
  pub image: vk::Image,
  pub view: vk::ImageView,
}
impl ImageAndView {
  /// Creates an image and imageview pairing, with a Drop implementation.
  /// Unsafe because you must clean up the vk::Image and vk::ImageView still.
  pub unsafe fn new(image: vk::Image, view: vk::ImageView) -> Self {
    Self { image, view }
  }
}
