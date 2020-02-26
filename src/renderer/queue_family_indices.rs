#[derive(Default, Clone)]
pub struct QueueFamilyIndices {
  pub graphics_queue_family: Option<usize>,
}
impl QueueFamilyIndices {
  pub fn is_complete(&self) -> bool {
    self.graphics_queue_family.is_some()
  }
}
