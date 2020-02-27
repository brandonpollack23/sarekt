use ash::vk;
pub struct SwapchainAndExtension {
  pub swapchain: vk::SwapchainKHR,
  pub swapchain_functions: ash::extensions::khr::Swapchain,
}
impl SwapchainAndExtension {
  pub fn new(
    swapchain: vk::SwapchainKHR, swapchain_functions: ash::extensions::khr::Swapchain,
  ) -> Self {
    SwapchainAndExtension {
      swapchain,
      swapchain_functions,
    }
  }
}

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
