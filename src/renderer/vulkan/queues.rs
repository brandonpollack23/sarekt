use ash::vk;

#[derive(Default, Clone)]
pub struct QueueFamilyIndices {
  pub graphics_queue_family: Option<u32>,
  pub presentation_queue_family: Option<u32>,
}
impl QueueFamilyIndices {
  pub fn is_complete(&self) -> bool {
    self.graphics_queue_family.is_some() && self.presentation_queue_family.is_some()
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
