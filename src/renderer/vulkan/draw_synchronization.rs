use crate::{error::SarektResult, renderer::MAX_FRAMES_IN_FLIGHT};
use ash::{version::DeviceV1_0, vk, Device};
use log::info;
use std::cell::Cell;

/// Draw synchronization primitives for frames in flight and synchronizing
/// between acquiring images, presenting them.
/// Also contains some helper methods.
pub struct DrawSynchronization {
  pub image_available_semaphores: Vec<vk::Semaphore>,
  pub render_finished_semaphores: Vec<vk::Semaphore>,
  pub in_flight_fences: Vec<vk::Fence>,

  // Unowned tracking references to in_flight_fences.  This is to track which in flight fences
  // correspond to which images that are in flight.
  pub images_in_flight: Vec<Cell<vk::Fence>>,
}
impl DrawSynchronization {
  pub fn new(logical_device: &Device, num_render_targets: usize) -> SarektResult<Self> {
    let semaphore_ci = vk::SemaphoreCreateInfo::default();
    let fence_ci = vk::FenceCreateInfo::builder()
      .flags(vk::FenceCreateFlags::SIGNALED)
      .build();
    let mut image_available_semaphores = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    let mut render_finished_semaphores = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    let mut in_flight_fences = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    for _ in 0..MAX_FRAMES_IN_FLIGHT {
      unsafe {
        image_available_semaphores.push(logical_device.create_semaphore(&semaphore_ci, None)?);
        render_finished_semaphores.push(logical_device.create_semaphore(&semaphore_ci, None)?);
        in_flight_fences.push(logical_device.create_fence(&fence_ci, None)?);
      }
    }

    Ok(Self {
      image_available_semaphores,
      render_finished_semaphores,
      in_flight_fences,
      images_in_flight: vec![Cell::new(vk::Fence::null()); num_render_targets],
    })
  }

  /// Ensures that the image is not currently in flight and marks it to be for
  /// this upcoming draw.
  pub fn ensure_images_not_in_flight(
    &self, logical_device: &Device, image_index: usize, current_frame_num: usize,
  ) -> SarektResult<()> {
    if current_frame_num >= MAX_FRAMES_IN_FLIGHT || image_index >= self.images_in_flight.len() {
      panic!(
        "Invalid input! image_index: {} current_frame_num: {}",
        image_index, current_frame_num
      );
    }

    let image_in_flight_fence = self.images_in_flight[image_index as usize].get();

    if image_in_flight_fence != vk::Fence::null() {
      // It wasn't null, that swapchain image is in flight!
      unsafe { logical_device.wait_for_fences(&[image_in_flight_fence], true, u64::max_value())? };
    }

    // Mark the image as in use by this frame.
    self.images_in_flight[image_index as usize].set(self.in_flight_fences[current_frame_num]);

    Ok(())
  }

  pub unsafe fn destroy_all(&self, logical_device: &Device) {
    info!("Destroying all synchronization primitives...");
    for &sem in self.image_available_semaphores.iter() {
      logical_device.destroy_semaphore(sem, None);
    }
    for &sem in self.render_finished_semaphores.iter() {
      logical_device.destroy_semaphore(sem, None);
    }
    for &fence in self.in_flight_fences.iter() {
      logical_device.destroy_fence(fence, None);
    }
  }
}
