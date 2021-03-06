use crate::{error::SarektResult, renderer::MAX_FRAMES_IN_FLIGHT};
use ash::{version::DeviceV1_0, vk, Device};
use log::info;
use std::{cell::Cell, sync::Arc};

/// Draw synchronization primitives for frames in flight and synchronizing
/// between acquiring images, presenting them.
/// Also contains some helper methods.
pub struct DrawSynchronization {
  logical_device: Arc<Device>,
  acquire_fence: vk::Fence,
  image_available_semaphores: Vec<vk::Semaphore>,
  render_finished_semaphores: Vec<vk::Semaphore>,
  frame_fences: Vec<vk::Fence>,

  // Unowned tracking references to in_flight_fences.  This is to track which in flight fences
  // correspond to which images that are in flight.
  image_to_frame_fence: Vec<Cell<vk::Fence>>,
}
impl DrawSynchronization {
  pub fn new(logical_device: Arc<Device>, num_render_targets: usize) -> SarektResult<Self> {
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

    let acquire_fence = unsafe { logical_device.create_fence(&fence_ci, None)? };

    Ok(Self {
      logical_device,
      acquire_fence,
      image_available_semaphores,
      render_finished_semaphores,
      frame_fences: in_flight_fences,
      image_to_frame_fence: vec![Cell::new(vk::Fence::null()); num_render_targets],
    })
  }

  /// Returns fence used for image acquisition for swapchain.
  pub fn get_acquire_fence(&self) -> vk::Fence {
    self.acquire_fence
  }

  /// Waits for the image and its associated objects to be ready to be written
  /// to.
  pub fn wait_for_acquire_fence(&self) -> SarektResult<()> {
    unsafe {
      Ok(
        self
          .logical_device
          .wait_for_fences(&[self.acquire_fence], true, u64::max_value())?,
      )
    }
  }

  pub fn reset_acquire_fence(&self) -> SarektResult<()> {
    unsafe { Ok(self.logical_device.reset_fences(&[self.acquire_fence])?) }
  }

  /// Returns fence associated with swapchain image, with bounds checking.
  pub fn get_image_fence(&self, image_index: usize) -> vk::Fence {
    if image_index >= self.image_to_frame_fence.len() {
      panic!("Invalid input! image_index {}", image_index);
    }
    self.image_to_frame_fence[image_index].get()
  }

  /// Returns semaphore associated with swapchain image availability, with
  /// bounds checking.
  pub fn get_image_available_sem(&self, current_frame_num: usize) -> vk::Semaphore {
    if current_frame_num >= MAX_FRAMES_IN_FLIGHT {
      panic!("Invalid input! current_frame_num {}", current_frame_num);
    }
    self.image_available_semaphores[current_frame_num]
  }

  /// Returns semaphore associated with swapchain image render output to COLOR
  /// attachment, with bounds checking.
  pub fn get_render_finished_semaphore(&self, current_frame_num: usize) -> vk::Semaphore {
    if current_frame_num >= MAX_FRAMES_IN_FLIGHT {
      panic!("Invalid input! current_frame_num {}", current_frame_num);
    }
    self.render_finished_semaphores[current_frame_num]
  }

  /// Ensures that the image is not currently in flight, so the command buffers
  /// for it are safe to write to (they are in the ready state).
  ///
  /// Returns the frame fence to submit the next queue with.
  pub fn ensure_image_resources_ready(
    &self, image_index: usize, current_frame_num: usize,
  ) -> SarektResult<vk::Fence> {
    if current_frame_num >= MAX_FRAMES_IN_FLIGHT || image_index >= self.image_to_frame_fence.len() {
      panic!(
        "Invalid input! image_index: {} current_frame_num: {}",
        image_index, current_frame_num
      );
    }

    unsafe {
      // Wait for swapchain image resources to be ready.
      let image_fence = self.image_to_frame_fence[image_index as usize].get();
      if image_fence != vk::Fence::null() {
        self
          .logical_device
          .wait_for_fences(&[image_fence], true, u64::max_value())?;
      }

      // Wait for the frame in flight to be ready (there are a max number of frames in
      // flight).
      let frame_fence = self.frame_fences[current_frame_num];
      if frame_fence != image_fence {
        // Wait for swap chain image to be ready.
        self
          .logical_device
          .wait_for_fences(&[frame_fence], true, u64::max_value())?;
      }

      self.logical_device.reset_fences(&[frame_fence])?;

      Ok(frame_fence)
    }
  }

  /// Mark the image as in use by the given frame.
  pub fn set_image_to_in_flight_frame(&self, image_index: usize, current_frame_num: usize) {
    if current_frame_num >= MAX_FRAMES_IN_FLIGHT || image_index >= self.image_to_frame_fence.len() {
      panic!(
        "Invalid input! image_index: {} current_frame_num: {}",
        image_index, current_frame_num
      );
    }
    self.image_to_frame_fence[image_index as usize].set(self.frame_fences[current_frame_num]);
  }

  /// Waits for all the in flight frames, ie device idle.
  pub fn wait_for_all_frames(&self) -> SarektResult<()> {
    unsafe {
      Ok(
        self
          .logical_device
          .wait_for_fences(&self.frame_fences, true, u64::max_value())?,
      )
    }
  }

  /// Makes new semaphores for draw synchronization.  Useful for swapchain
  /// recreation.
  ///
  /// Unsafe because they must not be in use.
  pub unsafe fn recreate_semaphores(&mut self) -> SarektResult<()> {
    let semaphore_ci = vk::SemaphoreCreateInfo::default();
    for i in 0..MAX_FRAMES_IN_FLIGHT {
      let to_destroy = self.image_available_semaphores[i];
      self.image_available_semaphores[i] =
        self.logical_device.create_semaphore(&semaphore_ci, None)?;
      self.logical_device.destroy_semaphore(to_destroy, None);

      let to_destroy = self.render_finished_semaphores[i];
      self.render_finished_semaphores[i] =
        self.logical_device.create_semaphore(&semaphore_ci, None)?;
      self.logical_device.destroy_semaphore(to_destroy, None);
    }

    Ok(())
  }

  pub unsafe fn destroy_all(&self) {
    info!("Destroying all synchronization primitives...");
    for &sem in self.image_available_semaphores.iter() {
      self.logical_device.destroy_semaphore(sem, None);
    }
    for &sem in self.render_finished_semaphores.iter() {
      self.logical_device.destroy_semaphore(sem, None);
    }
    for &fence in self.frame_fences.iter() {
      self.logical_device.destroy_fence(fence, None);
    }

    // TODO(issue#9) OFFSCREEN this fence won't be the same.
    self
      .logical_device
      .wait_for_fences(&[self.acquire_fence], true, u64::max_value())
      .expect("Failed to wait for fence during destruction");
    self.logical_device.destroy_fence(self.acquire_fence, None);
  }
}
