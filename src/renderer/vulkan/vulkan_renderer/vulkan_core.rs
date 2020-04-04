use crate::{
  error::{SarektError, SarektResult},
  renderer::{
    vulkan::{
      queues::{QueueFamilyIndices, Queues},
      vulkan_renderer::{
        debug_utils_ext::{DebugUserData, DebugUtilsAndMessenger},
        surface::SurfaceAndExtension,
        swap_chain::SwapchainSupportDetails,
      },
    },
    ApplicationDetails, EngineDetails, ENABLE_VALIDATION_LAYERS, IS_DEBUG_MODE,
  },
};
use ash::{
  extensions::ext::DebugUtils,
  version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
  vk,
  vk::{DebugUtilsMessageSeverityFlagsEXT, DebugUtilsMessageTypeFlagsEXT},
  Device, Entry, Instance,
};
use lazy_static::lazy_static;
use log::{info, warn};
use raw_window_handle::HasRawWindowHandle;
use std::{
  ffi::{CStr, CString},
  os::raw::c_char,
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

  pub fn get_debug_user_data(&self) -> Option<&Pin<Arc<DebugUserData>>> {
    self
      .debug_utils_and_messenger
      .as_ref()
      .map(|d| &d.debug_user_data)
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

pub struct VulkanDeviceStructures {
  pub physical_device: vk::PhysicalDevice,
  pub logical_device: Arc<Device>,
  pub queue_families: QueueFamilyIndices,
  pub queues: Queues,
}
impl VulkanDeviceStructures {
  pub fn new(vulkan_core: &VulkanCoreStructures) -> SarektResult<VulkanDeviceStructures> {
    let physical_device =
      Self::pick_physical_device(&vulkan_core.instance, &vulkan_core.surface_and_extension)?;

    let (logical_device, queue_families, queues) = Self::create_logical_device_and_queues(
      &vulkan_core.instance,
      physical_device,
      &vulkan_core.surface_and_extension,
    )?;

    Ok(VulkanDeviceStructures {
      physical_device,
      logical_device,
      queue_families,
      queues,
    })
  }

  // ================================================================================
  //  Physical Device Helper Methods
  // ================================================================================
  /// Evaluates all the available physical devices in the system and picks the
  /// best one based on a heuristic.
  ///
  /// TODO CONFIG have this be overridable somehow with config etc.
  fn pick_physical_device(
    instance: &Instance, surface_and_extension: &SurfaceAndExtension,
  ) -> SarektResult<vk::PhysicalDevice> {
    let available_physical_devices = unsafe {
      instance
        .enumerate_physical_devices()
        .expect("Unable to enumerate physical devices")
    };

    // Assign some rank to all devices and get the highest one.
    let mut suitable_devices_ranked: Vec<_> = available_physical_devices
      .into_iter()
      .map(|device| Self::rank_device(instance, device, surface_and_extension))
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
  /// TODO CONFIG add ways to configure device selection later.
  fn rank_device(
    instance: &Instance, physical_device: vk::PhysicalDevice,
    surface_and_extension: &SurfaceAndExtension,
  ) -> (vk::PhysicalDevice, i32) {
    let device_properties = unsafe { instance.get_physical_device_properties(physical_device) };
    // TODO CONFIG utilize physicsl_device_features

    if !Self::is_device_suitable(instance, physical_device, surface_and_extension).unwrap_or(false)
    {
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
  /// This means it has what is needed by this configuration in terms of:
  /// * Supported Queue Families (Graphics, Presentation if drawing to a window)
  /// * Required Extensions (swapchain creation when drawing to a window)
  /// * Swapchain support for the physical device (when drawing to a window).
  ///
  /// This will become more complex as more features are added.
  ///
  /// Certain features can be behind cargo feature flags that also affect this
  /// function.
  fn is_device_suitable(
    instance: &Instance, physical_device: vk::PhysicalDevice,
    surface_and_extension: &SurfaceAndExtension,
  ) -> SarektResult<bool> {
    let has_needed_features = unsafe {
      instance
        .get_physical_device_features(physical_device)
        .sampler_anisotropy
        == vk::TRUE
    };

    let has_queues = Self::find_queue_families(instance, physical_device, surface_and_extension)
      .map(|qf| qf.is_complete())
      .unwrap_or(false);

    let supports_required_extensions =
      Self::device_supports_required_extensions(instance, physical_device);
    if supports_required_extensions.is_err() {
      warn!(
        "Could not enumerate physical device properties on device {:?}",
        physical_device
      );
      return Ok(false);
    }

    let sc_support_details =
      Self::query_swap_chain_support(surface_and_extension, physical_device)?;

    // TODO OFFSCREEN only if drawing to a window.
    let swap_chain_adequate =
      !sc_support_details.formats.is_empty() && !sc_support_details.present_modes.is_empty();

    // TODO OFFSCREEN only if drawing window need swap chain adequete.
    Ok(
      has_needed_features
        && has_queues
        && supports_required_extensions.unwrap()
        && swap_chain_adequate,
    )
  }

  /// Goes through and checks if the device supports all needed extensions for
  /// current configuration, such as swapchains when drawing to a window.
  fn device_supports_required_extensions(
    instance: &Instance, physical_device: vk::PhysicalDevice,
  ) -> SarektResult<bool> {
    let device_extension_properties =
      unsafe { instance.enumerate_device_extension_properties(physical_device)? };

    let supports_swapchain = device_extension_properties
      .iter()
      .map(|ext_props| ext_props.extension_name)
      .any(|ext_name| unsafe {
        // TODO OFFSCREEN only if drawing to a window.
        CStr::from_ptr(ext_name.as_ptr() as *const c_char)
          .eq(ash::extensions::khr::Swapchain::name())
      });

    Ok(supports_swapchain)
  }

  /// Finds the queue family indices to use for the rendering command
  /// submissions.  Right now only picks the first suitable queue family for
  /// each type of command.
  fn find_queue_families(
    instance: &Instance, physical_device: vk::PhysicalDevice,
    surface_and_extension: &SurfaceAndExtension,
  ) -> SarektResult<QueueFamilyIndices> {
    let surface_functions = &surface_and_extension.surface_functions;
    let surface = surface_and_extension.surface;

    let mut queue_family_indices = QueueFamilyIndices::default();
    let queue_family_properties =
      unsafe { instance.get_physical_device_queue_family_properties(physical_device) };

    for (i, queue_family_properties) in queue_family_properties.iter().enumerate() {
      if queue_family_indices.transfer_queue_family.is_none()
        && queue_family_properties
          .queue_flags
          .intersects(vk::QueueFlags::TRANSFER)
        && !queue_family_properties
          .queue_flags
          .intersects(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
      {
        // Is transfer but NOT graphics or compute.  This queue is optimal for
        // transfer.
        queue_family_indices.transfer_queue_family = Some(i as u32);
      }

      if queue_family_indices.graphics_queue_family.is_none()
        && queue_family_properties
          .queue_flags
          .intersects(vk::QueueFlags::GRAPHICS)
      {
        queue_family_indices.graphics_queue_family = Some(i as u32);
      }

      if queue_family_indices.presentation_queue_family.is_none() {
        let presentation_support = unsafe {
          surface_functions.get_physical_device_surface_support(
            physical_device,
            i as u32,
            surface,
          )?
        };
        if presentation_support {
          queue_family_indices.presentation_queue_family = Some(i as u32);
        }
      }

      if queue_family_indices.is_complete() {
        return Ok(queue_family_indices);
      }
    }

    // Iterated through all queue types, but explicit transfer queue family not
    // found, just set it to the same as graphics queue family.
    queue_family_indices.transfer_queue_family = queue_family_indices.graphics_queue_family;

    Ok(queue_family_indices)
  }

  // ================================================================================
  //  Logical Device Helper Methods
  // ================================================================================
  /// Creates the logical device after confirming all the features and queues
  /// needed are present, and returns the logical device, and a
  /// [Queues](struct.Queues.html) containing all the command queues. otherwise
  /// returns the [SarektError](enum.SarektError.html) that occurred.
  /// TODO CONFIG ANISOTROPY
  fn create_logical_device_and_queues(
    instance: &Instance, physical_device: vk::PhysicalDevice,
    surface_and_extension: &SurfaceAndExtension,
  ) -> SarektResult<(Arc<Device>, QueueFamilyIndices, Queues)> {
    let queue_family_indices =
      Self::find_queue_families(instance, physical_device, surface_and_extension)?;
    let mut indices = queue_family_indices.as_vec().unwrap();
    indices.dedup();

    let queue_prios = [1.0];
    let queue_cis: Vec<_> = indices
      .iter()
      .map(|&queue_index| {
        vk::DeviceQueueCreateInfo::builder()
          .queue_family_index(queue_index)
          .queue_priorities(&queue_prios) // MULTITHREADING All queues have the same priority, and there's one. more than 1 if multiple threads (one for each thread)
          .build()
      })
      .collect();

    let device_features = vk::PhysicalDeviceFeatures::builder()
      .sampler_anisotropy(true)
      .build();

    let enabled_extension_names = [ash::extensions::khr::Swapchain::name().as_ptr()];
    let device_ci = vk::DeviceCreateInfo::builder()
      .queue_create_infos(&queue_cis)
      .enabled_features(&device_features)
      // TODO OFFSCREEN only if drawing to a window
      .enabled_extension_names(&enabled_extension_names)
      .build();

    unsafe {
      // TODO VULKAN_INQUIRY when would i have seperate queues even if in the same
      // family for presentation and graphics?
      // TODO OFFSCREEN no presentation queue needed when not presenting to a
      // swapchain, right?
      //
      // TODO MULTITHREADING I would create one queue for each
      // thread, right now I'm only using one.
      let graphics_queue_family = queue_family_indices.graphics_queue_family.unwrap();
      let presentation_queue_family = queue_family_indices.presentation_queue_family.unwrap();
      let transfer_queue_family = queue_family_indices.transfer_queue_family.unwrap();

      let logical_device = instance.create_device(physical_device, &device_ci, None)?;
      let graphics_queue = logical_device.get_device_queue(graphics_queue_family, 0);
      let presentation_queue = logical_device.get_device_queue(presentation_queue_family, 0);
      let transfer_queue = logical_device.get_device_queue(transfer_queue_family, 0);

      let queues = Queues::new(graphics_queue, presentation_queue, transfer_queue);
      Ok((Arc::new(logical_device), queue_family_indices, queues))
    }
  }

  /// Retrieves the details of the swapchain's supported formats, present modes,
  /// and capabilities.
  pub fn query_swap_chain_support(
    surface_and_extension: &SurfaceAndExtension, physical_device: vk::PhysicalDevice,
  ) -> SarektResult<SwapchainSupportDetails> {
    let surface = surface_and_extension.surface;
    let surface_functions = &surface_and_extension.surface_functions;

    let phys_d_surface_capabilities = unsafe {
      surface_functions.get_physical_device_surface_capabilities(physical_device, surface)?
    };
    let phys_d_formats =
      unsafe { surface_functions.get_physical_device_surface_formats(physical_device, surface)? };
    let phys_d_present_modes = unsafe {
      surface_functions.get_physical_device_surface_present_modes(physical_device, surface)?
    };

    Ok(SwapchainSupportDetails::new(
      phys_d_surface_capabilities,
      phys_d_formats,
      phys_d_present_modes,
    ))
  }
}
impl Drop for VulkanDeviceStructures {
  fn drop(&mut self) {
    unsafe {
      info!("Destrying logical device...");
      self.logical_device.destroy_device(None);
    }
  }
}
