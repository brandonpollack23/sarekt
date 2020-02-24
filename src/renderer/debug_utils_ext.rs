use ash::{extensions::ext::DebugUtils, vk};
use log::error;
use std::ffi::{c_void, CStr};

pub struct DebugUtilsAndMessenger {
  pub debug_utils: DebugUtils,
  pub messenger: vk::DebugUtilsMessengerEXT,
}
impl DebugUtilsAndMessenger {
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
}
