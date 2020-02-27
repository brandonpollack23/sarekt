use ash::vk;

pub struct ImageAndView {
  pub image: vk::Image,
  pub view: vk::ImageView,
}
impl ImageAndView {
  pub fn new(image: vk::Image, view: vk::ImageView) -> Self {
    Self { image, view }
  }
}
