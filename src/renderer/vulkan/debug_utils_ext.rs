use ash::{extensions::ext::DebugUtils, vk, Entry, Instance};
use log::{debug, error, warn};
use static_assertions::assert_impl_all;
use std::{
  ffi::CStr,
  os::raw::c_void,
  pin::Pin,
  sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
  },
};

/// The debug callbacks for vulkan that are enabled when in debug mode.  Called
/// by validation layers (mostly). Keeps track of errors etc for unit tests and logs all errors with [the log crate](https://www.crates.io/crate/log).
#[repr(C)]
pub struct DebugUtilsAndMessenger {
  pub debug_utils: DebugUtils,
  pub messenger: vk::DebugUtilsMessengerEXT,
  pub debug_user_data: Pin<Arc<DebugUserData>>,
}
impl DebugUtilsAndMessenger {
  /// Creates a new Debug Extension for vulkan with the associated user data for
  /// the debug callback, if provided.
  ///
  /// This user data must be Sync, which is garunteed by Arc.
  pub fn new(
    entry: &Entry, instance: &Instance, severity_flags: vk::DebugUtilsMessageSeverityFlagsEXT,
    type_flags: vk::DebugUtilsMessageTypeFlagsEXT,
    debug_user_data: Option<Pin<Arc<DebugUserData>>>,
  ) -> Self {
    let debug_user_data = if let Some(debug_user_data) = debug_user_data {
      debug_user_data
    } else {
      Arc::pin(DebugUserData::new())
    };

    let debug_user_data_ptr =
      unsafe { Arc::into_raw(Pin::into_inner_unchecked(debug_user_data.clone())) as *mut c_void };

    let debug_utils = DebugUtils::new(entry, instance);
    let messenger_ci = vk::DebugUtilsMessengerCreateInfoEXT::builder()
      .message_severity(severity_flags)
      .message_type(type_flags)
      .pfn_user_callback(Some(Self::debug_callback))
      .user_data(debug_user_data_ptr)
      .build();
    let messenger = unsafe {
      debug_utils
        .create_debug_utils_messenger(&messenger_ci, None)
        .expect("Could not create debug utils messenger")
    };

    DebugUtilsAndMessenger {
      debug_utils,
      messenger,
      debug_user_data,
    }
  }

  /// It is invariant in the vulkan renderer setup that p_user_data is of type
  /// DebugUserData, it is set up in new.
  pub unsafe extern "system" fn debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _message_types: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT, p_user_data: *mut c_void,
  ) -> u32 {
    // Transmute the user data to its appropriate type, but not a box (we don't want
    // to drop it), if it exists.
    let user_data: Option<&mut DebugUserData> = if p_user_data.is_null() {
      Some(&mut *(p_user_data as *mut DebugUserData))
    } else {
      None
    };

    // Update user data if necessary.
    if let Some(user_data) = user_data {
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
    }

    // Log messages.
    let message = CStr::from_ptr((*p_callback_data).p_message as *const i8)
      .to_str()
      .unwrap();
    match message_severity {
      vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => {
        error!("Validation Error! {}", message);
      }
      vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => {
        warn!("Validation Warning! {}", message);
      }
      vk::DebugUtilsMessageSeverityFlagsEXT::INFO => {
        debug!("Validation Info {}", message);
      }
      vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE => {
        debug!("Validation Verbose {}", message);
      }
      _ => {}
    }

    vk::FALSE // Returning false indicates no error in callback.
  }
}

assert_impl_all!(DebugUserData: Sync);
#[repr(C)]
pub struct DebugUserData {
  info_count: AtomicUsize,
  warning_count: AtomicUsize,
  error_count: AtomicUsize,
}
impl DebugUserData {
  pub fn new() -> Self {
    Self::default()
  }

  /// Returns the number of errors, warning, and info messages created by the
  /// debug layers.
  pub fn get_error_counts(&self) -> DebugUserDataCopy {
    DebugUserDataCopy {
      info_count: self.info_count.load(Ordering::SeqCst),
      warning_count: self.warning_count.load(Ordering::SeqCst),
      error_count: self.error_count.load(Ordering::SeqCst),
    }
  }
}
impl Default for DebugUserData {
  fn default() -> Self {
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
