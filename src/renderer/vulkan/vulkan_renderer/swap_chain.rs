use ash::vk;

/// Wrapper for the swapchain, its format, and various methods.
pub struct SwapchainAndExtension {
  pub swapchain: vk::SwapchainKHR,
  pub format: vk::Format,
  pub swapchain_functions: ash::extensions::khr::Swapchain,
}
impl SwapchainAndExtension {
  pub fn new(
    swapchain: vk::SwapchainKHR, format: vk::Format,
    swapchain_functions: ash::extensions::khr::Swapchain,
  ) -> Self {
    SwapchainAndExtension {
      swapchain,
      format,
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
