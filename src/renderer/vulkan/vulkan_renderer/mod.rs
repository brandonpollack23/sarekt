mod base_pipeline_bundle;
mod debug_utils_ext;
mod depth_buffer;
mod draw_synchronization;
mod surface;
mod swap_chain;
mod vulkan_core;

use crate::{
  error::{SarektError, SarektResult},
  image_data::{ImageData, Monocolor},
  renderer::{
    buffers_and_images::{
      BufferAndImageLoader, BufferImageHandle, BufferImageStore, BufferOrImage, BufferType,
      IndexBufferElemSize, MagnificationMinificationFilter, ResourceType, TextureAddressMode,
      UniformBufferHandle,
    },
    drawable_object::DrawableObject,
    shaders::ShaderStore,
    vertex_bindings::{
      DefaultForwardShaderLayout, DefaultForwardShaderVertex, DescriptorLayoutInfo, VertexBindings,
    },
    vulkan::{
      images::ImageAndView,
      queues::QueueFamilyIndices,
      vulkan_buffer_image_functions::{BufferAndMemoryMapped, ImageAndMemory, ResourceWithMemory},
      vulkan_renderer::{
        base_pipeline_bundle::BasePipelineBundle,
        debug_utils_ext::DebugUserData,
        depth_buffer::DepthResources,
        draw_synchronization::DrawSynchronization,
        surface::SurfaceAndExtension,
        swap_chain::SwapchainAndExtension,
        vulkan_core::{VulkanCoreStructures, VulkanDeviceStructures},
      },
      vulkan_shader_functions::VulkanShaderFunctions,
      VulkanShaderHandle,
    },
    ApplicationDetails, Drawer, EngineDetails, Renderer, ShaderCode, ShaderHandle, ShaderType,
    VulkanBufferFunctions, MAX_FRAMES_IN_FLIGHT,
  },
};
use ash::{
  version::{DeviceV1_0, InstanceV1_0},
  vk,
  vk::{Extent2D, Offset2D},
  Device, Instance,
};
use log::{error, info, warn};
use raw_window_handle::HasRawWindowHandle;
use static_assertions::_core::mem::ManuallyDrop;
use std::{
  cell::Cell,
  ffi::CStr,
  pin::Pin,
  sync::{Arc, RwLock},
};
use vk_shader_macros::include_glsl;

// TODO PERFORMANCE can i make things like descriptor set count and uniform
// buffers allocate number of frames in flight, not render target image count?

/// Default vertex shader that contain their own vertices, will be removed in
/// the future.
pub const DEFAULT_VERTEX_SHADER: &[u32] = include_glsl!("shaders/sarekt_forward.vert");
/// Default fragment shader that contain their own vertices, will be removed in
/// the future.
pub const DEFAULT_FRAGMENT_SHADER: &[u32] = include_glsl!("shaders/sarekt_forward.frag");

pub struct VulkanRenderer {
  vulkan_core: ManuallyDrop<VulkanCoreStructures>,
  vulkan_device_structures: ManuallyDrop<VulkanDeviceStructures>,

  // TODO NOW AFTER seperate sections into sub files, like now depth_buffer is.
  /// The Sarekt Vulkan Renderer, see module level documentation for details.
  // Rendering related.
  swapchain_and_extension: SwapchainAndExtension, // TODO OFFSCREEN option
  render_targets: Vec<ImageAndView>, // aka SwapChainImages if presenting.
  extent: vk::Extent2D,

  // Pipeline related
  forward_render_pass: vk::RenderPass,
  base_graphics_pipeline_bundle: BasePipelineBundle,
  framebuffers: Vec<vk::Framebuffer>,

  // Command pools, buffers, drawing, and synchronization related primitives and information.
  main_gfx_command_pool: vk::CommandPool,
  primary_gfx_command_buffers: Vec<vk::CommandBuffer>,
  transfer_command_pool: vk::CommandPool,
  draw_synchronization: DrawSynchronization,
  // Frame count since swapchain creation, not beginning of rendering.
  // TODO CRITICAL renderer function that returns this.
  frame_count: Cell<usize>,
  // Frame in flight number 0..MAX_FRAMES_IN_FLIGHT
  current_frame_num: Cell<usize>,
  next_image_index: Cell<usize>,

  // Descriptor pools.
  main_descriptor_pools: Vec<vk::DescriptorPool>,

  // Application controllable fields
  rendering_enabled: bool,

  // Utilities
  #[allow(dead_code)]
  allocator: Arc<vk_mem::Allocator>,
  shader_store: Arc<RwLock<ShaderStore<VulkanShaderFunctions>>>,
  // Manually drop so that the underlying allocator can be dropped in this class.
  buffer_image_store: ManuallyDrop<Arc<RwLock<BufferImageStore<VulkanBufferFunctions>>>>,

  // Null objects for default pipeline.
  default_texture: Option<(
    BufferImageHandle<VulkanBufferFunctions>,
    BufferOrImage<ResourceWithMemory>,
  )>,
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
  ) -> SarektResult<Self> {
    let window = window
      .into()
      .expect("Sarekt only supports rendering to a window right now :(");

    // TODO OFFSCREEN Support rendering to a non window surface if window is None
    // (change it to an Enum of WindowHandle or OtherSurface).
    info!("Creating Sarekt Renderer with Vulkan Backend...");

    let vulkan_core = ManuallyDrop::new(VulkanCoreStructures::new(
      window.as_ref(),
      application_details,
      engine_details,
      debug_user_data,
    )?);

    let vulkan_device_structures = ManuallyDrop::new(VulkanDeviceStructures::new(&vulkan_core)?);
    let physical_device = vulkan_device_structures.physical_device;
    let logical_device = &vulkan_device_structures.logical_device;
    let queue_families = &vulkan_device_structures.queue_families;
    let queues = &vulkan_device_structures.queues;

    // TODO OFFSCREEN only create if drawing to window, get format and extent
    // elsewhere.
    let swapchain_extension =
      ash::extensions::khr::Swapchain::new(vulkan_core.instance.as_ref(), logical_device.as_ref());
    let (swapchain, format, extent) = Self::create_swap_chain(
      &vulkan_core.surface_and_extension,
      &swapchain_extension,
      physical_device,
      queue_families,
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

    let (main_gfx_command_pool, transfer_command_pool) =
      Self::create_primary_command_pools(queue_families, &logical_device)?;

    let allocator = Self::create_memory_allocator(
      vulkan_core.instance.as_ref().clone(),
      physical_device,
      logical_device.as_ref().clone(),
    )?;
    let shader_store = Self::create_shader_store(&logical_device);

    // TODO MULTITHREADING all graphics command pools needed here to specify
    // concurrent access.
    let buffer_image_store = ManuallyDrop::new(Self::create_buffer_image_store(
      &logical_device,
      &allocator,
      queue_families.graphics_queue_family.unwrap(),
      queue_families.transfer_queue_family.unwrap(),
      transfer_command_pool,
      queues.transfer_queue,
      main_gfx_command_pool,
      queues.graphics_queue,
    )?);

    // TODO RENDERING_CAPABILITIES support other render pass types.
    let depth_buffer = DepthResources::new(
      &vulkan_core.instance,
      physical_device,
      &buffer_image_store,
      (extent.width, extent.height),
    )?;
    let forward_render_pass =
      Self::create_forward_render_pass(&logical_device, format, &depth_buffer)?;

    // TODO RENDERING_CAPABILITIES when I can have multiple render pass types I need
    // new framebuffers.
    let framebuffers = Self::create_framebuffers(
      &logical_device,
      forward_render_pass,
      &depth_buffer,
      &render_targets,
      extent,
    )?;

    let base_graphics_pipeline_bundle = Self::create_base_graphics_pipeline_and_shaders(
      &logical_device,
      &shader_store, // Unlock and get a local mut ref to shaderstore.
      extent,
      forward_render_pass,
      depth_buffer,
    )?;

    let primary_gfx_command_buffers =
      Self::create_main_gfx_command_buffers(&logical_device, main_gfx_command_pool, &framebuffers)?;

    let draw_synchronization =
      DrawSynchronization::new(logical_device.clone(), render_targets.len())?;

    let main_descriptor_pools = Self::create_main_descriptor_pools(
      &vulkan_core.instance,
      physical_device,
      &logical_device,
      &render_targets,
    )?;

    let mut renderer = Self {
      vulkan_core,
      vulkan_device_structures,

      swapchain_and_extension,
      render_targets,
      extent,

      forward_render_pass,
      base_graphics_pipeline_bundle,
      framebuffers,

      main_gfx_command_pool,
      primary_gfx_command_buffers,
      transfer_command_pool,
      draw_synchronization,
      frame_count: Cell::new(0),
      current_frame_num: Cell::new(0),
      next_image_index: Cell::new(0),

      main_descriptor_pools,

      rendering_enabled: true,

      allocator,
      shader_store,
      buffer_image_store,

      default_texture: None,
    };

    renderer.create_default_texture();

    // Begin recording first command buffer so the first call to Drawer::draw is
    // ready to record.  The rest are started by Renderer::frame.
    renderer.setup_next_main_command_buffer()?;

    Ok(renderer)
  }
}
/// Private implementation details.
impl VulkanRenderer {
  // ================================================================================
  //  Presentation and Swapchain Helper Methods
  // ================================================================================
  /// Based on the capabilities of the surface, the physical device, and the
  /// configuration of sarekt, creates a swapchain with the appropriate
  /// configuration (format, color space, present mode, and extent).
  fn create_swap_chain(
    surface_and_extension: &SurfaceAndExtension,
    swapchain_extension: &ash::extensions::khr::Swapchain, physical_device: vk::PhysicalDevice,
    queue_family_indices: &QueueFamilyIndices, requested_width: u32, requested_height: u32,
    old_swapchain: Option<vk::SwapchainKHR>,
  ) -> SarektResult<(vk::SwapchainKHR, vk::Format, vk::Extent2D)> {
    let swapchain_support =
      VulkanDeviceStructures::query_swap_chain_support(surface_and_extension, physical_device)?;

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
    let max_image_count = swapchain_support.capabilities.max_image_count;
    let max_image_count = if max_image_count == 0 {
      u32::max_value()
    } else {
      max_image_count
    };
    let min_image_count = (swapchain_support.capabilities.min_image_count + 1).min(max_image_count);

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
      .queue_family_indices(&queue_family_indices.as_vec().unwrap())
      .pre_transform(swapchain_support.capabilities.current_transform) // Match the transform of the swapchain, I'm not trying to redner upside down!
      .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE) // No alpha blending within the window system for now.
      .present_mode(present_mode)
      .clipped(true) // Go ahead and discard rendering ops we dont need (window half off screen).
      .old_swapchain(old_swapchain.unwrap_or_else(vk::SwapchainKHR::null)) // Pass old swapchain for recreation.
      .build();

    let swapchain = unsafe { swapchain_extension.create_swapchain(&swapchain_ci, None)? };
    Ok((swapchain, format.format, extent))
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
        format.format == vk::Format::B8G8R8A8_UNORM
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
  unsafe fn do_recreate_swapchain(&mut self, width: u32, height: u32) -> SarektResult<()> {
    let instance = &self.vulkan_core.instance;
    let surface_and_extension = &self.vulkan_core.surface_and_extension;
    let logical_device = &self.vulkan_device_structures.logical_device;
    let physical_device = self.vulkan_device_structures.physical_device;
    let queue_family_indices = &self.vulkan_device_structures.queue_families;
    let old_swapchain = self.swapchain_and_extension.swapchain;
    let swapchain_extension = &self.swapchain_and_extension.swapchain_functions;
    let shader_store = &self.shader_store;

    // Procedure: Wait for the device to be idle, make new Swapchain (recycling old
    // one), cleanup old resources and recreate them:
    // * ImageViews
    // * Render Passes
    // * Graphics Pipelines
    // * Framebuffers
    // * Command Buffers.
    logical_device.device_wait_idle()?;

    let (new_swapchain, new_format, new_extent) = Self::create_swap_chain(
      surface_and_extension,
      swapchain_extension,
      physical_device,
      queue_family_indices,
      width,
      height,
      Some(old_swapchain),
    )?;
    self.cleanup_swapchain()?;

    // Create all new resources and set them in this struct.
    self.swapchain_and_extension.swapchain = new_swapchain;
    self.swapchain_and_extension.format = new_format;
    self.extent = new_extent;

    // TODO OFFSCREEN if not swapchain create images that im rendering to.
    let render_target_images = swapchain_extension.get_swapchain_images(new_swapchain)?;
    self.render_targets =
      Self::create_render_target_image_views(logical_device, render_target_images, new_format)?;

    let depth_buffer = DepthResources::new(
      &instance,
      physical_device,
      &self.buffer_image_store,
      (width, height),
    )?;

    self.forward_render_pass =
      Self::create_forward_render_pass(logical_device, new_format, &depth_buffer)?;

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
    let descriptor_set_layouts = self
      .base_graphics_pipeline_bundle
      .descriptor_set_layouts
      .take();

    self.framebuffers = Self::create_framebuffers(
      logical_device,
      self.forward_render_pass,
      &depth_buffer,
      &self.render_targets,
      new_extent,
    )?;

    self.base_graphics_pipeline_bundle = Self::create_base_graphics_pipeline(
      logical_device,
      shader_store,
      new_extent,
      self.forward_render_pass,
      depth_buffer,
      descriptor_set_layouts.unwrap(),
      vertex_shader_handle.unwrap(),
      fragment_shader_handle.unwrap(),
    )?;

    self.main_descriptor_pools = Self::create_main_descriptor_pools(
      instance,
      physical_device,
      logical_device,
      &self.render_targets,
    )?;

    // Reset render_frame_count
    self.current_frame_num.set(0);

    // Reset command buffers and rerun setup.
    logical_device.reset_command_pool(
      self.main_gfx_command_pool,
      vk::CommandPoolResetFlags::empty(),
    )?;
    self.draw_synchronization.recreate_semaphores()?;
    self.setup_next_main_command_buffer()?;

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
    let logical_device = &self.vulkan_device_structures.logical_device;

    self.draw_synchronization.wait_for_all_frames()?;

    info!("Destroying descriptor pools...");
    for &desc_pool in self.main_descriptor_pools.iter() {
      logical_device.destroy_descriptor_pool(desc_pool, None);
    }

    info!("Destroying all framebuffers...");
    for &fb in self.framebuffers.iter() {
      logical_device.destroy_framebuffer(fb, None);
    }

    info!("Destroying base graphics pipeline...");
    logical_device.destroy_pipeline(self.base_graphics_pipeline_bundle.pipeline, None);

    info!("Destroying base pipeline layouts...");
    logical_device
      .destroy_pipeline_layout(self.base_graphics_pipeline_bundle.pipeline_layout, None);

    info!("Destroying render pass...");
    logical_device.destroy_render_pass(self.forward_render_pass, None);

    info!("Destrying render target views...");
    for view in self.render_targets.iter() {
      logical_device.destroy_image_view(view.view, None);
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
    logical_device: &Device, format: vk::Format, depth_buffer: &DepthResources,
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
      .attachment(0u32) // Only using 1 (indexed from 0) attachment.
      .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL) // We're drawing color so optimize the pass to draw color to this attachment.
      .build();
    let color_attachment_refs = [color_attachment_ref];

    let depth_attachment = vk::AttachmentDescription::builder()
      .format(depth_buffer.format)
      .samples(vk::SampleCountFlags::TYPE_1)
      .load_op(vk::AttachmentLoadOp::CLEAR)
      .store_op(vk::AttachmentStoreOp::DONT_CARE)
      .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
      .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
      .initial_layout(vk::ImageLayout::UNDEFINED)
      .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
      .build();
    let depth_attachment_ref = vk::AttachmentReference::builder()
      .attachment(1)
      .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
      .build();

    let attachments = [color_attachment, depth_attachment];

    // Subpasses could also reference previous subpasses as input, depth/stencil
    // data, or preserve attachments to send them to the next subpass.
    let subpass_description = vk::SubpassDescription::builder()
      .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS) // This is a graphics subpass
      // index of this attachment here is a reference to the output of the shader in the form of layout(location = 0).
      .color_attachments(&color_attachment_refs)
      .depth_stencil_attachment(&depth_attachment_ref)
      .build();
    let subpass_descriptions = [subpass_description];

    let dependency = vk::SubpassDependency::builder()
      .src_subpass(vk::SUBPASS_EXTERNAL)
      .dst_subpass(0u32)
      .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT) // We need to wait until the image is not in use (by the swapchain for example).
      .src_access_mask(vk::AccessFlags::empty()) // We're not going to access the swapchain as a source.
      .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT) // Anyone waiting on this should wait in the color attachment stage.
      .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE) // Dependents should wait if they read or write the color attachment.
      .build();
    let dependencies = [dependency];

    let render_pass_ci = vk::RenderPassCreateInfo::builder()
      .attachments(&attachments)
      .subpasses(&subpass_descriptions) // Only one subpass in this case.
      .dependencies(&dependencies) // Only one dep.
      .build();

    Ok(unsafe { logical_device.create_render_pass(&render_pass_ci, None)? })
  }

  /// Creates the base pipeline for Sarekt.  A user can load custom shaders,
  /// etc, to create custom pipelines (passed back as opaque handles) based off
  /// this one that they can pass when requesting a draw.
  ///
  /// TODO RENDERING_CAPABILITIES allow for creating custom pipelines via
  /// LoadShaders etc.  When that is done, allow for disabling default pipeline
  /// creation via config if it wont be used to save resources.
  ///
  /// TODO RENDERING_CAPABILITIES enable pipeline cache.
  fn create_base_graphics_pipeline_and_shaders(
    logical_device: &Device, shader_store: &Arc<RwLock<ShaderStore<VulkanShaderFunctions>>>,
    extent: Extent2D, render_pass: vk::RenderPass, depth_buffer: DepthResources,
  ) -> SarektResult<BasePipelineBundle> {
    let (vertex_shader_handle, fragment_shader_handle) =
      Self::create_default_shaders(shader_store)?;

    let default_descriptor_set_layouts =
      Self::create_default_descriptor_set_layouts(logical_device)?;

    Self::create_base_graphics_pipeline(
      logical_device,
      shader_store,
      extent,
      render_pass,
      depth_buffer,
      default_descriptor_set_layouts,
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

  fn create_default_descriptor_set_layouts(
    logical_device: &Device,
  ) -> SarektResult<Vec<vk::DescriptorSetLayout>> {
    // Create descriptor set layouts for the default forward shader uniforms.
    let descriptor_set_layout_bindings =
      DefaultForwardShaderLayout::get_descriptor_set_layout_bindings();
    let descriptor_set_layout_ci = vk::DescriptorSetLayoutCreateInfo::builder()
      .bindings(&descriptor_set_layout_bindings)
      .build();

    // One descriptor set layout contains all the bindings for the shader.
    // Pipline_layout_ci requires an array because of something I've yet to learn in
    // descriptor_pools and descriptor_sets.

    let descriptor_set_layout =
      unsafe { logical_device.create_descriptor_set_layout(&descriptor_set_layout_ci, None)? };
    Ok(vec![descriptor_set_layout])
  }

  fn create_base_graphics_pipeline(
    logical_device: &Device, shader_store: &Arc<RwLock<ShaderStore<VulkanShaderFunctions>>>,
    extent: Extent2D, render_pass: vk::RenderPass, depth_buffer: DepthResources,
    descriptor_set_layouts: Vec<vk::DescriptorSetLayout>, vertex_shader_handle: VulkanShaderHandle,
    fragment_shader_handle: VulkanShaderHandle,
  ) -> SarektResult<BasePipelineBundle> {
    let shader_store = shader_store.read().unwrap();

    let entry_point_name = CStr::from_bytes_with_nul(b"main\0").unwrap();
    let vert_shader_stage_ci = vk::PipelineShaderStageCreateInfo::builder()
      .stage(vk::ShaderStageFlags::VERTEX)
      .module(
        shader_store
          .get_shader(&vertex_shader_handle)
          .unwrap()
          .shader_handle,
      )
      .name(entry_point_name)
      .build();
    let frag_shader_stage_ci = vk::PipelineShaderStageCreateInfo::builder()
      .stage(vk::ShaderStageFlags::FRAGMENT)
      .module(
        shader_store
          .get_shader(&fragment_shader_handle)
          .unwrap()
          .shader_handle,
      )
      .name(entry_point_name)
      .build();

    let shader_stage_cis = [vert_shader_stage_ci, frag_shader_stage_ci];

    let binding_descs = [DefaultForwardShaderVertex::get_binding_description()];
    let attr_descs = DefaultForwardShaderVertex::get_attribute_descriptions();
    let vertex_input_ci = vk::PipelineVertexInputStateCreateInfo::builder()
      .vertex_binding_descriptions(&binding_descs)
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
    let viewports = [viewport];
    let scissor = vk::Rect2D::builder()
      .offset(Offset2D::default())
      .extent(extent)
      .build();
    let scissors = [scissor];
    let viewport_state_ci = vk::PipelineViewportStateCreateInfo::builder()
      .viewports(&viewports)
      .scissors(&scissors)
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

    // TODO CONFIG enable stencil.
    // TODO TRANSPARENCY a transparent pipeline (would be a seperate, similar
    // pipeline) would set depth_write to false (test depth but use existing opaque
    // object for buffer).
    let depth_stencil_ci = vk::PipelineDepthStencilStateCreateInfo::builder()
      .depth_test_enable(true)
      .depth_write_enable(true)
      .depth_compare_op(vk::CompareOp::LESS) // Lower depth closer.
      .depth_bounds_test_enable(false) // Not using bounds test.
      .min_depth_bounds(0.0f32)
      .max_depth_bounds(1.0f32)
      .stencil_test_enable(false)
      // .front(vk::StencilOpState) // For use in stencil test
      // .back(vk::StencilOpState)
      .build();

    let color_blend_attachment_state = vk::PipelineColorBlendAttachmentState::builder()
      .color_write_mask(vk::ColorComponentFlags::all()) // RGBA
      .blend_enable(false)
      // everything else optional because its not enabled.
      .build();
    let attachments = [color_blend_attachment_state];
    let color_blend_ci = vk::PipelineColorBlendStateCreateInfo::builder()
      .logic_op_enable(false)
      .logic_op(vk::LogicOp::COPY)
      .attachments(&attachments)
      .build();

    let pipeline_layout_ci = vk::PipelineLayoutCreateInfo::builder()
      .set_layouts(&descriptor_set_layouts)
      .build();
    let pipeline_layout =
      unsafe { logical_device.create_pipeline_layout(&pipeline_layout_ci, None)? };

    let base_graphics_pipeline_ci = vk::GraphicsPipelineCreateInfo::builder()
      .flags(vk::PipelineCreateFlags::ALLOW_DERIVATIVES)
      .stages(&shader_stage_cis)
      .vertex_input_state(&vertex_input_ci)
      .input_assembly_state(&input_assembly_ci)
      .viewport_state(&viewport_state_ci)
      .rasterization_state(&raster_state_ci)
      .multisample_state(&multisample_state_ci)
      .depth_stencil_state(&depth_stencil_ci)
      .color_blend_state(&color_blend_ci)
      .layout(pipeline_layout)
      .render_pass(render_pass)
      .subpass(0) // The subpass where the pipeline will be used.
      // .base_pipeline_handle() // No basepipeline handle, this is the base pipeline!
      // .base_pipeline_index(-1)
      .build();

    // TODO CRITICAL RENDERING_CAPABILITIES use pipeline cache.
    let pipeline_create_infos = [base_graphics_pipeline_ci];
    let pipeline = unsafe {
      logical_device.create_graphics_pipelines(
        vk::PipelineCache::null(),
        &pipeline_create_infos,
        None,
      )
    };
    if let Err(err) = pipeline {
      return Err(err.1.into());
    }

    Ok(BasePipelineBundle::new(
      pipeline.unwrap()[0],
      pipeline_layout,
      base_graphics_pipeline_ci,
      descriptor_set_layouts,
      depth_buffer,
      vertex_shader_handle,
      fragment_shader_handle,
    ))
  }

  fn create_framebuffers(
    logical_device: &Device, render_pass: vk::RenderPass, depth_buffer: &DepthResources,
    render_target_images: &[ImageAndView], extent: vk::Extent2D,
  ) -> SarektResult<Vec<vk::Framebuffer>> {
    let mut framebuffers = Vec::with_capacity(render_target_images.len());

    for image_and_view in render_target_images.iter() {
      let attachments = [
        image_and_view.view,
        depth_buffer.image_and_memory.image_and_view.view,
      ];
      let framebuffer_ci = vk::FramebufferCreateInfo::builder()
        .render_pass(render_pass)
        .attachments(&attachments)
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
  //  Command & Descriptor Pool/Buffer Methods
  // ================================================================================
  /// Creates all command pools needed for drawing and presentation on one
  /// thread.
  ///
  /// return is (gfx command pool, transfer command pool).
  ///
  /// May be expanded in the future (compute etc).
  fn create_primary_command_pools(
    queue_family_indices: &QueueFamilyIndices, logical_device: &Device,
  ) -> SarektResult<(vk::CommandPool, vk::CommandPool)> {
    info!("Command Queues Selected: {:?}", queue_family_indices);

    let gfx_pool_ci = vk::CommandPoolCreateInfo::builder()
      .queue_family_index(queue_family_indices.graphics_queue_family.unwrap())
      .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER) // TODO PERFORMANCE create one command pool for each framebuffer to allow resetting individually at the pool level?
      .build();

    let gfx_pool = unsafe { logical_device.create_command_pool(&gfx_pool_ci, None)? };

    let transfer_pool =
      if queue_family_indices.graphics_queue_family == queue_family_indices.transfer_queue_family {
        gfx_pool
      } else {
        let transfer_pool_ci = vk::CommandPoolCreateInfo::builder()
          .queue_family_index(queue_family_indices.transfer_queue_family.unwrap())
          .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
          .build();
        unsafe { logical_device.create_command_pool(&transfer_pool_ci, None)? }
      };

    Ok((gfx_pool, transfer_pool))
  }

  /// Creates command buffer for main thread to make draw calls on.
  ///
  /// TODO MULTITHREADING one secondary per thread.
  fn create_main_gfx_command_buffers(
    logical_device: &Device, primary_gfx_command_pool: vk::CommandPool,
    framebuffers: &[vk::Framebuffer],
  ) -> SarektResult<Vec<vk::CommandBuffer>> {
    let image_count = framebuffers.len() as u32;
    let gfx_command_buffer_ci = vk::CommandBufferAllocateInfo::builder()
      .command_pool(primary_gfx_command_pool)
      .level(vk::CommandBufferLevel::PRIMARY)
      .command_buffer_count(image_count)
      .build();

    let primary_gfx_command_buffers =
      unsafe { logical_device.allocate_command_buffers(&gfx_command_buffer_ci)? };

    Ok(primary_gfx_command_buffers)
  }

  /// Sets up the command buffers for recording.
  /// The command buffers are written to by the [Drawer](trait.Drawer.html) draw
  /// commands.
  fn setup_next_main_command_buffer(&self) -> SarektResult<()> {
    let current_frame_num = self.current_frame_num.get();
    let image_available_sem = self
      .draw_synchronization
      .get_image_available_sem(current_frame_num);

    self.draw_synchronization.wait_for_acquire_fence()?;
    self.draw_synchronization.reset_acquire_fence()?;
    // TODO OFFSCREEN handle drawing without swapchain.
    // Get next image to render to.
    let (image_index, is_suboptimal) = unsafe {
      // Will return if swapchain is out of date.
      self
        .swapchain_and_extension
        .swapchain_functions
        .acquire_next_image(
          self.swapchain_and_extension.swapchain,
          u64::max_value(),
          image_available_sem,
          self.draw_synchronization.get_acquire_fence(),
        )?
    };
    if is_suboptimal {
      warn!("Swapchain is suboptimal!");
    }

    // TODO MULTITHREADING all things that were only main thread, do for all
    // renderers, too.
    let logical_device = &self.vulkan_device_structures.logical_device;
    let descriptor_pool = self.main_descriptor_pools[image_index as usize];
    let command_buffer = self.primary_gfx_command_buffers[image_index as usize];
    let framebuffer = self.framebuffers[image_index as usize];
    let extent = self.extent;
    let render_pass = self.forward_render_pass;
    let pipeline = self.base_graphics_pipeline_bundle.pipeline;

    // Make sure we wait on any fences for that swap chain image in flight.  Can't
    // write to a command buffer if it is in flight.
    let fence = self
      .draw_synchronization
      .get_image_fence(image_index as usize);
    if fence != vk::Fence::null() {
      unsafe {
        logical_device.wait_for_fences(&[fence], true, u64::max_value())?;
      }
    }

    unsafe {
      // TODO PERFORMANCE cache descriptor sets: https://github.com/KhronosGroup/Vulkan-Samples/blob/master/samples/performance/descriptor_management/descriptor_management_tutorial.md
      logical_device
        .reset_descriptor_pool(descriptor_pool, vk::DescriptorPoolResetFlags::empty())?;
    }

    // Start recording.
    unsafe {
      let begin_ci = vk::CommandBufferBeginInfo::builder()
        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
        .build();
      logical_device.begin_command_buffer(command_buffer, &begin_ci)?
    };

    unsafe {
      // Start the (forward) render pass.
      let render_area = vk::Rect2D::builder()
        .offset(vk::Offset2D::default())
        .extent(extent)
        .build();
      let clear_color_value = vk::ClearValue {
        color: vk::ClearColorValue {
          float32: [0f32, 0f32, 0f32, 1f32],
        },
      };
      let clear_depth_value = vk::ClearValue {
        depth_stencil: vk::ClearDepthStencilValue {
          depth: 1.0f32,
          stencil: 0u32,
        },
      };
      let clear_values = [clear_color_value, clear_depth_value];
      let render_pass_begin_info = vk::RenderPassBeginInfo::builder()
        .render_pass(render_pass)
        .framebuffer(framebuffer)
        .render_area(render_area)
        .clear_values(&clear_values) // Clear to black.
        .build();

      logical_device.cmd_begin_render_pass(
        command_buffer,
        &render_pass_begin_info,
        vk::SubpassContents::INLINE,
      );

      // Bind the pipeline. Can be overridden in secondary buffer by the user.
      // TODO RENDERING_CAPABILITIES MULTITHREADING we can keep track in each thread's
      // command buffer waht pipeline is bound so we don't insert extra rebind
      // commands.
      logical_device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline)
    };

    // Save image index for frame presentation.
    self.next_image_index.set(image_index as usize);

    // Draw occurs in in the Drawer::draw command.
    // Render pass completion occurs in Renderer::frame
    Ok(())
  }

  fn create_main_descriptor_pools(
    instance: &Instance, physical_device: vk::PhysicalDevice, logical_device: &Device,
    render_targets: &[ImageAndView],
  ) -> SarektResult<Vec<vk::DescriptorPool>> {
    // TODO MULTITHREADING one per per frame per thread.
    // TODO TEXTURE add texture descriptor set pool.

    let physical_device_properties =
      unsafe { instance.get_physical_device_properties(physical_device) };

    let max_uniform_buffers = physical_device_properties
      .limits
      .max_descriptor_set_uniform_buffers;
    let max_combined_image_samplers = physical_device_properties
      .limits
      .max_descriptor_set_samplers;

    let pool_sizes = [
      vk::DescriptorPoolSize::builder()
        .ty(vk::DescriptorType::UNIFORM_BUFFER)
        .descriptor_count(max_uniform_buffers)
        .build(),
      vk::DescriptorPoolSize::builder()
        .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(max_combined_image_samplers)
        .build(),
    ];

    info!("Creating descriptor pool with sizes: {:?}", pool_sizes);

    let descriptor_pool_ci = vk::DescriptorPoolCreateInfo::builder()
      .pool_sizes(&pool_sizes)
      .max_sets(
        max_uniform_buffers.min(max_combined_image_samplers)
      ) // One set per render object.
      .build();

    info!(
      "Creating descriptor pool with configuration: {:?}",
      descriptor_pool_ci
    );

    let mut main_descriptor_pools = Vec::with_capacity(render_targets.len());
    for _ in 0..render_targets.len() {
      unsafe {
        main_descriptor_pools
          .push(logical_device.create_descriptor_pool(&descriptor_pool_ci, None)?);
      };
    }

    Ok(main_descriptor_pools)
  }

  // ================================================================================
  //  Storage Creation Methods
  // ================================================================================
  /// Creates a [Vulkan Memory ALlocator](https://github.com/gwihlidal/vk-mem-rs)
  fn create_memory_allocator(
    instance: Instance, physical_device: vk::PhysicalDevice, logical_device: Device,
  ) -> SarektResult<Arc<vk_mem::Allocator>> {
    let allocator_create_info = vk_mem::AllocatorCreateInfo {
      physical_device,
      device: logical_device,
      instance,
      flags: vk_mem::AllocatorCreateFlags::default(),
      preferred_large_heap_block_size: 0,
      frame_in_use_count: (MAX_FRAMES_IN_FLIGHT - 1) as u32,
      heap_size_limits: None,
    };

    Ok(vk_mem::Allocator::new(&allocator_create_info).map(Arc::new)?)
  }

  /// Creates a shader store in the vulkan backend configuration to load and
  /// delete shaders from.
  fn create_shader_store(
    logical_device: &Arc<Device>,
  ) -> Arc<RwLock<ShaderStore<VulkanShaderFunctions>>> {
    let functions = VulkanShaderFunctions::new(logical_device.clone());
    Arc::new(RwLock::new(ShaderStore::new(functions)))
  }

  fn create_buffer_image_store(
    logical_device: &Arc<Device>, allocator: &Arc<vk_mem::Allocator>, graphics_queue_family: u32,
    transfer_queue_family: u32, transfer_command_pool: vk::CommandPool,
    transfer_command_queue: vk::Queue, graphics_command_pool: vk::CommandPool,
    graphics_command_queue: vk::Queue,
  ) -> SarektResult<Arc<RwLock<BufferImageStore<VulkanBufferFunctions>>>> {
    let functions = VulkanBufferFunctions::new(
      logical_device.clone(),
      allocator.clone(),
      graphics_queue_family,
      transfer_queue_family,
      transfer_command_pool,
      transfer_command_queue,
      graphics_command_pool,
      graphics_command_queue,
    )?;
    Ok(Arc::new(RwLock::new(BufferImageStore::new(functions))))
  }

  // ================================================================================
  //  Draw Helper Methods
  // ================================================================================
  fn draw_vertices_cmd<UniformBufElem: Sized + Copy>(
    &self, object: &DrawableObject<Self, UniformBufElem>, command_buffer: vk::CommandBuffer,
  ) -> SarektResult<()> {
    let logical_device = &self.vulkan_device_structures.logical_device;

    unsafe {
      // Draw vertices.
      let vertex_buffers = [object.vertex_buffer.buffer()?.buffer];
      let vertex_buffer_length = object.vertex_buffer.buffer()?.length;
      let offsets = [0];
      logical_device.cmd_bind_vertex_buffers(
        command_buffer,
        0,
        &vertex_buffers,
        &offsets, // There may be offset into memory, but not into the buffer.
      );

      if object.index_buffer.is_none() {
        // Non indexed draw.
        logical_device.cmd_draw(command_buffer, vertex_buffer_length, 1, 0, 0);
      } else {
        // Indexed Draw.
        let index_buffer = &object.index_buffer.unwrap().buffer()?;
        let index_buffer_element_size = match index_buffer.index_buffer_elem_size.unwrap() {
          IndexBufferElemSize::UInt16 => vk::IndexType::UINT16,
          IndexBufferElemSize::UInt32 => vk::IndexType::UINT32,
        };
        logical_device.cmd_bind_index_buffer(
          command_buffer,
          index_buffer.buffer,
          0,
          index_buffer_element_size,
        );
        logical_device.cmd_draw_indexed(
          command_buffer,
          index_buffer.length,
          1, // One instance
          0, // No index offset
          0, // No vertex offset
          0, // Zeroth instance.
        )
      }
    }
    Ok(())
  }

  fn bind_descriptor_sets<DescriptorLayoutStruct>(
    &self, uniform_buffer: vk::Buffer, texture_image: &Option<ImageAndMemory>,
    descriptor_pool: vk::DescriptorPool, command_buffer: vk::CommandBuffer,
  ) -> SarektResult<()>
  where
    DescriptorLayoutStruct: Sized + Copy + DescriptorLayoutInfo,
  {
    let logical_device = &self.vulkan_device_structures.logical_device;

    // First allocate descriptor sets.
    // TODO PIPELINES pass in current pipeline layout.
    let layouts = [self
      .base_graphics_pipeline_bundle
      .descriptor_set_layouts
      .as_ref()
      .unwrap()[0]];
    let alloc_info = vk::DescriptorSetAllocateInfo::builder()
      .descriptor_pool(descriptor_pool)
      .set_layouts(&layouts) // Sets descriptor set count.
      .build();
    // A vec of a single set.
    let descriptor_sets = unsafe { logical_device.allocate_descriptor_sets(&alloc_info)? };

    // Then configure them to bind the buffer to the pipeline.
    let bind_uniform_info = DescriptorLayoutStruct::get_bind_uniform_info()?;
    let uniform_buffer_infos = vec![vk::DescriptorBufferInfo::builder()
      .buffer(uniform_buffer)
      .offset(bind_uniform_info.offset as vk::DeviceSize)
      .range(bind_uniform_info.range as vk::DeviceSize)
      .build()];

    // TODO TEXTURES SHADERS when there is more than one texture allowed fill a vec
    // with null textures for all unused textures in drawable objects, which will
    // now be a option vec.
    // Either load the texture in the drawable object or use a transparent null
    // texture.
    let bind_texture_info = DescriptorLayoutStruct::get_bind_texture_info()?;
    let image_infos = vec![match texture_image {
      Option::Some(image_and_memory) => vk::DescriptorImageInfo::builder()
        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .image_view(image_and_memory.image_and_view.view)
        .sampler(image_and_memory.sampler.unwrap())
        .build(),
      None => {
        let default_texture = self
          .default_texture
          .as_ref()
          .unwrap()
          .1
          .handle
          .image()
          .unwrap();
        vk::DescriptorImageInfo::builder()
          .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
          .sampler(default_texture.sampler.unwrap())
          .image_view(default_texture.image_and_view.view)
          .build()
      }
    }];

    // TODO SHADERS array elements.
    // Create descriptor writes for uniforms.
    let uniform_descriptor_writes = bind_uniform_info.bindings.iter().map(|&binding| {
      vk::WriteDescriptorSet::builder()
      .dst_set(descriptor_sets[0])
      .dst_binding(binding) // corresponds to binding in layout.
      .dst_array_element(0) // We're not using an array yet, just one MVP so index is 0.
      .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
      .buffer_info(&uniform_buffer_infos)
      // No image infos or texel buffer views because this is a buffer.
      .build()
    });

    // Create and append descriptor writes for textures.
    let texture_descriptor_writes = bind_texture_info.bindings.iter().map(|&binding| {
      vk::WriteDescriptorSet::builder()
        .dst_set(descriptor_sets[0])
        .dst_binding(binding)
        .dst_array_element(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(&image_infos)
        .build()
    });

    let mut descriptor_writes =
      Vec::with_capacity(uniform_descriptor_writes.len() + texture_descriptor_writes.len());
    descriptor_writes.extend(uniform_descriptor_writes);
    descriptor_writes.extend(texture_descriptor_writes);

    unsafe {
      logical_device.update_descriptor_sets(&descriptor_writes, &[]); // No descriptor copies.

      // Bind them to the pipeline layout.
      // TODO PIPELINES select correct pipeline layout.
      logical_device.cmd_bind_descriptor_sets(
        command_buffer,
        vk::PipelineBindPoint::GRAPHICS,
        self.base_graphics_pipeline_bundle.pipeline_layout,
        0,
        &descriptor_sets,
        &[], // No dynamic offsets.
      );
    }

    Ok(())
  }

  // ================================================================================
  //  Null object setup methods
  // ================================================================================
  fn create_default_texture(&mut self) {
    let image_and_handle = BufferImageStore::load_image_with_staging_initialization(
      &self.buffer_image_store,
      Monocolor::clear(),
      MagnificationMinificationFilter::Nearest,
      MagnificationMinificationFilter::Nearest,
      TextureAddressMode::ClampToEdge,
      TextureAddressMode::ClampToEdge,
      TextureAddressMode::ClampToEdge,
    )
    .unwrap();

    self.default_texture = Some(image_and_handle);
  }

  // ================================================================================
  //  Renderer Utility Methods
  // ================================================================================
  fn increment_frame_count(&self) {
    self.frame_count.set(self.frame_count.get() + 1);
    self
      .current_frame_num
      .set((self.current_frame_num.get() + 1) % MAX_FRAMES_IN_FLIGHT);
  }
}
impl Renderer for VulkanRenderer {
  type BL = VulkanBufferFunctions;
  type SL = VulkanShaderFunctions;

  fn set_rendering_enabled(&mut self, enabled: bool) {
    self.rendering_enabled = enabled;
  }

  // TODO OFFSCREEN handle off screen rendering.
  fn frame(&self) -> SarektResult<()> {
    let logical_device = &self.vulkan_device_structures.logical_device;
    let queues = &self.vulkan_device_structures.queues;

    if !self.rendering_enabled {
      return Ok(());
    }

    let current_frame_num = self.current_frame_num.get();
    let image_available_sem = self
      .draw_synchronization
      .get_image_available_sem(current_frame_num);
    let render_finished_sem = self
      .draw_synchronization
      .get_render_finished_semaphore(current_frame_num);

    let image_index = self.next_image_index.get();
    let current_command_buffer = self.primary_gfx_command_buffers[image_index as usize];
    unsafe {
      // End Render Pass.
      logical_device.cmd_end_render_pass(current_command_buffer);

      // Finish recording on all command buffers.
      // TODO MULTITHREADING all of them not just main.
      logical_device.end_command_buffer(current_command_buffer)?;
    }

    // Wait for max images in flight.
    let frame_fence = self
      .draw_synchronization
      .ensure_image_resources_ready(image_index as usize, current_frame_num)?;
    self
      .draw_synchronization
      .set_image_to_in_flight_frame(image_index as usize, current_frame_num);

    // Submit draw commands.
    let wait_semaphores = [image_available_sem];
    let wait_dst_stage_mask = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
    let command_buffers = [current_command_buffer];
    let signal_semaphores = [render_finished_sem];
    let submit_info = vk::SubmitInfo::builder()
      .wait_semaphores(&wait_semaphores) // Don't draw until it is ready.
      .wait_dst_stage_mask(&wait_dst_stage_mask) // Don't we only need to wait until Color Attachment is ready to start drawing.  Vertex and other shaders can begin sooner.
      .command_buffers(&command_buffers) // Only use the command buffer corresponding to this image index.
      .signal_semaphores(&signal_semaphores) // Signal we're done drawing when we are.
      .build();
    unsafe { logical_device.queue_submit(queues.graphics_queue, &[submit_info], frame_fence)? };

    // TODO OFFSCREEN only if presenting to swapchain.
    // Present to swapchain and display completed frame.
    let wait_semaphores = [render_finished_sem];
    let swapchains = [self.swapchain_and_extension.swapchain];
    let image_indices = [image_index as u32];
    let present_info = vk::PresentInfoKHR::builder()
      .wait_semaphores(&wait_semaphores)
      .swapchains(&swapchains)
      .image_indices(&image_indices)
      .build();
    unsafe {
      self
        .swapchain_and_extension
        .swapchain_functions
        .queue_present(queues.presentation_queue, &present_info)?
    };

    // Increment frames rendered count.
    self.increment_frame_count();

    // Set up the next frame for drawing. Will wait on fence.
    self.setup_next_main_command_buffer()?;

    Ok(())
  }

  fn load_shader(
    &mut self, code: &ShaderCode, shader_type: ShaderType,
  ) -> SarektResult<ShaderHandle<VulkanShaderFunctions>> {
    ShaderStore::load_shader(&self.shader_store, &code, shader_type)
  }

  // TODO CRITICAL investigate the ability to allow all these to propogate up the
  // backend handle as well to avoid having to load in drawable object, will this
  // break the lifetime stuff?
  fn load_buffer<BufElem: Sized + Copy>(
    &mut self, buffer_type: BufferType, buffer: &[BufElem],
  ) -> SarektResult<BufferImageHandle<VulkanBufferFunctions>> {
    if matches!(buffer_type, BufferType::Uniform) {
      return Err(SarektError::IncorrectLoaderFunction);
    }

    Ok(BufferImageStore::load_buffer_with_staging(&self.buffer_image_store, buffer_type, buffer)?.0)
  }

  fn load_image_with_staging_initialization(
    &mut self, pixels: impl ImageData, magnification_filter: MagnificationMinificationFilter,
    minification_filter: MagnificationMinificationFilter, address_x: TextureAddressMode,
    address_y: TextureAddressMode, address_z: TextureAddressMode,
  ) -> SarektResult<BufferImageHandle<VulkanBufferFunctions>> {
    Ok(
      BufferImageStore::load_image_with_staging_initialization(
        &self.buffer_image_store,
        pixels,
        magnification_filter,
        minification_filter,
        address_x,
        address_y,
        address_z,
      )?
      .0,
    )
  }

  fn get_buffer(
    &self, handle: &BufferImageHandle<VulkanBufferFunctions>,
  ) -> SarektResult<ResourceWithMemory> {
    let store = self
      .buffer_image_store
      .read()
      .expect("Panic occured can't read from buffer store");

    if let result @ ResourceWithMemory::Buffer(_) = store.get_buffer(handle)?.handle {
      Ok(result)
    } else {
      Err(SarektError::IncorrectResourceType)
    }
  }

  fn load_uniform_buffer<UniformBufElem: Sized + Copy>(
    &mut self, buffer: UniformBufElem,
  ) -> SarektResult<UniformBufferHandle<VulkanBufferFunctions, UniformBufElem>> {
    info!("Loading a uniform buffer...");
    // Since each framebuffer may have different values for uniforms, they each need
    // their own UB.  These are stored in the same ordering as the render target
    // images.
    let mut uniform_buffers = Vec::with_capacity(self.framebuffers.len());
    for _ in 0..self.framebuffers.len() {
      // TODO PERFORMANCE EASY create a "locked" version of the loading function
      // so I don't have to keep reacquiring it.
      let (uniform_buffer_handle, _) = BufferImageStore::load_buffer_without_staging(
        &self.buffer_image_store,
        BufferType::Uniform,
        &[buffer],
      )?;
      uniform_buffers.push(uniform_buffer_handle);
    }

    Ok(UniformBufferHandle::new(uniform_buffers))
  }

  fn get_uniform_buffer<UniformBufElem: Sized + Copy>(
    &self, handle: &UniformBufferHandle<VulkanBufferFunctions, UniformBufElem>,
  ) -> SarektResult<Vec<BufferAndMemoryMapped>>
  where
    Self::BL: BufferAndImageLoader,
  {
    let store = self
      .buffer_image_store
      .read()
      .expect("Panic occured can't read from buffer store");
    let mut buffer_handles: Vec<BufferAndMemoryMapped> =
      Vec::with_capacity(self.render_targets.len());
    for ubh in handle.uniform_buffer_backend_handle.iter() {
      let handle = store.get_buffer(ubh)?;

      // Check this is a uniform buffer handle.
      match handle.resource_type {
        ResourceType::Buffer(BufferType::Uniform) => (),
        ResourceType::Buffer(_) => return Err(SarektError::IncorrectBufferType),
        _ => return Err(SarektError::IncorrectResourceType),
      };

      let allocation = &handle.handle.buffer()?.allocation;
      let ptr = self.allocator.map_memory(allocation)?;
      let buffer_and_mem_mapped = if let ResourceWithMemory::Buffer(buffer_handle) = handle.handle {
        Ok(BufferAndMemoryMapped::new(buffer_handle, ptr))
      } else {
        Err(SarektError::IncorrectResourceType)
      }?;

      buffer_handles.push(buffer_and_mem_mapped);
    }

    Ok(buffer_handles)
  }

  fn set_uniform<BufElem: Sized + Copy>(
    &self, handle_data: &Vec<BufferAndMemoryMapped>, data: &BufElem,
  ) -> SarektResult<()> {
    self.draw_synchronization.wait_for_acquire_fence()?;

    let next_image_index = self.next_image_index.get();
    unsafe {
      let ptr = handle_data[next_image_index].ptr as *mut BufElem;
      ptr.copy_from(data, std::mem::size_of_val(data));
    }

    Ok(())
  }

  fn recreate_swapchain(&mut self, width: u32, height: u32) -> SarektResult<()> {
    if width == 0 || height == 0 {
      // It violates the vulkan spec to make extents this small, rendering should be
      // disabled explicitly in this case, but its up the application/platform.
      return Ok(());
    }

    if self.extent.width == width && self.extent.height == height {
      info!("No change, nothing to do");
      return Ok(());
    }

    unsafe { self.do_recreate_swapchain(width, height) }
  }

  fn get_image(
    &self, handle: &BufferImageHandle<VulkanBufferFunctions>,
  ) -> SarektResult<ResourceWithMemory> {
    let store = self
      .buffer_image_store
      .read()
      .expect("Panic occured can't read from buffer store");

    if let result @ ResourceWithMemory::Image(_) = store.get_image(handle)?.handle {
      Ok(result)
    } else {
      Err(SarektError::IncorrectResourceType)
    }
  }
}
impl Drawer for VulkanRenderer {
  type R = VulkanRenderer;

  // TODO BUFFERS BACKLOG do push_constant uniform buffers and example.
  fn draw<DescriptorLayoutStruct>(
    &self, object: &DrawableObject<Self, DescriptorLayoutStruct>,
  ) -> SarektResult<()>
  where
    DescriptorLayoutStruct: Sized + Copy + DescriptorLayoutInfo,
  {
    if !self.rendering_enabled {
      return Ok(());
    }

    let current_render_target_index = self.next_image_index.get();

    // Current render target command buffer.
    let current_command_buffer = self.primary_gfx_command_buffers[current_render_target_index];
    let current_uniform_buffer = object.uniform_buffer[current_render_target_index]
      .buffer_and_memory
      .buffer;
    let current_descriptor_pool = self.main_descriptor_pools[current_render_target_index];

    // Allocate and bind the correct uniform descriptors.
    self.bind_descriptor_sets::<DescriptorLayoutStruct>(
      current_uniform_buffer,
      &object.texture_image.map(|ti| ti.image().unwrap()),
      current_descriptor_pool,
      current_command_buffer,
    )?;

    // Draw the vertices (indexed or otherwise).
    self.draw_vertices_cmd(object, current_command_buffer)?;

    Ok(())
  }
}
impl Drop for VulkanRenderer {
  fn drop(&mut self) {
    unsafe {
      let logical_device = &self.vulkan_device_structures.logical_device;

      info!("Waiting for the device to be idle before cleaning up...");
      if let Err(e) = logical_device.device_wait_idle() {
        error!("Failed to wait for idle! {}", e);
      }

      info!("Destroying default null texture...");
      let default_texture = self.default_texture.take();
      std::mem::drop(default_texture);

      info!("Destroying all images, buffers, and associated synchronization semaphores...");
      self.buffer_image_store.write().unwrap().cleanup().unwrap();
      ManuallyDrop::drop(&mut self.buffer_image_store);

      info!("Destroying VMA...");
      Arc::get_mut(&mut self.allocator).unwrap().destroy();

      // TODO MULTITHREADING do I need to free others?
      info!("Freeing main command buffer...");
      logical_device.free_command_buffers(
        self.main_gfx_command_pool,
        &self.primary_gfx_command_buffers,
      );

      self
        .cleanup_swapchain()
        .expect("Could not clean up swapchain while cleaning up VulkanRenderer...");

      info!("Destroying default descriptor set layouts for default pipeline...");
      if let Some(descriptor_set_layouts) =
        &self.base_graphics_pipeline_bundle.descriptor_set_layouts
      {
        for &layout in descriptor_set_layouts.iter() {
          logical_device.destroy_descriptor_set_layout(layout, None);
        }
      }

      self.draw_synchronization.destroy_all();

      info!("Destroying all command pools...");
      logical_device.destroy_command_pool(self.main_gfx_command_pool, None);
      if self.main_gfx_command_pool != self.transfer_command_pool {
        logical_device.destroy_command_pool(self.transfer_command_pool, None);
      }

      info!("Destroying all shaders...");
      self.shader_store.write().unwrap().destroy_all_shaders();

      ManuallyDrop::drop(&mut self.vulkan_device_structures);
      ManuallyDrop::drop(&mut self.vulkan_core);
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::renderer::{
    vulkan::{debug_utils_ext::DebugUserData, vulkan_renderer::debug_utils_ext::DebugUserData},
    ApplicationDetails, EngineDetails, Version, VulkanRenderer, IS_DEBUG_MODE,
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

  // TODO CRITICAL TESTING write triangle sanity check that can dump buffer and
  // compare to golden image.

  // TODO CRITICAL TESTING write tests for public api using this.  rust doesn't
  // run in test harness so in some platforms calls to frame don't present correctly. also consider [this](https://stackoverflow.com/questions/43458194/is-there-any-way-to-tell-cargo-to-run-its-tests-on-the-main-thread)
}
