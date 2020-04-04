use crate::{
  error::SarektResult,
  renderer::{
    vulkan::vulkan_renderer::{
      debug_utils_ext::{DebugUserData, DebugUtilsAndMessenger},
      surface::SurfaceAndExtension,
    },
    ApplicationDetails, EngineDetails, ENABLE_VALIDATION_LAYERS, IS_DEBUG_MODE,
  },
};
use ash::{
  extensions::ext::DebugUtils,
  version::{EntryV1_0, InstanceV1_0},
  vk,
  vk::{DebugUtilsMessageSeverityFlagsEXT, DebugUtilsMessageTypeFlagsEXT},
  Entry, Instance,
};
use lazy_static::lazy_static;
use log::info;
use raw_window_handle::HasRawWindowHandle;
use std::{
  ffi::{CStr, CString},
  pin::Pin,
  sync::Arc,
};

lazy_static! {
  static ref VALIDATION_LAYERS: Vec<CString> =
    vec![CString::new("VK_LAYER_KHRONOS_validation").unwrap()];
}

/// Base vulkan items, driver loader, instance, extensions.
pub struct VulkanCoreStructures {
  _entry: Entry,
  pub instance: Arc<Instance>,
  pub surface_and_extension: SurfaceAndExtension, // TODO OFFSCREEN option
  debug_utils_and_messenger: Option<DebugUtilsAndMessenger>,
}
impl VulkanCoreStructures {
  pub fn new<W: HasRawWindowHandle>(
    window: &W, application_details: ApplicationDetails, engine_details: EngineDetails,
    debug_user_data: Option<Pin<Arc<DebugUserData>>>,
  ) -> SarektResult<VulkanCoreStructures> {
    // Load vulkan driver dynamic library and populate functions.
    let _entry = ash::Entry::new().expect("Failed to load dynamic library and create Vulkan Entry");

    // Create client side vulkan instance.
    let instance = Self::create_instance(
      &_entry,
      window,
      application_details.name,
      application_details.get_u32_version(),
      engine_details.name,
      engine_details.get_u32_version(),
    )?;

    // Only setup the debug utils extension and callback messenger if we are in
    // debug mode.
    let debug_utils_and_messenger = if IS_DEBUG_MODE {
      Some(Self::setup_debug_callback_messenger(
        &_entry,
        &instance,
        debug_user_data,
      ))
    } else {
      None
    };

    // TODO OFFSCREEN only create surface and swapchain if window was
    // passed, otherwise make images directly.
    // vkCreateXcbSurfaceKHR/VkCreateWin32SurfaceKHR/
    // vkCreateStreamDescriptorSurfaceGGP(Stadia)/etc
    let surface = unsafe { ash_window::create_surface(&_entry, instance.as_ref(), window, None)? };
    let surface_and_extension = SurfaceAndExtension::new(
      surface,
      ash::extensions::khr::Surface::new(&_entry, instance.as_ref()),
    );

    Ok(VulkanCoreStructures {
      _entry,
      instance,
      surface_and_extension,
      debug_utils_and_messenger,
    })
  }

  // ================================================================================
  //  Instance Creation
  // ================================================================================
  /// Creates an instance of the Vulkan client side driver given the raw handle.
  /// Currently Sarekt doesn't support drawing to anything but a presentable
  /// window surface.
  fn create_instance<W: HasRawWindowHandle>(
    entry: &Entry, window: &W, application_name: &str, application_version: u32, engine_name: &str,
    engine_version: u32,
  ) -> SarektResult<Arc<Instance>> {
    // TODO Detect vulkan versions available?
    let app_info = vk::ApplicationInfo::builder()
      .application_name(CString::new(application_name)?.as_c_str())
      .application_version(application_version)
      .engine_name(CString::new(engine_name)?.as_c_str())
      .engine_version(engine_version)
      .api_version(ash::vk::make_version(1, 2, 131))
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

    let extension_names = Self::get_required_extensions(window)?;
    unsafe {
      if IS_DEBUG_MODE {
        Self::log_extensions_dialog(entry, &extension_names);
      }
    }
    let extension_names: Vec<_> = extension_names
      .iter()
      .map(|&ext| ext.as_ptr() as *const i8)
      .collect();

    let mut debug_create_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
      .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::all())
      .message_type(vk::DebugUtilsMessageTypeFlagsEXT::all())
      .pfn_user_callback(Some(DebugUtilsAndMessenger::debug_callback))
      .build();

    let instance_create_info = vk::InstanceCreateInfo::builder()
      .application_info(&app_info)
      .enabled_layer_names(&layer_names)
      .enabled_extension_names(&extension_names)
      .push_next(&mut debug_create_info)
      .build();

    let instance = unsafe { entry.create_instance(&instance_create_info, None) }?;
    Ok(Arc::new(instance))
  }

  // ================================================================================
  //  Instance Helper Methods
  // ================================================================================
  /// Returns all extension needed for this renderer, depending on windowing
  /// system (or lack thereof) etc.
  fn get_required_extensions<W: HasRawWindowHandle>(window: &W) -> SarektResult<Vec<&CStr>> {
    // Includes VK_KHR_Surface and
    // VK_KHR_Win32_Surface/VK_KHR_xcb_surface/
    // VK_GGP_stream_descriptor_surface(stadia)
    let mut extensions = ash_window::enumerate_required_extensions(window)?;

    if IS_DEBUG_MODE {
      extensions.push(DebugUtils::name());
    }

    Ok(extensions)
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

  /// Logs extensions that are available and what was requested.
  unsafe fn log_extensions_dialog(entry: &Entry, extension_names: &[&CStr]) {
    let available_extensions: Vec<CString> = entry
      .enumerate_instance_extension_properties()
      .expect("Couldn't enumerate extensions")
      .iter_mut()
      .map(|e| CStr::from_ptr(e.extension_name.as_mut_ptr()).to_owned())
      .collect();
    info!(
      "Available Instance Extensions:\n\t{:?}\nRequested Instance Extensions:\n\t{:?}\n",
      available_extensions, extension_names
    );
  }

  // ================================================================================
  //  Debug Extension Helper Methods.
  // ================================================================================
  /// Creates a debug messenger within the VK_EXT_debug_utils extension that
  /// counts number of errors, warnings, and info messages and logs them using the [log](https://www.crates.io/crate/log) crate.
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
impl Drop for VulkanCoreStructures {
  fn drop(&mut self) {
    unsafe {
      // TODO OFFSCREEN if there is one
      info!("Destrying surface...");
      let surface_functions = &self.surface_and_extension.surface_functions;
      let surface = self.surface_and_extension.surface;
      surface_functions.destroy_surface(surface, None);

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
