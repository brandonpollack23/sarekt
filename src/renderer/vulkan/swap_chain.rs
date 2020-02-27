use ash::vk;
pub struct SwapchainSupportDetails {
  pub capabilities: vk::SurfaceCapabilitiesKHR,
  pub formats: Vec<vk::SurfaceFormatKHR>,
  pub present_modes: Vec<vk::PresentModeKHR>,
}
impl SwapchainSupportDetails {
  pub fn new(
    capabilities: vk::SurfaceCapabilitiesKHR, formats: Vec<vk::SurfaceFormatKHR>,
    present_modes: Vec<vk::PresentModeKHR>,
  ) -> Self {
    Self {
      capabilities,
      formats,
      present_modes,
    }
  }
}
