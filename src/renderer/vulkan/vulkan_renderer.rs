use crate::{
  error::{SarektError, SarektResult},
  renderer::{
    shaders::{ShaderCode, ShaderHandle, ShaderStore, ShaderType},
    vertex_bindings::{DefaultForwardShaderVertex, VertexBindings},
    vulkan::{
      base_pipeline_bundle::BasePipelineBundle,
      debug_utils_ext::{DebugUserData, DebugUtilsAndMessenger},
      draw_synchronization::DrawSynchronization,
      images::ImageAndView,
      queues::{QueueFamilyIndices, Queues},
      surface::SurfaceAndExtension,
      swap_chain::{SwapchainAndExtension, SwapchainSupportDetails},
      vulkan_shader_functions::VulkanShaderFunctions,
      vulkan_vertex_bindings, VulkanShaderHandle,
    },
    ApplicationDetails, Drawer, EngineDetails, Renderer, ENABLE_VALIDATION_LAYERS, IS_DEBUG_MODE,
    MAX_FRAMES_IN_FLIGHT,
  },
};
use ash::{
  extensions::ext::DebugUtils,
  version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
  vk,
  vk::{DebugUtilsMessageSeverityFlagsEXT, DebugUtilsMessageTypeFlagsEXT, Extent2D, Offset2D},
  Device, Entry, Instance,
};
use lazy_static::lazy_static;
use log::{error, info, warn};
use raw_window_handle::HasRawWindowHandle;
use std::{
  cell::Cell,
  ffi::{CStr, CString},
  os::raw::c_char,
  pin::Pin,
  sync::{Arc, RwLock},
};
use vk_shader_macros::include_glsl;

// TODO MAINTENANCE shouldn't # of command buffers be equal to frames in flight,
// not # of framebuffers?

/// Default vertex shader that contain their own vertices, will be removed in
/// the future.
pub const DEFAULT_VERTEX_SHADER: &[u32] = include_glsl!("shaders/sarekt_forward.vert");
/// Default fragment shader that contain their own vertices, will be removed in
/// the future.
pub const DEFAULT_FRAGMENT_SHADER: &[u32] = include_glsl!("shaders/sarekt_forward.frag");

lazy_static! {
  static ref VALIDATION_LAYERS: Vec<CString> =
    vec![CString::new("VK_LAYER_KHRONOS_validation").unwrap()];
}

/// The Sarekt Vulkan Renderer, see module level documentation for details.
pub struct VulkanRenderer {
  // Base vulkan items, driver loader, instance, extensions.
  _entry: Entry,
  instance: Instance,
  debug_utils_and_messenger: Option<DebugUtilsAndMessenger>,
  surface_and_extension: SurfaceAndExtension, // TODO OFFSCREEN option

  // Device related fields
  #[allow(dead_code)]
  physical_device: vk::PhysicalDevice,
  logical_device: Arc<Device>,
  #[allow(dead_code)]
  queues: Queues,

  // Rendering related.
  swapchain_and_extension: SwapchainAndExtension, // TODO OFFSCREEN option
  render_targets: Vec<ImageAndView>,              // aka SwapChainImages if presenting.

  // Pipeline related
  forward_render_pass: vk::RenderPass,
  base_graphics_pipeline_bundle: BasePipelineBundle,
  framebuffers: Vec<vk::Framebuffer>,

  // Command pools, buffers, drawing, and synchronization related primitives and information.
  primary_gfx_command_pool: vk::CommandPool,
  primary_gfx_command_buffers: Vec<vk::CommandBuffer>,
  draw_synchronization: DrawSynchronization,
  current_frame_num: Cell<usize>,

  // Application controllable fields
  rendering_enabled: bool,

  // Utilities
  shader_store: Arc<RwLock<ShaderStore<VulkanShaderFunctions>>>,
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
    // TODO OFFSCREEN Support rendering to a non window surface if window is None
    // (change it to an Enum of WindowHandle or OtherSurface).
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

    // TODO OFFSCREEN only create surface and swapchain if window was
    // passed, otherwise make images directly.
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

    // TODO OFFSCREEN only create if drawing to window, get format and extent
    // elsewhere.
    let swapchain_extension =
      ash::extensions::khr::Swapchain::new(&instance, logical_device.as_ref());
    let (swapchain, format, extent) = Self::create_swap_chain(
      &instance,
      &logical_device,
      &surface_and_extension,
      &swapchain_extension,
      physical_device,
      requested_width,
      requested_height,
      None,
    )?;
    let swapchain_and_extension =
      SwapchainAndExtension::new(swapchain, format, swapchain_extension);

    // TODO OFFSCREEN if not swapchain create images that im rendering to.
    let render_target_images = unsafe {
      swapchain_and_extension
        .swapchain_functions
        .get_swapchain_images(swapchain_and_extension.swapchain)?
    };
    let render_targets = Self::create_render_target_image_views(
      &logical_device,
      render_target_images,
      swapchain_and_extension.format,
    )?;

    let shader_store = Self::create_shader_store(&logical_device);

    // TODO RENDERING_CAPABILITIES support other render pass types.
    let forward_render_pass = Self::create_forward_render_pass(&logical_device, format)?;

    let base_graphics_pipeline_bundle = Self::create_base_graphics_pipeline_and_shaders(
      &logical_device,
      &shader_store, // Unlock and get a local mut ref to shaderstore.
      extent,
      forward_render_pass,
    )?;

    // TODO when I can have multiple render pass types I need new framebuffers.
    let framebuffers = Self::create_framebuffers(
      &logical_device,
      forward_render_pass,
      &render_targets,
      extent,
    )?;

    let (primary_gfx_command_pool) = Self::create_primary_command_pools(
      &instance,
      physical_device,
      &surface_and_extension,
      &logical_device,
    )?;

    let primary_gfx_command_buffers = Self::create_primary_gfx_command_buffers(
      &logical_device,
      primary_gfx_command_pool,
      &framebuffers,
      extent,
      forward_render_pass,
      base_graphics_pipeline_bundle.pipeline,
    )?;

    let draw_synchronization = DrawSynchronization::new(&logical_device, render_targets.len())?;

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

      forward_render_pass,
      base_graphics_pipeline_bundle,
      framebuffers,

      primary_gfx_command_pool,
      primary_gfx_command_buffers,
      draw_synchronization,
      current_frame_num: Cell::new(0),

      rendering_enabled: true,

      shader_store,
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

    // TODO OFFSCREEN only if drawing to a window.
    let swap_chain_adequate =
      !sc_support_details.formats.is_empty() && !sc_support_details.present_modes.is_empty();

    // TODO OFFSCREEN only if drawing window need swap chain adequete.
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
  ) -> SarektResult<(Arc<Device>, Queues)> {
    let queue_family_indices =
      Self::find_queue_families(instance, physical_device, surface_and_extension)?;
    let graphics_queue_family = queue_family_indices.graphics_queue_family.unwrap();
    let presentation_queue_family = queue_family_indices.presentation_queue_family.unwrap();

    let mut indices = vec![graphics_queue_family, presentation_queue_family];
    indices.dedup();
    let queue_cis: Vec<_> = indices
      .iter()
      .map(|&queue_index| {
        vk::DeviceQueueCreateInfo::builder()
        .queue_family_index(queue_index)
        .queue_priorities(&[1.0]) // MULTITHREADING All queues have the same priority, and there's one. more than 1 if multiple threads (one for each thread)
        .build()
      })
      .collect();

    let device_features = vk::PhysicalDeviceFeatures::default();

    let device_ci = vk::DeviceCreateInfo::builder()
      .queue_create_infos(&queue_cis)
      .enabled_features(&device_features)
      // TODO OFFSCREEN only if drawing to a window
      .enabled_extension_names(&[ash::extensions::khr::Swapchain::name().as_ptr()])
      .build();

    unsafe {
      // TODO VULKAN_INQUIRY when would i have seperate queues even if in the same
      // family for presentation and graphics?
      // TODO OFFSCREEN no presentation queue needed when not presenting to a
      // swapchain, right?
      //
      // TODO MULTITHREADING I would create one queue for each
      // thread, right now I'm only using one.
      let logical_device = instance.create_device(physical_device, &device_ci, None)?;
      let graphics_queue = logical_device.get_device_queue(graphics_queue_family, 0);
      let presentation_queue = logical_device.get_device_queue(presentation_queue_family, 0);

      let queues = Queues::new(graphics_queue, presentation_queue);
      Ok((Arc::new(logical_device), queues))
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
    swapchain_extension: &ash::extensions::khr::Swapchain, physical_device: vk::PhysicalDevice,
    requested_width: u32, requested_height: u32, old_swapchain: Option<vk::SwapchainKHR>,
  ) -> SarektResult<(vk::SwapchainKHR, vk::Format, vk::Extent2D)> {
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
      .old_swapchain(old_swapchain.unwrap_or(vk::SwapchainKHR::null())) // Pass old swapchain for recreation.
      .build();

    let swapchain = unsafe { swapchain_extension.create_swapchain(&swapchain_ci, None)? };
    Ok((swapchain, format.format, extent))
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
  /// TODO CONFIG support immediate mode if possible and allow the user to have
  /// tearing if they wish.
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
    logical_device: &Arc<Device>, targets: Vec<vk::Image>, format: vk::Format,
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
      unsafe { views.push(ImageAndView::new(image, view)) };
    }
    Ok(views)
  }

  /// When the target dimensions or requirments change, we must recreate a bunch
  /// of stuff to remain compabible and continue rendering to the new surface.
  ///
  /// TODO MAYBE put everything that may need to be recreated in a cell?
  unsafe fn recreate_swap_chain(&mut self, width: u32, height: u32) -> SarektResult<()> {
    let instance = &self.instance;
    let logical_device = &self.logical_device;
    let surface_and_extension = &self.surface_and_extension;
    let physical_device = self.physical_device;
    let old_swapchain = self.swapchain_and_extension.swapchain;
    let swapchain_extension = &self.swapchain_and_extension.swapchain_functions;
    let shader_store = &self.shader_store;
    let primary_gfx_command_pool = self.primary_gfx_command_pool;

    // Procedure: Make new Swapchain (recycling old one), cleanup old resources and
    // recreate them:
    // * ImageViews
    // * Render Passes
    // * Graphics Pipelines
    // * Framebuffers
    // * Command Buffers.
    let (new_swapchain, new_format, new_extent) = Self::create_swap_chain(
      instance,
      logical_device,
      surface_and_extension,
      swapchain_extension,
      physical_device,
      width,
      height,
      Some(old_swapchain),
    )?;
    self.cleanup_swapchain()?;

    // Create all new resources and set them in this struct.
    self.swapchain_and_extension.swapchain = new_swapchain;
    self.swapchain_and_extension.format = new_format;

    // TODO OFFSCREEN if not swapchain create images that im rendering to.
    let render_target_images = swapchain_extension.get_swapchain_images(new_swapchain)?;
    self.render_targets =
      Self::create_render_target_image_views(logical_device, render_target_images, new_format)?;

    self.forward_render_pass = Self::create_forward_render_pass(logical_device, new_format)?;

    // Save the handles to the base shaders so they don't have to be recreated for
    // no reason.
    let vertex_shader_handle = self
      .base_graphics_pipeline_bundle
      .vertex_shader_handle
      .take();
    let fragment_shader_handle = self
      .base_graphics_pipeline_bundle
      .fragment_shader_handle
      .take();
    self.base_graphics_pipeline_bundle = Self::create_base_graphics_pipeline(
      logical_device,
      shader_store,
      new_extent,
      self.forward_render_pass,
      vertex_shader_handle.unwrap(),
      fragment_shader_handle.unwrap(),
    )?;

    self.framebuffers = Self::create_framebuffers(
      logical_device,
      self.forward_render_pass,
      &self.render_targets,
      new_extent,
    )?;

    self.primary_gfx_command_buffers = Self::create_primary_gfx_command_buffers(
      logical_device,
      primary_gfx_command_pool,
      &self.framebuffers,
      new_extent,
      self.forward_render_pass,
      self.base_graphics_pipeline_bundle.pipeline,
    )?;

    Ok(())
  }

  /// Cleans up all resources dependent on the swapchain and the swapchain
  /// itself.
  /// * ImageViews
  /// * Render Passes
  /// * Graphics Pipelines
  /// * Framebuffers
  /// * Command Buffers.
  unsafe fn cleanup_swapchain(&self) -> SarektResult<()> {
    // Wait for all in flight frames.
    self.logical_device.wait_for_fences(
      &self.draw_synchronization.in_flight_fences,
      true,
      u64::max_value(),
    )?;

    // TODO MULTITHREADING do I need to free others?
    info!("Freeing primary command buffers...");
    self.logical_device.free_command_buffers(
      self.primary_gfx_command_pool,
      &self.primary_gfx_command_buffers,
    );

    info!("Destroying all framebuffers...");
    for &fb in self.framebuffers.iter() {
      self.logical_device.destroy_framebuffer(fb, None);
    }

    info!("Destroying base graphics pipeline...");
    self
      .logical_device
      .destroy_pipeline(self.base_graphics_pipeline_bundle.pipeline, None);

    info!("Destroying base pipeline layouts...");
    self
      .logical_device
      .destroy_pipeline_layout(self.base_graphics_pipeline_bundle.pipeline_layout, None);

    info!("Destroying render pass...");
    self
      .logical_device
      .destroy_render_pass(self.forward_render_pass, None);

    info!("Destrying render target views...");
    for view in self.render_targets.iter() {
      self.logical_device.destroy_image_view(view.view, None);
    }
    // TODO OFFSCREEN if images and not swapchain destroy images.

    // TODO OFFSCREEN if there is one, if not destroy images (as above todo states).
    info!("Destrying swapchain...");
    let swapchain_functions = &self.swapchain_and_extension.swapchain_functions;
    let swapchain = self.swapchain_and_extension.swapchain;
    swapchain_functions.destroy_swapchain(swapchain, None);

    Ok(())
  }

  // ================================================================================
  //  Pipeline Helper Methods
  // ================================================================================
  /// Creates a simple forward render pass with one subpass.
  fn create_forward_render_pass(
    logical_device: &Device, format: vk::Format,
  ) -> SarektResult<vk::RenderPass> {
    // Used to reference attachments in render passes.
    let color_attachment = vk::AttachmentDescription::builder()
      .format(format)
      .samples(vk::SampleCountFlags::TYPE_1)
      .load_op(vk::AttachmentLoadOp::CLEAR) // Clear on loading the color attachment, since we're writing over it.
      .store_op(vk::AttachmentStoreOp::STORE) // Want to save to this attachment in the pass.
      .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE) // Not using stencil.
      .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE) // Not using stencil.
      .initial_layout(vk::ImageLayout::UNDEFINED) // Don't know the layout coming in.
      .final_layout(vk::ImageLayout::PRESENT_SRC_KHR) // TODO OFFSCREEN only do this if going to present. Otherwise TransferDST optimal would be good.
      .build();
    // Used to reference attachments in subpasses.
    let color_attachment_ref = vk::AttachmentReference::builder()
      .attachment(0) // Only using 1 (indexed from 0) attachment.
      .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL) // We're drawing color so optimize the pass to draw color to this attachment.
      .build();

    // Subpasses could also reference previous subpasses as input, depth/stencil
    // data, or preserve attachments to send them to the next subpass.
    let subpass_description = vk::SubpassDescription::builder()
      .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS) // This is a graphics subpass
      // index of this attachment here is a reference to the output of the shader in the form of layout(location = 0).
      .color_attachments(&[color_attachment_ref])
      .build();

    let dependency = vk::SubpassDependency::builder()
      .src_subpass(vk::SUBPASS_EXTERNAL)
      .dst_subpass(0u32)
      .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT) // We need to wait until the image is not in use (by the swapchain for example).
      .src_access_mask(vk::AccessFlags::empty()) // We're not going to access the swapchain as a source.
      .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT) // Anyone waiting on this should wait in the color attachment stage.
      .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE) // Dependents should wait if they read or write the color attachment.
      .build();

    let render_pass_ci = vk::RenderPassCreateInfo::builder()
      .attachments(&[color_attachment])
      .subpasses(&[subpass_description]) // Only one subpass in this case.
      .dependencies(&[dependency]) // Only one dep.
      .build();

    Ok(unsafe { logical_device.create_render_pass(&render_pass_ci, None)? })
  }

  /// Creates the base pipeline for Sarekt.  A user can load custom shaders,
  /// etc, to create custom pipelines (passed back as opaque handles) based off
  /// this one that they can pass when requesting a draw.
  ///
  /// TODO RENDERING_CAPABILITIES allow for creating custom pipelines via
  /// LoadShaders etc.
  ///
  /// TODO RENDERING_CAPABILITIES enable pipeline cache.
  fn create_base_graphics_pipeline_and_shaders(
    logical_device: &Device, shader_store: &Arc<RwLock<ShaderStore<VulkanShaderFunctions>>>,
    extent: Extent2D, render_pass: vk::RenderPass,
  ) -> SarektResult<BasePipelineBundle> {
    let (vertex_shader_handle, fragment_shader_handle) =
      Self::create_default_shaders(shader_store)?;
    Self::create_base_graphics_pipeline(
      logical_device,
      shader_store,
      extent,
      render_pass,
      vertex_shader_handle,
      fragment_shader_handle,
    )
  }

  fn create_default_shaders(
    shader_store: &Arc<RwLock<ShaderStore<VulkanShaderFunctions>>>,
  ) -> SarektResult<(VulkanShaderHandle, VulkanShaderHandle)> {
    let vertex_shader_handle = ShaderStore::load_shader(
      shader_store,
      &ShaderCode::Spirv(DEFAULT_VERTEX_SHADER),
      ShaderType::Vertex,
    )?;
    let fragment_shader_handle = ShaderStore::load_shader(
      shader_store,
      &ShaderCode::Spirv(DEFAULT_FRAGMENT_SHADER),
      ShaderType::Vertex,
    )?;

    Ok((vertex_shader_handle, fragment_shader_handle))
  }

  fn create_base_graphics_pipeline(
    logical_device: &Device, shader_store: &Arc<RwLock<ShaderStore<VulkanShaderFunctions>>>,
    extent: Extent2D, render_pass: vk::RenderPass, vertex_shader_handle: VulkanShaderHandle,
    fragment_shader_handle: VulkanShaderHandle,
  ) -> SarektResult<BasePipelineBundle> {
    let shader_store = shader_store.read().unwrap();

    let entry_point_name = CString::new("main").unwrap();
    let vert_shader_stage_ci = vk::PipelineShaderStageCreateInfo::builder()
      .stage(vk::ShaderStageFlags::VERTEX)
      .module(
        shader_store
          .get_shader(&vertex_shader_handle)
          .unwrap()
          .shader_handle,
      )
      .name(&entry_point_name)
      .build();
    let frag_shader_stage_ci = vk::PipelineShaderStageCreateInfo::builder()
      .stage(vk::ShaderStageFlags::FRAGMENT)
      .module(
        shader_store
          .get_shader(&fragment_shader_handle)
          .unwrap()
          .shader_handle,
      )
      .name(&entry_point_name)
      .build();

    let shader_stage_cis = [vert_shader_stage_ci, frag_shader_stage_ci];

    let binding_desc = DefaultForwardShaderVertex::get_binding_description();
    let attr_descs = DefaultForwardShaderVertex::get_attribute_descriptions();
    let vertex_input_ci = vk::PipelineVertexInputStateCreateInfo::builder()
      .vertex_binding_descriptions(&[binding_desc])
      .vertex_attribute_descriptions(&attr_descs)
      .build();

    let input_assembly_ci = vk::PipelineInputAssemblyStateCreateInfo::builder()
      .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
      .primitive_restart_enable(false)
      .build();

    let viewport = vk::Viewport::builder()
      .x(0f32)
      .y(0f32)
      .width(extent.width as f32)
      .height(extent.height as f32)
      .min_depth(0f32)
      .max_depth(1.0f32)
      .build();
    let scissor = vk::Rect2D::builder()
      .offset(Offset2D::default())
      .extent(extent)
      .build();
    let viewport_state_ci = vk::PipelineViewportStateCreateInfo::builder()
      .viewports(&[viewport])
      .scissors(&[scissor])
      .build();

    let raster_state_ci = vk::PipelineRasterizationStateCreateInfo::builder()
      .depth_clamp_enable(false) // Don't clamp things to the edge, cull them.
      .rasterizer_discard_enable(false) // Don't discard geometry.
      .polygon_mode(vk::PolygonMode::FILL) // Fill stuff in. Could also be point or line.
      .line_width(1.0f32)
      .cull_mode(vk::CullModeFlags::BACK) // Back face culling.
      .front_face(vk::FrontFace::CLOCKWISE)
      // Dont turn on depth bias, not adding constants to depth, same with depth_bias_clamp, bias_constant_factor, bias_slope_factor.
      .depth_bias_enable(false)
      .build();

    // Pretty much totall disable this.
    // TODO CONFIG make configurable
    let multisample_state_ci = vk::PipelineMultisampleStateCreateInfo::builder()
      .sample_shading_enable(false)
      .rasterization_samples(vk::SampleCountFlags::TYPE_1)
      .min_sample_shading(1.0f32)
      .alpha_to_coverage_enable(false)
      .alpha_to_one_enable(false)
      .build();

    let color_blend_attachment_state = vk::PipelineColorBlendAttachmentState::builder()
      .color_write_mask(vk::ColorComponentFlags::all()) // RGBA
      .blend_enable(false)
      // everything else optional because its not enabled.
      .build();
    let color_blend_ci = vk::PipelineColorBlendStateCreateInfo::builder()
      .logic_op_enable(false)
      .logic_op(vk::LogicOp::COPY)
      .attachments(&[color_blend_attachment_state])
      .build();

    // No uniforms so there is nothing to layout.
    let pipeline_layout_ci = vk::PipelineLayoutCreateInfo::default();
    let pipeline_layout =
      unsafe { logical_device.create_pipeline_layout(&pipeline_layout_ci, None)? };

    let graphics_pipeline_ci = vk::GraphicsPipelineCreateInfo::builder()
      .flags(vk::PipelineCreateFlags::ALLOW_DERIVATIVES)
      .stages(&shader_stage_cis)
      .vertex_input_state(&vertex_input_ci)
      .input_assembly_state(&input_assembly_ci)
      .viewport_state(&viewport_state_ci)
      .rasterization_state(&raster_state_ci)
      .multisample_state(&multisample_state_ci)
      .color_blend_state(&color_blend_ci)
      .layout(pipeline_layout)
      .render_pass(render_pass)
      .subpass(0) // The subpass where the pipeline will be used.
      // .base_pipeline_handle() // No basepipeline handle, this is the base pipeline!
      // .base_pipeline_index(-1)
      .build();

    // TODO CRITICAL RENDERING_CAPABILITIES use pipeline cache.
    let pipeline = unsafe {
      logical_device.create_graphics_pipelines(
        vk::PipelineCache::null(),
        &[graphics_pipeline_ci],
        None,
      )
    };
    if let Err(err) = pipeline {
      return Err(err.1.into());
    }

    Ok(BasePipelineBundle::new(
      pipeline.unwrap()[0],
      pipeline_layout,
      graphics_pipeline_ci,
      vertex_shader_handle,
      fragment_shader_handle,
    ))
  }

  fn create_framebuffers(
    logical_device: &Device, render_pass: vk::RenderPass, render_target_images: &[ImageAndView],
    extent: vk::Extent2D,
  ) -> SarektResult<Vec<vk::Framebuffer>> {
    let mut framebuffers = Vec::with_capacity(render_target_images.len());

    for image_and_view in render_target_images.iter() {
      let framebuffer_ci = vk::FramebufferCreateInfo::builder()
        .render_pass(render_pass)
        .attachments(&[image_and_view.view])
        .width(extent.width)
        .height(extent.height)
        .layers(1)
        .build();
      let framebuffer = unsafe { logical_device.create_framebuffer(&framebuffer_ci, None)? };
      framebuffers.push(framebuffer);
    }

    Ok(framebuffers)
  }

  // ================================================================================
  //  Command Pool/Buffer Methods
  // ================================================================================
  /// Creates all command pools needed for drawing and presentation on one
  /// thread.
  ///
  /// return is (gfx command pool).  May be expanded in the future (compute
  /// etc).
  fn create_primary_command_pools(
    instance: &Instance, physical_device: vk::PhysicalDevice,
    surface_and_extension: &SurfaceAndExtension, logical_device: &Device,
  ) -> SarektResult<(vk::CommandPool)> {
    let queue_family_indices =
      Self::find_queue_families(instance, physical_device, surface_and_extension)?;

    info!("Command Queues Selected: {:?}", queue_family_indices);

    let gfx_pool_ci = vk::CommandPoolCreateInfo::builder()
      .queue_family_index(queue_family_indices.graphics_queue_family.unwrap())
      .build();

    let gfx_pool = unsafe { logical_device.create_command_pool(&gfx_pool_ci, None)? };
    Ok((gfx_pool))
  }

  fn create_primary_gfx_command_buffers(
    logical_device: &Device, primary_gfx_command_pool: vk::CommandPool,
    framebuffers: &[vk::Framebuffer], extent: vk::Extent2D, render_pass: vk::RenderPass,
    pipeline: vk::Pipeline,
  ) -> SarektResult<Vec<vk::CommandBuffer>> {
    let image_count = framebuffers.len() as u32;
    let gfx_command_buffer_ci = vk::CommandBufferAllocateInfo::builder()
      .command_pool(primary_gfx_command_pool)
      .level(vk::CommandBufferLevel::PRIMARY)
      .command_buffer_count(image_count)
      .build();

    let gfx_command_buffers =
      unsafe { logical_device.allocate_command_buffers(&gfx_command_buffer_ci)? };

    // TODO CRITICAL make delegate user application work to a secondary buffer.
    // Same as for other Drawers for other threads, but just for single
    // threaded.
    for (i, &buffer) in gfx_command_buffers.iter().enumerate() {
      // Start recording.
      let command_buffer_begin_info = vk::CommandBufferBeginInfo::default();
      unsafe { logical_device.begin_command_buffer(buffer, &command_buffer_begin_info)? };

      // Start the (forward) render pass.
      let render_area = vk::Rect2D::builder()
        .offset(vk::Offset2D::default())
        .extent(extent)
        .build();
      let clear_value = vk::ClearValue {
        color: vk::ClearColorValue {
          float32: [0f32, 0f32, 0f32, 1f32],
        },
      };
      let render_pass_begin_info = vk::RenderPassBeginInfo::builder()
        .render_pass(render_pass)
        .framebuffer(framebuffers[i])
        .render_area(render_area)
        .clear_values(&[clear_value]) // Clear to black.
        .build();
      // TODO change from inline to secondary command buffers.
      unsafe {
        logical_device.cmd_begin_render_pass(
          buffer,
          &render_pass_begin_info,
          vk::SubpassContents::INLINE,
        )
      };

      // Bind the pipeline. Can be overridden in secondary buffer by the user.
      // TODO RENDERING_CAPABILITIES MULTITHREADING we can keep track in each thread's
      // command buffer waht pipeline is bound so we don't insert extra rebind
      // commands.
      unsafe {
        logical_device.cmd_bind_pipeline(buffer, vk::PipelineBindPoint::GRAPHICS, pipeline)
      };

      // Draw.  TODO CRITICAL make this a secondary buffer execution.
      unsafe { logical_device.cmd_draw(buffer, 3, 1, 0, 0) };

      // End Render Pass.
      unsafe { logical_device.cmd_end_render_pass(buffer) };

      // End Command Buffer Recording.
      unsafe { logical_device.end_command_buffer(buffer)? };
    }

    Ok(gfx_command_buffers)
  }

  // ================================================================================
  //  Utility Helper Methods
  // ================================================================================
  /// Creates a shader store in the vulkan backend configuration to load and
  /// delete shaders from.
  fn create_shader_store(
    logical_device: &Arc<Device>,
  ) -> Arc<RwLock<ShaderStore<VulkanShaderFunctions>>> {
    let functions = VulkanShaderFunctions::new(logical_device.clone());
    Arc::new(RwLock::new(ShaderStore::new(functions)))
  }
}
impl Renderer for VulkanRenderer {
  type SL = VulkanShaderFunctions;

  // TODO OFFSCREEN handle off screen rendering.
  fn frame(&self) -> SarektResult<()> {
    if !self.rendering_enabled {
      return Ok(());
    }

    let current_fence = self.draw_synchronization.in_flight_fences[self.current_frame_num.get()];
    let image_available_sem =
      self.draw_synchronization.image_available_semaphores[self.current_frame_num.get()];
    let render_finished_sem =
      self.draw_synchronization.render_finished_semaphores[self.current_frame_num.get()];

    // Wait for this frames fence.  Cannot write to the command buffers of this
    // frame.
    unsafe {
      self
        .logical_device
        .wait_for_fences(&[current_fence], true, u64::max_value())?;
    }

    // TODO OFFSCREEN handle drawing without swapchain.
    let (image_index, is_suboptimal) = unsafe {
      // Will return if swapchain is out of date.
      self
        .swapchain_and_extension
        .swapchain_functions
        .acquire_next_image(
          self.swapchain_and_extension.swapchain,
          u64::max_value(),
          image_available_sem,
          vk::Fence::null(),
        )?
    };
    if is_suboptimal {
      warn!("Swapchain is suboptimal!");
    }

    // Make sure we wait on any fences for that swap chain image in flight.
    self.draw_synchronization.ensure_images_not_in_flight(
      &self.logical_device,
      image_index as usize,
      self.current_frame_num.get(),
    )?;

    // Submit draw commands.
    let submit_info = vk::SubmitInfo::builder()
      .wait_semaphores(&[image_available_sem]) // Don't draw until it is ready.
      .wait_dst_stage_mask(&[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT]) // Don't we only need to wait until Color Attachment is ready to start drawing.  Vertex and other shaders can begin sooner.
      .command_buffers(&[self.primary_gfx_command_buffers[image_index as usize]]) // Only use the command buffer corresponding to this image index.
      .signal_semaphores(&[render_finished_sem]) // Signal we're done drawing when we are.
      .build();
    unsafe {
      self.logical_device.reset_fences(&[current_fence])?;
      self
        .logical_device
        .queue_submit(self.queues.graphics_queue, &[submit_info], current_fence)?
    };

    // TODO OFFSCREEN only if presenting to swapchain.
    // Present to swapchain and display completed frame.
    let present_info = vk::PresentInfoKHR::builder()
      .wait_semaphores(&[render_finished_sem])
      .swapchains(&[self.swapchain_and_extension.swapchain])
      .image_indices(&[image_index])
      .build();
    unsafe {
      self
        .swapchain_and_extension
        .swapchain_functions
        .queue_present(self.queues.presentation_queue, &present_info)?
    };

    self
      .current_frame_num
      .set((self.current_frame_num.get() + 1) % MAX_FRAMES_IN_FLIGHT);

    Ok(())
  }

  fn set_rendering_enabled(&mut self, enabled: bool) {
    self.rendering_enabled = enabled;
  }

  fn load_shader(
    &mut self, code: &ShaderCode, shader_type: ShaderType,
  ) -> SarektResult<ShaderHandle<Self::SL>> {
    ShaderStore::load_shader(&self.shader_store, &code, shader_type)
  }

  fn recreate_swapchain(&mut self, width: u32, height: u32) -> SarektResult<()> {
    if width == 0 || height == 0 {
      // It violates the vulkan spec to make extents this small, rendering should be
      // disabled explicitly in this case, but its up the application/platform.
      return Ok(());
    }

    unsafe { self.recreate_swap_chain(width, height) }
  }
}
impl Drawer for VulkanRenderer {
  fn draw(&self) -> SarektResult<()> {
    if !self.rendering_enabled {
      return Ok(());
    }
    Ok(())
  }
}
impl Drop for VulkanRenderer {
  fn drop(&mut self) {
    unsafe {
      info!("Waiting for the device to be idle before cleaning up...");
      if let Err(e) = self.logical_device.device_wait_idle() {
        error!("Failed to wait for idle! {}", e);
      }

      self
        .cleanup_swapchain()
        .expect("Could not clean up swapchain while cleaning up VulkanRenderer...");

      self.draw_synchronization.destroy_all(&self.logical_device);

      info!("Destroying all command pools...");
      self
        .logical_device
        .destroy_command_pool(self.primary_gfx_command_pool, None);

      info!("Destroying all shaders...");
      self.shader_store.write().unwrap().destroy_all_shaders();

      // TODO OFFSCREEN if there is one
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
    vulkan::debug_utils_ext::DebugUserData, ApplicationDetails, EngineDetails, Version,
    VulkanRenderer, IS_DEBUG_MODE,
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

  // TODO AFTER VERTEX write triangle sanity check that can dump buffer and
  // compare to golden image.
}
