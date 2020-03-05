use ash::vk;

#[derive(Debug, Default, Clone)]
pub struct QueueFamilyIndices {
  pub graphics_queue_family: Option<u32>,
  pub presentation_queue_family: Option<u32>,
}
impl QueueFamilyIndices {
  // TODO OFFSCREEN is_complete_for_offscreen also that doesn't need presentation.
  pub fn is_complete(&self) -> bool {
    self.graphics_queue_family.is_some() && self.presentation_queue_family.is_some()
  }

  /// Returns all the queue indices as an array for easily handing over to
  /// Vulkan.  Returns None if not complete
  pub fn into_vec(self) -> Option<Vec<u32>> {
    // TODO OFFSCREEN is_complete_for_offscreen also that doesn't need presentation.
    if !self.is_complete() {
      return None;
    }

    Some(vec![
      self.graphics_queue_family.unwrap(),
      // TODO OFFSCREEN no presentation if it is none since that's allowed.
      self.presentation_queue_family.unwrap(),
    ])
  }
}

pub struct Queues {
  pub graphics_queue: vk::Queue,
  pub presentation_queue: vk::Queue,
}
impl Queues {
  pub fn new(graphics_queue: vk::Queue, presentation_queue: vk::Queue) -> Self {
    Queues {
      graphics_queue,
      presentation_queue,
    }
  }
}
