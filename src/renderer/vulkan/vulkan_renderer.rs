use crate::{
  error::{SarektError, SarektResult},
  renderer::{
    vulkan::{
      debug_utils_ext::DebugUtilsAndMessenger,
      images::ImageAndView,
      queues::{QueueFamilyIndices, Queues},
      surface::SurfaceAndExtension,
      swap_chain::{SwapchainAndExtension, SwapchainSupportDetails},
    },
    ApplicationDetails, DebugUserData, EngineDetails, Renderer, ENABLE_VALIDATION_LAYERS,
    IS_DEBUG_MODE,
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

// TODO do the below but for image vies first.
// TODO implement shader store (vec with handles) and load in default shaders,
// and pass it the logical device to copy the function for deleting them to use
// in drop.

/// The Sarekt Vulkan Renderer, see module level documentation for details.
pub struct VulkanRenderer {
  _entry: Entry,
  instance: Instance,
  debug_utils_and_messenger: Option<DebugUtilsAndMessenger>,
  surface_and_extension: SurfaceAndExtension, // TODO option

  #[allow(dead_code)]
  physical_device: vk::PhysicalDevice,
  logical_device: Device,
  #[allow(dead_code)]
  queues: Queues,

  swapchain_and_extension: SwapchainAndExtension, // TODO option
  render_targets: Vec<ImageAndView>,              // aka SwapChainImages if presenting.

  base_graphics_pipeline: vk::Pipeline,
}
impl VulkanRenderer {
  /// Creates a VulkanRenderer for the window with no application name, no
  /// engine, and base versions of 0.1.0.
  pub fn new<W: HasRawWindowHandle, OW: Into<Option<Arc<W>>>>(
    window: OW, requested_width: u32, requested_height: u32,
  ) -> Result<Self, SarektError> {
    Self::new_detailed(
      window,
      requested_width,
      requested_height,
      ApplicationDetails::default(),
      EngineDetails::default(),
    )
  }

  /// Creates a VulkanRenderer with a given name/version/engine name/engine
  /// version.
  pub fn new_detailed<W: HasRawWindowHandle, OW: Into<Option<Arc<W>>>>(
    window: OW, requested_width: u32, requested_height: u32,
    application_details: ApplicationDetails, engine_details: EngineDetails,
  ) -> Result<Self, SarektError> {
    Self::new_detailed_with_debug_user_data(
      window,
      requested_width,
      requested_height,
      application_details,
      engine_details,
      None,
    )
  }

  /// Like new_detailed but allows injection of user data, for unit testing.
  fn new_detailed_with_debug_user_data<W: HasRawWindowHandle, OW: Into<Option<Arc<W>>>>(
    window: OW, requested_width: u32, requested_height: u32,
    application_details: ApplicationDetails, engine_details: EngineDetails,
    debug_user_data: Option<Pin<Arc<DebugUserData>>>,
  ) -> Result<Self, SarektError> {
    // TODO Support rendering to a non window surface if window is None (change it
    // to an Enum of WindowHandle or OtherSurface).
    info!("Creating Sarekt Renderer with Vulkan Backend...");

    // TODO clean up

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
    let surface = unsafe { ash_window::create_surface(&entry, &instance, window.as_ref(), None)? };
    let surface_and_extension = SurfaceAndExtension::new(
      surface,
      ash::extensions::khr::Surface::new(&entry, &instance),
    );

    let physical_device = Self::pick_physical_device(&instance, &surface_and_extension)?;

    let (logical_device, queues) =
      Self::create_logical_device_and_queues(&instance, physical_device, &surface_and_extension)?;

    // TODO only create if drawing to window
    let swapchain_and_extension = Self::create_swap_chain(
      &instance,
      &logical_device,
      &surface_and_extension,
      physical_device,
      requested_width,
      requested_height,
    )?;

    // TODO if not swapchain create images that im rendering to.
    let render_target_images = unsafe {
      swapchain_and_extension
        .swapchain_functions
        .get_swapchain_images(swapchain_and_extension.swapchain)?
    };
    let render_targets = Self::create_render_target_image_views(
      &logical_device,
      &render_target_images,
      swapchain_and_extension.format,
    )?;

    let base_graphics_pipeline = Self::create_base_graphics_pipeline(
      &logical_device,
      &queues,
      &render_targets.iter().map(|rt| rt.view).collect::<Vec<_>>(),
    )?;

    Ok(Self {
      _entry: entry,
      instance,
      debug_utils_and_messenger,
      surface_and_extension,
      physical_device,
      logical_device,
      queues,
      swapchain_and_extension,
      render_targets,
      base_graphics_pipeline,
    })
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
    Ok(instance)
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

  // ================================================================================
  //  Physical Device Helper Methods
  // ================================================================================
  /// Evaluates all the available physical devices in the system and picks the
  /// best one based on a heuristic.
  ///
  /// TODO have this be overridable somehow with config etc.
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
  /// TODO add ways to configure device selection later.
  fn rank_device(
    instance: &Instance, physical_device: vk::PhysicalDevice,
    surface_and_extension: &SurfaceAndExtension,
  ) -> (vk::PhysicalDevice, i32) {
    let device_properties = unsafe { instance.get_physical_device_properties(physical_device) };
    // TODO utilize device_features
    // let device_features = unsafe {
    // instance.get_physical_device_features(physical_device) };

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
    let has_queues = Self::find_queue_families(instance, physical_device, surface_and_extension)
      .map(|qf| qf.is_complete())
      .unwrap_or(false);

    let supports_required_extensions =
      VulkanRenderer::device_supports_required_extensions(instance, physical_device);
    if supports_required_extensions.is_err() {
      warn!(
        "Could not enumerate physical device properties on device {:?}",
        physical_device
      );
      return Ok(false);
    }

    let sc_support_details =
      Self::query_swap_chain_support(surface_and_extension, physical_device)?;

    // TODO only if drawing to a window.
    let swap_chain_adequate =
      !sc_support_details.formats.is_empty() && !sc_support_details.present_modes.is_empty();

    // TODO only if drawing window need swap chain adequete.
    Ok(has_queues && supports_required_extensions.unwrap() && swap_chain_adequate)
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
        // TODO only if drawing to a window.
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
      if queue_family_properties
        .queue_flags
        .intersects(vk::QueueFlags::GRAPHICS)
      {
        queue_family_indices.graphics_queue_family = Some(i as u32);
      }

      let presentation_support = unsafe {
        surface_functions.get_physical_device_surface_support(physical_device, i as u32, surface)?
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
  /// needed are present, and returns the logical device, and a
  /// [Queues](struct.Queues.html) containing all the command queues. otherwise
  /// returns the [SarektError](enum.SarektError.html) that occurred.
  fn create_logical_device_and_queues(
    instance: &Instance, physical_device: vk::PhysicalDevice,
    surface_and_extension: &SurfaceAndExtension,
  ) -> SarektResult<(Device, Queues)> {
    let queue_family_indices =
      Self::find_queue_families(instance, physical_device, surface_and_extension)?;
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
      // TODO only if drawing to a window
      .enabled_extension_names(&[ash::extensions::khr::Swapchain::name().as_ptr()])
      .build();

    unsafe {
      // TODO when would i have seperate queues even if in the same family for
      // presentation and graphics?
      // TODO no presentation queue needed when not presenting to a swapchain, right?
      // MULTITHREADING I would create one queue for each thread, right now I'm only
      // using one.
      let logical_device = instance.create_device(physical_device, &device_ci, None)?;
      let graphics_queue = logical_device.get_device_queue(graphics_queue_family, 0);
      let presentation_queue = logical_device.get_device_queue(presentation_queue_family, 0);

      let queues = Queues::new(graphics_queue, presentation_queue);
      Ok((logical_device, queues))
    }
  }

  // ================================================================================
  //  Presentation and Swapchain Helper Methods
  // ================================================================================
  /// Based on the capabilities of the surface, the physical device, and the
  /// configuration of sarekt, creates a swapchain with the appropriate
  /// configuration (format, color space, present mode, and extent).
  fn create_swap_chain(
    instance: &Instance, logical_device: &Device, surface_and_extension: &SurfaceAndExtension,
    physical_device: vk::PhysicalDevice, requested_width: u32, requested_height: u32,
  ) -> SarektResult<SwapchainAndExtension> {
    let swapchain_support = Self::query_swap_chain_support(surface_and_extension, physical_device)?;

    let format = Self::choose_swap_surface_format(&swapchain_support.formats);
    let present_mode = Self::choose_presentation_mode(&swapchain_support.present_modes);
    let extent = Self::choose_swap_extent(
      &swapchain_support.capabilities,
      requested_width,
      requested_height,
    );

    // Select minimum number of images to render to.  For triple buffering this
    // would be 3, etc. But don't exceed the max.  Implementation may create more
    // than this depending on present mode.
    // [vulkan tutorial](https://vulkan-tutorial.com/Drawing_a_triangle/Presentation/Swap_chain)
    // recommends setting this to min + 1 because if we select minimum we may wait
    // on internal driver operations.
    let min_image_count = (swapchain_support.capabilities.min_image_count + 1)
      .min(swapchain_support.capabilities.max_image_count);

    let queue_family_indices =
      Self::find_queue_families(instance, physical_device, surface_and_extension)?;
    let sharing_mode = if queue_family_indices.graphics_queue_family.unwrap()
      != queue_family_indices.presentation_queue_family.unwrap()
    {
      // Concurrent sharing mode because the images will need to be accessed by more
      // than one queue family.
      vk::SharingMode::CONCURRENT
    } else {
      // Exclusive (probly) has best performance, not sharing the image with other
      // queue families.
      vk::SharingMode::EXCLUSIVE
    };

    let swapchain_ci = vk::SwapchainCreateInfoKHR::builder()
      .surface(surface_and_extension.surface)
      .min_image_count(min_image_count)
      .image_format(format.format)
      .image_color_space(format.color_space)
      .image_extent(extent)
      .image_array_layers(1) // Number of views (multiview/stereo surface for 3D applications with glasses or maybe VR).
      .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT) // We'll just be rendering colors to this.  We could render to another image and transfer here after post processing but we're not.
      .image_sharing_mode(sharing_mode)
      .queue_family_indices(&queue_family_indices.into_vec().unwrap())
      .pre_transform(swapchain_support.capabilities.current_transform) // Match the transform of the swapchain, I'm not trying to redner upside down!
      .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE) // No alpha blending within the window system for now.
      .present_mode(present_mode)
      .clipped(true) // Go ahead and discard rendering ops we dont need (window half off screen).
      .build();

    let swapchain_extension = ash::extensions::khr::Swapchain::new(instance, logical_device);
    let swapchain = unsafe { swapchain_extension.create_swapchain(&swapchain_ci, None)? };

    Ok(SwapchainAndExtension::new(
      swapchain,
      format.format,
      swapchain_extension,
    ))
  }

  /// Retrieves the details of the swapchain's supported formats, present modes,
  /// and capabilities.
  fn query_swap_chain_support(
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

  /// If drawing to a surface, chooses the best format from the ones available
  /// for the surface.  Tries to use B8G8R8A8_SRGB format with SRGB_NONLINEAR
  /// colorspace.
  ///
  /// If that isn't available, for now we just use the 0th SurfaceFormatKHR.
  fn choose_swap_surface_format(
    available_formats: &[vk::SurfaceFormatKHR],
  ) -> vk::SurfaceFormatKHR {
    // TODO change to unorm?
    *available_formats
      .iter()
      .find(|format| {
        format.format == vk::Format::B8G8R8A8_SRGB
          && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
      })
      .unwrap_or(&available_formats[0])
  }

  /// Selects Mailbox if available, but if not tries to fallback to FIFO. See the [spec](https://renderdoc.org/vkspec_chunked/chap32.html#VkPresentModeKHR) for details on modes.
  ///
  /// TODO support immediate mode if possible and allow the user to have tearing
  /// if they wish.
  fn choose_presentation_mode(
    available_presentation_modes: &[vk::PresentModeKHR],
  ) -> vk::PresentModeKHR {
    *available_presentation_modes
      .iter()
      .find(|&pm| *pm == vk::PresentModeKHR::MAILBOX)
      .unwrap_or(&vk::PresentModeKHR::FIFO)
  }

  /// Selects the resolution of the swap chain images.
  /// This is almost always equal to the resolution of the Surface we're drawing
  /// too, but we need to double check since some window managers allow us to
  /// differ.
  fn choose_swap_extent(
    capabilities: &vk::SurfaceCapabilitiesKHR, requested_width: u32, requested_height: u32,
  ) -> vk::Extent2D {
    if capabilities.current_extent.width != u32::max_value() {
      return capabilities.current_extent;
    }
    // The window system indicates that we can specify our own extent if this is
    // true
    let clipped_requested_width = requested_width.min(capabilities.max_image_extent.width);
    let width = capabilities
      .min_image_extent
      .width
      .max(clipped_requested_width);
    let clipped_requested_height = requested_height.min(capabilities.max_image_extent.height);
    let height = capabilities
      .min_image_extent
      .height
      .max(clipped_requested_height);

    if width != requested_width || height != requested_height {
      warn!(
        "Could not create a swapchain with the requested height and width, rendering to a \
         resolution of {}x{} instead",
        width, height
      );
    }

    vk::Extent2D::builder().width(width).height(height).build()
  }

  /// Given the render target images and format, create an image view suitable
  /// for rendering on. (one level, no mipmapping, color bit access).
  fn create_render_target_image_views(
    logical_device: &Device, targets: &[vk::Image], format: vk::Format,
  ) -> SarektResult<Vec<ImageAndView>> {
    let mut views = Vec::with_capacity(targets.len());
    for &image in targets.iter() {
      // Not swizzling rgba around.
      let component_mapping = vk::ComponentMapping::default();
      let image_subresource_range = vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR) // We're writing color to this view
        .base_mip_level(0) // access to all mipmap levels
        .level_count(1) // Only one level, no mipmapping
        .base_array_layer(0) // access to all layers
        .layer_count(1) // Only one layer. (not sterescopic)
        .build();

      let ci = vk::ImageViewCreateInfo::builder()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .components(component_mapping)
        .subresource_range(image_subresource_range);

      let view = unsafe { logical_device.create_image_view(&ci, None)? };
      views.push(ImageAndView::new(image, view));
    }
    Ok(views)
  }

  // ================================================================================
  //  Pipeline Helper Methods
  // ================================================================================
  /// Creates the base pipeline for Sarekt.  A user can load custom shaders,
  /// etc, to create custom pipelines (passed back as opaque handles) based off
  /// this one that they can pass when requesting a draw.
  ///
  /// TODO allow for creating custom pipelines via LoadShaders etc.
  /// TODO enable pipeline cache.
  fn create_base_graphics_pipeline(
    logical_device: &Device, queues: &Queues, render_targets: &[vk::ImageView],
  ) -> SarektResult<vk::Pipeline> {
    Err(SarektError::Unknown)
  }
}
impl Renderer for VulkanRenderer {}
impl Drop for VulkanRenderer {
  fn drop(&mut self) {
    unsafe {
      info!("Destrying render target views...");
      self.render_targets.iter().for_each(|rt| {
        self.logical_device.destroy_image_view(rt.view, None);
      });

      // TODO if there is one, if not destroy images
      info!("Destrying swapchain...");
      let swapchain_functions = &self.swapchain_and_extension.swapchain_functions;
      let swapchain = self.swapchain_and_extension.swapchain;
      swapchain_functions.destroy_swapchain(swapchain, None);

      // TODO if there is one
      info!("Destrying surface...");
      let surface_functions = &self.surface_and_extension.surface_functions;
      let surface = self.surface_and_extension.surface;
      surface_functions.destroy_surface(surface, None);

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

  const WIDTH: u32 = 800;
  const HEIGHT: u32 = 600;

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
    let renderer = VulkanRenderer::new(window, WIDTH, HEIGHT).unwrap();

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
      window,
      WIDTH,
      HEIGHT,
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
      window,
      WIDTH,
      HEIGHT,
      ApplicationDetails::new("Testing App", Version::new(0, 1, 0)),
      EngineDetails::new("Test Engine", Version::new(0, 1, 0)),
      Some(debug_user_data.clone()),
    )
    .unwrap();

    std::mem::drop(renderer);
    assert_no_warnings_or_errors_in_debug_user_data(&debug_user_data);
  }
}
