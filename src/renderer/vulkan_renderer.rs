use crate::{
  error::{SarektError, SarektResult},
  renderer::{
    debug_utils_ext::{DebugUserData, DebugUtilsAndMessenger},
    queue_family_indices::QueueFamilyIndices,
    ApplicationDetails, EngineDetails, Renderer, ENABLE_VALIDATION_LAYERS, IS_DEBUG_MODE,
  },
};
use ash::{
  extensions::{ext::DebugUtils, khr::Surface},
  version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
  vk,
  vk::{DebugUtilsMessageSeverityFlagsEXT, DebugUtilsMessageTypeFlagsEXT},
  Device, Entry, Instance,
};
use lazy_static::lazy_static;
use log::info;
use raw_window_handle::HasRawWindowHandle;
use std::{
  ffi::{CStr, CString},
  pin::Pin,
  sync::Arc,
};

// TODO group surface and surface_functions
// TODO group queues, put in queue_family_indices file and rename just queues.
// TODO ensure all methods (private included) are documented.

lazy_static! {
  static ref VALIDATION_LAYERS: Vec<CString> =
    vec![CString::new("VK_LAYER_KHRONOS_validation").unwrap()];
}

/// The Sarekt Vulkan Renderer, see module level documentation for details.
pub struct VulkanRenderer {
  _entry: Entry,
  instance: Instance,
  debug_utils_and_messenger: Option<DebugUtilsAndMessenger>,
  surface: vk::SurfaceKHR, // TODO option
  surface_functions: ash::extensions::khr::Surface,

  physical_device: vk::PhysicalDevice,
  logical_device: Device,

  graphics_queue: vk::Queue,
  presentation_queue: vk::Queue,
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

    // Load vulkan driver dynamic library and populate functions.
    let entry = ash::Entry::new().expect("Failed to load dynamic library and create Vulkan Entry");

    // Create client side vulkan instance.
    let instance = Self::create_instance(
      &entry,
      window.as_ref(),
      application_details.name,
      application_details.get_u32_version(),
      engine_details.name,
      engine_details.get_u32_version(),
    )?;

    // Only setup the debug utils extension and callback messenger if we are in
    // debug mode.
    let debug_utils_and_messenger = if IS_DEBUG_MODE {
      Some(Self::setup_debug_callback_messenger(
        &entry,
        &instance,
        debug_user_data,
      ))
    } else {
      None
    };

    // TODO unit testing, only create surface and swapchain if window was passed,
    // otherwise make images directly.
    // vkCreateXcbSurfaceKHR/VkCreateWin32SurfaceKHR/
    // vkCreateStreamDescriptorSurfaceGGP(Stadia)/etc
    let surface = unsafe {
      ash_window::create_surface(&entry, &instance, window.clone().as_ref(), None)
        .or(Err(SarektError::CouldNotCreateSurface))?
    };
    let surface_functions = ash::extensions::khr::Surface::new(&entry, &instance);

    let physical_device = Self::pick_physical_device(&instance, &surface_functions, surface)?;

    let (logical_device, graphics_queue, presentation_queue) =
      Self::create_logical_device_and_queues(
        &instance,
        physical_device,
        &surface_functions,
        surface,
      )?;

    Ok(Self {
      _entry: entry,
      instance,
      debug_utils_and_messenger,
      surface,
      surface_functions,
      physical_device,
      logical_device,
      graphics_queue,
      presentation_queue,
    })
  }
}
impl Renderer for VulkanRenderer {}
impl Drop for VulkanRenderer {
  fn drop(&mut self) {
    unsafe {
      info!("Destrying logical device...");
      self.logical_device.destroy_device(None);

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

    unsafe {
      entry
        .create_instance(&instance_create_info, None)
        .map_err(|err| SarektError::CouldNotCreateInstance("Error creating instance", err))
    }
  }

  // ================================================================================
  //  Instance Helper Methods
  // ================================================================================
  fn get_required_extensions<W: HasRawWindowHandle>(window: &W) -> SarektResult<Vec<&CStr>> {
    // Includes VK_KHR_Surface and
    // VK_KHR_Win32_Surface/VK_KHR_xcb_surface/
    // VK_GGP_stream_descriptor_surface(stadia)
    let mut extensions = ash_window::enumerate_required_extensions(window)
      .or(Err(SarektError::CouldNotEnumerateExtensionsForWindowSystem))?;

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

  unsafe fn log_extensions_dialog(entry: &Entry, extension_names: &Vec<&CStr>) -> () {
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

  // ================================================================================
  //  Physical Device Helper Methods
  // ================================================================================
  fn pick_physical_device(
    instance: &Instance, surface_functions: &Surface, surface: vk::SurfaceKHR,
  ) -> SarektResult<vk::PhysicalDevice> {
    let available_physical_devices = unsafe {
      instance
        .enumerate_physical_devices()
        .expect("Unable to enumerate physical devices")
    };

    // Assign some rank to all devices and get the highest one.
    let mut suitable_devices_ranked: Vec<_> = available_physical_devices
      .into_iter()
      .map(|device| Self::rank_device(instance, device, surface_functions, surface))
      .filter(|&(_, rank)| rank > -1i32)
      .collect();
    suitable_devices_ranked.sort_by(|&(_, l_rank), &(_, r_rank)| l_rank.cmp(&r_rank));

    info!(
      "Physical Devices most to least desirable:\n\t{:?}",
      suitable_devices_ranked
    );

    suitable_devices_ranked
      .first()
      .map(|&(device, _)| device)
      .ok_or(SarektError::CouldNotSelectPhysicalDevice)
  }

  /// Rank the devices based on an internal scoring mechanism.
  /// A score of -1 means the device is not supported.
  ///
  /// TODO add ways to configure device selection later.
  fn rank_device(
    instance: &Instance, physical_device: vk::PhysicalDevice, surface_functions: &Surface,
    surface: vk::SurfaceKHR,
  ) -> (vk::PhysicalDevice, i32) {
    let device_properties = unsafe { instance.get_physical_device_properties(physical_device) };
    // TODO utilize?
    // let device_features = unsafe {
    // instance.get_physical_device_features(physical_device) };

    if !Self::is_device_suitable(instance, physical_device, surface_functions, surface) {
      return (physical_device, -1);
    }

    let mut score = 0;
    if device_properties.device_type == vk::PhysicalDeviceType::DISCRETE_GPU {
      score += 10;
    } else if device_properties.device_type == vk::PhysicalDeviceType::INTEGRATED_GPU {
      score += 5;
    }

    (physical_device, score)
  }

  /// Tells us if this device is compatible with Sarekt.
  ///
  /// This will become more complex as more features are added.
  ///
  /// Certain features can be behind cargo feature flags that also affect this
  /// function.
  fn is_device_suitable(
    instance: &Instance, physical_device: vk::PhysicalDevice, surface_functions: &Surface,
    surface: vk::SurfaceKHR,
  ) -> bool {
    Self::find_queue_families(instance, physical_device, surface_functions, surface)
      .map(|qf| qf.is_complete())
      .unwrap_or(false)
  }

  fn find_queue_families(
    instance: &Instance, physical_device: vk::PhysicalDevice, surface_functions: &Surface,
    surface: vk::SurfaceKHR,
  ) -> SarektResult<QueueFamilyIndices> {
    let mut queue_family_indices = QueueFamilyIndices::default();
    let queue_family_properties =
      unsafe { instance.get_physical_device_queue_family_properties(physical_device) };

    for (i, queue_family_properties) in queue_family_properties.iter().enumerate() {
      if queue_family_properties
        .queue_flags
        .intersects(vk::QueueFlags::GRAPHICS)
      {
        queue_family_indices.graphics_queue_family = Some(i as u32);
      }

      let presentation_support = unsafe {
        surface_functions
          .get_physical_device_surface_support(physical_device, i as u32, surface)
          .or(Err(SarektError::CouldNotQueryDeviceSurfaceSupport))?
      };
      if presentation_support {
        queue_family_indices.presentation_queue_family = Some(i as u32);
      }
    }

    Ok(queue_family_indices)
  }

  // ================================================================================
  //  Logical Device Helper Methods
  // ================================================================================
  /// Creates the logical device after confirming all the features and queues
  /// needed are present, and returns the logical device, the graphics queue,
  /// and the presentation queue, otherwise returns the
  /// [SarektError](enum.SarektError.html) that occurred.
  fn create_logical_device_and_queues(
    instance: &Instance, physical_device: vk::PhysicalDevice, surface_functions: &Surface,
    surface: vk::SurfaceKHR,
  ) -> SarektResult<(Device, vk::Queue, vk::Queue)> {
    let queue_family_indices =
      Self::find_queue_families(instance, physical_device, surface_functions, surface)?;
    let graphics_queue_family = queue_family_indices.graphics_queue_family.unwrap();
    let presentation_queue_family = queue_family_indices.presentation_queue_family.unwrap();

    let graphics_queue_ci = vk::DeviceQueueCreateInfo::builder()
      .queue_family_index(graphics_queue_family)
      .queue_priorities(&[1.0]) // MULTITHREADING All queues have the same priority, and there's one. more than 1 if multiple threads (one for each thread)
      .build();
    let presentation_queue_ci = vk::DeviceQueueCreateInfo::builder()
      .queue_family_index(presentation_queue_family)
      .queue_priorities(&[1.0])
      .build();

    let device_features = vk::PhysicalDeviceFeatures::default();

    let device_ci = vk::DeviceCreateInfo::builder()
      .queue_create_infos(&[graphics_queue_ci, presentation_queue_ci])
      .enabled_features(&device_features)
      .build();

    unsafe {
      instance
        .create_device(physical_device, &device_ci, None)
        .map_or(Err(SarektError::CouldNotCreateLogicalDevice), |device| {
          // MULTITHREADING I would create one queue for each thread, right now I'm only
          // using one.
          let graphics_queue = device.get_device_queue(graphics_queue_family, 0);
          // TODO when would i have seperate queues even if in the same family for
          // presentation and graphics?
          let presentation_queue = device.get_device_queue(presentation_queue_family, 0);
          Ok((device, graphics_queue, presentation_queue))
        })
    }
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
