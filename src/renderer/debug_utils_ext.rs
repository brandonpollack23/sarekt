use ash::{extensions::ext::DebugUtils, vk, Entry, Instance};
use log::error;
use std::{
  ffi::{c_void, CStr},
  mem::MaybeUninit,
  pin::Pin,
  sync::atomic::{AtomicUsize, Ordering},
};

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
  pub user_data: Pin<Box<DebugUserData>>,
}
impl DebugUtilsAndMessenger {
  pub fn new(
    entry: &Entry, instance: &Instance, severity_flags: vk::DebugUtilsMessageSeverityFlagsEXT,
    type_flags: vk::DebugUtilsMessageTypeFlagsEXT,
  ) -> Self {
    let mut user_data = Box::pin(DebugUserData::new());

    let debug_utils = DebugUtils::new(entry, instance);
    let messenger_ci = vk::DebugUtilsMessengerCreateInfoEXT::builder()
      .message_severity(severity_flags)
      .message_type(type_flags)
      .pfn_user_callback(Some(Self::debug_callback))
      .user_data(user_data.as_mut().get_mut() as *mut DebugUserData as *mut c_void)
      .build();
    let messenger = unsafe {
      debug_utils
        .create_debug_utils_messenger(&messenger_ci, None)
        .expect("Could not create debug utils messenger")
    };

    DebugUtilsAndMessenger {
      debug_utils,
      messenger,
      user_data,
    }
  }

  /// It is invariant in the vulkan renderer setup that p_user_data is of type
  /// DebugUserData, it is set up in new.
  pub unsafe extern "system" fn debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_types: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT, p_user_data: *mut c_void,
  ) -> u32 {
    // Transmute the user data to its appropriate type, but not a box (we don't want
    // to drop it).
    let mut user_data: &mut DebugUserData = std::mem::transmute(p_user_data);

    match message_severity {
      vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => {
        user_data.error_count.fetch_add(1, Ordering::SeqCst);
      }
      vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => {
        user_data.warning_count.fetch_add(1, Ordering::SeqCst);
      }
      vk::DebugUtilsMessageSeverityFlagsEXT::INFO => {
        user_data.info_count.fetch_add(1, Ordering::SeqCst);
      }
      _ => {}
    }

    println!(
      "ERROR COUNT IS NOW: {}",
      user_data.error_count.load(Ordering::SeqCst)
    );

    match (message_severity, message_types) {
      _ => {
        error!(
          "Validation Error! {}",
          CStr::from_ptr((*p_callback_data).p_message as *const i8)
            .to_str()
            .unwrap()
        );
      }
    }

    vk::FALSE // Returning false indicates no error in callback.
  }

  pub fn get_error_counts(&self) -> DebugUserDataCopy {
    DebugUserDataCopy {
      info_count: self.user_data.info_count.load(Ordering::SeqCst),
      warning_count: self.user_data.warning_count.load(Ordering::SeqCst),
      error_count: self.user_data.error_count.load(Ordering::SeqCst),
    }
  }
}

#[repr(C)]
pub struct DebugUserData {
  info_count: AtomicUsize,
  warning_count: AtomicUsize,
  error_count: AtomicUsize,
}
impl DebugUserData {
  fn new() -> Self {
    Self {
      info_count: AtomicUsize::new(0),
      warning_count: AtomicUsize::new(0),
      error_count: AtomicUsize::new(0),
    }
  }
}

#[derive(Debug)]
pub struct DebugUserDataCopy {
  pub info_count: usize,
  pub warning_count: usize,
  pub error_count: usize,
}
