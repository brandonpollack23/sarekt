use ash::{extensions::ext::DebugUtils, vk};
use log::error;
use std::ffi::{c_void, CStr};

// TODO to make unit tests etc work, we can pass this structure itself the
// callback data, cast it as mut, and delegate to a do_debug_callback.
// That function can access mutable internal data like error counters etc.
// It can be called from multiple threads simultaneously, so we can use an
// atomic counter.

/// The debug callbacks for vulkan that are enabled when in debug mode.  Called
/// by validation layers (mostly). Keeps track of errors etc for unit tests and logs all errors with [the log crate](https://www.crates.io/crate/log).
#[repr(C)]
pub struct DebugUtilsAndMessenger {
  pub debug_utils: DebugUtils,
  pub messenger: vk::DebugUtilsMessengerEXT,

  info_count: AtomicUsize,
  warning_count: AtomicUsize,
  error_count: AtomicUsize,
}
impl DebugUtilsAndMessenger {
  /// It is invariant in the vulkan renderer setup that p_user_data is of type
  /// DebugUtilsAndMessenger (see the [vulkan
  /// renderer](struct.VulkanRenderer.html) implementation.
  pub unsafe extern "system" fn debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_types: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT, p_user_data: *mut c_void,
  ) -> u32 {
    error!(
      "Validation Error! {}",
      CStr::from_ptr((*p_callback_data).p_message as *const i8)
        .to_str()
        .unwrap()
    );

    // Returning false indicates no error in callback.
    vk::FALSE
  }

  fn handle_debug_callback(&mut self) -> u32 {

  }
}
