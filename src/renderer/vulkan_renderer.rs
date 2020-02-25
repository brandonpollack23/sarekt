use std::{
  ffi::{CStr, CString},
  pin::Pin,
  sync::Arc,
};

use ash::{
  extensions::{ext::DebugUtils, khr::Surface},
  version::{EntryV1_0, InstanceV1_0},
  vk,
  vk::{DebugUtilsMessageSeverityFlagsEXT, DebugUtilsMessageTypeFlagsEXT},
  Entry, Instance,
};
use log::info;
use raw_window_handle::HasRawWindowHandle;

use lazy_static::lazy_static;

use crate::{
  error::SarektError,
  renderer::{
    debug_utils_ext::{DebugUserData, DebugUtilsAndMessenger},
    ApplicationDetails, EngineDetails, Renderer, ENABLE_VALIDATION_LAYERS, IS_DEBUG_MODE,
  },
};

// TODO Debugging instance creation and destruction

lazy_static! {
  static ref VALIDATION_LAYERS: Vec<CString> =
    vec![CString::new("VK_LAYER_KHRONOS_validation").unwrap()];
}

/// The Sarekt Vulkan Renderer, see module level documentation for details.
pub struct VulkanRenderer {
  _entry: Entry,
  instance: Instance,
  debug_utils_and_messenger: Option<DebugUtilsAndMessenger>,
}
impl VulkanRenderer {
  /// Creates a VulkanRenderer for the window with no application name, no
  /// engine, and base versions of 0.1.0.
  pub fn new<W: HasRawWindowHandle, OW: Into<Option<Arc<W>>>>(
    window: OW,
  ) -> Result<Self, SarektError> {
    Self::new_detailed(
      window,
      ApplicationDetails::default(),
      EngineDetails::default(),
    )
  }

  /// Creates a VulkanRenderer with a given name/version/engine name/engine
  /// version.
  pub fn new_detailed<W: HasRawWindowHandle, OW: Into<Option<Arc<W>>>>(
    window: OW, application_details: ApplicationDetails, engine_details: EngineDetails,
  ) -> Result<Self, SarektError> {
    Self::new_detailed_with_debug_user_data(window, application_details, engine_details, None)
  }

  /// Like new_detailed but allows injection of user data, for unit testing.
  fn new_detailed_with_debug_user_data<W: HasRawWindowHandle, OW: Into<Option<Arc<W>>>>(
    window: OW, application_details: ApplicationDetails, engine_details: EngineDetails,
    debug_user_data: Option<Pin<Arc<DebugUserData>>>,
  ) -> Result<Self, SarektError> {
    // TODO
    // * Support rendering to a non window surface if window is None (change it to
    //   an Enum of WindowHandle or OtherSurface).
    info!("Creating Sarekt Renderer with Vulkan Backend...");

    let window = window
      .into()
      .expect("Sarekt only supports rendering to a window right now :(");

    let entry = ash::Entry::new().expect("Failed to load dynamic library and create Vulkan Entry");
    let instance = Self::create_instance(
      &entry,
      window.as_ref(),
      application_details.name,
      application_details.get_u32_version(),
      engine_details.name,
      engine_details.get_u32_version(),
    )?;

    let debug_utils_and_messenger = if ENABLE_VALIDATION_LAYERS {
      Some(Self::setup_debug_callback_messenger(
        &entry,
        &instance,
        debug_user_data,
      ))
    } else {
      None
    };

    Ok(Self {
      _entry: entry,
      instance,
      debug_utils_and_messenger,
    })
  }
}
impl Renderer for VulkanRenderer {}
impl Drop for VulkanRenderer {
  fn drop(&mut self) {
    unsafe {
      info!("Destroying debug messenger...");
      if let Some(dbum) = &self.debug_utils_and_messenger {
        dbum
          .debug_utils
          .destroy_debug_utils_messenger(dbum.messenger, None);
      }

      info!("Destroying renderer...");
      self.instance.destroy_instance(None);
    }
  }
}

/// Private implementation details.
impl VulkanRenderer {
  // ================================================================================
  //  Instance Creation
  // ================================================================================
  /// Creates an instance of the Vulkan client side driver given the raw handle.
  /// Currently Sarekt doesn't support drawing to anything but a presentable
  /// window surface.
  fn create_instance<W: HasRawWindowHandle>(
    entry: &Entry, window: &W, application_name: &str, application_version: u32, engine_name: &str,
    engine_version: u32,
  ) -> Result<Instance, SarektError> {
    // TODO
    // * Detect vulkan versions available?
    let app_info = vk::ApplicationInfo::builder()
      .application_name(CString::new(application_name)?.as_c_str())
      .application_version(application_version)
      .engine_name(CString::new(engine_name)?.as_c_str())
      .engine_version(engine_version)
      .api_version(ash::vk::make_version(0, 1, 0))
      .build();

    let mut layer_names: Vec<_> = Vec::new(); // Will not alloc until stuff put in, so no problem.
    unsafe {
      if ENABLE_VALIDATION_LAYERS {
        assert!(
          Self::check_validation_layer_support(entry),
          "The requested validation layers were not available!"
        );
        layer_names = VALIDATION_LAYERS.iter().map(|name| name.as_ptr()).collect();
      }
    }

    let extension_names = Self::get_required_extensions(window);
    unsafe {
      if IS_DEBUG_MODE {
        Self::log_extensions_dialog(entry, &extension_names);
      }
    }

    let instance_create_info = vk::InstanceCreateInfo::builder()
      .application_info(&app_info)
      .enabled_layer_names(&layer_names)
      .enabled_extension_names(&extension_names)
      .build();

    unsafe {
      entry
        .create_instance(&instance_create_info, None)
        .map_err(|err| SarektError::CouldNotCreateInstance("Error creating instance", err))
    }
  }

  // ================================================================================
  //  Instance Helper Methods
  // ================================================================================
  fn get_required_extensions<W: HasRawWindowHandle>(window: &W) -> Vec<*const i8> {
    let surface_extension = Surface::name().as_ptr();
    let window_system_surface_extension = ash_window::enumerate_required_extension(window).as_ptr();
    let mut extensions = vec![surface_extension, window_system_surface_extension];

    if IS_DEBUG_MODE {
      extensions.push(DebugUtils::name().as_ptr());
    }

    extensions
  }

  /// Checks if all the validation layers specified are supported supported in
  /// this machine.
  unsafe fn check_validation_layer_support(entry: &Entry) -> bool {
    let available_layers: Vec<_> = entry
      .enumerate_instance_layer_properties()
      .expect("Unable to enumerate layers")
      .iter()
      .map(|layer| CStr::from_ptr(layer.layer_name.as_ptr()).to_owned())
      .collect();

    info!(
      "Supported Layers:\n\t{:?}\nRequested Layers:\n\t{:?}",
      available_layers,
      VALIDATION_LAYERS
        .iter()
        .map(|vl| vl.to_str().unwrap())
        .collect::<Vec<_>>()
    );

    VALIDATION_LAYERS
      .iter()
      .map(|requested_layer| available_layers.contains(&requested_layer))
      .all(|b| b)
  }

  unsafe fn log_extensions_dialog(entry: &Entry, extension_names: &Vec<*const i8>) -> () {
    let available_extensions: Vec<CString> = entry
      .enumerate_instance_extension_properties()
      .expect("Couldn't enumerate extensions")
      .iter_mut()
      .map(|e| CStr::from_ptr(e.extension_name.as_mut_ptr()).to_owned())
      .collect();
    let mut extension_names = extension_names.clone();
    let extension_names_cstr: Vec<CString> = extension_names
      .iter_mut()
      .map(|&mut e| CStr::from_ptr(e).to_owned())
      .collect();
    info!(
      "Available Instance Extensions:\n\t{:?}\nRequested Instance Extensions:\n\t{:?}\n",
      available_extensions, extension_names_cstr
    );
  }

  fn setup_debug_callback_messenger(
    entry: &Entry, instance: &Instance, debug_user_data: Option<Pin<Arc<DebugUserData>>>,
  ) -> DebugUtilsAndMessenger {
    DebugUtilsAndMessenger::new(
      entry,
      instance,
      DebugUtilsMessageSeverityFlagsEXT::all(),
      DebugUtilsMessageTypeFlagsEXT::all(),
      debug_user_data,
    )
  }
}

#[cfg(test)]
mod tests {
  use crate::renderer::{
    ApplicationDetails, DebugUserData, EngineDetails, Version, VulkanRenderer, IS_DEBUG_MODE,
  };
  use log::Level;
  use std::{pin::Pin, sync::Arc};
  #[cfg(unix)]
  use winit::platform::unix::EventLoopExtUnix;
  #[cfg(windows)]
  use winit::platform::windows::EventLoopExtWindows;
  use winit::{event_loop::EventLoop, window::WindowBuilder};

  fn assert_no_warnings_or_errors_in_debug_user_data(debug_user_data: &Pin<Arc<DebugUserData>>) {
    if !IS_DEBUG_MODE {
      return;
    }

    let error_counts = debug_user_data.get_error_counts();

    assert_eq!(error_counts.error_count, 0);
    assert_eq!(error_counts.warning_count, 0);
    assert_eq!(error_counts.info_count, 0);
  }

  #[test]
  fn can_construct_renderer_with_new() {
    let _log = simple_logger::init_with_level(Level::Info);
    let event_loop = EventLoop::<()>::new_any_thread();
    let window = Arc::new(WindowBuilder::new().build(&event_loop).unwrap());
    let renderer = VulkanRenderer::new(window.clone()).unwrap();

    assert_no_warnings_or_errors_in_debug_user_data(
      &renderer
        .debug_utils_and_messenger
        .as_ref()
        .unwrap()
        .debug_user_data,
    );
  }

  #[test]
  fn can_construct_renderer_with_new_detailed() {
    let _log = simple_logger::init_with_level(Level::Info);
    let event_loop = EventLoop::<()>::new_any_thread();
    let window = Arc::new(WindowBuilder::new().build(&event_loop).unwrap());
    let renderer = VulkanRenderer::new_detailed(
      window.clone(),
      ApplicationDetails::new("Testing App", Version::new(0, 1, 0)),
      EngineDetails::new("Test Engine", Version::new(0, 1, 0)),
    )
    .unwrap();
    assert_no_warnings_or_errors_in_debug_user_data(
      &renderer
        .debug_utils_and_messenger
        .as_ref()
        .unwrap()
        .debug_user_data,
    );
  }

  #[test]
  fn can_construct_renderer_with_new_detailed_and_user_data() {
    let _log = simple_logger::init_with_level(Level::Info);
    let event_loop = EventLoop::<()>::new_any_thread();
    let window = Arc::new(WindowBuilder::new().build(&event_loop).unwrap());
    let debug_user_data = Arc::pin(DebugUserData::new());
    let renderer = VulkanRenderer::new_detailed_with_debug_user_data(
      window.clone(),
      ApplicationDetails::new("Testing App", Version::new(0, 1, 0)),
      EngineDetails::new("Test Engine", Version::new(0, 1, 0)),
      Some(debug_user_data.clone()),
    )
    .unwrap();

    std::mem::drop(renderer);
    assert_no_warnings_or_errors_in_debug_user_data(&debug_user_data);
  }
}
