use ash::vk;

pub struct SurfaceAndExtension {
  pub surface: vk::SurfaceKHR,
  pub surface_functions: ash::extensions::khr::Surface,
}
impl SurfaceAndExtension {
  pub fn new(surface: vk::SurfaceKHR, surface_functions: ash::extensions::khr::Surface) -> Self {
    SurfaceAndExtension {
      surface,
      surface_functions,
    }
  }
}
