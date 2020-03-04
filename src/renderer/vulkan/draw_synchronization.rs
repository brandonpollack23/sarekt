use crate::error::SarektResult;
use ash::{version::DeviceV1_0, vk, Device};

pub struct DrawSynchronization {
  pub image_available_semaphore: vk::Semaphore,
  pub render_finished_semaphore: vk::Semaphore,
}
impl DrawSynchronization {
  pub fn new(logical_device: &Device) -> SarektResult<Self> {
    let semaphore_ci = vk::SemaphoreCreateInfo::default();
    let image_available_semaphore =
      unsafe { logical_device.create_semaphore(&semaphore_ci, None)? };
    let render_finished_semaphore =
      unsafe { logical_device.create_semaphore(&semaphore_ci, None)? };

    Ok(Self {
      image_available_semaphore,
      render_finished_semaphore,
    })
  }
}
