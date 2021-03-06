use ash::vk;

#[derive(Debug, Default, Clone)]
pub struct QueueFamilyIndices {
  pub graphics_queue_family: Option<u32>,
  pub presentation_queue_family: Option<u32>,
  pub transfer_queue_family: Option<u32>,
}
impl QueueFamilyIndices {
  // TODO(issue#9) OFFSCREEN is_complete_for_offscreen also that doesn't need
  // presentation.
  pub fn is_complete(&self) -> bool {
    self.graphics_queue_family.is_some()
      && self.presentation_queue_family.is_some()
      && self.transfer_queue_family.is_some()
  }

  /// Returns all the queue indices as an array for easily handing over to
  /// Vulkan.  Returns None if not complete
  pub fn as_vec(&self) -> Option<Vec<u32>> {
    // TODO(issue#9) OFFSCREEN is_complete_for_offscreen also that doesn't need
    // presentation.
    if !self.is_complete() {
      return None;
    }

    Some(vec![
      self.graphics_queue_family.unwrap(),
      // TODO(issue#9) OFFSCREEN no presentation if it is none since that's allowed.
      self.presentation_queue_family.unwrap(),
      self.transfer_queue_family.unwrap(),
    ])
  }
}

pub struct Queues {
  pub graphics_queue: vk::Queue,
  pub presentation_queue: vk::Queue,
  pub transfer_queue: vk::Queue,
}
impl Queues {
  pub fn new(
    graphics_queue: vk::Queue, presentation_queue: vk::Queue, transfer_queue: vk::Queue,
  ) -> Self {
    Queues {
      graphics_queue,
      presentation_queue,
      transfer_queue,
    }
  }
}
